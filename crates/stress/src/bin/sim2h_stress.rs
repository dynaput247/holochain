//! `cargo run --bin sim2h_stress -- --help`

extern crate base64;
extern crate env_logger;
extern crate hcid;
extern crate holochain_stress;
extern crate lib3h_crypto_api;
extern crate lib3h_protocol;
extern crate lib3h_sodium;
#[macro_use]
extern crate log;
extern crate serde;
extern crate serde_json;
extern crate sim2h;
extern crate structopt;
extern crate url2;

use holochain_stress::*;
use lib3h_crypto_api::CryptoSystem;
use lib3h_protocol::{data_types::*, protocol::*, uri::Lib3hUri};
use lib3h_sodium::SodiumCryptoSystem;
use sim2h::{
    crypto::{Provenance, SignedWireMessage},
    websocket::{streams::*, tls::TlsConfig},
    Sim2h, WireMessage,
};
use std::sync::{Arc, Mutex};
use structopt::StructOpt;
use url2::prelude::*;

/// give us some cli command line options
#[derive(StructOpt, Debug, Clone)]
#[structopt(name = "sim2h_stress")]
struct Opt {
    #[structopt(short, long, default_value = "10")]
    /// how many threads to spin up in the job executor pool
    thread_count: usize,

    #[structopt(short, long, default_value = "100")]
    /// how many parallel jobs to execute
    job_count: usize,

    #[structopt(short, long, default_value = "10000")]
    /// total runtime for the test
    run_time_ms: u64,

    #[structopt(short, long, default_value = "5000")]
    /// how often to output in-progress statistics
    progress_interval_ms: u64,

    #[structopt(long, default_value = "0")]
    /// port on which to spin up the sim2h server
    sim2h_port: u16,

    #[structopt(long)]
    /// optional sim2h log file path
    sim2h_message_log_file: Option<std::path::PathBuf>,

    #[structopt(long, default_value = "100")]
    /// how often each job should send a ping to sim2h
    ping_freq_ms: u64,

    #[structopt(long, default_value = "100")]
    /// how often each job should publish a new entry
    publish_freq_ms: u64,
}

impl Opt {
    /// private convert our cli options into a stress job config
    fn create_stress_run_config<S: StressSuite, J: StressJob>(
        &self,
        suite: S,
        job_factory: JobFactory<J>,
    ) -> StressRunConfig<S, J> {
        StressRunConfig {
            thread_pool_size: self.thread_count,
            job_count: self.job_count,
            run_time_ms: self.run_time_ms,
            progress_interval_ms: self.progress_interval_ms,
            suite,
            job_factory,
        }
    }
}

