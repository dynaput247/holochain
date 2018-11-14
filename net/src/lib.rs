#![feature(try_from)]

//! holochain_net is a library that defines an abstract networking layer for
//! different network transports, providing a configurable interface
//! for swapping different backends connection modules at load time

extern crate base64;
#[macro_use]
extern crate failure;
extern crate holochain_core_types;
extern crate holochain_net_connection;
extern crate holochain_net_ipc;
#[macro_use]
extern crate serde_json;

pub mod error;
pub mod ipc_net_worker;
pub mod p2p_network;
