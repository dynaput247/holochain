//! all DHT reducers

use action::{Action, ActionWrapper};
use context::Context;
use dht::dht_store::DhtStore;
use holochain_core_types::{
    cas::content::AddressableContent, eav::EntityAttributeValue,
    entry::Entry,
    error::HolochainError,
    crud_status::{CrudStatus, create_crud_status_eav, STATUS_NAME},
    entry::SerializedEntry,
};
use std::sync::Arc;
use std::{
    convert::{TryFrom},
};
use std::collections::HashSet;

// A function that might return a mutated DhtStore
type DhtReducer = fn(Arc<Context>, &DhtStore, &ActionWrapper) -> Option<DhtStore>;

/// DHT state-slice Reduce entry point.
/// Note: Can't block when dispatching action here because we are inside the reduce's mutex
pub fn reduce(
    context: Arc<Context>,
    old_store: Arc<DhtStore>,
    action_wrapper: &ActionWrapper,
) -> Arc<DhtStore> {
    // Get reducer
    let maybe_reducer = resolve_reducer(action_wrapper);
    if maybe_reducer.is_none() {
        return old_store;
    }
    let reducer = maybe_reducer.unwrap();
    // Reduce
    let store = old_store.clone();
    let maybe_new_store = reducer(context, &store, &action_wrapper);
    match maybe_new_store {
        None => old_store,
        Some(new_store) => Arc::new(new_store),
    }
}

/// Maps incoming action to the correct reducer
fn resolve_reducer(action_wrapper: &ActionWrapper) -> Option<DhtReducer> {
    match action_wrapper.action() {
        Action::Commit(_) => Some(reduce_commit_entry),
        Action::GetEntry(_) => Some(reduce_get_entry_from_network),
        Action::UpdateEntry(_) => Some(reduce_update_entry),
        Action::RemoveEntry(_) => Some(reduce_remove_entry),
        Action::AddLink(_) => Some(reduce_add_link),
        //Action::GetLinks(_) => Some(reduce_get_links),
        _ => None,
    }
}

//
pub(crate) fn commit_sys_entry(
    _context: Arc<Context>,
    old_store: &DhtStore,
    entry: &Entry,
) -> Option<DhtStore> {
    // system entry type must be publishable
    if !entry.entry_type().to_owned().can_publish() {
        return None;
    }
    // Add it local storage
    let new_store = (*old_store).clone();
    let storage = &new_store.content_storage().clone();
    let res = storage.write().unwrap().add(entry);
    if res.is_err() {
        // TODO #439 - Log the error. Once we have better logging.
        return None;
    }
    // Note: System entry types are not published to the network
    Some(new_store)
}

//
pub(crate) fn commit_app_entry(
    context: Arc<Context>,
    old_store: &DhtStore,
    entry: &Entry,
) -> Option<DhtStore> {
    println!("\ncommit_app_entry!!!\n");
    // pre-condition: if app entry_type must be valid
    // get entry_type definition
    let dna = context
        .state()
        .expect("context must have a State.")
        .nucleus()
        .dna()
        .expect("context.state must hold DNA in order to commit an app entry.");
    let maybe_def = dna.get_entry_type_def(&entry.entry_type().to_string());
    if maybe_def.is_none() {
        // TODO #439 - Log the error. Once we have better logging.
        return None;
    }
    let entry_type_def = maybe_def.unwrap();

    // app entry type must be publishable
    if !entry_type_def.sharing.clone().can_publish() {
        return None;
    }

    println!("commit_app_entry: entry: {:?}", entry);
    // Add entry and meta to local storage...
    let mut new_store = (*old_store).clone();
    let content_storage = &new_store.content_storage().clone();
    let res = (*content_storage.write().unwrap()).add(entry);
    if res.is_err() {
        // TODO #439 - Log the error. Once we have better logging.
        return None;
    }
    let meta_storage = &new_store.meta_storage().clone();
    let status_eav = create_crud_status_eav(&entry.address(), CrudStatus::LIVE);
    let res = (*meta_storage.write().unwrap()).add_eav(&status_eav);
    if res.is_err() {
        // TODO #439 - Log the error. Once we have better logging.
        println!("commit_app_entry: meta_storage write failed!: {:?}", res.err().unwrap());
        return None;
    }
    println!("commit_app_entry: eav: {:?}", status_eav);

    // ...and publish to the network if its not private
    new_store.network_mut().publish(entry);
    new_store.network_mut().publish_meta(&status_eav);
    // Done
    Some(new_store)
}

