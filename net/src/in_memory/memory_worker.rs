//! provides fake in-memory p2p worker for use in scenario testing

use super::memory_server::*;
use crate::connection::{
    net_connection::{NetHandler, NetWorker},
    protocol::Protocol,
    NetResult,
};
use lib3h_protocol::protocol_client::Lib3hClientProtocol;

use holochain_json_api::json::JsonString;
use holochain_persistence_api::{cas::content::Address, hash::HashString};
use std::{
    collections::{hash_map::Entry, HashMap},
    convert::{TryFrom, TryInto},
    sync::{mpsc, Mutex},
};

/// a p2p worker for mocking in-memory scenario tests
#[allow(non_snake_case)]
pub struct InMemoryWorker {
    handler: NetHandler,
    receiver_per_dna: HashMap<Address, mpsc::Receiver<Protocol>>,
    server_name: String,
    can_send_P2pReady: bool,
}

impl NetWorker for InMemoryWorker {
    /// we got a message from holochain core
    /// forward to our in-memory server
    fn receive(&mut self, data: Protocol) -> NetResult<()> {
        // InMemoryWorker doesn't have to do anything on shutdown
        if data == Protocol::Shutdown {
            self.handler.handle(Ok(Protocol::Terminated))?;
            return Ok(());
        }
        let server_map = MEMORY_SERVER_MAP.read().unwrap();
        let mut server = server_map
            .get(&self.server_name)
            .expect("InMemoryServer should have been initialized by now")
            .lock()
            .unwrap();
        if let Ok(json_msg) = Lib3hClientProtocol::try_from(&data) {
            match json_msg {
                Lib3hClientProtocol::JoinSpace(track_msg) => {
                    let dna_address: HashString = track_msg.space_address.try_into().unwrap();
                    match self.receiver_per_dna.entry(dna_address.clone()) {
                        Entry::Occupied(_) => (),
                        Entry::Vacant(e) => {
                            let (tx, rx) = mpsc::channel();
                            server.register_chain(
                                &dna_address,
                                &track_msg.agent_id.try_into().unwrap(),
                                tx,
                            )?;
                            e.insert(rx);
                        }
                    };
                }
                _ => (),
            }
        }
        // Serve
        server.serve(data.clone())?;
        // After serve
        if let Ok(json_msg) = Lib3hClientProtocol::try_from(&data) {
            match json_msg {
                Lib3hClientProtocol::LeaveSpace(untrack_msg) => {
                    let dna_address: HashString = untrack_msg.space_address.try_into().unwrap();
                    match self.receiver_per_dna.entry(dna_address.clone()) {
                        Entry::Vacant(_) => (),
                        Entry::Occupied(e) => {
                            server.unregister_chain(
                                &dna_address,
                                &untrack_msg.agent_id.try_into().unwrap(),
                            );
                            e.remove();
                        }
                    };
                }
                _ => (),
            }
        }
        // Done
        Ok(())
    }

    /// check for messages from our InMemoryServer
    fn tick(&mut self) -> NetResult<bool> {
        // Send p2pready on first tick
        if self.can_send_P2pReady {
            self.can_send_P2pReady = false;
            self.handler.handle(Ok(Protocol::P2pReady))?;
        }
        // check for messages from our InMemoryServer
        let mut did_something = false;
        for (_, receiver) in self.receiver_per_dna.iter_mut() {
            if let Ok(data) = receiver.try_recv() {
                did_something = true;
                self.handler.handle(Ok(data))?;
            }
        }
        Ok(did_something)
    }

    /// stop the net worker
    fn stop(self: Box<Self>) -> NetResult<()> {
        Ok(())
    }

    /// Set server's name as worker's endpoint
    fn endpoint(&self) -> Option<String> {
        Some(self.server_name.clone())
    }
}

impl InMemoryWorker {
    /// create a new memory worker connected to an in-memory server
    pub fn new(handler: NetHandler, backend_config: &JsonString) -> NetResult<Self> {
        // Get server name from config
        let config: serde_json::Value = serde_json::from_str(backend_config.into())?;
        // println!("InMemoryWorker::new() config = {:?}", config);
        let server_name = config["serverName"]
            .as_str()
            .unwrap_or("(unnamed)")
            .to_string();
        // Create server with that name if it doesn't already exist
        let mut server_map = MEMORY_SERVER_MAP.write().unwrap();
        if !server_map.contains_key(&server_name) {
            server_map.insert(
                server_name.clone(),
                Mutex::new(InMemoryServer::new(server_name.clone())),
            );
        }
        let mut server = server_map
            .get(&server_name)
            .expect("InMemoryServer should exist")
            .lock()
            .unwrap();
        server.clock_in();

        Ok(InMemoryWorker {
            handler,
            receiver_per_dna: HashMap::new(),
            server_name,
            can_send_P2pReady: true,
        })
    }
}

// unregister on Drop
impl Drop for InMemoryWorker {
    fn drop(&mut self) {
        let server_map = MEMORY_SERVER_MAP.read().unwrap();
        let mut server = server_map
            .get(&self.server_name)
            .expect("InMemoryServer should exist")
            .lock()
            .unwrap();
        server.clock_out();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        connection::json_protocol::{Lib3hClientProtocol, SpaceData},
        p2p_config::P2pConfig,
    };
    use crossbeam_channel::unbounded;
    use holochain_persistence_api::{cas::content::Address, hash::HashString};

    fn example_dna_address() -> Address {
        "blabladnaAddress".into()
    }

    static AGENT_ID_1: &'static str = "agent-hash-test-1";

    #[test]
    #[cfg_attr(tarpaulin, skip)]
    fn can_memory_worker_double_track() {
        // setup client 1
        let memory_config = &JsonString::from(P2pConfig::unique_memory_backend_json());
        let (handler_send_1, handler_recv_1) = unbounded::<Protocol>();

        let mut memory_worker_1 = Box::new(
            InMemoryWorker::new(
                NetHandler::new(Box::new(move |r| {
                    handler_send_1.send(r?)?;
                    Ok(())
                })),
                memory_config,
            )
            .unwrap(),
        );

        // Should receive p2pready on first tick
        memory_worker_1.tick().unwrap();
        let message = handler_recv_1.recv().unwrap();
        assert_eq!(message, Protocol::P2pReady);

        // First Track
        memory_worker_1
            .receive(
                Lib3hClientProtocol::JoinSpace(SpaceData {
                    dna_address: example_dna_address(),
                    agent_id: HashString::from(AGENT_ID_1),
                })
                .into(),
            )
            .unwrap();

        // Should receive PeerConnected
        memory_worker_1.tick().unwrap();
        let _res = Lib3hClientProtocol::try_from(handler_recv_1.recv().unwrap()).unwrap();

        // Second Track
        memory_worker_1
            .receive(
                Lib3hClientProtocol::JoinSpace(SpaceData {
                    dna_address: example_dna_address(),
                    agent_id: HashString::from(AGENT_ID_1),
                })
                .into(),
            )
            .unwrap();

        memory_worker_1.tick().unwrap();
    }
}
