pub mod get;
pub mod send;
pub mod store;

use crate::{
    context::Context,
    network::handler::{get::*, send::*, store::*},
};
use holochain_net_connection::{net_connection::NetHandler, protocol_wrapper::ProtocolWrapper};
use std::{convert::TryFrom, sync::Arc};

pub fn create_handler(c: &Arc<Context>) -> NetHandler {
    let context = c.clone();
    Box::new(move |message| {
        let message = message.unwrap();
        let protocol_wrapper = ProtocolWrapper::try_from(message);
        match protocol_wrapper {
            Ok(ProtocolWrapper::StoreDht(dht_data)) => handle_store_dht(dht_data, context.clone()),
            Ok(ProtocolWrapper::StoreDhtMeta(dht_meta_data)) => {
                handle_store_dht_meta(dht_meta_data, context.clone())
            }
            Ok(ProtocolWrapper::GetDht(get_dht_data)) => {
                handle_get_dht(get_dht_data, context.clone())
            }
            Ok(ProtocolWrapper::GetDhtResult(dht_data)) => {
                handle_get_dht_result(dht_data, context.clone())
            }
            Ok(ProtocolWrapper::HandleSend(message_data)) => {
                handle_send(message_data, context.clone())
            }
            Ok(ProtocolWrapper::HandleSendResult(message_data)) => {
                handle_send_result(message_data, context.clone())
            }
            _ => {}
        }
        Ok(())
    })
}
