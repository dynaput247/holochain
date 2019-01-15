//! provides fake in-memory p2p worker for use in scenario testing

use holochain_core_types::json::JsonString;
use holochain_net_connection::{
    net_connection::{NetHandler, NetWorker},
    protocol::Protocol,
    protocol_wrapper::ProtocolWrapper,
    NetResult,
};
use crate::mock_system::*;
use std::{
    convert::TryFrom,
    sync::{mpsc, Mutex},
};

/// a p2p worker for mocking in-memory scenario tests
pub struct MockWorker {
    handler: NetHandler,
    mock_msgs: Vec<mpsc::Receiver<Protocol>>,
    network_name: String,
}

impl NetWorker for MockWorker {
    /// we got a message from holochain core
    /// forward to our mock singleton
    fn receive(&mut self, data: Protocol) -> NetResult<()> {
        let map_lock = MOCK_MAP.read().unwrap();
        let mut mock = map_lock
            .get(&self.network_name)
            .expect("MockSystem should have been initialized by now")
            .lock()
            .unwrap();
        if let Ok(wrap) = ProtocolWrapper::try_from(&data) {
            if let ProtocolWrapper::TrackApp(app) = wrap {
                let (tx, rx) = mpsc::channel();
                self.mock_msgs.push(rx);
                mock.register(&app.dna_address, &app.agent_id, tx)?;
            }
        }
        mock.handle(data)?;
        Ok(())
    }

    /// check for messages from our mock singleton
    fn tick(&mut self) -> NetResult<bool> {
        let mut did_something = false;
        for msg in self.mock_msgs.iter_mut() {
            if let Ok(data) = msg.try_recv() {
                did_something = true;
                (self.handler)(Ok(data))?;
            }
        }
        Ok(did_something)
    }

    /// stop the net worker
    fn stop(self: Box<Self>) -> NetResult<()> {
        Ok(())
    }

    /// Set network's name as worker's endpoint
    fn endpoint(&self) -> Option<String> {
        Some(self.network_name.clone())
    }
}

