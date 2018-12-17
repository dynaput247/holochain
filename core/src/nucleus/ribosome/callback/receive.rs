use crate::{
    context::Context,
    nucleus::{
        ribosome::{
            self,
            callback::{Callback, CallbackParams, CallbackResult},
            Defn,
        },
        ZomeFnCall,
    },
};
use holochain_core_types::{error::HolochainError, json::JsonString};
use std::sync::Arc;

#[derive(Serialize, Deserialize, Debug, DefaultJson)]
struct ReceiveReturnValue(Result<String, String>);

pub fn receive(
    context: Arc<Context>,
    zome: &str,
    // we ignore params for genesis
    params: &CallbackParams,
) -> CallbackResult {
    let params = match params {
        CallbackParams::Receive(payload) => payload,
        _ => return CallbackResult::NotImplemented,
    };

    let zome_call = ZomeFnCall::new(
        zome,
        "no capability since this is a callback",
        "no token since this is a callback?",
        &Callback::Receive.as_str().to_string(),
        params.clone(),
    );

    let dna = context.get_dna().expect("Callback called without DNA set!");

    let maybe_wasm = dna.get_wasm_from_zome_name(zome);
    if maybe_wasm.is_none() {
        return CallbackResult::NotImplemented;
    }
    let wasm = maybe_wasm.unwrap();
    if wasm.code.is_empty() {
        return CallbackResult::NotImplemented;
    }

    match ribosome::run_dna(
        &dna.name,
        context,
        wasm.code.clone(),
        &zome_call,
        Some(zome_call.clone().parameters.into_bytes()),
    ) {
        Ok(call_result) => CallbackResult::ReceiveResult(call_result.to_string()),
        Err(_) => CallbackResult::NotImplemented,
    }
}

#[cfg(test)]
pub mod tests {

    use super::receive;
    use crate::{
        instance::tests::test_context,
        nucleus::ribosome::{
            callback::{tests::test_callback_instance, Callback, CallbackParams, CallbackResult},
            Defn,
        },
    };

    #[test]
    fn not_implemented() {
        let zome = "test_zome";
        let instance = test_callback_instance(
            zome,
            // anything other than Genesis is fine here
            Callback::MissingNo.as_str(),
            0,
        )
        .expect("Test callback instance could not be initialized");
        let context = instance.initialize_context(test_context("test"));

        let result = receive(context, zome, &CallbackParams::Receive(String::from("")));

        assert_eq!(CallbackResult::NotImplemented, result);
    }

    #[test]
    fn implemented_with_null() {
        let zome = "test_zome";
        let instance = test_callback_instance(zome, Callback::Receive.as_str(), 0)
            .expect("Test callback instance could not be initialized");
        let context = instance.initialize_context(test_context("test"));

        let result = receive(context, zome, &CallbackParams::Receive(String::from("")));

        assert_eq!(CallbackResult::ReceiveResult(String::from("null")), result);
    }
}
