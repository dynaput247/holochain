//! This module provides the main abstraction for differing p2p backends
//! P2pNetwork instances take a json configuration string
//! and at load-time instantiate the configured "backend"

use crate::{
    connection::{
        net_connection::{NetHandler, NetSend, NetWorker, NetWorkerFactory},
        net_connection_thread::NetConnectionThread,
        protocol::Protocol,
        NetResult,
    },
    in_memory::memory_worker::InMemoryWorker,
    ipc_net_worker::IpcNetWorker,
    p2p_config::*,
};
use holochain_core_types::json::JsonString;
use std::{
    convert::TryFrom,
    sync::mpsc::{channel, Receiver},
    time::Duration,
};

const P2P_READY_TIMEOUT_MS: u64 = 10000;

/// Facade handling a p2p module responsable for the network connection
/// Holds a NetConnectionThread and implements itself the NetSend Trait
/// `send()` is used for sending Protocol messages to the network
/// `handler` closure provide on construction for handling Protocol messages received from the network.
pub struct P2pNetwork {
    connection: NetConnectionThread,
}

impl P2pNetwork {
    /// Constructor
    /// `config` is the configuration of the p2p module
    /// `handler` is the closure for handling Protocol messages received from the network.
    pub fn new(mut handler: NetHandler, p2p_config: &P2pConfig) -> NetResult<Self> {
        // Create Config struct
        let backend_config = JsonString::from_json(&p2p_config.backend_config.to_string());

        // Provide worker factory depending on backend kind
        let worker_factory: NetWorkerFactory = match p2p_config.backend_kind {
            // Create an IpcNetWorker with the passed backend config
            P2pBackendKind::IPC => {
                let enduser_config = p2p_config
                    .maybe_end_user_config
                    .clone()
                    .expect("P2pConfig for IPC networking is missing an end-user config")
                    .to_string();
                Box::new(move |h| {
                    Ok(
                        Box::new(IpcNetWorker::new(h, &backend_config, enduser_config)?)
                            as Box<NetWorker>,
                    )
                })
            }
            // Create an InMemoryWorker
            P2pBackendKind::MEMORY => Box::new(move |h| {
                Ok(Box::new(InMemoryWorker::new(h, &backend_config)?) as Box<NetWorker>)
            }),
        };

        let (t, rx) = channel();
        let tx = t.clone();
        let wrapped_handler: NetHandler = Box::new(move |message| {
            let unwrapped = message.unwrap();
            let cloned = unwrapped.clone();
            match Protocol::try_from(unwrapped.clone()) {
                Ok(Protocol::P2pReady) => {
                    //context.log(format!("debug/net/handle: handle P2pReady start"));
                    tx.send(Protocol::P2pReady).unwrap();
                    ()
                    //context.log(format!("debug/net/handle: handle P2pReady end"));
                }
                Ok(_protocol_message) => {
                    //context.log(format!(
                    //  "debug/net/handle: ignoring protocol message {:?}",
                    //   protocol_message
                    // ));
                }
                Err(_protocol_error) => {
                    // TODO why can't I use the above variable?
                    // Generates compiler error.
                }
            };
            handler(Ok(cloned))
        });

        // Create NetConnectionThread with appropriate worker factory
        let connection = NetConnectionThread::new(wrapped_handler, worker_factory, None)?;

        P2pNetwork::wait_p2p_ready(&rx);
        // Done
        Ok(P2pNetwork { connection })
    }

    fn wait_p2p_ready(rx: &Receiver<Protocol>) {
        let maybe_message = rx.recv_timeout(Duration::from_millis(P2P_READY_TIMEOUT_MS));
        match maybe_message {
            Ok(Protocol::P2pReady) => {
                println!("wait_p2p_ready: READY!");
                ()
            }
            //context.log("debug/net/handle: p2p networking ready"),
            Ok(_protocol_message) => {
                println!("wait_p2p_ready: unsupported protocol message.");
                ()
                //            context.log(format!(
                //                "warn/net/handle: unexpected \
                //                 protocol message {:?}",
                //                protocol_message
                //            ));
            }
            Err(_e) => {
                println!("wait_p2p_ready: PANIC!");
                //            context.log(format!(
                //                "err/net/handle: timed out waiting for p2p \
                //                network to be ready: {:?}",
                //              e
                //          ));
                panic!(
                    "p2p network not ready within alloted time of {:?} ms",
                    P2P_READY_TIMEOUT_MS
                );
            }
        };
    }

    /// Stop the network connection (disconnect any sockets, join any threads, etc)
    pub fn stop(self) -> NetResult<()> {
        self.connection.stop()
    }

    /// Getter of the endpoint of its connection
    pub fn endpoint(&self) -> String {
        self.connection.endpoint.clone()
    }
}

impl std::fmt::Debug for P2pNetwork {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "P2pNetwork {{}}")
    }
}

impl NetSend for P2pNetwork {
    /// send a Protocol message to the p2p network instance
    fn send(&mut self, data: Protocol) -> NetResult<()> {
        self.connection.send(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_should_create_memory_network() {
        let mut res = P2pNetwork::new(
            Box::new(|_r| Ok(())),
            &P2pConfig::new_with_unique_memory_backend(),
        )
        .unwrap();
        res.send(Protocol::P2pReady).unwrap();
        res.stop().unwrap();
    }
}
