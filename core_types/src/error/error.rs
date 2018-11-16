use self::HolochainError::*;
use error::{DnaError, RibosomeErrorCode};
use futures::channel::oneshot::Canceled as FutureCanceled;
use json::*;
use serde_json::Error as SerdeError;
use std::{
    error::Error,
    fmt,
    io::{self, Error as IoError},
};

//--------------------------------------------------------------------------------------------------
// CoreError
//--------------------------------------------------------------------------------------------------

/// Holochain Core Error struct
/// Any Error in Core should be wrapped in a CoreError so it can be passed to the Zome
/// and back to the Holochain Instance via wasm memory.
/// Follows the Error + ErrorKind pattern
/// Holds extra debugging info for indicating where in code ther error occured.
#[derive(Clone, Debug, Serialize, Deserialize, DefaultJson, PartialEq, Eq, Hash)]
pub struct CoreError {
    pub kind: HolochainError,
    pub file: String,
    pub line: String,
    // TODO #395 - Add advance error debugging info
    // pub stack_trace: Backtrace
}

// Error trait by using the inner Error
impl Error for CoreError {
    fn description(&self) -> &str {
        self.kind.description()
    }
    fn cause(&self) -> Option<&Error> {
        self.kind.cause()
    }
}
impl CoreError {
    pub fn new(hc_err: HolochainError) -> Self {
        CoreError {
            kind: hc_err,
            file: String::new(),
            line: String::new(),
        }
    }

    // TODO - get the u32 error code from a CoreError
    //    pub fn code(&self) -> u32 {
    //        u32::from(self.kind.code()) << 16 as u32
    //    }
}

impl ::std::convert::TryFrom<ZomeApiInternalResult> for CoreError {
    type Error = HolochainError;
    fn try_from(zome_api_internal_result: ZomeApiInternalResult) -> Result<Self, Self::Error> {
        if zome_api_internal_result.ok {
            Err(HolochainError::ErrorGeneric(
                "Attempted to deserialize CoreError from a non-error ZomeApiInternalResult".into(),
            ))
        } else {
            CoreError::try_from(JsonString::from(zome_api_internal_result.error))
        }
    }
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Holochain Core error: {}\n  --> {}:{}\n",
            self.description(),
            self.file,
            self.line,
        )
    }
}

//--------------------------------------------------------------------------------------------------
// HolochainError
//--------------------------------------------------------------------------------------------------

/// TODO rename to CoreErrorKind
/// Enum holding all Holochain Core errors
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, DefaultJson, Hash)]
pub enum HolochainError {
    ErrorGeneric(String),
    NotImplemented,
    LoggingError,
    DnaMissing,
    Dna(DnaError),
    IoError(String),
    SerializationError(String),
    InvalidOperationOnSysEntry,
    DoesNotHaveCapabilityToken,
    ValidationFailed(String),
    Ribosome(RibosomeErrorCode),
    RibosomeFailed(String),
    ConfigError(String),
}

pub type HcResult<T> = Result<T, HolochainError>;

impl HolochainError {
    pub fn new(msg: &str) -> HolochainError {
        HolochainError::ErrorGeneric(msg.to_string())
    }
}

impl fmt::Display for HolochainError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.description())
    }
}

impl Error for HolochainError {
    fn description(&self) -> &str {
        match self {
            ErrorGeneric(err_msg) => &err_msg,
            NotImplemented => "not implemented",
            LoggingError => "logging failed",
            DnaMissing => "DNA is missing",
            Dna(dna_err) => dna_err.description(),
            IoError(err_msg) => &err_msg,
            SerializationError(err_msg) => &err_msg,
            InvalidOperationOnSysEntry => "operation cannot be done on a system entry type",
            DoesNotHaveCapabilityToken => "Caller does not have Capability to make that call",
            ValidationFailed(fail_msg) => &fail_msg,
            Ribosome(err_code) => err_code.as_str(),
            RibosomeFailed(fail_msg) => &fail_msg,
            ConfigError(err_msg) => &err_msg,
        }
    }
}

impl From<HolochainError> for String {
    fn from(holochain_error: HolochainError) -> Self {
        holochain_error.to_string()
    }
}

/// standard strings for std io errors
fn reason_for_io_error(error: &IoError) -> String {
    match error.kind() {
        io::ErrorKind::InvalidData => format!("contains invalid data: {}", error),
        io::ErrorKind::PermissionDenied => format!("missing permissions to read: {}", error),
        _ => format!("unexpected error: {}", error),
    }
}

