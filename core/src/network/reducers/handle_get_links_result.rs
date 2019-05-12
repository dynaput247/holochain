use crate::{
    action::{ActionWrapper, GetLinksKey},
    network::state::NetworkState,
    state::State,
};
use holochain_core_types::{cas::content::Address, error::HolochainError};
use holochain_net::connection::json_protocol::FetchMetaResultData;

fn reduce_handle_get_links_result_inner(
    network_state: &mut NetworkState,
    dht_meta_data: &FetchMetaResultData,
) -> Result<Vec<Address>, HolochainError> {
    network_state.initialized()?;
    // expecting dht_meta_data.content_list to be a jsonified array of EntryWithHeader or Address
    // TODO: do a loop on content once links properly implemented
    assert_eq!(dht_meta_data.content_list.len(), 1);
    serde_json::from_str(
        &serde_json::to_string(&dht_meta_data.content_list[0])
            .expect("Failed to deserialize dht_meta_data"),
    )
    .map_err(|_| {
        HolochainError::ErrorGeneric(
            "Failed to deserialize Vec<Address> from HandleGetLinkResult DhtMetaData content"
                .to_string(),
        )
    })
}

pub fn reduce_handle_get_links_result(
    network_state: &mut NetworkState,
    _root_state: &State,
    action_wrapper: &ActionWrapper,
) {
    let action = action_wrapper.action();
    let (dht_meta_data, tag) = unwrap_to!(action => crate::action::Action::HandleGetLinksResult);
    let result = reduce_handle_get_links_result_inner(network_state, dht_meta_data);
    let key = GetLinksKey {
        base_address: Address::from(dht_meta_data.entry_address.clone()),
        tag: tag.clone(),
        id: dht_meta_data.request_id.clone(),
    };

    network_state.get_links_results.insert(key, Some(result));
}
