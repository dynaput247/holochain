#![feature(try_from)]

extern crate holochain_net;
extern crate holochain_net_connection;
#[macro_use]
extern crate serde_json;
extern crate tempfile;

use holochain_net_connection::{
    net_connection::NetConnection,
    protocol::Protocol,
    protocol_wrapper::{
        ConnectData, DhtData, DhtMetaData, GetDhtData, GetDhtMetaData, MessageData,
        P2pProtocol, TrackAppData,
    },
    NetResult,
};

use holochain_net::p2p_network::P2pNetworkNode;

use std::{convert::TryFrom, sync::mpsc};

// this is all debug code, no need to track code test coverage
#[cfg_attr(tarpaulin, skip)]
fn usage() {
    println!("Usage: test_bin_ipc <path_to_n3h>");
    std::process::exit(1);
}

struct IpcNode {
    pub temp_dir_ref: tempfile::TempDir,
    pub dir: String,
    pub p2p_node: P2pNetworkNode,
    pub receiver: mpsc::Receiver<Protocol>,
}

impl IpcNode {

    // See if there is a message to receive
    #[cfg_attr(tarpaulin, skip)]
    pub fn try_recv(&mut self) -> NetResult<P2pProtocol> {
        let data = self.receiver.try_recv()?;
        match P2pProtocol::try_from(&data) {
            Ok(r) => Ok(r),
            Err(e) => {
                let s = format!("{:?}", e);
                if !s.contains("Empty") && !s.contains("Pong(PongData") {
                    println!("##### parse error ##### : {} {:?}", s, data);
                }
                Err(e)
            }
        }
    }

