//! holochain_core_types::dna::zome is a set of structs for working with holochain dna.

use crate::{
    dna::{
        bridges::{Bridge, BridgePresence},
        wasm::DnaWasm,
    },
    entry::entry_type::EntryType,
    error::HolochainError,
    json::JsonString,
};
use dna::{
    capabilities,
    entry_types::{self, deserialize_entry_types, serialize_entry_types, EntryTypeDef},
};
use std::collections::BTreeMap;

/// Enum for "zome" "config" "error_handling" property.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Hash)]
pub enum ErrorHandling {
    #[serde(rename = "throw-errors")]
    ThrowErrors,
}

impl Default for ErrorHandling {
    /// Default zome config error_handling is "throw-errors"
    fn default() -> Self {
        ErrorHandling::ThrowErrors
    }
}

/// Represents the "config" object on a "zome".
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Hash)]
pub struct Config {
    /// How errors should be handled within this zome.
    #[serde(default)]
    pub error_handling: ErrorHandling,
}

impl Default for Config {
    /// Provide defaults for the "zome" "config" object.
    fn default() -> Self {
        Config {
            error_handling: ErrorHandling::ThrowErrors,
        }
    }
}

impl Config {
    /// Allow sane defaults for `Config::new()`.
    pub fn new() -> Self {
        Default::default()
    }
}

pub type ZomeEntryTypes = BTreeMap<EntryType, EntryTypeDef>;
pub type ZomeCapabilities = BTreeMap<String, capabilities::Capability>;

/// Represents an individual "zome".
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, DefaultJson)]
pub struct Zome {
    /// A description of this zome.
    #[serde(default)]
    pub description: String,

    /// Configuration associated with this zome.
    /// Note, this should perhaps be a more free-form serde_json::Value,
    /// "throw-errors" may not make sense for wasm, or other ribosome types.
    #[serde(default)]
    pub config: Config,

    /// An array of entry_types associated with this zome.
    #[serde(default)]
    #[serde(serialize_with = "serialize_entry_types")]
    #[serde(deserialize_with = "deserialize_entry_types")]
    pub entry_types: ZomeEntryTypes,

    /// An array of capabilities associated with this zome.
    #[serde(default)]
    pub capabilities: ZomeCapabilities,

    /// Validation code for this entry_type.
    #[serde(default)]
    pub code: DnaWasm,

    /// A list of bridges to other DNAs that this DNA can use or depends on.
    pub bridges: Option<Vec<Bridge>>,
}

impl Eq for Zome {}

impl Default for Zome {
    /// Provide defaults for an individual "zome".
    fn default() -> Self {
        Zome {
            description: String::new(),
            config: Config::new(),
            entry_types: BTreeMap::new(),
            capabilities: BTreeMap::new(),
            code: DnaWasm::new(),
            bridges: None,
        }
    }
}

impl Zome {
    /// Allow sane defaults for `Zome::new()`.
    pub fn new(
        description: &str,
        config: &Config,
        entry_types: &BTreeMap<EntryType, entry_types::EntryTypeDef>,
        capabilities: &BTreeMap<String, capabilities::Capability>,
        code: &DnaWasm,
    ) -> Zome {
        Zome {
            description: description.into(),
            config: config.clone(),
            entry_types: entry_types.to_owned(),
            capabilities: capabilities.to_owned(),
            code: code.clone(),
            bridges: None,
        }
    }

    pub fn get_required_bridges(&self) -> Vec<Bridge> {
        match self.bridges {
            None => Vec::new(),
            Some(ref bridges) => bridges
                .iter()
                .filter(|bridge| match bridge {
                    Bridge::Address(b) => b.presence == BridgePresence::Required,
                    Bridge::Trait(b) => b.presence == BridgePresence::Required,
                })
                .cloned()
                .collect(),
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::dna::zome::{entry_types::EntryTypeDef, Zome};
    use serde_json;
    use std::{collections::BTreeMap, convert::TryFrom};

    pub fn test_zome() -> Zome {
        Zome::default()
    }

    #[test]
    fn build_and_compare() {
        let fixture: Zome = serde_json::from_str(
            r#"{
                "description": "test",
                "config": {
                    "error_handling": "throw-errors"
                },
                "entry_types": {},
                "capabilities": {}
            }"#,
        )
        .unwrap();

        let mut zome = Zome::default();
        zome.description = String::from("test");
        zome.config.error_handling = ErrorHandling::ThrowErrors;

        assert_eq!(fixture, zome);
    }

    #[test]
    fn zome_json_test() {
        let mut entry_types = BTreeMap::new();
        entry_types.insert(EntryType::from("foo"), EntryTypeDef::new());
        let zome = Zome {
            entry_types,
            ..Default::default()
        };

        let expected = "{\"description\":\"\",\"config\":{\"error_handling\":\"throw-errors\"},\"entry_types\":{\"foo\":{\"description\":\"\",\"sharing\":\"public\",\"links_to\":[],\"linked_from\":[]}},\"capabilities\":{},\"code\":{\"code\":\"\"}}";

        assert_eq!(
            JsonString::from(expected.clone()),
            JsonString::from(zome.clone()),
        );

        assert_eq!(zome, Zome::try_from(JsonString::from(expected)).unwrap(),);
    }
}
