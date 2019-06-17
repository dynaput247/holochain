use holochain_core_types::{error::HolochainError, json::*};

#[derive(Deserialize, Default, Clone, PartialEq, Eq, Hash, Debug, Serialize, DefaultJson)]
pub struct EncryptArgs {
    pub payload: String,
}