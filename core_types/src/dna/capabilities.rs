//! File holding all the structs for handling capabilities defined in DNA.

use crate::cas::content::Address;
use std::str::FromStr;

//--------------------------------------------------------------------------------------------------
// Reserved Capabilities names
//--------------------------------------------------------------------------------------------------

#[derive(Debug, PartialEq)]
/// Enumeration of all Capabilities known and used by HC Core
/// Enumeration converts to str
pub enum ReservedCapabilityNames {
    /// Development placeholder, no production fn should use MissingNo
    MissingNo,

    /// @TODO document what LifeCycle is
    /// @see https://github.com/holochain/holochain-rust/issues/204
    LifeCycle,

    /// @TODO document what Communication is
    /// @see https://github.com/holochain/holochain-rust/issues/204
    Communication,
}

impl FromStr for ReservedCapabilityNames {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "hc_lifecycle" => Ok(ReservedCapabilityNames::LifeCycle),
            "hc_web_gateway" => Ok(ReservedCapabilityNames::Communication),
            _ => Err("Cannot convert string to ReservedCapabilityNames"),
        }
    }
}

impl ReservedCapabilityNames {
    pub fn as_str(&self) -> &'static str {
        match *self {
            ReservedCapabilityNames::LifeCycle => "hc_lifecycle",
            ReservedCapabilityNames::Communication => "hc_web_gateway",
            ReservedCapabilityNames::MissingNo => "",
        }
    }
}

//--------------------------------------------------------------------------------------------------
// CapabilityCall
//--------------------------------------------------------------------------------------------------
/// a struct to hold the signature of the call
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, Hash)]
pub struct CallSignature {}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, Hash)]
pub struct CapabilityCall {
    pub cap_token: Address,
    pub caller: Option<Address>,
    pub signature: CallSignature,
}

impl CapabilityCall {
    pub fn new(token: Address, caller: Option<Address>) -> Self {
        CapabilityCall {
            cap_token: token,
            caller,
            signature: CallSignature {}, // FIXME
        }
    }
}

//--------------------------------------------------------------------------------------------------
// CapabilityType
//--------------------------------------------------------------------------------------------------

/// Enum for Zome CapabilityType.  Public capabilities require no token.  Transferable
/// capabilities require a token, but don't limit the capability to specific agent(s);
/// this functions like a password in that you can give the token to someone else and it works.
/// Assigned capabilities check the request's signature against the list of agents to which
/// the capability has been granted.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Hash)]
pub enum CapabilityType {
    #[serde(rename = "public")]
    Public,
    #[serde(rename = "transferable")]
    Transferable,
    #[serde(rename = "assigned")]
    Assigned,
}

/// Represents an individual capability definition in the Zomes's "capabilities" array
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Hash)]
pub struct Capability {
    /// capability type enum
    #[serde(rename = "type")]
    pub cap_type: CapabilityType,

    /// "functions" array
    #[serde(default)]
    pub functions: Vec<String>,
}

impl Default for Capability {
    /// Provide defaults for a Capability object
    fn default() -> Self {
        Capability {
            cap_type: CapabilityType::Assigned,
            functions: Vec::new(),
        }
    }
}

impl Capability {
    /// Capability Constructor
    pub fn new(cap_type: CapabilityType) -> Self {
        Capability {
            cap_type,
            functions: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    /// test that a canonical string can be created from ReservedCapabilityNames
    fn test_capabilities_new() {
        let cap = Capability::default();
        assert_eq!(cap.cap_type, CapabilityType::Assigned);
        let cap = Capability::new(CapabilityType::Public);
        assert_eq!(cap.cap_type, CapabilityType::Public);
        let cap = Capability::new(CapabilityType::Transferable);
        assert_eq!(cap.cap_type, CapabilityType::Transferable);
    }

    #[test]
    /// test that ReservedCapabilityNames can be created from a canonical string
    fn test_capabilities_from_str() {
        assert_eq!(
            Ok(ReservedCapabilityNames::LifeCycle),
            ReservedCapabilityNames::from_str("hc_lifecycle"),
        );
        assert_eq!(
            Ok(ReservedCapabilityNames::Communication),
            ReservedCapabilityNames::from_str("hc_web_gateway"),
        );
        assert_eq!(
            Err("Cannot convert string to ReservedCapabilityNames"),
            ReservedCapabilityNames::from_str("foo"),
        );
    }

    #[test]
    /// test that a canonical string can be created from ReservedCapabilityNames
    fn test_capabilities_as_str() {
        assert_eq!(ReservedCapabilityNames::LifeCycle.as_str(), "hc_lifecycle");
        assert_eq!(
            ReservedCapabilityNames::Communication.as_str(),
            "hc_web_gateway",
        );
    }

    #[test]
    fn test_capability_build_and_compare() {
        let fixture: Capability = serde_json::from_str(
            r#"{
                "type": "transferable",
                "functions": ["test"]
            }"#,
        )
        .unwrap();

        let mut cap = Capability::new(CapabilityType::Transferable);
        cap.functions.push(String::from("test"));
        assert_eq!(fixture, cap);
    }

}
