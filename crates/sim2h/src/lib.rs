#![feature(vec_remove_item)]

extern crate backtrace;
extern crate env_logger;
extern crate lib3h_crypto_api;
extern crate log;
extern crate nanoid;
extern crate num_cpus;
#[macro_use]
extern crate serde;
#[macro_use]
extern crate lazy_static;
extern crate newrelic;

#[macro_use]
extern crate holochain_common;

#[allow(dead_code)]
mod naive_sharding;

pub mod cache;
pub mod connection_state;
pub mod crypto;
pub mod error;
use lib3h_protocol::types::{AgentPubKey, AspectHash, EntryHash};
mod message_log;
pub mod websocket;
pub mod wire_message;

pub use crate::message_log::MESSAGE_LOGGER;
use crate::{crypto::*, error::*, naive_sharding::entry_location};
use cache::*;
use connection_state::*;
use lib3h_crypto_api::CryptoSystem;
use lib3h_protocol::{
    data_types::{
        EntryData, EntryListData, FetchEntryData, GetListData, Opaque, QueryEntryData, SpaceData,
        StoreEntryAspectData,
    },
    protocol::*,
    types::SpaceHash,
    uri::Lib3hUri,
};

pub use wire_message::{
    HelloData, StatusData, WireError, WireMessage, WireMessageVersion, WIRE_VERSION,
};

use futures::{
    future::{BoxFuture, FutureExt},
    stream::StreamExt,
};
use in_stream::*;
use log::*;
use rand::{seq::SliceRandom, thread_rng};
use std::{
    collections::{HashMap, HashSet},
    convert::TryFrom,
    sync::Arc,
};

use holochain_locksmith::Mutex;
use holochain_metrics::{
    config::MetricPublisherConfig, with_latency_publishing, Metric, MetricPublisher,
};

/// if we can't acquire a lock in 20 seconds, panic!
const MAX_LOCK_TIMEOUT: u64 = 20000;

//set up license_key
new_relic_setup!("NEW_RELIC_LICENSE_KEY");

/// extention trait for making sure deadlocks are fatal
pub(crate) trait MutexExt<T> {
    /// will attempt to aquire a lock within a time-frame and panic after
    /// this way deadlocks don't just lock forever
    fn f_lock(&self) -> holochain_locksmith::MutexGuard<T>;
}

impl<T> MutexExt<T> for Mutex<T> {
    fn f_lock(&self) -> holochain_locksmith::MutexGuard<T> {
        // if we can't acquire a lock in 20 seconds, panic!
        self.try_lock_for(std::time::Duration::from_millis(MAX_LOCK_TIMEOUT))
            .expect("failed to obtain mutex lock")
    }
}

/// if a channel send fails, it means it is disconnected
/// this extension trait simplifies panic!ing in that case
/// in a lot of places, we expect the channel to always be open
/// and don't have the infrustructure to deal with degenerate cases
/// this trait makes sending more readable when we want to panic! on disconnects
pub(crate) trait SendExt<T> {
    fn f_send(&self, v: T);
}

impl<T> SendExt<T> for crossbeam_channel::Sender<T> {
    fn f_send(&self, v: T) {
        if let Err(e) = self.send(v) {
            error!("failed to send on channel -- shutting down? {:?}", e);
        }
    }
}

const RETRY_FETCH_MISSING_ASPECTS_INTERVAL_MS: u64 = 30000; // 30 seconds

fn conn_lifecycle(desc: &str, uuid: &str, obj: &ConnectionState, uri: &Lib3hUri) {
    debug!(
        "connection event conn: {} for {}@{} {:?}",
        desc, uuid, uri, obj
    );
}

fn open_lifecycle(desc: &str, uuid: &str, uri: &Lib3hUri) {
    debug!("connection event open_conns: {} for {}@{}", desc, uuid, uri);
}

#[derive(Clone)]
struct MetricsTimerGenerator {
    sender: tokio::sync::mpsc::UnboundedSender<(&'static str, f64)>,
}

impl MetricsTimerGenerator {
    pub fn new() -> (Self, BoxFuture<'static, ()>) {
        let (sender, mut recv) = tokio::sync::mpsc::unbounded_channel::<(&'static str, f64)>();
        let out = async move {
            let metric_publisher = MetricPublisherConfig::default().create_metric_publisher();
            loop {
                let msg = match recv.next().await {
                    None => return,
                    Some(msg) => msg,
                };
                // TODO - this write is technically blocking
                //        move to spawn_blocking?? use tokio::sync::Mutex??
                metric_publisher
                    .write()
                    .unwrap()
                    .publish(&Metric::new_timestamped_now(msg.0, None, msg.1));
            }
        }
        .boxed();
        (Self { sender }, out)
    }

    pub fn timer(&self, tag: &'static str) -> MetricsTimer {
        MetricsTimer::new(tag, self.sender.clone())
    }
}

struct MetricsTimer {
    tag: &'static str,
    create_time: std::time::Instant,
    sender: tokio::sync::mpsc::UnboundedSender<(&'static str, f64)>,
}

impl MetricsTimer {
    pub fn new(
        tag: &'static str,
        sender: tokio::sync::mpsc::UnboundedSender<(&'static str, f64)>,
    ) -> Self {
        Self {
            tag,
            create_time: std::time::Instant::now(),
            sender,
        }
    }
}

impl Drop for MetricsTimer {
    fn drop(&mut self) {
        let elapsed = self.create_time.elapsed().as_millis() as f64;
        if elapsed >= 1000.0 {
            error!("VERY SLOW metric - {} - {} ms", self.tag, elapsed);
        } else if elapsed >= 100.0 {
            warn!("SLOW metric - {} - {} ms", self.tag, elapsed);
        } else if elapsed >= 10.0 {
            info!("metric - {} - {} ms", self.tag, elapsed);
        }
        if let Err(e) = self.sender.send((self.tag, elapsed)) {
            error!(
                "failed to send metric - shutting down? {} {:?}",
                self.tag, e
            );
        }
    }
}

//pub(crate) type TcpWssServer = InStreamListenerWss<InStreamListenerTls<InStreamListenerTcp>>;
//pub(crate) type TcpWss = InStreamWss<InStreamTls<InStreamTcp>>;
pub(crate) type TcpWssServer = InStreamListenerWss<InStreamListenerTcp>;
pub type TcpWss = InStreamWss<InStreamTcp>;

mod connection_mgr;
use connection_mgr::*;

#[derive(Clone)]
pub enum DhtAlgorithm {
    FullSync,
    NaiveSharding { redundant_count: u64 },
}

mod sim2h_state;
pub(crate) use sim2h_state::*;

#[derive(Debug)]
struct Sim2hComHandleMessage {
    uri: Lib3hUri,
    message: WireMessage,
    signer: AgentId,
}

#[derive(Debug)]
struct Sim2hComHandleJoined {
    uri: Lib3hUri,
    space_address: SpaceHash,
    agent_id: AgentId,
    message: WireMessage,
}

#[derive(Debug)]
enum Sim2hCom {
    HandleMessage(Box<Sim2hComHandleMessage>),
    HandleJoined(Box<Sim2hComHandleJoined>),
    Disconnect(Vec<Lib3hUri>),
}

#[derive(Clone)]
/// A clonable reference to our Sim2h instance that can be passed
/// into `'static` async blocks && still be able to make sim2h calls
struct Sim2hHandle {
    state: Arc<tokio::sync::Mutex<Sim2hState>>,
    send_com: tokio::sync::mpsc::UnboundedSender<Sim2hCom>,
    dht_algorithm: DhtAlgorithm,
    metric_gen: MetricsTimerGenerator,
    connection_mgr: ConnectionMgrHandle,
}

impl Sim2hHandle {
    pub fn new(
        state: Arc<tokio::sync::Mutex<Sim2hState>>,
        send_com: tokio::sync::mpsc::UnboundedSender<Sim2hCom>,
        dht_algorithm: DhtAlgorithm,
        metric_gen: MetricsTimerGenerator,
        connection_mgr: ConnectionMgrHandle,
    ) -> Self {
        Self {
            state,
            send_com,
            dht_algorithm,
            metric_gen,
            connection_mgr,
        }
    }

