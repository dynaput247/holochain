use crate::{
    agent::actions::{commit::commit_entry, update_entry::update_entry},
    workflows::get_entry_history::get_entry_history_workflow,
    nucleus::{
        actions::{build_validation_package::*, validate::*},
        ribosome::{api::ZomeApiResult, Runtime},
    },
};
use futures::{
    executor::block_on,
    future::{self, TryFutureExt},
};
use holochain_core_types::{
    cas::content::{Address, AddressableContent},
    entry::Entry,
    error::HolochainError,
    hash::HashString,
    validation::{EntryAction, EntryLifecycle, ValidationData},
};
use holochain_wasm_utils::api_serialization::UpdateEntryArgs;
use holochain_wasm_utils::api_serialization::get_entry::*;
use std::convert::TryFrom;
use wasmi::{RuntimeArgs, RuntimeValue};

/// ZomeApiFunction::UpdateEntry function code
/// args: [0] encoded MemoryAllocation as u32
/// Expected complex argument: UpdateEntryArgs
/// Returns an HcApiReturnCode as I32
pub fn invoke_update_entry(runtime: &mut Runtime, args: &RuntimeArgs) -> ZomeApiResult {
    // deserialize args
    let args_str = runtime.load_json_string_from_args(&args);
    let entry_args = match UpdateEntryArgs::try_from(args_str.clone()) {
        Ok(entry_input) => entry_input,
        // Exit on error
        Err(_) => {
            println!(
                "invoke_update_entry failed to deserialize SerializedEntry: {:?}",
                args_str
            );
            return ribosome_error_code!(ArgumentDeserializationFailed);
        }
    };

    // Get Current entry's latest version
    let get_args = GetEntryArgs {
        address: entry_args.address,
        options: GetEntryOptions::default(),
    };
    let get_entry_history_result = block_on(get_entry_history_workflow(&runtime.context, &get_args));
    if let Err(err) = get_entry_history_result {
        return ribosome_error_code!(Unspecified);
    }
    let entry_history = get_entry_history_result.unwrap();
    if entry_history.entries.is_empty() {
        return ribosome_error_code!(Unspecified);
    }
    let latest_entry = entry_history.entries.iter().next().unwrap().clone();

    // Get latest entry's ChainHeader
    let agent_state = &runtime.context.state().unwrap().agent();
    let chain_header_address = agent_state
        .chain()
        .iter(&agent_state.top_chain_header())
        .find(|header| header.entry_address() == &latest_entry.address())
        .map(|header| header.address().clone())
        .expect("Modified entry should be in chain");

    // Create Chain Entry
    let entry = Entry::from(entry_args.new_entry.clone());

    // Wait for future to be resolved
    let task_result: Result<Address, HolochainError> = block_on(
        // 1. Build the context needed for validation of the entry
        build_validation_package(&entry, &runtime.context)
            .and_then(|validation_package| {
                future::ready(Ok(ValidationData {
                    package: validation_package,
                    sources: vec![HashString::from("<insert your agent key here>")],
                    lifecycle: EntryLifecycle::Chain,
                    action: EntryAction::Modify,
                }))
            })
            // 2. Validate the entry
            .and_then(|validation_data| {
                validate_entry(entry.clone(), validation_data, &runtime.context)
            })
            // 3. Commit the valid entry to chain and DHT
            .and_then(|_| commit_entry(entry.clone(), Some(chain_header_address), &runtime.context))
            // 4. Update the entry in DHT metadata
            .and_then(|new_address| {
                update_entry(
                    &runtime.context,
                    &runtime.context.action_channel,
                    latest_entry.address().clone(),
                    new_address,
                )
            }),
    );

    runtime.store_result(task_result)
}