impl From<IoError> for HolochainError {
    fn from(error: IoError) -> Self {
        HolochainError::IoError(reason_for_io_error(&error))
    }
}

impl From<SerdeError> for HolochainError {
    fn from(error: SerdeError) -> Self {
        HolochainError::SerializationError(error.to_string())
    }
}

impl From<FutureCanceled> for HolochainError {
    fn from(_: FutureCanceled) -> Self {
        HolochainError::ErrorGeneric("Failed future".to_string())
    }
}

#[derive(Serialize, Deserialize, Default, Debug, DefaultJson)]
pub struct ZomeApiInternalResult {
    pub ok: bool,
    pub value: String,
    pub error: String,
}

impl ZomeApiInternalResult {
    pub fn success<J: Into<JsonString>>(value: J) -> ZomeApiInternalResult {
        let json_string: JsonString = value.into();
        ZomeApiInternalResult {
            ok: true,
            value: json_string.into(),
            error: JsonString::null().into(),
        }
    }

    pub fn failure<J: Into<JsonString>>(value: J) -> ZomeApiInternalResult {
        let json_string: JsonString = value.into();
        ZomeApiInternalResult {
            ok: false,
            value: JsonString::null().into(),
            error: json_string.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // a test function that returns our error result
    fn raises_holochain_error(yes: bool) -> Result<(), HolochainError> {
        if yes {
            Err(HolochainError::new("borked"))
        } else {
            Ok(())
        }
    }

    #[test]
    /// test that we can convert an error to a string
    fn to_string() {
        let err = HolochainError::new("foo");
        assert_eq!("foo", err.to_string());
    }

    #[test]
    /// test that we can convert an error to valid JSON
    fn test_to_json() {
        let err = HolochainError::new("foo");
        assert_eq!(
            JsonString::from("{\"ErrorGeneric\":\"foo\"}"),
            JsonString::from(err),
        );
    }

    #[test]
    /// smoke test new errors
    fn can_instantiate() {
        let err = HolochainError::new("borked");

        assert_eq!(HolochainError::ErrorGeneric("borked".to_string()), err);
    }

    #[test]
    /// test errors as a result and destructuring
    fn can_raise_holochain_error() {
        let err = raises_holochain_error(true).expect_err("should return an error when yes=true");

        match err {
            HolochainError::ErrorGeneric(msg) => assert_eq!(msg, "borked"),
            _ => panic!("raises_holochain_error should return an ErrorGeneric"),
        };
    }

    #[test]
    /// test errors as a returned result
    fn can_return_result() {
        let result = raises_holochain_error(false);

        assert!(result.is_ok());
    }

    #[test]
    /// show Error implementation for HolochainError
    fn error_test() {
        for (input, output) in vec![
            (HolochainError::ErrorGeneric(String::from("foo")), "foo"),
            (HolochainError::NotImplemented, "not implemented"),
            (HolochainError::LoggingError, "logging failed"),
            (HolochainError::DnaMissing, "DNA is missing"),
            (HolochainError::ConfigError(String::from("foo")), "foo"),
            (
                HolochainError::Dna(DnaError::ZomeNotFound(String::from("foo"))),
                "foo",
            ),
            (
                HolochainError::Dna(DnaError::CapabilityNotFound(String::from("foo"))),
                "foo",
            ),
            (
                HolochainError::Dna(DnaError::ZomeFunctionNotFound(String::from("foo"))),
                "foo",
            ),
            (HolochainError::IoError(String::from("foo")), "foo"),
            (
                HolochainError::SerializationError(String::from("foo")),
                "foo",
            ),
            (
                HolochainError::InvalidOperationOnSysEntry,
                "operation cannot be done on a system entry type",
            ),
            (
                HolochainError::DoesNotHaveCapabilityToken,
                "Caller does not have Capability to make that call",
            ),
        ] {
            assert_eq!(output, input.description());
        }
    }

    #[test]
    fn core_error_to_string() {
        let error =
            HolochainError::ErrorGeneric("This is a unit test error description".to_string());
        let report = CoreError {
            kind: error.clone(),
            file: file!().to_string(),
            line: line!().to_string(),
        };

        assert_ne!(
            report.to_string(),
            CoreError {
                kind: error,
                file: file!().to_string(),
                line: line!().to_string(),
            }.to_string(),
        );
    }
}