    /// generate a new metrics timer
    pub fn metric_timer(&self, tag: &'static str) -> MetricsTimer {
        self.metric_gen.timer(tag)
    }

    /// get our current dht algorithm
    pub fn dht_algorithm(&self) -> &DhtAlgorithm {
        &self.dht_algorithm
    }

    /// acquire a mutex lock to our state data
    pub async fn lock_state(&self) -> tokio::sync::MutexGuard<'_, Sim2hState> {
        self.state.lock().await
    }

    /// forward a message to be handled
    pub fn handle_message(&self, uri: Lib3hUri, message: WireMessage, signer: AgentId) {
        if let Err(e) =
            self.send_com
                .send(Sim2hCom::HandleMessage(Box::new(Sim2hComHandleMessage {
                    uri,
                    message,
                    signer,
                })))
        {
            error!("error sending message to sim2h - shutting down? {:?}", e);
        }
    }

    /// forward a message to an already joined connection to be handled
    pub fn handle_joined(
        &self,
        uri: Lib3hUri,
        space_address: SpaceHash,
        agent_id: AgentId,
        message: WireMessage,
    ) {
        self.send_com
            .send(Sim2hCom::HandleJoined(Box::new(Sim2hComHandleJoined {
                uri,
                space_address,
                agent_id,
                message,
            })))
            .expect("can send");
    }

    /// disconnect an active connection
    pub fn disconnect(&self, disconnect: Vec<Lib3hUri>) {
        self.send_com
            .send(Sim2hCom::Disconnect(disconnect))
            .expect("can send");
    }
}

/// creates a tokio runtime and executes the Sim2h instance within it
/// returns the runtime so the user can choose how to manage the main loop
pub fn run_sim2h(
    crypto: Box<dyn CryptoSystem>,
    bind_spec: Lib3hUri,
    dht_algorithm: DhtAlgorithm,
) -> (
    tokio::runtime::Runtime,
    tokio::sync::oneshot::Receiver<Lib3hUri>,
) {
    let rt = tokio::runtime::Builder::new()
        .enable_all()
        .threaded_scheduler()
        .core_threads(num_cpus::get())
        .thread_name("sim2h-tokio-thread")
        .build()
        .expect("can build tokio runtime");

    let (bind_send, bind_recv) = tokio::sync::oneshot::channel();

    rt.spawn(async move {
        let sim2h = Sim2h::new(crypto, bind_spec, dht_algorithm);
        let _ = bind_send.send(sim2h.bound_uri.clone().unwrap());

        /*
        tokio::task::spawn(async move {
            let mut listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("failed to bind");
            warn!("TT BOUND TO: {:?}", listener.local_addr());
            while let Ok((stream, addr)) = listener.accept().await {
                let stream: tokio::net::TcpStream = stream;
                tokio::task::spawn(async move {
                    warn!("GOT TT CONNECTION: {:?}", addr);
                    let ws_stream = tokio_tungstenite::accept_async(stream)
                        .await
                        .expect("failed to handshake websocket");
                    let (write, read) = ws_stream.split();
                    read.forward(write)
                        .await
                        .expect("failed to forward message")
                });
            }
        });
        */

        let gen_blocking_fn = move |mut sim2h: Sim2h| {
            move || {
                let res = sim2h.process();
                (sim2h, res)
            }
        };
        let mut blocking_fn = Some(gen_blocking_fn(sim2h));
        loop {
            // NOTE - once we move everything in sim2h to futures
            //        we can get rid of the `process()` function
            //        and remove this spawn_blocking code
            let sim2h = match tokio::task::spawn_blocking(blocking_fn.take().unwrap()).await {
                Err(e) => {
                    // sometimes we get errors on shutdown...
                    // we can't recover because the sim2h instance is lost
                    // but don't panic... just exit
                    error!("sim2h process failed: {:?}", e);
                    return;
                }
                Ok((sim2h, Err(e))) => {
                    if e.to_string().contains("Bind error:") {
                        println!("{:?}", e);
                        std::process::exit(1);
                    } else {
                        error!("{}", e.to_string())
                    }
                    sim2h
                }
                Ok((sim2h, Ok(did_work))) => {
                    if did_work {
                        tokio::task::yield_now().await;
                    } else {
                        tokio::time::delay_for(std::time::Duration::from_millis(1)).await;
                    }
                    sim2h
                }
            };
            blocking_fn = Some(gen_blocking_fn(sim2h));
        }
    });

    (rt, bind_recv)
}

/// a Sim2h server instance - manages connections between holochain instances
pub struct Sim2h {
    bound_listener: Option<TcpWssServer>,
    metric_task: Option<BoxFuture<'static, ()>>,
    pub bound_uri: Option<Lib3hUri>,
    wss_send: crossbeam_channel::Sender<TcpWss>,
    wss_recv: crossbeam_channel::Receiver<TcpWss>,
    connection_mgr_evt_recv: ConnectionMgrEventRecv,
    num_ticks: u64,
    /// when should we try to resync nodes that are still missing aspect data
    missing_aspects_resync: std::time::Instant,
    dht_algorithm: DhtAlgorithm,
    recv_com: tokio::sync::mpsc::UnboundedReceiver<Sim2hCom>,
    metric_publisher: std::sync::Arc<holochain_locksmith::RwLock<dyn MetricPublisher>>,
    sim2h_handle: Sim2hHandle,
    connection_count: usize,
    metric_gen: MetricsTimerGenerator,
}

#[holochain_tracing_macros::newrelic_autotrace(SIM2H)]
impl Sim2h {
    /// create a new Sim2h server instance
    pub fn new(
        crypto: Box<dyn CryptoSystem>,
        bind_spec: Lib3hUri,
        dht_algorithm: DhtAlgorithm,
    ) -> Self {
        let (metric_gen, metric_task) = MetricsTimerGenerator::new();

        let (connection_mgr, connection_mgr_evt_recv) = ConnectionMgr::new();

        let (wss_send, wss_recv) = crossbeam_channel::unbounded();
        let state = Arc::new(tokio::sync::Mutex::new(Sim2hState {
            crypto: crypto.box_clone(),
            connection_states: HashMap::new(),
            spaces: HashMap::new(),
            metric_gen: metric_gen.clone(),
            connection_mgr: connection_mgr.clone(),
        }));
        let (send_com, recv_com) = tokio::sync::mpsc::unbounded_channel();
        let sim2h_handle = Sim2hHandle::new(
            state,
            send_com,
            dht_algorithm.clone(),
            metric_gen.clone(),
            connection_mgr,
        );

        let config = TcpBindConfig::default();
        //        let config = TlsBindConfig::new(config).dev_certificate();
        let config = WssBindConfig::new(config);
        let url = url::Url::from(bind_spec).into();
        let listen: TcpWssServer = InStreamListenerWss::bind(&url, config).unwrap();
        let bound_uri = Some(url::Url::from(listen.binding()).into());

        let sim2h = Sim2h {
            // TODO - (db) - Sim2h::new() is now called inside tokio runtime
            //               we can move these back into the constructor
            bound_listener: Some(listen),
            metric_task: Some(metric_task),
            bound_uri,
            wss_send,
            wss_recv,
            connection_mgr_evt_recv,
            num_ticks: 0,
            missing_aspects_resync: std::time::Instant::now(),
            dht_algorithm,
            metric_publisher: MetricPublisherConfig::default().create_metric_publisher(),
            recv_com,
            sim2h_handle,
            connection_count: 0,
            metric_gen,
        };

        sim2h
    }

