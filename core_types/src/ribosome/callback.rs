use convert::TryFrom;
use error::HolochainError;
use json::*;
use validation::ValidationPackageDefinition;
use entry::{Entry, SerializedEntry};
use error::RibosomeReturnCode;
use serde_json;

#[derive(Debug)]
pub enum CallbackParams {
    Genesis,
    ValidateCommit(Entry),
    // @TODO call this from somewhere
    // @see https://github.com/holochain/holochain-rust/issues/201
    Receive,
}

impl ToString for CallbackParams {
    fn to_string(&self) -> String {
        match self {
            CallbackParams::Genesis => String::new(),
            CallbackParams::ValidateCommit(entry) => {
                String::from(JsonString::from(SerializedEntry::from(entry.to_owned())))
            }
            CallbackParams::Receive => String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CallbackResult {
    Pass,
    Fail(String),
    NotImplemented,
    ValidationPackageDefinition(ValidationPackageDefinition),
}

impl TryFrom<CallbackResult> for JsonString {
    type Error = HolochainError;
    fn try_from(v: CallbackResult) -> JsonResult {
        default_try_to_json(v)
    }
}

impl From<JsonString> for CallbackResult {
    fn from(json_string: JsonString) -> CallbackResult {
        let try: Result<CallbackResult, serde_json::Error> =
            serde_json::from_str(&String::from(json_string.clone()));
        match try {
            Ok(callback_result) => callback_result,
            Err(_) => CallbackResult::Fail(String::from(json_string)),
        }
    }
}

impl From<RibosomeReturnCode> for CallbackResult {
    fn from(ribosome_return_code: RibosomeReturnCode) -> CallbackResult {
        match ribosome_return_code {
            RibosomeReturnCode::Failure(ribosome_error_code) => {
                CallbackResult::Fail(ribosome_error_code.to_string())
            }
            RibosomeReturnCode::Success => CallbackResult::Pass,
        }
    }
}
