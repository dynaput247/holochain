use crate::{
    action::ActionWrapper,
    network::{actions::ActionResponse, direct_message::DirectMessage},
};
use holochain_core_types::{
    cas::content::Address, entry::Entry, error::HolochainError, validation::ValidationPackage,
};
use holochain_net::p2p_network::P2pNetwork;
use snowflake;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

type Actions = HashMap<ActionWrapper, ActionResponse>;

/// This represents the state of a get_entry network process:
/// None: process started, but no response yet from the network
/// Some(Err(_)): there was a problem at some point
/// Some(Ok(None)): no problem but also no entry -> it does not exist
/// Some(Ok(Some(entry))): we have it
type GetEntryResult = Option<Result<Option<Entry>, HolochainError>>;

/// This represents the state of a get_validation_package network process:
/// None: process started, but no response yet from the network
/// Some(Err(_)): there was a problem at some point
/// Some(Ok(None)): no error but also no validation package -> we seem to have asked the wrong
///   agent which actually should not happen. Something weird is going on.
/// Some(Ok(Some(entry))): we have it
type GetValidationPackageResult = Option<Result<Option<ValidationPackage>, HolochainError>>;

#[derive(Clone, Debug)]
pub struct NetworkState {
    /// every action and the result of that action
    // @TODO this will blow up memory, implement as some kind of dropping/FIFO with a limit?
    // @see https://github.com/holochain/holochain-rust/issues/166
    pub actions: Actions,
    pub network: Option<Arc<Mutex<P2pNetwork>>>,
    pub dna_hash: Option<String>,
    pub agent_id: Option<String>,
    pub get_entry_results: HashMap<Address, GetEntryResult>,
    pub get_validation_package_results: HashMap<Address, GetValidationPackageResult>,
    pub direct_message_connections: HashMap<String, DirectMessage>,
    id: snowflake::ProcessUniqueId,
}

impl PartialEq for NetworkState {
    fn eq(&self, other: &NetworkState) -> bool {
        self.id == other.id
    }
}

impl NetworkState {
    pub fn new() -> Self {
        NetworkState {
            actions: HashMap::new(),
            network: None,
            dna_hash: None,
            agent_id: None,
            get_entry_results: HashMap::new(),
            get_validation_package_results: HashMap::new(),
            direct_message_connections: HashMap::new(),
            id: snowflake::ProcessUniqueId::new(),
        }
    }

    pub fn actions(&self) -> Actions {
        self.actions.clone()
    }
}