    /// if our listening socket has accepted any new connections, set them up
    fn priv_check_incoming_connections(&mut self) -> bool {
        let _m = self
            .metric_gen
            .timer("sim2h-priv_check_incoming_connections");

        let mut did_work = false;
        let mut wss_list = Vec::new();
        for _ in 0..100 {
            if let Ok(wss) = self.wss_recv.try_recv() {
                did_work = true;
                wss_list.push(wss);
            } else {
                break;
            }
        }
        if !wss_list.is_empty() {
            let sim2h_handle = self.sim2h_handle.clone();
            tokio::task::spawn(async move {
                let _m =
                    sim2h_handle.metric_timer("sim2h-priv_check_incoming_connections-async-add");
                let mut state = sim2h_handle.lock_state().await;

                for wss in wss_list.drain(..) {
        with_latency_publishing!(
            "sim2h-priv_check_incoming_connections",
            self.metric_publisher,
            || {
                if let Ok(wss) = self.wss_recv.try_recv() {
                    let url: Lib3hUri = url::Url::from(wss.remote_url()).into();
                    let (job, outgoing_send) = ConnectionJob::new(wss, self.msg_send.clone());
                    let job = Arc::new(Mutex::new(job));
                    if let Err(error) = self.handle_incoming_connect(url.clone()) {
                        error!(
                            "Error handling incoming connection from {}: {:?}",
                            url, error
                        );
                        return;
                    }
                    let uuid = nanoid::simple();
                    open_lifecycle("adding conn job", &uuid, &url);
                    self.open_connections
                        .insert(url, (uuid, job.clone(), outgoing_send));
                    self.pool.push_job(Box::new(job));
                        .insert(url.clone(), (nanoid::simple(), ConnectionState::new()));

                    state.connection_mgr.connect(url, wss);
                }
            });
                }
        did_work
            }
        )
    }

    /// we received some kind of error related to a stream/socket
    /// print some debugging and disconnect it
    fn priv_drop_connection_for_error(&mut self, uri: Lib3hUri, error: Sim2hError) {
        debug!(
            "dropping connection to {} because of error: {:?}",
            uri, error,
        );
        self.sim2h_handle.disconnect(vec![uri]);
    }

    /// if our connections sent us any data, process it
    fn priv_check_incoming_messages(&mut self) -> bool {
        with_latency_publishing!(
            "sim2h-priv_check_incoming_messages",
            self.metric_publisher,
            || {
                let len = self.msg_recv.len();
                if len > 0 {
                    debug!("Handling {} incoming messages", len);
                    did_work = true;
                    match evt {
                        ConMgrEvent::Disconnect(uri, maybe_err) => {
                            debug!("disconnect {} {:?}", uri, maybe_err);
                            disconnect.push(uri);
                }
                let v: Vec<_> = self.msg_recv.try_iter().collect();
                for (url, msg) in v {
                    let url: Lib3hUri = url::Url::from(url).into();
                    match msg {
                        Ok(frame) => match frame {
                        }
                    }
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                Err(tokio::sync::mpsc::error::TryRecvError::Closed) => {
                    panic!("connection mgr channel broken");
                }
            }
        }

        if !disconnect.is_empty() {
            self.sim2h_handle.disconnect(disconnect);
        }

        did_work
    }

    /// process an actual incoming message
    fn priv_handle_recv_data(&mut self, uri: Lib3hUri, data: WsFrame) {
        match data {
                            WsFrame::Text(s) => self.priv_drop_connection_for_error(
                                url,
                                format!("unexpected text message: {:?}", s).into(),
                            ),
                            WsFrame::Binary(b) => {
                                trace!("received a frame from {}", url);
                                let payload: Opaque = b.into();
                                match self.verify_payload(payload.clone()) {
                                    Ok((source, wire_message)) => {
                                        trace!(
                                            "frame from from {} verified and decoded to {:?}",
                                            url,
                                            wire_message
                                        );
                                        if let Err(error) =
                                            self.handle_message(&url, wire_message, &source)
                                        {
                                            error!(
                                                "Error handling message from {}: {:?}",
                                                url, error
                                            );
                                        }
                                    }
                                    Err(error) => error!(
                                }
                            }
                            // TODO - we should use websocket ping/pong
                            //        instead of rolling our own on top of Binary
                            WsFrame::Ping(_) => (),
                            WsFrame::Pong(_) => (),
                            WsFrame::Close(c) => {
                                debug!("Disconnecting {} after connection reset {:?}", url, c);
                                self.disconnect(&url);
                            }
                        },
                        Err(e) => self.priv_drop_connection_for_error(url, e),
                    }
                }
            }
        )
    }

    // adds an agent to a space
    async fn join(sim2h_handle: Sim2hHandle, uri: Lib3hUri, data: SpaceData) {
        let _m = sim2h_handle.metric_timer("sim2h-join");
        debug!("join entered for {} with {:?}", uri, data);
        let mut pending_messages = {
            let mut state = sim2h_handle.lock_state().await;
        with_latency_publishing!(
            "sim2h-request_authoring_list",
            self.metric_publisher,
            || {
                let wire_message = WireMessage::Lib3hToClient(
                    Lib3hToClient::HandleGetAuthoringEntryList(GetListData {
                        request_id: "".into(),
                        space_address,
                        provider_agent_id: provider_agent_id.clone(),
                    }),
                );
                self.send(provider_agent_id, uri, &wire_message);
            }
        )
    }

            let (_uuid, conn) = match state.connection_states.get_mut(&uri) {
                Some((uuid, conn)) => (uuid, conn),
                None => {
                    error!("no agent found at {} ", uri);
                    sim2h_handle.disconnect(vec![uri]);
                    return;
        with_latency_publishing!(
            "sim2h-request_gossiping_list",
            self.metric_publisher,
            || {
                let wire_message = WireMessage::Lib3hToClient(
                    Lib3hToClient::HandleGetGossipingEntryList(GetListData {
                        request_id: "".into(),
                        space_address,
                        provider_agent_id: provider_agent_id.clone(),
                    }),
                );
                self.send(provider_agent_id, uri, &wire_message);
            }
        )
    }

            let pending_messages = match conn {
        let clock = std::time::SystemTime::now();

                }

        let rw_lock = self.spaces.get(space_address).unwrap();
        self.metric_publisher
            .write()
            .unwrap()
            .publish(&Metric::new_timestamped_now(
                "sim2h-get_or_create_space.latency",
                None,
                clock.elapsed().unwrap().as_millis() as f64,
            ));
        rw_lock
                }
            };

            let new_conn = match ConnectionState::new_joined(
                data.space_address.clone(),
        with_latency_publishing!("sim2h-join", self.metric_publisher, || {
            debug!("join entered for {} with {:?}", uri, data);
            let result = if let Some((uuid, conn)) = self.get_connection(uri) {
                if let ConnectionState::Limbo(pending_messages) = conn {
                    let conn = ConnectionState::new_joined(
                        data.space_address.clone(),
                        data.agent_id.clone(),
                    )?;
                    let _ = self.connection_states.write().insert(
                        uri.clone(),
                        // MDD: we are overwriting the existing connection state here, so we keep the same uuid.
                        // (This could be done more directly with a Hashmap entry update)
                        (uuid, conn),
                    );

                    self.get_or_create_space(&data.space_address)
                        .write()
                        .join_agent(data.agent_id.clone(), uri.clone())?;
                state.join_agent(&data.space_address, data.agent_id.clone(), uri.clone())
            {
                error!("error joining agent {} - {:?}", uri, e);
                sim2h_handle.disconnect(vec![uri]);
                return;
            }
                    info!(
                        "Agent {:?} @ {} joined space {:?}",
                        data.agent_id, uri, data.space_address
                    );
                    self.request_authoring_list(
                        uri.clone(),
                        data.space_address.clone(),
                        data.agent_id.clone(),
                    );
                    self.request_gossiping_list(
                        uri.clone(),
                        data.space_address.clone(),
                        data.agent_id.clone(),
                    );
                    debug!("pending messages in join: {}", pending_messages.len());
                    for message in *pending_messages {
                        if let Err(err) = self.handle_message(uri, message.clone(), &data.agent_id)
                        {
                            error!(
                                "Error while handling limbo pending message {:?} for {}: {}",
                                message, uri, err
                            );
                        }
                    }
                    Ok(())
                } else {
                    Err(format!("no agent found in limbo at {} ", uri).into())
                }
            } else {
                Err(format!("no agent found at {} ", uri).into())
            };
            trace!("join done");
            result
        })

            pending_messages
        };
        with_latency_publishing!("sim2h-disconnnect", self.metric_publisher, || {
            if let Some((uuid, state)) = self.get_connection(uri) {
                conn_lifecycle("leave -> disconnect", &uuid, &state, uri);
                if let ConnectionState::Joined(space_address, agent_id) = state {
                    if (data.agent_id != agent_id) || (data.space_address != space_address) {
                        Err(SPACE_MISMATCH_ERR_STR.into())
                    } else {
                        self.disconnect(uri);
                        Ok(())
                    }
                } else {
                    Err(format!("no joined agent found at {} ", &uri).into())
                }
            } else {
                Err(format!("no agent found at {} ", &uri).into())
            }
        })
        with_latency_publishing!("sim2h-disconnnect", self.metric_publisher, || {
            trace!("disconnect entered");
            if let Some((uuid, con, _outgoing_send)) = self.open_connections.remove(uri) {
                open_lifecycle("disconnect", &uuid, uri);
                con.f_lock().stop();
            }
            if let Some((uuid, conn)) = self.connection_states.write().remove(uri) {
                conn_lifecycle("disconnect", &uuid, &conn, uri);
                if let ConnectionState::Joined(space_address, agent_id) = conn {
                    if let Some(space) = self.spaces.get_mut(&space_address) {
                        if space.write().remove_agent(&agent_id) == 0 {
                            self.spaces.remove(&space_address);
                        }
                    }
                }
            }
            trace!("disconnect done");
        })
        with_latency_publishing!("sim2h-get_connection", self.metric_publisher, || {
            let reader = self.connection_states.read();
            reader.get(uri).map(|ca| (*ca).clone())
        })

        debug!("pending messages in join: {}", pending_messages.len());
        for message in pending_messages.drain(..) {
        with_latency_publishing!(
            "sim2h-handle_incoming_connect",
            self.metric_publisher,
            || {
                self.spaces
                    .get(space_address)?
                    .read()
                    .agent_id_to_uri(agent_id)
            }
        )
    }
        with_latency_publishing!(
            "sim2h-handle_incoming_connect",
            self.metric_publisher,
            || {
                trace!("handle_incoming_connect entered");
                debug!("New connection from {:?}", uri);
                if let Some(_old) = self
                    .connection_states
                    .write()
                    .insert(uri.clone(), (nanoid::simple(), ConnectionState::new()))
                {
                    println!("TODO should remove {}", uri); //TODO
                };
                trace!("handle_incoming_connect done");
                Ok(true)
            }
        )
    }