//
pub(crate) fn reduce_commit_entry(
    context: Arc<Context>,
    old_store: &DhtStore,
    action_wrapper: &ActionWrapper,
) -> Option<DhtStore> {
    let action = action_wrapper.action();
    let entry = unwrap_to!(action => Action::Commit);

    println!("\nreduce_commit_entry!!!");

    // Handle sys entries and app entries differently
    if entry.entry_type().to_owned().is_sys() {
        return commit_sys_entry(context, old_store, entry);
    }
    return commit_app_entry(context, old_store, entry);
}


//
pub(crate) fn reduce_update_entry(
    _context: Arc<Context>,
    old_store: &DhtStore,
    action_wrapper: &ActionWrapper,
) -> Option<DhtStore> {
    let action = action_wrapper.action();
    let (old_address, new_address) = unwrap_to!(action => Action::UpdateEntry);
    let mut new_store = (*old_store).clone();

    println!("\n DHT reduce_update_entry!!!");

    // pre-condition: Must already have old and new entry in local content_storage
    // FIXME
    // pre-condition: old_entry current status must be LIVE
    // FIXME
    // Update crud-status
    let new_status = create_crud_status_eav(old_address, CrudStatus::MODIFIED);
    let meta_storage = &new_store.meta_storage().clone();
    let res = (*meta_storage.write().unwrap()).add_eav(&new_status);
    if res.is_err() {
        new_store.actions_mut().insert(
            action_wrapper.clone(),
            Err(HolochainError::ErrorGeneric(String::from(
                "add_eav() for crud-status failed",
            ))),
        );
        return Some(new_store);
    }
    new_store
        .actions_mut()
        .insert(action_wrapper.clone(), res);

    println!("\n DHT reduce_update_entry: new_status = {:?}", new_status);
    // Update crud-link
    // FIXME
    // Done
    Some(new_store)
}

//
pub(crate) fn reduce_remove_entry(
    context: Arc<Context>,
    old_store: &DhtStore,
    action_wrapper: &ActionWrapper,
) -> Option<DhtStore> {
    let action = action_wrapper.action();
    let address = unwrap_to!(action => Action::RemoveEntry);
    let mut new_store = (*old_store).clone();
    println!("\n reduce_remove_entry!!!");

    // pre-condition: Must already have entry in local content_storage
    let content_storage = &old_store.content_storage().clone();
    let maybe_entry = content_storage.read().unwrap().fetch(address).unwrap();
    if maybe_entry.is_none() {
        new_store.actions_mut().insert(
            action_wrapper.clone(),
            Err(HolochainError::ErrorGeneric(String::from(
                "trying to remove a missing entry",
            ))),
        );
        return Some(new_store);
    }
    let ser_entry = SerializedEntry::try_from(maybe_entry.unwrap()).unwrap();
    let entry = Entry::from(ser_entry);
    // pre-condition entry_type must not by sys type, since they cannot be deleted
    if entry.entry_type().to_owned().is_sys() {
        new_store.actions_mut().insert(
            action_wrapper.clone(),
            Err(HolochainError::ErrorGeneric(String::from(
                "trying to remove a system entry type",
            ))),
        );
        return Some(new_store);
    }
    // pre-condition: Current status must be LIVE
    // get current status
    let meta_storage = &old_store.meta_storage().clone();
    let maybe_status_eav =  meta_storage.read().unwrap().fetch_eav(
        Some(address.clone()), Some(STATUS_NAME.to_string()), None);
    if maybe_status_eav.is_err() {
        new_store.actions_mut().insert(
            action_wrapper.clone(),
            Err(HolochainError::ErrorGeneric(String::from(
                "entry does not have a status",
            ))),
        );
        return Some(new_store);
    }
    let status_eavs = maybe_status_eav.unwrap();
    assert!(!status_eavs.is_empty(), "Entry should have a Status");
    println!("reduce_remove_entry: status_eavs = {:?}", status_eavs);
    // FIXME waiting for update/remove_eav() assert!(status_eavs.len() <= 1);
    let status_eav =
        if status_eavs.len() > 1 {
            status_eavs.iter().last().unwrap()
        } else {
            status_eavs.iter().next().unwrap()
        };
    let entry_status = CrudStatus::from(String::from(status_eav.value()));
    println!("reduce_remove_entry: entry_status = {:?}", entry_status);
    let status_eavs = status_eavs
        .iter()
        .filter(|e| CrudStatus::from(String::from(e.value())) != CrudStatus::LIVE)
        .collect::<HashSet<&EntityAttributeValue>>();
    println!("reduce_remove_entry: status_eavs FILTERED = {:?}", status_eavs);
    //if entry_status != CrudStatus::LIVE {
    if status_eavs.len() > 0 {
        println!("reduce_remove_entry NOT LIVE !!");
        new_store.actions_mut().insert(
            action_wrapper.clone(),
            Err(HolochainError::ErrorGeneric(String::from(
                "entry_status != CrudStatus::LIVE",
            ))),
        );
        return Some(new_store);
    }
    // Update crud-status
    let new_status = create_crud_status_eav(address, CrudStatus::DELETED);
    let meta_storage = &new_store.meta_storage().clone();
    let res = (*meta_storage.write().unwrap()).add_eav(&new_status);
    if res.is_err() {
        new_store.actions_mut().insert(
            action_wrapper.clone(),
            Err(HolochainError::ErrorGeneric(String::from(
                "add_eav() for crud-status failed",
            ))),
        );
        return Some(new_store);
    }
    new_store
        .actions_mut()
        .insert(action_wrapper.clone(), res);

    println!("\n reduce_remove_entry: new_status = {:?}", new_status);
    // Done
    Some(new_store)
}

