extern crate futures;
use agent::{actions::commit::*, state::ActionResponse};
use futures::{executor::block_on, FutureExt};
use json::ToJson;
use nucleus::{
    actions::validate::*,
    ribosome::api::{HcApiReturnCode, Runtime},
};
use serde_json;
use wasmi::{RuntimeArgs, RuntimeValue, Trap};

/// Struct for input data received when Commit API function is invoked
#[derive(Deserialize, Default, Debug, Serialize)]
struct CommitArgs {
    entry_type_name: String,
    entry_content: String,
}

/// HcApiFuncIndex::COMMIT function code
/// args: [0] encoded MemoryAllocation as u32
/// expected complex argument: r#"{"entry_type_name":"post","entry_content":"hello"}"#
/// Returns an HcApiReturnCode as I32
pub fn invoke_commit_entry(
    runtime: &mut Runtime,
    args: &RuntimeArgs,
) -> Result<Option<RuntimeValue>, Trap> {
    // deserialize args
    let args_str = runtime.load_utf8_from_args(&args);
    let entry_input: CommitArgs = match serde_json::from_str(&args_str) {
        Ok(entry_input) => entry_input,
        // Exit on error
        Err(_) => {
            // Return Error code in i32 format
            return Ok(Some(RuntimeValue::I32(
                HcApiReturnCode::ArgumentDeserializationFailed as i32,
            )));
        }
    };

    // Create Chain Entry
    let entry =
        ::hash_table::entry::Entry::new(&entry_input.entry_type_name, &entry_input.entry_content);

    // Wait for future to be resolved
    let task_result: Result<ActionResponse, String> = block_on(
        // First validate entry:
        validate_entry(entry.clone(), &runtime.action_channel, &runtime.context)
            // if successful, commit entry:
            .and_then(|_| commit_entry(entry.clone(), &runtime.action_channel, &runtime.context)),
    );

    let json = match task_result {
        Ok(action_response) => match action_response {
            ActionResponse::Commit(_) => action_response.to_json(),
            _ => Ok("Unknown error".to_string()),
        },
        Err(error_string) => Ok(json!({ "error": error_string }).to_string()),
    };

    // allocate and encode result
    match json {
        Ok(j) => runtime.store_utf8(&j),
        Err(_) => Ok(Some(RuntimeValue::I32(
            HcApiReturnCode::ResponseSerializationFailed as i32,
        ))),
    }

    // @TODO test that failing validation prevents commits happening
    // @see https://github.com/holochain/holochain-rust/issues/206
}

#[cfg(test)]
pub mod tests {
    extern crate test_utils;
    extern crate wabt;

    use super::CommitArgs;
    use hash_table::entry::tests::test_entry;
    use key::Key;
    use nucleus::ribosome::{
        api::{tests::test_zome_api_function_runtime, ZomeApiFunction},
        Defn,
    };
    use serde_json;

    /// dummy commit args from standard test entry
    pub fn test_commit_args_bytes() -> Vec<u8> {
        let e = test_entry();
        let args = CommitArgs {
            entry_type_name: e.entry_type().into(),
            entry_content: e.content().into(),
        };
        serde_json::to_string(&args)
            .expect("args should serialize")
            .into_bytes()
    }

    #[test]
    /// test that we can round trip bytes through a commit action and get the result from WASM
    fn test_commit_round_trip() {
        let (runtime, _) = test_zome_api_function_runtime(
            ZomeApiFunction::CommitAppEntry.as_str(),
            test_commit_args_bytes(),
        );

        assert_eq!(
            runtime.result,
            format!(r#"{{"hash":"{}"}}"#, test_entry().key()) + "\u{0}",
        );
    }

}