/// private wait for a websocket connection to connect && return it
fn await_connection(connect_uri: &Lib3hUri) -> StreamManager<std::net::TcpStream> {
    let timeout = std::time::Instant::now()
        .checked_add(std::time::Duration::from_millis(1000))
        .unwrap();

    // keep trying to connect
    loop {
        // StreamManager is dual sided, but we're only using the client side
        // this tls config is for the not used server side, it can be fake
        let tls_config = TlsConfig::FakeServer;
        let mut stream_manager = StreamManager::with_std_tcp_stream(tls_config);

        // TODO - wtf, we don't want a listening socket : (
        //        but the logs are way too complainy
        stream_manager
            .bind(&Url2::parse("wss://127.0.0.1:0").into())
            .unwrap();

        // the actual connect request
        if let Err(e) = stream_manager.connect(connect_uri) {
            error!("e1 {:?}", e);

            if std::time::Instant::now() >= timeout {
                panic!("could not connect within timeout");
            }

            std::thread::sleep(std::time::Duration::from_millis(100));
            continue;
        }

        // now loop to see if we can communicate
        loop {
            let (_, evs) = match stream_manager.process() {
                Err(e) => {
                    error!("e2 {:?}", e);
                    break;
                }
                Ok(s) => s,
            };

            let mut did_err = false;
            for ev in evs {
                match ev {
                    StreamEvent::ConnectResult(_, _) => return stream_manager,
                    StreamEvent::ErrorOccured(_, e) => {
                        error!("e3 {:?}", e);
                        did_err = true;
                        break;
                    }
                    _ => (),
                }
            }

            if did_err {
                break;
            }
        }

        if std::time::Instant::now() >= timeout {
            panic!("could not connect within timeout");
        }

        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

thread_local! {
    pub static CRYPTO: Box<dyn CryptoSystem> = Box::new(SodiumCryptoSystem::new());
}

/// our job is a websocket connection to sim2h immitating a holochain-rust core
struct Job {
    agent_id: String,
    #[allow(dead_code)]
    pub_key: Arc<Mutex<Box<dyn lib3h_crypto_api::Buffer>>>,
    sec_key: Arc<Mutex<Box<dyn lib3h_crypto_api::Buffer>>>,
    remote_url: Url2,
    stream_manager: StreamManager<std::net::TcpStream>,
    ping_freq_ms: u64,
    next_ping: std::time::Instant,
    publish_freq_ms: u64,
    next_publish: std::time::Instant,
}

impl Job {
    /// create a new job - connected to sim2h
    pub fn new(connect_uri: &Lib3hUri, ping_freq_ms: u64, publish_freq_ms: u64) -> Self {
        let (pub_key, sec_key) = CRYPTO.with(|crypto| {
            let mut pub_key = crypto.buf_new_insecure(crypto.sign_public_key_bytes());
            let mut sec_key = crypto.buf_new_secure(crypto.sign_secret_key_bytes());
            crypto.sign_keypair(&mut pub_key, &mut sec_key).unwrap();
            (pub_key, sec_key)
        });
        let enc = hcid::HcidEncoding::with_kind("hcs0").unwrap();
        let agent_id = enc.encode(&*pub_key).unwrap();
        info!("GENERATED AGENTID {}", agent_id);
        let stream_manager = await_connection(connect_uri);
        let mut out = Self {
            agent_id,
            pub_key: Arc::new(Mutex::new(pub_key)),
            sec_key: Arc::new(Mutex::new(sec_key)),
            remote_url: Url2::parse(connect_uri.clone().to_string()),
            stream_manager,
            ping_freq_ms,
            next_ping: std::time::Instant::now(),
            publish_freq_ms,
            next_publish: std::time::Instant::now(),
        };

        out.join_space();

        out
    }

    /// sign a message and send it to sim2h
    pub fn send_wire(&mut self, message: WireMessage) {
        let payload: Opaque = message.into();
        let payload_buf: Box<dyn lib3h_crypto_api::Buffer> = Box::new(payload.clone().as_bytes());
        let sig = base64::encode(
            &*CRYPTO
                .with(|crypto| {
                    let mut sig = crypto.buf_new_insecure(crypto.sign_bytes());
                    crypto
                        .sign(&mut sig, &payload_buf, &*self.sec_key.lock().unwrap())
                        .unwrap();
                    sig
                })
                .read_lock(),
        );
        let signed_message = SignedWireMessage {
            provenance: Provenance::new(self.agent_id.clone().into(), sig.into()),
            payload,
        };
        let to_send: Opaque = signed_message.into();
        self.stream_manager
            .send(
                &self.remote_url.clone().into(),
                to_send.as_bytes().as_slice(),
            )
            .unwrap();
    }

    /// join the space "abcd" : )
    pub fn join_space(&mut self) {
        self.send_wire(WireMessage::ClientToLib3h(ClientToLib3h::JoinSpace(
            SpaceData {
                agent_id: self.agent_id.clone().into(),
                request_id: "".to_string(),
                space_address: "abcd".to_string().into(),
            },
        )));
    }

    /// send a ping message to sim2h
    pub fn ping(&mut self, logger: &mut StressJobMetricLogger) {
        self.send_wire(WireMessage::Ping);
        logger.log("send_ping_count", 1.0);
    }

    /// send a ping message to sim2h
    pub fn publish(&mut self, logger: &mut StressJobMetricLogger) {
        let (addr, aspect) = CRYPTO.with(|crypto| {
            let mut addr = crypto.buf_new_insecure(32);
            crypto.randombytes_buf(&mut addr).unwrap();
            let addr = base64::encode(&*addr.read_lock());

            let mut aspect_data = crypto.buf_new_insecure(32);
            crypto.randombytes_buf(&mut aspect_data).unwrap();

            let mut aspect_hash = crypto.buf_new_insecure(crypto.hash_sha256_bytes());
            crypto.hash_sha256(&mut aspect_hash, &aspect_data).unwrap();

            let enc = hcid::HcidEncoding::with_kind("hca0").unwrap();
            let aspect_hash = enc.encode(&*aspect_hash).unwrap();

            let aspect_data: Opaque = (*aspect_data.read_lock()).to_vec().into();

            let aspect = EntryAspectData {
                aspect_address: aspect_hash.into(),
                type_hint: "stress-test".to_string(),
                aspect: aspect_data,
                publish_ts: 0,
            };

            (addr, aspect)
        });

        self.send_wire(WireMessage::ClientToLib3h(ClientToLib3h::PublishEntry(
            ProvidedEntryData {
                space_address: "abcd".to_string().into(),
                provider_agent_id: self.agent_id.clone().into(),
                entry: EntryData {
                    entry_address: addr.into(),
                    aspect_list: vec![aspect],
                },
            },
        )));

        logger.log("send_publish_count", 1.0);
    }
}

impl StressJob for Job {
    /// check for any messages from sim2h and also send a ping
    fn tick(&mut self, logger: &mut StressJobMetricLogger) -> StressJobTickResult {
        let (_, evs) = self.stream_manager.process().unwrap();
        for ev in evs {
            match ev {
                StreamEvent::ErrorOccured(_, e) => panic!("{:?}", e),
                StreamEvent::ConnectResult(_, _) => panic!("got ConnectResult"),
                StreamEvent::IncomingConnectionEstablished(_) => unimplemented!(),
                StreamEvent::ReceivedData(_, data) => {
                    let data = String::from_utf8_lossy(&data).to_string();
                    if &data == "\"Pong\"" {
                        logger.log("received_pong_count", 1.0);
                    } else if data.contains("HandleGetAuthoringEntryList")
                        || data.contains("HandleGetGossipingEntryList")
                    {
                    } else if data.contains("HandleStoreEntryAspect") {
                        logger.log("received_handle_store_aspect", 1.0);
                    } else {
                        panic!(data);
                    }
                }
                StreamEvent::ConnectionClosed(_) => panic!("connection cloned"),
            }
        }

        let now = std::time::Instant::now();

        if now >= self.next_ping {
            self.next_ping = now
                .checked_add(std::time::Duration::from_millis(self.ping_freq_ms))
                .unwrap();

            self.ping(logger);
        }

        if now >= self.next_publish {
            self.next_publish = now
                .checked_add(std::time::Duration::from_millis(self.publish_freq_ms))
                .unwrap();
            self.publish(logger);
        }

        StressJobTickResult::default()
    }
}

/// our suite creates a thread for sim2h and gives the code processor time
struct Suite {
    sim2h_cont: Arc<Mutex<bool>>,
    sim2h_join: Option<std::thread::JoinHandle<()>>,
    bound_uri: Lib3hUri,
    snd_thread_logger: crossbeam_channel::Sender<StressJobMetricLogger>,
}

impl Suite {
    /// create a new sim2h server instance on given port
    #[allow(clippy::mutex_atomic)]
    pub fn new(port: u16) -> Self {
        let (snd1, rcv1) = crossbeam_channel::unbounded();
        let (snd2, rcv2) = crossbeam_channel::unbounded::<StressJobMetricLogger>();

        let sim2h_cont = Arc::new(Mutex::new(true));
        let sim2h_cont_clone = sim2h_cont.clone();
        let sim2h_join = Some(std::thread::spawn(move || {
            let tls_config = TlsConfig::build_from_entropy();
            let stream_manager = StreamManager::with_std_tcp_stream(tls_config);
            let url = Url2::parse(&format!("wss://127.0.0.1:{}", port));

            let mut sim2h = Sim2h::new(
                Box::new(SodiumCryptoSystem::new()),
                stream_manager,
                Lib3hUri(url.into()),
            );

            snd1.send(sim2h.bound_uri.clone().unwrap()).unwrap();
            drop(snd1);

            let mut logger = None;

            while *sim2h_cont_clone.lock().unwrap() {
                std::thread::sleep(std::time::Duration::from_millis(1));

                if let Ok(l) = rcv2.try_recv() {
                    logger.replace(l);
                }

                let start = std::time::Instant::now();

                if let Err(e) = sim2h.process() {
                    panic!("{:?}", e);
                }

                if let Some(logger) = &mut logger {
                    logger.log("sim2h_tick_elapsed_ms", start.elapsed().as_millis() as f64);
                }
            }
        }));

        let bound_uri = rcv1.recv().unwrap();
        println!("GOT BOUND: {:?}", bound_uri);

        info!("sim2h started, attempt test self connection");

        // wait 'till server is accepting connections.
        // let this one get dropped
        await_connection(&bound_uri);

        Self {
            sim2h_cont,
            sim2h_join,
            bound_uri,
            snd_thread_logger: snd2,
        }
    }
}

impl StressSuite for Suite {
    fn start(&mut self, logger: StressJobMetricLogger) {
        self.snd_thread_logger.send(logger).unwrap();
    }

    fn progress(&mut self, stats: &StressStats) {
        println!("PROGRESS: {:#?}", stats);
    }

    fn stop(&mut self, stats: StressStats) {
        *self.sim2h_cont.lock().unwrap() = false;
        self.sim2h_join.take().unwrap().join().unwrap();
        println!("FINAL STATS: {:#?}", stats);
    }
}

/// main function executes the stress suite given the cli arguments
pub fn main() {
    env_logger::init();
    let opt = Opt::from_args();
    if opt.sim2h_message_log_file.is_some() {
        unimplemented!();
    }
    let suite = Suite::new(opt.sim2h_port);
    let bound_uri = suite.bound_uri.clone();
    let config = opt.clone().create_stress_run_config(
        suite,
        Box::new(move |_| Job::new(&bound_uri, opt.ping_freq_ms, opt.publish_freq_ms)),
    );
    println!("RUNNING WITH CONFIG: {:#?}", config);
    stress_run(config);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_should_start_sim2h_and_connect() {
        env_logger::init();
        let suite = Suite::new(0);
        let mut job = Some(Job::new(&suite.bound_uri, 100, 100));
        std::thread::sleep(std::time::Duration::from_millis(500));
        stress_run(StressRunConfig {
            thread_pool_size: 1,
            job_count: 1,
            run_time_ms: 1000,
            progress_interval_ms: 2000,
            suite,
            job_factory: Box::new(move |_| std::mem::replace(&mut job, None).unwrap()),
        });
    }
}
