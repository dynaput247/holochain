#![feature(try_from)]

extern crate holochain_core_types;
extern crate holochain_net;
extern crate holochain_net_connection;
#[macro_use]
extern crate serde_json;
extern crate failure;

pub mod p2p_node;

use holochain_net_connection::{
    net_connection::NetSend,
    protocol_wrapper::{MessageData, ProtocolMessage, TrackAppData},
    NetResult,
};
use p2p_node::P2pNode;

/// Macro for transforming a type check into a predicate
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
fn usage() {
    println!("Usage: test_bin_mock_net");
    std::process::exit(1);
}

// this is all debug code, no need to track code test coverage
#[cfg_attr(tarpaulin, skip)]
fn exec_mock_test() -> NetResult<()> {
    println!("Testing: exec_mock_test()");

    let mut node_a = P2pNode::new_mock();
    let mut node_b = P2pNode::new_with_config(&node_a.config, None);

    node_a
        .send(
            ProtocolMessage::TrackDna(TrackAppData {
                dna_address: "sandwich".into(),
                agent_id: "node-1".to_string(),
            })
            .into(),
        )
        .expect("Failed sending TrackAppData on node_a");
    node_b
        .send(
            ProtocolMessage::TrackDna(TrackAppData {
                dna_address: "sandwich".into(),
                agent_id: "node-2".to_string(),
            })
            .into(),
        )
        .expect("Failed sending TrackAppData on node_b");

    node_a
        .send(
            ProtocolMessage::SendMessage(MessageData {
                dna_address: "sandwich".into(),
                from_agent_id: "node-1".to_string(),
                to_agent_id: "node-2".to_string(),
                msg_id: "yada".to_string(),
                data: json!("hello"),
            })
            .into(),
        )
        .expect("Failed sending GenericMessage to node_b");
    let res = node_b.wait(Box::new(one_is!(ProtocolWrapper::HandleSend(_))))?;
    println!("got: {:?}", res);

    if let ProtocolMessage::HandleSendMessage(msg) = res {
        node_b
            .send(
                ProtocolMessage::HandleSendMessageResult(MessageData {
                    dna_address: "sandwich".into(),
                    from_agent_id: "node-2".to_string(),
                    to_agent_id: "node-1".to_string(),
                    msg_id: "yada".to_string(),
                    data: json!(format!("echo: {}", msg.data.to_string())),
                })
                .into(),
            )
            .expect("Failed sending HandleSendResult on node_b");;
    } else {
        panic!("bad generic msg");
    }

    let res = node_a.wait(Box::new(one_is!(ProtocolWrapper::SendResult(_))))?;
    println!("got response: {:?}", res);

    if let ProtocolMessage::SendMessageResult(msg) = res {
        assert_eq!("\"echo: \\\"hello\\\"\"".to_string(), msg.data.to_string());
    } else {
        panic!("bad msg");
    }

    // yay, everything worked
    println!("test complete");

    // shut down the nodes
    node_a.stop();
    node_b.stop();

    Ok(())
}

// this is all debug code, no need to track code test coverage
#[cfg_attr(tarpaulin, skip)]
fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() != 1 {
        usage();
    }
    let res = exec_mock_test();
    assert!(res.is_ok());

    // Wait a bit before closing
    for i in (0..4).rev() {
        println!("tick... {}", i);
        std::thread::sleep(std::time::Duration::from_millis(1000));
    }
}
