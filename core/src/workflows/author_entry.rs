use crate::{
    agent::actions::commit::commit_entry,
    context::Context,
    network::actions::publish::publish,
    nucleus::{
        actions::build_validation_package::build_validation_package, validation::validate_entry,
    },
};

use holochain_core_types::{
    entry::Entry,
    error::HolochainError,
    signature::Provenance,
    validation::{EntryLifecycle, ValidationData},
};

use holochain_persistence_api::cas::content::{Address, AddressableContent};

use holochain_wasm_utils::api_serialization::commit_entry::CommitEntryResult;

use crate::nucleus::ribosome::callback::links_utils::get_link_entries;
use std::{sync::Arc, vec::Vec};

pub async fn author_entry<'a>(
    entry: &'a Entry,
    maybe_link_update_delete: Option<Address>,
    context: &'a Arc<Context>,
    provenances: &'a Vec<Provenance>,
) -> Result<CommitEntryResult, HolochainError> {
    let address = entry.address();
    context.log(format!(
        "debug/workflow/authoring_entry: {} with content: {:?}",
        address, entry
    ));

    // 0. If we are trying to author a link or link removal, make sure the linked entries exist:
    if let Entry::LinkAdd(link_data) = entry {
        get_link_entries(&link_data.link, context)?;
    }
    if let Entry::LinkRemove((link_data, _)) = entry {
        get_link_entries(&link_data.link, context)?;
    }

    // 1. Build the context needed for validation of the entry
    let validation_package = await!(build_validation_package(
        &entry,
        context.clone(),
        provenances
    ))?;
    let validation_data = ValidationData {
        package: validation_package,
        lifecycle: EntryLifecycle::Chain,
    };

    // 2. Validate the entry
    context.log(format!(
        "debug/workflow/authoring_entry/{}: validating...",
        address
    ));
    await!(validate_entry(
        entry.clone(),
        maybe_link_update_delete.clone(),
        validation_data,
        &context
    ))?;
    context.log(format!("Authoring entry {}: is valid!", address));

    // 3. Commit the entry
    context.log(format!(
        "debug/workflow/authoring_entry/{}: committing...",
        address
    ));
    let addr = await!(commit_entry(
        entry.clone(),
        maybe_link_update_delete,
        &context
    ))?;
    context.log(format!(
        "debug/workflow/authoring_entry/{}: committed",
        address
    ));

    // 4. Publish the valid entry to DHT.
    // For publishable entires this will publish the entry and the header
    // For non-publishable entries this will only publish the header
    context.log(format!(
        "debug/workflow/authoring_entry/{}: publishing...",
        address
    ));
    await!(publish(entry.address(), &context))?;
    context.log(format!(
        "debug/workflow/authoring_entry/{}: published!",
        address
    ));
    Ok(CommitEntryResult::new(addr))
}

#[cfg(test)]
pub mod tests {
    use super::author_entry;
    use crate::nucleus::actions::get_entry::get_entry_from_dht;
    use crate::nucleus::actions::tests::*;
    use holochain_core_types::{
        entry::{test_entry_with_value, Entry}
    };
    use holochain_persistence_api::cas::content::AddressableContent;
    use std::{thread, time};

    #[test]
    /// test that a commit will publish and entry to the dht of a connected instance via the in-memory network
    fn test_commit_with_dht_publish() {
        let mut dna = test_dna();
        dna.uuid = "test_commit_with_dht_publish".to_string();
        let netname = Some("test_commit_with_dht_publish, the network");
        let (_instance1, context1) = instance_by_name("jill", dna.clone(), netname);
        let (_instance2, context2) = instance_by_name("jack", dna, netname);

        let entry_address = context1
            .block_on(author_entry(
                &test_entry_with_value("{\"stuff\":\"test entry value\"}"),
                None,
                &context1,
                &vec![],
            ))
            .unwrap()
            .address();
        thread::sleep(time::Duration::from_millis(500));

        let mut entry: Option<Entry> = None;
        let mut tries = 0;
        while entry.is_none() && tries < 120 {
            tries = tries + 1;
            {
                entry = get_entry_from_dht(&context2, &entry_address).expect("Could not retrieve entry from DHT");
            }
            println!("Try {}: {:?}", tries, entry);
            if entry.is_none() {
                thread::sleep(time::Duration::from_millis(1000));
            }
        }
        assert_eq!(
            entry,
            Some(test_entry_with_value("{\"stuff\":\"test entry value\"}"))
        );
    }

    #[test]
    /// test that the header of an entry can be retrieved directly by its hash by another agent connected
    /// via the in-memory network
    fn test_commit_with_dht_publish_header_is_published() {
        let mut dna = test_dna();
        dna.uuid = "test_commit_with_dht_publish_header_is_published".to_string();
        let netname = Some("test_commit_with_dht_publish_header_is_published, the network");
        let (_instance1, context1) = instance_by_name("jill", dna.clone(), netname);
        let (_instance2, context2) = instance_by_name("jack", dna, netname);

        let entry_address = context1
            .block_on(author_entry(
                &test_entry_with_value("{\"stuff\":\"test entry value\"}"),
                None,
                &context1,
                &vec![],
            ))
            .unwrap()
            .address();

        thread::sleep(time::Duration::from_millis(500));

        // get the header from the top of Jill's chain
        let state = &context1.state().unwrap();
        let header = state.get_headers(entry_address)
            .expect("Could not retrieve headers from authors chain")
            .into_iter()
            .next()
            .expect("No headers were found for this entry in the authors chain");
        let header_entry = Entry::ChainHeader(header);

        // try and load it by its address as Jack. This means it has been communicated over the mock network
        let mut entry: Option<Entry> = None;
        let mut tries = 0;
        while entry.is_none() && tries < 10 {
            tries = tries + 1;
            {
                entry = get_entry_from_dht(&context2, &header_entry.address()).expect("Could not retrieve entry from DHT");
            }
            println!("Try {}: {:?}", tries, entry);
            if entry.is_none() {
                thread::sleep(time::Duration::from_millis(1000));
            }
        }
        assert_eq!(
            entry,
            Some(header_entry),
        );
    }
}