    // handler for messages sent to sim2h
    fn handle_message(
        &mut self,
        uri: Lib3hUri,
        message: WireMessage,
        signer: AgentId,
    ) -> Sim2hResult<()> {
        with_latency_publishing!("sim2h-handle_messsage", self.metric_publisher, || {
            trace!("handle_message entered for {}", uri);

            MESSAGE_LOGGER
                .lock()
                .log_in(signer.clone(), uri.clone(), message.clone());
            let (uuid, mut agent) = self
                .get_connection(uri)
                .ok_or_else(|| format!("no connection for {}", uri))?;
            conn_lifecycle("handle_message", &uuid, &agent, uri);

            // TODO: anyway, but especially with this Ping/Pong, mitigate DoS attacks.
            if message == WireMessage::Ping {
                debug!("Sending Pong in response to Ping");
                self.send(signer.clone(), uri.clone(), &WireMessage::Pong);
            tokio::task::spawn(async move {
                sim2h_handle
                    .lock_state()
                    .await
                    .send(signer, uri, &WireMessage::Pong);
            });
                return Ok(());
            }
            if message == WireMessage::Status {
                debug!("Sending StatusResponse in response to Status");
                self.send(
            let connection_count = self.connection_count;
            tokio::task::spawn(async move {
                let state = sim2h_handle.lock_state().await;
                let spaces_len = state.spaces.len();
                state.send(
                    signer.clone(),
                    uri.clone(),
                    &WireMessage::StatusResponse(StatusData {
                        spaces: self.spaces.len(),
                        connections: self.open_connections.len(),
                        redundant_count: match self.dht_algorithm {
                            DhtAlgorithm::FullSync => 0,
                            DhtAlgorithm::NaiveSharding { redundant_count } => redundant_count,
                        },
                        version: WIRE_VERSION,
                    }),
                );
            });
                return Ok(());
            }
        if let WireMessage::Hello(version) = message {
            debug!("Sending HelloResponse in response to Hello({})", version);
            let sim2h_handle = self.sim2h_handle.clone();
            tokio::task::spawn(async move {
                let state = sim2h_handle.lock_state().await;
                state.send(
                    signer.clone(),
                    uri.clone(),
                    &WireMessage::HelloResponse(HelloData {
                        redundant_count: match sim2h_handle.dht_algorithm() {
                            DhtAlgorithm::FullSync => 0,
                            DhtAlgorithm::NaiveSharding { redundant_count } => *redundant_count,
                        },
                        version: WIRE_VERSION,
                        extra: None,
                    }),
                );
                // versions do not match - disconnect them
                if version != WIRE_VERSION {
                    warn!("Disconnecting client for bad version this WIRE_VERSIO = {}, client WIRE_VERSION = {}", WIRE_VERSION, version);
                    sim2h_handle.disconnect(vec![uri]);
                }
            });
            return Ok(());
        }

        tokio::task::spawn(Sim2h::handle_connection_msg(
            self.sim2h_handle.clone(),
            uri,
            message,
            signer,
        ));
        Ok(())
    }

    async fn handle_connection_msg(
        sim2h_handle: Sim2hHandle,
        uri: Lib3hUri,
        message: WireMessage,
        signer: AgentId,
    ) {
        let _m = sim2h_handle.metric_timer("sim2h-handle_connection_msg");
        let state = sim2h_handle.clone();
        let mut state = state.lock_state().await;
        let (uuid, agent) = match state.connection_states.get_mut(&uri) {
            Some((uuid, agent)) => (uuid, agent),
            None => {
                error!("handle message for disconnected agent: {}", uri);
                return;
            }
        };
        conn_lifecycle("handle_message", &uuid, &agent, &uri);

            match agent {
                // if the agent sending the message is in limbo, then the only message
                // allowed is a join message.
                ConnectionState::Limbo(ref mut pending_messages) => {
                    if let WireMessage::ClientToLib3h(ClientToLib3h::JoinSpace(data)) = message {
                        if &data.agent_id != signer {
                            return Err(SIGNER_MISMATCH_ERR_STR.into());
                        return;
                        }
                        self.join(uri, &data)
                    } else {
                    debug!("inserting into pending message while in limbo.");
                        // TODO: maybe have some upper limit on the number of messages
                        // we allow to queue before dropping the connections
                        pending_messages.push(message);
                        let _ = self
                            .connection_states
                            .write()
                            .insert(uri.clone(), (uuid, agent));
                        self.send(
                    state.send(
                            signer.clone(),
                            uri.clone(),
                            &WireMessage::Err(WireError::MessageWhileInLimbo),
                        );
                        Ok(())
                    }
                }
                // if the agent sending the messages has been vetted and is in the space
                // then build a message to be proxied to the correct destination, and forward it
                ConnectionState::Joined(space_address, agent_id) => {
                    if &agent_id != signer {
                        return Err(SIGNER_MISMATCH_ERR_STR.into());
                    return;
                    }
                    self.handle_joined(uri, &space_address, &agent_id, message)
                }
            }
        })
    }

    fn verify_payload(&self, payload: Opaque) -> Sim2hResult<(AgentId, WireMessage)> {
        with_latency_publishing!("sim2h-verify_payload", self.metric_publisher, || {
            let signed_message = SignedWireMessage::try_from(payload)?;
            match (|| -> Sim2hResult<(AgentId, WireMessage)> {
                let signed_message = SignedWireMessage::try_from(payload.clone())?;
            let result = signed_message.verify().unwrap();
            if !result {
                return Err(VERIFY_FAILED_ERR_STR.into());
            }
            let wire_message = WireMessage::try_from(signed_message.payload)?;
            Ok((signed_message.provenance.source().into(), wire_message))
        })
                Ok((source, wire_message)) => {
                    sim2h_handle.handle_message(url, wire_message, source)
                }
                Err(error) => {
                    error!(
                        "Could not verify payload from {}!\nError: {:?}\nPayload was: {:?}",
                        url, error, payload
                    );
                    sim2h_handle.disconnect(vec![url]);
                }
            }
        });
    }

    /// process transport and incoming messages from it
    pub fn process(&mut self) -> Sim2hResult<bool> {
        with_latency_publishing!("sim2h-process", self.metric_publisher, || {
        if self.bound_listener.is_some() {
            let mut listen = self.bound_listener.take().unwrap();
            let wss_send = self.wss_send.clone();
            tokio::task::spawn(async move {
                loop {
                    let mut did_work = false;
                    for _ in 0..100 {
                        match listen.accept() {
                            Ok(wss) => {
                                wss_send.f_send(wss);
                                did_work = true;
                            }
                            Err(e) if e.would_block() => {
                                break;
                            }
                            Err(e) => {
                                error!(
                                    "LISTEN ACCEPT FAIL: {:?}\nbacktrace: {:?}",
                                    e,
                                    backtrace::Backtrace::new()
                                );
                                did_work = true;
                            }
                        }
                    }
                    if did_work {
                        tokio::task::yield_now().await;
                    } else {
                        tokio::time::delay_for(std::time::Duration::from_millis(10)).await;
                    }
                }
            });
        }
        if self.metric_task.is_some() {
            tokio::task::spawn(self.metric_task.take().unwrap());
        }

        let mut did_work = false;

            self.num_ticks += 1;
            if self.num_ticks % 60000 == 0 {
                debug!(".");
                self.num_ticks = 0;
            }

            self.priv_check_incoming_connections();
            self.priv_check_incoming_messages();
            match self.recv_com.try_recv() {
                Ok(Sim2hCom::Disconnect(mut disconnects)) => {
                    did_work = true;
                    d_list.append(&mut disconnects);
                }
                Ok(Sim2hCom::HandleMessage(m)) => {
                    did_work = true;
                    self.handle_message(m.uri, m.message, m.signer)?;
                }
                Ok(Sim2hCom::HandleJoined(m)) => {
                    did_work = true;
                    self.handle_joined(m.uri, m.space_address, m.agent_id, m.message)?;
                }
                _ => (),
            }
        }
        if !d_list.is_empty() {
            let sim2h_handle = self.sim2h_handle.clone();
            tokio::task::spawn(async move {
                let mut state = sim2h_handle.lock_state().await;
                for url in d_list {
                    state.disconnect(&url);
                }
            });
        }

        if self.priv_check_incoming_connections() {
            did_work = true;
        }

        if self.priv_check_incoming_messages() {
            did_work = true;
        }

            if std::time::Instant::now() >= self.missing_aspects_resync {
                self.missing_aspects_resync = std::time::Instant::now()
                    .checked_add(std::time::Duration::from_millis(
                        RETRY_FETCH_MISSING_ASPECTS_INTERVAL_MS,
                    ))
                    .expect("can add interval ms");

                self.retry_sync_missing_aspects();
            }
                sim2h_handle.lock_state().await.retry_sync_missing_aspects();
            Ok(())
        })
        }