    // Wait for a message corresponding to predicate
    #[cfg_attr(tarpaulin, skip)]
    pub fn wait(
        &mut self,
        predicate: Box<dyn Fn(&P2pProtocol) -> bool>,
    ) -> NetResult<P2pProtocol> {
        loop {
            let mut did_something = false;

            if let Ok(p2p_msg) = self.try_recv() {
                did_something = true;
                if predicate(&p2p_msg) {
                    return Ok(p2p_msg);
                }
            }

            if !did_something {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
    }

    // Stop node
    #[cfg_attr(tarpaulin, skip)]
    pub fn stop(self) {
        self.p2p_node.stop().unwrap();
    }
}

// Spawn an IPC node that uses n3h and a temp folder
#[cfg_attr(tarpaulin, skip)]
fn spawn_connection(n3h_path: &str) -> NetResult<IpcNode> {
    let dir_ref = tempfile::tempdir()?;
    let dir = dir_ref.path().to_string_lossy().to_string();

    let (sender, receiver) = mpsc::channel::<Protocol>();

    let p2p_node = P2pNetworkNode::new(
        Box::new(move |r| {
            sender.send(r?)?;
            Ok(())
        }),
        &json!({
            "backend": "ipc",
            "config": {
                "socketType": "zmq",
                "spawn": {
                    "cmd": "node",
                    "args": [
                        format!("{}/packages/n3h/bin/n3h", n3h_path)
                    ],
                    "workDir": dir.clone(),
                    "env": {
                        "N3H_HACK_MODE": "1",
                        "N3H_WORK_DIR": dir.clone(),
                        "N3H_IPC_SOCKET": "tcp://127.0.0.1:*",
                    }
                },
            }
        })
        .into(),
    )?;

    Ok(IpcNode {
        temp_dir_ref: dir_ref,
        dir,
        p2p_node,
        receiver,
    })
}

macro_rules! one_let {
    ($p:pat = $enum:ident $code:tt) => {
        if let $p = $enum {
            $code
        } else {
            unimplemented!();
        }
    };
}

macro_rules! one_is {
    ($p:pat) => {
        |d| {
            if let $p = d {
                return true;
            }
            return false;
        }
    };
}

// this is all debug code, no need to track code test coverage
#[cfg_attr(tarpaulin, skip)]
fn exec() -> NetResult<()> {
    static DNA_HASH: &'static str = "TEST_DNA_HASH";
    static AGENT_1: &'static str = "1_TEST_AGENT_1";
    static AGENT_2: &'static str = "2_TEST_AGENT_2";

    // Check args
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        usage();
    }
    let n3h_path = args[1].clone();
    if n3h_path == "" {
        usage();
    }

    // Create two nodes
    let mut node1 = spawn_connection(&n3h_path)?;
    let mut node2 = spawn_connection(&n3h_path)?;
    println!("node1 path: {}", node1.dir);
    println!("node2 path: {}", node2.dir);

    // Get each node's current state
    let node1_state = node1.wait(Box::new(one_is!(P2pProtocol::State(_))))?;
    let node2_state = node2.wait(Box::new(one_is!(P2pProtocol::State(_))))?;

    // get node IDs from their state
    let node1_id;
    let node2_id;
    let node2_binding;
    one_let!(P2pProtocol::State(s) = node1_state {
        node1_id = s.id
    });
    one_let!(P2pProtocol::State(s) = node2_state {
        node2_id = s.id;
        node2_binding = s.bindings[0].clone();
    });

    // Send TrackApp message on both nodes
    node1.p2p_node.send(
        P2pProtocol::TrackApp(TrackAppData {
            dna_hash: DNA_HASH.to_string(),
            agent_id: AGENT_1.to_string(),
        })
        .into(),
    )?;
    let connect_result_1 = node1.wait(Box::new(one_is!(P2pProtocol::PeerConnected(_))))?;
    println!("self connected result 1: {:?}", connect_result_1);
    node2.p2p_node.send(
        P2pProtocol::TrackApp(TrackAppData {
            dna_hash: DNA_HASH.to_string(),
            agent_id: AGENT_2.to_string(),
        })
        .into(),
    )?;
    let connect_result_2 = node2.wait(Box::new(one_is!(P2pProtocol::PeerConnected(_))))?;
    println!("self connected result 2: {:?}", connect_result_2);

    // Connect nodes between them
    println!("connect node1 ({}) to node2 ({})", node1_id, node2_binding);
    node1.p2p_node.send(
        P2pProtocol::Connect(ConnectData {
            address: node2_binding,
        })
        .into(),
    )?;
    let result_1 = node1.wait(Box::new(one_is!(P2pProtocol::PeerConnected(_))))?;
    println!("got connect result 1: {:?}", result_1);
    one_let!(P2pProtocol::PeerConnected(d) = result_1 {
        assert_eq!(d.agent_id, AGENT_2);
    });
    let result_2 = node2.wait(Box::new(one_is!(P2pProtocol::PeerConnected(_))))?;
    println!("got connect result 2: {:?}", result_2);
    one_let!(P2pProtocol::PeerConnected(d) = result_2 {
        assert_eq!(d.agent_id, AGENT_1);
    });

    // Send a generic message
    node1.p2p_node.send(
        P2pProtocol::SendMessage(MessageData {
            msg_id: "test".to_string(),
            dna_hash: DNA_HASH.to_string(),
            to_agent_id: AGENT_2.to_string(),
            from_agent_id: AGENT_1.to_string(),
            data: json!("hello"),
        })
        .into(),
    )?;
    // Check if node2 received it
    let result_2 = node2.wait(Box::new(one_is!(P2pProtocol::HandleSend(_))))?;
    println!("got handle send 2: {:?}", result_2);
    node2.p2p_node.send(
        P2pProtocol::HandleSendResult(MessageData {
            msg_id: "test".to_string(),
            dna_hash: DNA_HASH.to_string(),
            to_agent_id: AGENT_1.to_string(),
            from_agent_id: AGENT_2.to_string(),
            data: json!("echo: hello"),
        })
        .into(),
    )?;
    let result_1 = node1.wait(Box::new(one_is!(P2pProtocol::SendResult(_))))?;
    println!("got send result 1: {:?}", result_1);

    // Send store DHT data
    node1.p2p_node.send(
        P2pProtocol::PublishDht(DhtData {
            msg_id: "testPub".to_string(),
            dna_hash: DNA_HASH.to_string(),
            agent_id: AGENT_1.to_string(),
            address: "test_addr".to_string(),
            content: json!("hello"),
        })
        .into(),
    )?;
    // Check if both nodes received it
    let result_1 = node1.wait(Box::new(one_is!(P2pProtocol::StoreDht(_))))?;
    println!("got store result 1: {:?}", result_1);
    let result_2 = node2.wait(Box::new(one_is!(P2pProtocol::StoreDht(_))))?;
    println!("got store result 2: {:?}", result_2);

    // Send get DHT data
    node2.p2p_node.send(
        P2pProtocol::GetDht(GetDhtData {
            msg_id: "testGet".to_string(),
            dna_hash: DNA_HASH.to_string(),
            from_agent_id: AGENT_2.to_string(),
            address: "test_addr".to_string(),
        })
        .into(),
    )?;
    let result_2 = node2.wait(Box::new(one_is!(P2pProtocol::GetDht(_))))?;
    println!("got dht get: {:?}", result_2);

    // Send get DHT data result
    node2.p2p_node.send(
        P2pProtocol::GetDhtResult(DhtData {
            msg_id: "testGetResult".to_string(),
            dna_hash: DNA_HASH.to_string(),
            agent_id: AGENT_1.to_string(),
            address: "test_addr".to_string(),
            content: json!("hello"),
        })
        .into(),
    )?;
    let result_2 = node2.wait(Box::new(one_is!(P2pProtocol::GetDhtResult(_))))?;
    println!("got dht get result: {:?}", result_2);

    // Send store DHT metadata
    node1.p2p_node.send(
        P2pProtocol::PublishDhtMeta(DhtMetaData {
            msg_id: "testPubMeta".to_string(),
            dna_hash: DNA_HASH.to_string(),
            agent_id: AGENT_1.to_string(),
            address: "test_addr_meta".to_string(),
            attribute: "link:yay".to_string(),
            content: json!("hello-meta"),
        })
        .into(),
    )?;
    // Check if both nodes received it
    let result_1 = node1.wait(Box::new(one_is!(P2pProtocol::StoreDhtMeta(_))))?;
    println!("got store meta result 1: {:?}", result_1);
    let result_2 = node2.wait(Box::new(one_is!(P2pProtocol::StoreDhtMeta(_))))?;
    println!("got store meta result 2: {:?}", result_2);

    // Send get DHT metadata
    node2.p2p_node.send(
        P2pProtocol::GetDhtMeta(GetDhtMetaData {
            msg_id: "testGetMeta".to_string(),
            dna_hash: DNA_HASH.to_string(),
            from_agent_id: AGENT_2.to_string(),
            address: "test_addr".to_string(),
            attribute: "link:yay".to_string(),
        })
        .into(),
    )?;
    let result_2 = node2.wait(Box::new(one_is!(P2pProtocol::GetDhtMeta(_))))?;
    println!("got dht get: {:?}", result_2);

    // Send get DHT metadata result
    node2.p2p_node.send(
        P2pProtocol::GetDhtMetaResult(DhtMetaData {
            msg_id: "testGetMetaResult".to_string(),
            dna_hash: DNA_HASH.to_string(),
            agent_id: AGENT_1.to_string(),
            address: "test_addr".to_string(),
            attribute: "link:yay".to_string(),
            content: json!("hello"),
        })
        .into(),
    )?;
    let result_2 = node2.wait(Box::new(one_is!(P2pProtocol::GetDhtMetaResult(_))))?;
    println!("got dht get result: {:?}", result_2);

    // Wait a bit before closing
    for i in (0..4).rev() {
        println!("tick... {}", i);
        std::thread::sleep(std::time::Duration::from_millis(1000));
    }

    // Kill nodes
    node1.stop();
    node2.stop();

    // Done
    Ok(())
}

// this is all debug code, no need to track code test coverage
#[cfg_attr(tarpaulin, skip)]
fn main() {
    exec().unwrap();
}
