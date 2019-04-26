use crate::{
    nucleus::ribosome::{api::ZomeApiResult, Runtime},
    workflows::author_entry::author_entry,
};
use holochain_core_types::{
    cas::content::Address,
    entry::{Entry, EntryWithProvenance},
    error::HolochainError,
};
use std::convert::TryFrom;
use wasmi::{RuntimeArgs, RuntimeValue};

/// ZomeApiFunction::CommitAppEntry function code
/// args: [0] encoded MemoryAllocation as u64
/// Expected complex argument: CommitArgs
/// Returns an HcApiReturnCode as I64
pub fn invoke_commit_app_entry(runtime: &mut Runtime, args: &RuntimeArgs) -> ZomeApiResult {
    let context = runtime.context()?;
    // deserialize args
    let args_str = runtime.load_json_string_from_args(&args);
    let entry = match Entry::try_from(args_str.clone()) {
        Ok(entry_input) => entry_input,
        // Exit on error
        Err(_) => {
            context.log(format!(
                "err/zome: invoke_commit_app_entry failed to deserialize Entry: {:?}",
                args_str
            ));
            return ribosome_error_code!(ArgumentDeserializationFailed);
        }
    };
    // Wait for future to be resolved
    let task_result: Result<Address, HolochainError> =
        context.block_on(author_entry(&entry, None, &context, &vec![]));

    runtime.store_result(task_result)
}

/// ZomeApiFunction::CommitAppEntryWithProvenance function code
/// args: [0] encoded MemoryAllocation as u64
/// Expected complex argument: EntryWithProvenance
/// Returns an HcApiReturnCode as I64
pub fn invoke_commit_app_entry_with_provenance(
    runtime: &mut Runtime,
    args: &RuntimeArgs,
) -> ZomeApiResult {
    let context = runtime.context()?;
    // deserialize args
    let args_str = runtime.load_json_string_from_args(&args);
    let entry_with_provenance = match EntryWithProvenance::try_from(args_str.clone()) {
        Ok(entry_with_provenance_input) => entry_with_provenance_input,
        // Exit on error
        Err(error) => {
            context.log(format!(
                "err/zome: invoke_commit_app_entry_with_provenance failed to \
                 deserialize Entry: {:?} with error {:?}",
                args_str, error
            ));
            return ribosome_error_code!(ArgumentDeserializationFailed);
        }
    };
    // Wait for future to be resolved
    let task_result: Result<Address, HolochainError> = context.block_on(author_entry(
        &entry_with_provenance.entry(),
        None,
        &context,
        &entry_with_provenance.provenances(),
    ));

    runtime.store_result(task_result)
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::nucleus::ribosome::{
        api::{tests::test_zome_api_function, ZomeApiFunction},
        Defn,
    };
    use holochain_core_types::{
        cas::content::{Address, AddressableContent},
        entry::{test_entry, Entry},
        error::ZomeApiInternalResult,
        json::JsonString,
        signature::{Provenance, Signature},
    };

    /// dummy commit args from standard test entry
    pub fn test_commit_args_bytes() -> Vec<u8> {
        let entry = test_entry();

        let serialized_entry = Entry::from(entry);
        JsonString::from(serialized_entry).to_bytes()
    }

    /// dummy commit with provenance args from standard test entry
    pub fn test_commit_with_provenance_args_bytes() -> Vec<u8> {
        let entry = test_entry();
        let address: Address = entry.address();

        let agent_nick = "counter-signer";
        let agent_id = test_utils::mock_signing::registered_test_agent(agent_nick);

        let signature = Signature::from(test_utils::mock_signing::mock_signer(
            address.clone().into(),
            &agent_id,
        ));

        let provenances = vec![Provenance::new(agent_id.address(), signature)];
        let serialized_entry_with_provenance = EntryWithProvenance::new(entry, provenances);
        JsonString::from(serialized_entry_with_provenance).to_bytes()
    }

    #[test]
    /// test that we can round trip bytes through a commit action and get the result from WASM
    fn test_commit_round_trip() {
        let (call_result, _) = test_zome_api_function(
            ZomeApiFunction::CommitAppEntry.as_str(),
            test_commit_args_bytes(),
        );

        assert_eq!(
            call_result,
            JsonString::from_json(
                &(String::from(JsonString::from(ZomeApiInternalResult::success(
                    Address::from("Qma6RfzvZRL127UCEVEktPhQ7YSS1inxEFw7SjEsfMJcrq")
                ))) + "\u{0}")
            ),
        );
    }

    #[test]
    /// test that we can round trip bytes through a commit action with
    /// additional provenance and get the result from WASM
    fn test_commit_with_provenance_round_trip() {
        let (call_result, _) = test_zome_api_function(
            ZomeApiFunction::CommitAppEntryWithProvenance.as_str(),
            test_commit_with_provenance_args_bytes(),
        );

        assert_eq!(
            call_result,
            JsonString::from_json(
                &(String::from(JsonString::from(ZomeApiInternalResult::success(
                    Address::from("Qma6RfzvZRL127UCEVEktPhQ7YSS1inxEFw7SjEsfMJcrq")
                ))) + "\u{0}")
            ),
        );
    }

}
