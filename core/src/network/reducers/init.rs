use crate::{
    action::{Action, ActionWrapper},
    context::Context,
    network::{handler::create_handler, state::NetworkState},
};
use holochain_net::p2p_network::P2pNetwork;
use holochain_net::p2p_config::P2pConfig;
use holochain_net_connection::{
    net_connection::NetConnection,
    protocol_wrapper::{ProtocolWrapper, TrackAppData},
};
use std::sync::{Arc, Mutex};
use std::str::FromStr;

pub fn reduce_init(
    context: Arc<Context>,
    state: &mut NetworkState,
    action_wrapper: &ActionWrapper,
) {
    let action = action_wrapper.action();
    let network_settings = unwrap_to!(action => Action::InitNetwork);
    let p2p_config = P2pConfig::from_str(&network_settings.config.to_string())
        .expect("network settings failed to deserialize");
    let mut network = P2pNetwork::new(create_handler(&context), &p2p_config).unwrap();

    let _ = network
        .send(
            ProtocolWrapper::TrackApp(TrackAppData {
                dna_hash: network_settings.dna_hash.clone(),
                agent_id: network_settings.agent_id.clone(),
            })
            .into(),
        )
        .and_then(|_| {
            state.network = Some(Arc::new(Mutex::new(network)));
            state.dna_hash = Some(network_settings.dna_hash.clone());
            state.agent_id = Some(network_settings.agent_id.clone());
            Ok(())
        });
}