        Ok(did_work)
        with_latency_publishing!("sim2h-handle-unseen_aspects", self.metric_publisher, || {
            let unseen_aspects = AspectList::from(list_data.address_map.clone())
                .diff(self.get_or_create_space(space_address).read().all_aspects());
            debug!("UNSEEN ASPECTS:\n{}", unseen_aspects.pretty_string());
            for entry_address in unseen_aspects.entry_addresses() {
                if let Some(aspect_address_list) = unseen_aspects.per_entry(entry_address) {
                    let wire_message = WireMessage::Lib3hToClient(Lib3hToClient::HandleFetchEntry(
                        FetchEntryData {
                            request_id: "".into(),
                            space_address: space_address.clone(),
                            provider_agent_id: agent_id.clone(),
                            entry_address: entry_address.clone(),
                            aspect_address_list: Some(aspect_address_list.clone()),
                        },
                    ));
                    self.send(agent_id.clone(), uri.clone(), &wire_message);
                }
            }
        })
    }

    /// given an incoming messages, prepare a proxy message and whether it's an publish or request
    #[allow(clippy::cognitive_complexity)]
    fn handle_joined(
        &mut self,
        uri: Lib3hUri,
        space_address: SpaceHash,
        agent_id: AgentId,
        message: WireMessage,
    ) -> Sim2hResult<()> {
        with_latency_publishing!("sim2h-joined", self.metric_publisher, || {
            trace!("handle_joined entered");
            debug!(
                "<<IN<< {} from {}",
                message.message_type(),
                agent_id.to_string()
            );
            match message {
            // First make sure we are not receiving a message in the wrong direction.
            // Panic for now so we can easily spot a mistake.
            // Should maybe break up WireMessage into two different structs so we get the
            // error already when parsing an incoming payload.
            WireMessage::Lib3hToClient(_) | WireMessage::ClientToLib3hResponse(_) =>
                panic!("This is soo wrong. Clients should never send a message that only servers can send."),
            // -- Space -- //
            WireMessage::ClientToLib3h(ClientToLib3h::JoinSpace(_)) => {
                Err("join message should have been processed elsewhere and can't be proxied".into())
            }
            WireMessage::ClientToLib3h(ClientToLib3h::LeaveSpace(data)) => {
                let sim2h_handle = self.sim2h_handle.clone();
                tokio::task::spawn(async move {
                    let mut state = sim2h_handle.lock_state().await;
                    if let Err(e) = state.leave(&uri, &data) {
                        error!("failed to leave space: {:?}", e);
                        sim2h_handle.disconnect(vec![uri]);
                    }
                });
                Ok(())
            }

            // -- Direct Messaging -- //
            // Send a message directly to another agent on the network
            WireMessage::ClientToLib3h(ClientToLib3h::SendDirectMessage(dm_data)) => {
                if (dm_data.from_agent_id != agent_id) || (dm_data.space_address != space_address) {
                    return Err(SPACE_MISMATCH_ERR_STR.into());
                }
                let sim2h_handle = self.sim2h_handle.clone();
                tokio::task::spawn(async move {
                    let state = sim2h_handle.lock_state().await;
                    let to_url = match state
                        .lookup_joined(&space_address, &dm_data.to_agent_id)
                    {
                        Some(to_url) => to_url,
                        None => {
                            error!("unvalidated proxy agent {}", &dm_data.to_agent_id);
                            return;
                        }
                    };
                    state.send(
                        dm_data.to_agent_id.clone(),
                        to_url,
                        &WireMessage::Lib3hToClient(Lib3hToClient::HandleSendDirectMessage(dm_data))
                    );
                });
                Ok(())
            }
            // Direct message response
            WireMessage::Lib3hToClientResponse(Lib3hToClientResponse::HandleSendDirectMessageResult(
                dm_data,
            )) => {
                if (dm_data.from_agent_id != agent_id) || (dm_data.space_address != space_address) {
                    return Err(SPACE_MISMATCH_ERR_STR.into());
                }
                let sim2h_handle = self.sim2h_handle.clone();
                tokio::task::spawn(async move {
                    let state = sim2h_handle.lock_state().await;
                    let to_url = match state
                        .lookup_joined(&space_address, &dm_data.to_agent_id)
                    {
                        Some(to_url) => to_url,
                        None => {
                            error!("unvalidated proxy agent {}", &dm_data.to_agent_id);
                            return;
                        }
                    };
                    state.send(
                        dm_data.to_agent_id.clone(),
                        to_url,
                        &WireMessage::Lib3hToClient(Lib3hToClient::SendDirectMessageResult(dm_data))
                    );
                });
                Ok(())
            }
            WireMessage::ClientToLib3h(ClientToLib3h::PublishEntry(data)) => {
                if (data.provider_agent_id != agent_id) || (data.space_address != space_address) {
                    return Err(SPACE_MISMATCH_ERR_STR.into());
                }
                let sim2h_handle = self.sim2h_handle.clone();
                tokio::task::spawn(Sim2hState::handle_new_entry_data(
                    sim2h_handle,
                    data.entry,
                    space_address,
                    agent_id,
                ));
                Ok(())
            }
            WireMessage::Lib3hToClientResponse(Lib3hToClientResponse::HandleGetAuthoringEntryListResult(list_data)) => {
                debug!("GOT AUTHORING LIST from {}", agent_id);
                if (list_data.provider_agent_id != agent_id) || (list_data.space_address != space_address) {
                    return Err(SPACE_MISMATCH_ERR_STR.into());
                }
                self.handle_unseen_aspects(&uri, &space_address, &agent_id, &list_data);
                Ok(())
            }
            WireMessage::Lib3hToClientResponse(Lib3hToClientResponse::HandleGetGossipingEntryListResult(list_data)) => {
                debug!("GOT GOSSIPING LIST from {}", agent_id);
                if (list_data.provider_agent_id != agent_id) || (list_data.space_address != space_address) {
                    return Err(SPACE_MISMATCH_ERR_STR.into());
                }
                self.handle_unseen_aspects(&uri, &space_address, &agent_id, &list_data);

                let sim2h_handle = self.sim2h_handle.clone();

                tokio::task::spawn(async move {
                    let l_state = sim2h_handle.clone();
                    let mut l_state = l_state.lock_state().await;

                    // Check if the node is missing any aspects
                    let aspects_missing_at_node = match sim2h_handle.dht_algorithm() {
                        DhtAlgorithm::FullSync => l_state
                            .get_space(&space_address)
                            .all_aspects()
                            .diff(&AspectList::from(list_data.address_map)),
                        DhtAlgorithm::NaiveSharding {redundant_count} => l_state
                            .get_space(&space_address)
                            .aspects_in_shard_for_agent(&agent_id, *redundant_count)
                            .diff(&AspectList::from(list_data.address_map))
                    };

                    if aspects_missing_at_node.entry_addresses().count() > 0 {
                        warn!("MISSING ASPECTS at {}:\n{}", agent_id, aspects_missing_at_node.pretty_string());

                        // Cache info about what this agent is missing so we can make sure it got it
                        let missing_hashes: HashSet<(EntryHash, AspectHash)> = (&aspects_missing_at_node).into();
                        if missing_hashes.len() > 0 {
                            l_state.add_missing_aspects(&space_address, &agent_id, missing_hashes);
                        }

                        match sim2h_handle.dht_algorithm() {

                            DhtAlgorithm::FullSync => {
                                let all_agents_in_space = l_state
                                    .get_space(&space_address)
                                    .all_agents()
                                    .keys()
                                    .cloned()
                                    .collect::<Vec<AgentPubKey>>();
                                if all_agents_in_space.len() == 1 {
                                    error!("MISSING ASPECTS and no way to get them. Agent is alone in space..");
                                } else {
                                    Sim2h::fetch_aspects_from_arbitrary_agent(
                                        sim2h_handle,
                                        aspects_missing_at_node,
                                        agent_id.clone(),
                                        all_agents_in_space,
                                        space_address.clone()
                                    );
                                }
                            },

                            DhtAlgorithm::NaiveSharding {redundant_count} => {
                                for entry_address in aspects_missing_at_node.entry_addresses() {
                                    let entry_loc = entry_location(&l_state.crypto, entry_address);
                                    let agent_pool = l_state
                                        .get_space(&space_address)
                                        .agents_supposed_to_hold_entry(entry_loc, *redundant_count)
                                        .keys()
                                        .cloned()
                                        .collect::<Vec<AgentPubKey>>();
                                    Sim2h::fetch_aspects_from_arbitrary_agent(
                                        sim2h_handle.clone(),
                                        aspects_missing_at_node.filtered_by_entry_hash(|e| e == entry_address),
                                        agent_id.clone(),
                                        agent_pool,
                                        space_address.clone()
                                    );
                                }
                            }
                        }
                    }
                });

                Ok(())
            }
            WireMessage::Lib3hToClientResponse(
                Lib3hToClientResponse::HandleFetchEntryResult(fetch_result)) => {
                if (fetch_result.provider_agent_id != agent_id) || (fetch_result.space_address != space_address) {
                    return Err(SPACE_MISMATCH_ERR_STR.into());
                }
                debug!("HANDLE FETCH ENTRY RESULT: {:?}", fetch_result);
                if fetch_result.request_id == "" {
                    debug!("Got FetchEntry result from {} without request id - must be from authoring list", agent_id);
                    let sim2h_handle = self.sim2h_handle.clone();
                    tokio::task::spawn(Sim2hState::handle_new_entry_data(
                        sim2h_handle,
                        fetch_result.entry,
                        space_address,
                        agent_id,
                    ));
                } else {
                    debug!("Got FetchEntry result with request id {} - this is for gossiping to agent with incomplete data", fetch_result.request_id);
                    let sim2h_handle = self.sim2h_handle.clone();
                    tokio::task::spawn(async move {
                        let to_agent_id = AgentPubKey::from(fetch_result.request_id);
                        let mut multi_messages = Vec::new();
                        let mut to_remove = Vec::new();
                        for aspect in fetch_result.entry.aspect_list {
                            to_remove.push((
                                fetch_result.entry.entry_address.clone(),
                                aspect.aspect_address.clone(),
                            ));
                            multi_messages.push(Lib3hToClient::HandleStoreEntryAspect(
                                StoreEntryAspectData {
                                    request_id: "".into(),
                                    space_address: space_address.clone(),
                                    provider_agent_id: agent_id.clone(),
                                    entry_address: fetch_result.entry.entry_address.clone(),
                                    entry_aspect: aspect,
                                },
                            ));
                        }

                        let store_message = WireMessage::MultiSend(multi_messages);

                        let mut state = sim2h_handle.lock_state().await;
                        let maybe_url = state.lookup_joined(&space_address, &to_agent_id);
                        if maybe_url.is_none() {
                            error!("Got FetchEntryResult with request id that is not a known agent id. I guess we lost that agent before we could deliver missing aspects.");
                            return;
                        }
                        let url = maybe_url.unwrap();
                        for (entry_address, aspect_address) in to_remove.drain(..) {
                            state.remove_missing_aspect(
                                &space_address,
                                &to_agent_id,
                                &entry_address,
                                &aspect_address,
                            );
                        }
                        state.send(to_agent_id, url, &store_message);
                    });
                }

                Ok(())
            }
            WireMessage::ClientToLib3h(ClientToLib3h::QueryEntry(query_data)) => {
                if let DhtAlgorithm::NaiveSharding {redundant_count} = self.dht_algorithm {
                    let sim2h_handle = self.sim2h_handle.clone();
                    tokio::task::spawn(async move {
                        let disconnects = sim2h_handle
                            .lock_state().await
                            .build_query(
                                space_address,
                                query_data,
                                redundant_count
                            );
                        sim2h_handle.disconnect(disconnects);
                    });
                    Ok(())
                } else {
                    Err("Got ClientToLib3h::QueryEntry in full-sync mode".into())
                }
            }
            WireMessage::Lib3hToClientResponse(Lib3hToClientResponse::HandleQueryEntryResult(query_result)) => {
                if (query_result.responder_agent_id != agent_id) || (query_result.space_address != space_address)
                {
                    return Err(SPACE_MISMATCH_ERR_STR.into());
                }
                let sim2h_handle = self.sim2h_handle.clone();
                tokio::task::spawn(async move {
                    let req_agent_id = query_result.requester_agent_id.clone();
                    let msg_out = WireMessage::ClientToLib3hResponse(
                        ClientToLib3hResponse::QueryEntryResult(query_result),
                    );
                    let state = sim2h_handle.lock_state().await;
                    let to_url = match state
                        .lookup_joined(&space_address, &req_agent_id)
                    {
                        Some(to_url) => to_url,
                        None => {
                            error!("unvalidated proxy agent {}", &req_agent_id);
                            return;
                        }
                    };
                    state.send(
                        req_agent_id,
                        to_url,
                        &msg_out,
                    );
                });
                Ok(())
            }
            _ => {
                warn!("Ignoring unimplemented message: {:?}", message );
                Err(format!("Message not implemented: {:?}", message).into())
            }
        }
        })
    }