impl MockWorker {
    /// create a new mock worker... no configuration required
    pub fn new(handler: NetHandler, network_config: &JsonString) -> NetResult<Self> {
        let config: serde_json::Value = serde_json::from_str(network_config.into())?;
        let network_name = config["networkName"]
            .as_str()
            .unwrap_or("(unnamed)")
            .to_string();

        let mut map_lock = MOCK_MAP.write().unwrap();
        if !map_lock.contains_key(&network_name) {
            map_lock.insert(network_name.clone(), Mutex::new(MockSystem::new()));
        }

        Ok(MockWorker {
            handler,
            mock_msgs: Vec::new(),
            network_name,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::p2p_config::P2pConfig;

    use holochain_core_types::cas::content::Address;
    use holochain_net_connection::protocol_wrapper::{
        SuccessResultData, TrackAppData,
        DhtData, DhtMetaData, FailureResultData, GetDhtData, GetDhtMetaData, MessageData,
        PeerData, ProtocolWrapper,
    };

    fn example_dna_address() -> Address {
        "blabladnaAddress".into()
    }

    static AGENT_ID_1: &'static str = "agent-hash-test-1";
    static AGENT_ID_2: &'static str = "agent-hash-test-2";

    #[test]
    #[cfg_attr(tarpaulin, skip)]
    fn it_mock_networker_flow() {
        // -- setup client 1 -- //

        let config = &JsonString::from(P2pConfig::unique_mock_as_string());

        let (handler_send_1, handler_recv_1) = mpsc::channel::<Protocol>();

        let mut cli1 = Box::new(
            MockWorker::new(
                Box::new(move |r| {
                    handler_send_1.send(r?)?;
                    Ok(())
                }),
                config,
            )
                .unwrap(),
        );

        cli1.receive(
            ProtocolWrapper::TrackApp(TrackAppData {
                dna_address: example_dna_address(),
                agent_id: AGENT_ID_1.to_string(),
            })
                .into(),
        )
            .unwrap();

        // -- setup client 2 -- //

        let (handler_send_2, handler_recv_2) = mpsc::channel::<Protocol>();

        let mut cli2 = Box::new(
            MockWorker::new(
                Box::new(move |r| {
                    handler_send_2.send(r?)?;
                    Ok(())
                }),
                config,
            )
                .unwrap(),
        );

        cli2.receive(
            ProtocolWrapper::TrackApp(TrackAppData {
                dna_address: example_dna_address(),
                agent_id: AGENT_ID_2.to_string(),
            })
                .into(),
        )
            .unwrap();

        // -- node 2 node / send / receive -- //

        cli1.receive(
            ProtocolWrapper::SendMessage(MessageData {
                dna_address: example_dna_address(),
                to_agent_id: AGENT_ID_2.to_string(),
                from_agent_id: AGENT_ID_1.to_string(),
                msg_id: "yada".to_string(),
                data: json!("hello"),
            })
                .into(),
        )
            .unwrap();

        cli2.tick().unwrap();

        let res = ProtocolWrapper::try_from(handler_recv_2.recv().unwrap()).unwrap();

        if let ProtocolWrapper::HandleSend(msg) = res {
            cli2.receive(
                ProtocolWrapper::HandleSendResult(MessageData {
                    dna_address: msg.dna_address,
                    to_agent_id: msg.from_agent_id,
                    from_agent_id: AGENT_ID_2.to_string(),
                    msg_id: msg.msg_id,
                    data: json!(format!("echo: {}", msg.data.to_string())),
                })
                    .into(),
            )
                .unwrap();
        } else {
            panic!("bad msg");
        }

        cli1.tick().unwrap();

        let res = ProtocolWrapper::try_from(handler_recv_1.recv().unwrap()).unwrap();

        if let ProtocolWrapper::SendResult(msg) = res {
            assert_eq!("\"echo: \\\"hello\\\"\"".to_string(), msg.data.to_string());
        } else {
            panic!("bad msg");
        }

        // -- dht get -- //

        cli2.receive(
            ProtocolWrapper::GetDht(GetDhtData {
                msg_id: "yada".to_string(),
                dna_address: example_dna_address(),
                from_agent_id: AGENT_ID_2.to_string(),
                address: "hello".to_string(),
            })
                .into(),
        )
            .unwrap();

        cli1.tick().unwrap();

        let res = ProtocolWrapper::try_from(handler_recv_1.recv().unwrap()).unwrap();

        if let ProtocolWrapper::GetDht(msg) = res {
            cli1.receive(
                ProtocolWrapper::GetDhtResult(DhtData {
                    msg_id: msg.msg_id.clone(),
                    dna_address: msg.dna_address.clone(),
                    agent_id: msg.from_agent_id.clone(),
                    address: msg.address.clone(),
                    content: json!(format!("data-for: {}", msg.address)),
                })
                    .into(),
            )
                .unwrap();
        } else {
            panic!("bad msg");
        }

        cli2.tick().unwrap();

        let res = ProtocolWrapper::try_from(handler_recv_2.recv().unwrap()).unwrap();

        if let ProtocolWrapper::GetDhtResult(msg) = res {
            assert_eq!("\"data-for: hello\"".to_string(), msg.content.to_string());
        } else {
            panic!("bad msg");
        }

        // -- dht publish / store -- //

        cli2.receive(
            ProtocolWrapper::PublishDht(DhtData {
                msg_id: "yada".to_string(),
                dna_address: example_dna_address(),
                agent_id: AGENT_ID_2.to_string(),
                address: "hello".to_string(),
                content: json!("test-data"),
            })
                .into(),
        )
            .unwrap();

        cli1.tick().unwrap();
        cli2.tick().unwrap();

        let res1 = ProtocolWrapper::try_from(handler_recv_1.recv().unwrap()).unwrap();
        let res2 = ProtocolWrapper::try_from(handler_recv_2.recv().unwrap()).unwrap();

        assert_eq!(res1, res2);

        if let ProtocolWrapper::StoreDht(msg) = res1 {
            cli1.receive(
                ProtocolWrapper::SuccessResult(SuccessResultData {
                    msg_id: msg.msg_id.clone(),
                    dna_address: msg.dna_address.clone(),
                    to_agent_id: msg.agent_id.clone(),
                    success_info: json!("signature here"),
                })
                    .into(),
            )
                .unwrap();
        } else {
            panic!("bad msg");
        }

        cli2.tick().unwrap();
        let res = ProtocolWrapper::try_from(handler_recv_2.recv().unwrap()).unwrap();

        if let ProtocolWrapper::SuccessResult(msg) = res {
            assert_eq!("\"signature here\"", &msg.success_info.to_string())
        } else {
            panic!("bad msg");
        }

        // -- dht meta get -- //

        cli2.receive(
            ProtocolWrapper::GetDhtMeta(GetDhtMetaData {
                msg_id: "yada".to_string(),
                dna_address: example_dna_address(),
                from_agent_id: AGENT_ID_2.to_string(),
                address: "hello".to_string(),
                attribute: "link:test".to_string(),
            })
                .into(),
        )
            .unwrap();

        cli1.tick().unwrap();

        let res = ProtocolWrapper::try_from(handler_recv_1.recv().unwrap()).unwrap();

        if let ProtocolWrapper::GetDhtMeta(msg) = res {
            cli1.receive(
                ProtocolWrapper::GetDhtMetaResult(DhtMetaData {
                    msg_id: msg.msg_id.clone(),
                    dna_address: msg.dna_address.clone(),
                    agent_id: msg.from_agent_id.clone(),
                    from_agent_id: AGENT_ID_1.to_string(),
                    address: msg.address.clone(),
                    attribute: msg.attribute.clone(),
                    content: json!(format!("meta-data-for: {}", msg.address)),
                })
                    .into(),
            )
                .unwrap();
        } else {
            panic!("bad msg");
        }

        cli2.tick().unwrap();

        let res = ProtocolWrapper::try_from(handler_recv_2.recv().unwrap()).unwrap();

        if let ProtocolWrapper::GetDhtMetaResult(msg) = res {
            assert_eq!(
                "\"meta-data-for: hello\"".to_string(),
                msg.content.to_string()
            );
        } else {
            panic!("bad msg");
        }

        // -- dht meta publish / store -- //

        cli2.receive(
            ProtocolWrapper::PublishDhtMeta(DhtMetaData {
                msg_id: "yada".to_string(),
                dna_address: example_dna_address(),
                agent_id: AGENT_ID_2.to_string(),
                from_agent_id: AGENT_ID_1.to_string(),
                address: "hello".to_string(),
                attribute: "link:test".to_string(),
                content: json!("test-data"),
            })
                .into(),
        )
            .unwrap();

        cli1.tick().unwrap();
        cli2.tick().unwrap();

        let res1 = ProtocolWrapper::try_from(handler_recv_1.recv().unwrap()).unwrap();
        let res2 = ProtocolWrapper::try_from(handler_recv_2.recv().unwrap()).unwrap();

        assert_eq!(res1, res2);

        if let ProtocolWrapper::StoreDhtMeta(msg) = res1 {
            cli1.receive(
                ProtocolWrapper::SuccessResult(SuccessResultData {
                    msg_id: msg.msg_id.clone(),
                    dna_address: msg.dna_address.clone(),
                    to_agent_id: msg.agent_id.clone(),
                    success_info: json!("signature here"),
                })
                    .into(),
            )
                .unwrap();
        } else {
            panic!("bad msg");
        }

        cli2.tick().unwrap();
        let res = ProtocolWrapper::try_from(handler_recv_2.recv().unwrap()).unwrap();

        if let ProtocolWrapper::SuccessResult(msg) = res {
            assert_eq!("\"signature here\"", &msg.success_info.to_string())
        } else {
            panic!("bad msg");
        }

        // -- cleanup -- //

        cli1.stop().unwrap();
        cli2.stop().unwrap();
    }
}
