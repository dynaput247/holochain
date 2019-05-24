use crate::{
    action::{ActionWrapper, GetLinksKey},
    network::{reducers::send, state::NetworkState},
    state::State,
};
use holochain_core_types::{error::HolochainError, hash::HashString};
use holochain_net::connection::json_protocol::{FetchMetaData, JsonProtocol};

fn reduce_get_links_inner(
    network_state: &mut NetworkState,
    key: &GetLinksKey,
) -> Result<(), HolochainError> {
    network_state.initialized()?;
    let link_type = key.link_type.clone().unwrap_or("*".to_string()); // map from None to "*"
    let tag = key.tag.clone().unwrap_or("*".to_string());
    send(
        network_state,
        JsonProtocol::FetchMeta(FetchMetaData {
            requester_agent_id: network_state.agent_id.clone().unwrap(),
            request_id: key.id.clone(),
            dna_address: network_state.dna_address.clone().unwrap(),
            entry_address: HashString::from(key.base_address.clone()),
            attribute: format!("link__{}__{}", link_type, tag),
        }),
    )
}

pub fn reduce_get_links(
    network_state: &mut NetworkState,
    _root_state: &State,
    action_wrapper: &ActionWrapper,
) {
    let action = action_wrapper.action();
    let key = unwrap_to!(action => crate::action::Action::GetLinks);

    let result = match reduce_get_links_inner(network_state, &key) {
        Ok(()) => None,
        Err(err) => Some(Err(err)),
    };

    network_state.get_links_results.insert(key.clone(), result);
}

pub fn reduce_get_links_timeout(
    network_state: &mut NetworkState,
    _root_state: &State,
    action_wrapper: &ActionWrapper,
) {
    let action = action_wrapper.action();
    let key = unwrap_to!(action => crate::action::Action::GetLinksTimeout);

    if network_state.get_links_results.get(key).is_none() {
        return;
    }

    if network_state.get_links_results.get(key).unwrap().is_none() {
        network_state
            .get_links_results
            .insert(key.clone(), Some(Err(HolochainError::Timeout)));
    }
}

#[cfg(test)]
mod tests {

    use crate::{
        action::{Action, ActionWrapper, GetLinksKey},
        instance::tests::test_context,
        state::test_store,
    };
    use holochain_core_types::error::HolochainError;
    //use std::sync::{Arc, RwLock};

    #[test]
    pub fn reduce_get_links_without_network_initialized() {
        let context = test_context("alice", None);
        let store = test_store(context.clone());

        let entry = test_entry();
        let link_type = String::from("test-link");
        let key = GetLinksKey {
            base_address: entry.address(),
            link_type: Some(link_type.clone()),
            tag: Some("link-tag".into()),
            id: snowflake::ProcessUniqueId::new().to_string(),
        };
        let action_wrapper = ActionWrapper::new(Action::GetLinks(key.clone()));

        let store = store.reduce(action_wrapper);
        let maybe_get_links_result = store
            .network()
            .get_links_results
            .get(&key)
            .map(|result| result.clone());
        assert_eq!(
            maybe_get_links_result,
            Some(Some(Err(HolochainError::ErrorGeneric(
                "Network not initialized".to_string()
            ))))
        );
    }

    use holochain_core_types::{cas::content::AddressableContent, entry::test_entry};

    #[test]
    // This test needs to be refactored.
    // It is non-deterministically failing with "sending on a closed channel" originating form
    // within the in-memory network.
    #[cfg(feature = "broken-tests")]
    pub fn reduce_get_links_test() {
        let netname = Some("reduce_get_links_test");
        let context = test_context("alice", netname);
        let store = test_store(context.clone());

        let action_wrapper = ActionWrapper::new(Action::InitNetwork(NetworkSettings {
            config: test_memory_network_config(netname),
            dna_address: "reduce_get_links_test".into(),
            agent_id: String::from("alice"),
        }));
        let store = store.reduce(action_wrapper);

        let entry = test_entry();
        let link_type = String::from("test-link");
        let key = GetLinksKey {
            base_address: entry.address(),
            link_type: link_type.clone(),
            id: snowflake::ProcessUniqueId::new().to_string(),
        };
        let action_wrapper = ActionWrapper::new(Action::GetLinks(key.clone()));

        let store = store.reduce(action_wrapper);
        let maybe_get_entry_result = store.network().get_links_results.get(&key).cloned();

        assert_eq!(maybe_get_entry_result, Some(None));
    }

    #[test]
    // This test needs to be refactored.
    // It is non-deterministically failing with "sending on a closed channel" originating form
    // within the in-memory network.
    #[cfg(feature = "broken-tests")]
    pub fn reduce_get_links_timeout_test() {
        let netname = Some("reduce_get_links_timeout_test");
        let mut context = test_context("alice", netname);
        let store = test_store(context.clone());
        let store = Arc::new(RwLock::new(store));

        Arc::get_mut(&mut context).unwrap().set_state(store.clone());

        let action_wrapper = ActionWrapper::new(Action::InitNetwork(NetworkSettings {
            config: test_memory_network_config(netname),
            dna_address: "reduce_get_links_timeout_test".into(),
            agent_id: String::from("alice"),
        }));

        {
            let mut new_store = store.write().unwrap();
            *new_store = new_store.reduce(context.clone(), action_wrapper);
        }

        let entry = test_entry();
        let link_type = String::from("test-link");
        let key = GetLinksKey {
            base_address: entry.address(),
            link_type: link_type.clone(),
            id: snowflake::ProcessUniqueId::new().to_string(),
        };
        let action_wrapper = ActionWrapper::new(Action::GetLinks(key.clone()));

        {
            let mut new_store = store.write().unwrap();
            *new_store = new_store.reduce(context.clone(), action_wrapper);
        }

        let maybe_get_entry_result = store
            .read()
            .unwrap()
            .network()
            .get_links_results
            .get(&key)
            .cloned();

        assert_eq!(maybe_get_entry_result, Some(None));

        let action_wrapper = ActionWrapper::new(Action::GetLinksTimeout(key.clone()));
        {
            let mut new_store = store.write().unwrap();
            *new_store = new_store.reduce(context.clone(), action_wrapper);
        }
        let maybe_get_entry_result = store
            .read()
            .unwrap()
            .network()
            .get_links_results
            .get(&key)
            .cloned();

        assert_eq!(
            maybe_get_entry_result,
            Some(Some(Err(HolochainError::Timeout)))
        );
    }
}