    fn handle_unseen_aspects(
        with_latency_publishing!(
            "sim2h-fetch_aspects_from_arbitrary_agent",
            self.metric_publisher,
            || {
                let agent_pool = &mut agent_pool[..];
                agent_pool.shuffle(&mut thread_rng());
                for entry_address in aspects_to_fetch.entry_addresses() {
                    if let Some(aspect_address_list) = aspects_to_fetch.per_entry(entry_address) {
                        if let Some(arbitrary_agent) = self.get_agent_not_missing_aspects(
                            entry_address,
                            aspect_address_list,
                            &for_agent_id,
                            agent_pool,
                            &space_address,
                        ) {
                            debug!(
                                "FETCHING missing contents from RANDOM AGENT: {}",
                                arbitrary_agent
                            );
                            let maybe_url = self.lookup_joined(&space_address, &arbitrary_agent);
                            if maybe_url.is_none() {
                                error!("Could not find URL for randomly selected agent. This should not happen!");
                                return;
                            }
                            let random_url = maybe_url.unwrap();
                            let wire_message = WireMessage::Lib3hToClient(
                                Lib3hToClient::HandleFetchEntry(FetchEntryData {
                                    request_id: for_agent_id.clone().into(),
                                    space_address: space_address.clone(),
                                    provider_agent_id: arbitrary_agent.clone(),
                                    entry_address: entry_address.clone(),
                                    aspect_address_list: Some(aspect_address_list.clone()),
                                }),
                            );
                            debug!("SENDING fetch with request ID: {:?}", wire_message);
                            self.send(arbitrary_agent.clone(), random_url.clone(), &wire_message);
                        } else {
                            warn!("Could not find an agent that has any of the missing aspects. Trying again later...")
                        }
                    }
                }
            }
        )
    }
        &self,
        uri: &Lib3hUri,
        space_address: &SpaceHash,
        agent_id: &AgentId,
        with_latency_publishing!(
            "sim2h-get_agent_not_missing_aspects",
            self.metric_publisher,
            || {
                let space_lock = self.spaces.get(space_address)?.read();
                agent_pool
                    .into_iter()
                    // We ignore all agents that are missing all of the same aspects as well since
                    // they can't help us.
                    .find(|a| {
                        **a != *for_agent_id
                            && !space_lock.agent_is_missing_all_aspects(*a, entry_hash, aspects)
                    })
                    .cloned()
            sim2h_handle.disconnect(disconnects);
        });
            }
        )
    }

    fn fetch_aspects_from_arbitrary_agent(
        sim2h_handle: Sim2hHandle,
        aspects_to_fetch: AspectList,
        for_agent_id: AgentId,
        agent_pool: Vec<AgentId>,
        space_address: SpaceHash,
    ) {
        with_latency_publishing!("sim2h-handle_new_entry_data", self.metric_publisher, || {
            // Calculate list of agents that should store new data:
            let dht_agents = match self.dht_algorithm {
                DhtAlgorithm::FullSync => {
                    self.all_agents_except_one(space_address.clone(), Some(&provider))
                }
                DhtAlgorithm::NaiveSharding { redundant_count } => {
                    let entry_loc = entry_location(&self.crypto, &entry_data.entry_address);
                    self.agents_in_neighbourhood(space_address.clone(), entry_loc, redundant_count)
                }
            };
            let aspect_addresses = entry_data
                .aspect_list
                .iter()
                .cloned()
                .map(|aspect_data| aspect_data.aspect_address)
                .collect::<Vec<_>>();
            let mut map = HashMap::new();
            map.insert(entry_data.entry_address.clone(), aspect_addresses);
            let aspect_list = AspectList::from(map);
            debug!("GOT NEW ASPECTS:\n{}", aspect_list.pretty_string());
            for aspect in entry_data.aspect_list {
                // 1. Add hashes to our global list of all aspects in this space:
                {
                    let mut space = self.get_or_create_space(&space_address).write();
                    space.add_aspect(
                        entry_data.entry_address.clone(),
                        aspect.aspect_address.clone(),
                    );
                    debug!(
                        "Space {} now knows about these aspects:\n{}",
                        &space_address,
                        space.all_aspects().pretty_string()
                    );
                }
        });
                // 2. Create store message
                let store_message = WireMessage::Lib3hToClient(
                    Lib3hToClient::HandleStoreEntryAspect(StoreEntryAspectData {
                        request_id: "".into(),
                        space_address: space_address.clone(),
                        provider_agent_id: provider.clone(),
                        entry_address: entry_data.entry_address.clone(),
                        entry_aspect: aspect,
                    }),
                );
                // 3. Send store message to selected nodes
                self.broadcast(&store_message, dht_agents.clone());
            }
        })
        with_latency_publishing!("sim2h-broadcast", self.metric_publisher, || {
            for (agent, info) in agents {
                debug!("Broadcast: Sending to {:?}", info.uri);
                self.send(agent, info.uri, msg);
            }
        })
        with_latency_publishing!("sim2h-all_agents_except_one", self.metric_publisher, || {
            self.get_or_create_space(&space)
                .read()
                .all_agents()
                .clone()
                .into_iter()
                .filter(|(a, _)| {
                    if let Some(exception) = except {
                        *a != *exception
                    } else {
                        true
                    }
                })
                .collect::<Vec<(AgentId, AgentInfo)>>()
        })
        with_latency_publishing!(
            "sim2h-agents_in_neighbourhood",
            self.metric_publisher,
            || {
                self.get_or_create_space(&space)
                    .read()
                    .agents_supposed_to_hold_entry(entry_loc, redundant_count)
                    .into_iter()
                    .collect::<Vec<(AgentId, AgentInfo)>>()
            }
        )
    }
        with_latency_publishing!("sim2h-send", self.metric_publisher, || {
            match msg {
                _ => {
                    debug!(">>OUT>> {} to {}", msg.message_type(), uri);
                    MESSAGE_LOGGER
                        .lock()
                        .log_out(agent, uri.clone(), msg.clone());
                }
            }
            let payload: Opaque = msg.clone().into();
            match self.open_connections.get_mut(&uri) {
                None => {
                    error!("FAILED TO SEND, NO ROUTE: {}", uri);
                    return;
                }
                Some((uuid, _con, outgoing_send)) => {
                    open_lifecycle("send", uuid, &uri);
                    if let Err(_) = outgoing_send.send(payload.as_bytes().into()) {
                        self.disconnect(&uri);
                    }
                }
            }
            match msg {
                WireMessage::Ping | WireMessage::Pong => {}
                _ => debug!("sent."),
            }
        })
        with_latency_publishing!(
            "sim2h-retry_sync_missing_aspects",
            self.metric_publisher,
            || {
                debug!("Checking for nodes with missing aspects to retry sync...");
                // Extract all needed info for the call to self.request_gossiping_list() below
                // as copies so we don't have to keep a reference to self.
                let spaces_with_agents_and_uris = self
                    .spaces
                    .iter()
                    .filter_map(|(space_hash, space_lock)| {
                        let space = space_lock.read();
                        let agents = space.agents_with_missing_aspects();
                        // If this space doesn't have any agents with missing aspects,
                        // ignore it:
                        if agents.is_empty() {
                            None
                        } else {
                            // For spaces with agents with missing aspects,
                            // annotate all agent IDs with their corresponding URI:
                            let agent_ids_with_uris: Vec<(AgentId, Lib3hUri)> = agents
                                .iter()
                                .filter_map(|agent_id| {
                                    space
                                        .agent_id_to_uri(agent_id)
                                        .map(|uri| (agent_id.clone(), uri))
                                })
                                .collect();
                            Some((space_hash.clone(), agent_ids_with_uris))
                        }
                    })
                    .collect::<HashMap<SpaceHash, Vec<_>>>();
                for (space_hash, agents) in spaces_with_agents_and_uris {
                    for (agent_id, uri) in agents {
                        debug!("Re-requesting gossip list from {} at {}", agent_id, uri);
                        self.request_gossiping_list(uri, space_hash.clone(), agent_id);
                    }
                }
            }
        )
    }
}
