//! IPC Abstraction for P2P networking
//!
//! This module allows holochain to connect to a running P2P client node
//! over WebSocket-based socket connection.

mod transport;
mod transport_wss;

pub use transport::{DidWork, Transport, TransportError, TransportEvent, TransportResult};

pub use transport_wss::{TransportWss, DEFAULT_HEARTBEAT_MS, DEFAULT_HEARTBEAT_WAIT_MS};

#[macro_use]
pub mod errors;
pub mod util;

pub mod spawn;