//
pub(crate) fn reduce_get_entry_from_network(
    _context: Arc<Context>,
    old_store: &DhtStore,
    action_wrapper: &ActionWrapper,
) -> Option<DhtStore> {
    // Get Action's input data
    let action = action_wrapper.action();
    let address = unwrap_to!(action => Action::GetEntry);
    let storage = &old_store.content_storage().clone();
    // pre-condition check: Look in local storage if it already has it.
    if (*storage.read().unwrap()).contains(address).unwrap() {
        // TODO #439 - Log a warning saying this should not happen. Once we have better logging.
        return None;
    }
    // Retrieve it from the network...
    old_store
        .network()
        .clone()
        .get(address)
        .and_then(|content| {
            let entry =
                Entry::try_from_content(&content).expect("could not load entry from content");
            let new_store = (*old_store).clone();

            // ...and add it to the local storage
            let storage = &new_store.content_storage().clone();
            let res = (*storage.write().unwrap()).add(&entry);
            match res {
                Err(_) => None,
                Ok(()) => Some(new_store),
            }
        })
}

//
pub(crate) fn reduce_add_link(
    _context: Arc<Context>,
    old_store: &DhtStore,
    action_wrapper: &ActionWrapper,
) -> Option<DhtStore> {
    // Get Action's input data
    let action = action_wrapper.action();
    let link = unwrap_to!(action => Action::AddLink);

    let mut new_store = (*old_store).clone();
    let storage = &old_store.content_storage().clone();
    if !(*storage.read().unwrap()).contains(link.base()).unwrap() {
        new_store.actions_mut().insert(
            action_wrapper.clone(),
            Err(HolochainError::ErrorGeneric(String::from(
                "Base for link not found",
            ))),
        );
        return Some(new_store);
    }

    let eav =
        EntityAttributeValue::new(link.base(), &format!("link__{}", link.tag()), link.target());

    let storage = new_store.meta_storage();
    let result = storage.write().unwrap().add_eav(&eav);
    new_store
        .actions_mut()
        .insert(action_wrapper.clone(), result);
    Some(new_store)
}

#[allow(dead_code)]
pub(crate) fn reduce_get_links(
    _context: Arc<Context>,
    _old_store: &DhtStore,
    _action_wrapper: &ActionWrapper,
) -> Option<DhtStore> {
    // FIXME
    None
}

