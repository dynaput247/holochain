use nucleus::ribosome::api::Runtime;
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};
use wasmi::{RuntimeArgs, RuntimeValue, Trap};

use serde_json;

#[derive(Serialize)]
struct InitGlobalsOutput {
    app_name: String,
    app_dna_hash: String,
    app_agent_id_str: String,
    app_agent_key_hash: String,
    app_agent_initial_hash: String,
    app_agent_latest_hash: String,
}

/// ZomeApiFunction::InitGlobals secret function code
/// args: [0] encoded MemoryAllocation as u32
/// Not expecting any complex input
/// Returns an HcApiReturnCode as I32
pub fn invoke_init_globals(
    runtime: &mut Runtime,
    _args: &RuntimeArgs,
) -> Result<Option<RuntimeValue>, Trap> {
    let globals = InitGlobalsOutput {
        app_name: runtime.app_name.to_string(),

        app_dna_hash: match runtime.context.state() {
            Some(state) => {
                let mut s = DefaultHasher::new();
                state.nucleus().dna().hash(&mut s);
                String::from(s.finish().to_string())
            }
            None => String::from(""),
        },

        app_agent_id_str: runtime.context.agent.to_string(),

        // TODO #233 - Implement agent pub key hash
        app_agent_key_hash: "FIXME-app_agent_key_hash".to_string(),

        // TODO #234 - Implement agent identity entry hashes
        app_agent_initial_hash: "FIXME-app_agent_initial_hash".to_string(),
        app_agent_latest_hash: "FIXME-app_agent_latest_hash".to_string(),
    };

    return runtime.store_utf8(&serde_json::to_string(&globals).unwrap());
}

#[cfg(test)]
pub mod tests {
    use nucleus::ribosome::{
        api::{tests::test_zome_api_function_runtime, ZomeApiFunction},
        Defn,
    };

    #[test]
    /// test that bytes passed to debug end up in the log
    fn test_init_globals() {
        let input: Vec<u8> = vec![];
        let (runtime, _) =
            test_zome_api_function_runtime(ZomeApiFunction::InitGlobals.as_str(), input);
        assert_eq!(
      runtime.result.to_string(),
      "{\"app_name\":\"TestApp\",\"app_dna_hash\":\"FIXME-app_dna_hash\",\"app_agent_id_str\":\"joan\",\"app_agent_key_hash\":\"FIXME-app_agent_key_hash\",\"app_agent_initial_hash\":\"FIXME-app_agent_initial_hash\",\"app_agent_latest_hash\":\"FIXME-app_agent_latest_hash\"}\u{0}"
        .to_string());
    }
}