#[cfg(test)]
pub mod tests {

    use action::{Action, ActionWrapper};
    use dht::{
        dht_reducers::{commit_sys_entry, reduce},
        dht_store::DhtStore,
    };
    use holochain_core_types::{
        cas::content::AddressableContent,
        entry::{test_entry, test_sys_entry, test_unpublishable_entry, Entry},
        link::Link,
    };
    use instance::tests::test_context;
    use state::test_store;
    use std::sync::{Arc, RwLock};

    #[test]
    fn commit_sys_entry_test() {
        let context = test_context("bob");
        let store = test_store(context.clone());
        let entry = test_entry();

        let unpublishable_entry = test_unpublishable_entry();

        let new_dht_store =
            commit_sys_entry(Arc::clone(&context), &store.dht(), &unpublishable_entry);

        // test_entry is not sys so should do nothing
        let storage = &store.dht().content_storage().clone();
        assert_eq!(None, new_dht_store);
        assert_eq!(
            None,
            (*storage.read().unwrap())
                .fetch(&entry.address())
                .expect("could not fetch from cas")
        );

        let sys_entry = test_sys_entry();

        let new_dht_store = commit_sys_entry(Arc::clone(&context), &store.dht(), &sys_entry)
            .expect("there should be a new store for committing a sys entry");
        assert_eq!(
            Some(sys_entry.clone()),
            (*storage.read().unwrap())
                .fetch(&sys_entry.address())
                .expect("could not fetch from cas")
                .map(|s| Entry::try_from_content(&s).unwrap())
        );

        let new_storage = &new_dht_store.content_storage().clone();
        assert_eq!(
            Some(sys_entry.clone()),
            (*new_storage.read().unwrap())
                .fetch(&sys_entry.address())
                .expect("could not fetch from cas")
                .map(|s| Entry::try_from_content(&s).unwrap())
        );
    }

    #[test]
    fn can_add_links() {
        let context = test_context("bob");
        let store = test_store(context.clone());
        let entry = test_entry();

        let locked_state = Arc::new(RwLock::new(store));

        let mut context = (*context).clone();
        context.set_state(locked_state.clone());
        let storage = context.file_storage.clone();
        let _ = (storage.write().unwrap()).add(&entry);
        let context = Arc::new(context);

        let link = Link::new(&entry.address(), &entry.address(), "test-tag");
        let action = ActionWrapper::new(Action::AddLink(link.clone()));

        let new_dht_store: DhtStore;
        {
            let state = locked_state.read().unwrap();

            new_dht_store = (*reduce(Arc::clone(&context), state.dht(), &action)).clone();
        }
        let storage = new_dht_store.meta_storage();
        let fetched = storage
            .read()
            .unwrap()
            .fetch_eav(Some(entry.address()), None, None);

        assert!(fetched.is_ok());
        let hash_set = fetched.unwrap();
        assert_eq!(hash_set.len(), 1);
        let eav = hash_set.iter().nth(0).unwrap();
        assert_eq!(eav.entity(), *link.base());
        assert_eq!(eav.value(), *link.target());
        assert_eq!(eav.attribute(), format!("link__{}", link.tag()));
    }

    #[test]
    fn does_not_add_link_for_missing_base() {
        let context = test_context("bob");
        let store = test_store(context.clone());
        let entry = test_entry();

        let locked_state = Arc::new(RwLock::new(store));

        let mut context = (*context).clone();
        context.set_state(locked_state.clone());
        let context = Arc::new(context);

        let link = Link::new(&entry.address(), &entry.address(), "test-tag");
        let action = ActionWrapper::new(Action::AddLink(link.clone()));

        let new_dht_store: DhtStore;
        {
            let state = locked_state.read().unwrap();

            new_dht_store = (*reduce(Arc::clone(&context), state.dht(), &action)).clone();
        }
        let storage = new_dht_store.meta_storage();
        let fetched = storage
            .read()
            .unwrap()
            .fetch_eav(Some(entry.address()), None, None);

        assert!(fetched.is_ok());
        let hash_set = fetched.unwrap();
        assert_eq!(hash_set.len(), 0);

        let result = new_dht_store.actions().get(&action).unwrap();

        assert!(result.is_err());
    }

}
