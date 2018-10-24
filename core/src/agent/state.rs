use action::{Action, ActionWrapper, AgentReduceFn};
use agent::chain_store::ChainStore;
use context::Context;
use holochain_cas_implementations::cas::file::FilesystemStorage;
use holochain_core_types::{
    cas::{
        content::{Address, AddressableContent},
        storage::ContentAddressableStorage,
    },
    chain_header::ChainHeader,
    eav::{EntityAttributeValue, EntityAttributeValueStorage},
    entry::Entry,
    error::HolochainError,
    json::ToJson,
    keys::Keys,
    signature::Signature,
    time::Iso8601,
};

use serde::ser::{Serialize, Serializer, SerializeStruct};
use serde::de::{self, Deserialize, Deserializer, Visitor, MapAccess};
use std::{collections::HashMap, sync::Arc};
use std::fmt;


/// The state-slice for the Agent.
/// Holds the agent's source chain and keys.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentState {
    keys: Option<Keys>,
    /// every action and the result of that action
    // @TODO this will blow up memory, implement as some kind of dropping/FIFO with a limit?
    // @see https://github.com/holochain/holochain-rust/issues/166
    actions: HashMap<ActionWrapper, ActionResponse>,
    chain: ChainStore<FilesystemStorage>,
    top_chain_header: Option<ChainHeader>,
}

impl Serialize for AgentState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // 3 is the number of fields in the struct.
        let mut state = serializer.serialize_struct("AgentState", 2)?;
        state.serialize_field("chain_store", &self.chain)?;
        state.serialize_field("top_chain_header", &self.top_chain_header)?;
        state.end()
    }
}

struct AgentVisitor;
impl<'de> Visitor<'de> for AgentVisitor
{
    // The type that our Visitor is going to produce.
    type Value = AgentState;

    // Format a message stating what data this Visitor expects to receive.
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a very special map")
    }

    // Deserialize MyMap from an abstract "map" provided by the
    // Deserializer. The MapAccess input is a callback provided by
    // the Deserializer to let us see each entry in the map.
    fn visit_map<M>(self, mut access: M) -> Result<AgentState, M::Error>
    where
        M: MapAccess<'de>,
    {

    
    
        let chain : (String,ChainStore<FilesystemStorage>) = access.next_entry()?.expect("chain should be present");
        let top_chain_header : (String,ChainHeader) = access.next_entry()?.expect("Chain header should be present");
        let mut agent = AgentState::new(chain.1);
        agent.top_chain_header = Some(top_chain_header.1);
        Ok(agent)
    }
}

impl<'de> Deserialize<'de> for AgentState
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Instantiate our Visitor and ask the Deserializer to drive
        // it over the input data, resulting in an instance of MyMap.
        deserializer.deserialize_map(AgentVisitor)
    }
}

impl AgentState {
    /// builds a new, empty AgentState
    pub fn new(chain: ChainStore<FilesystemStorage>) -> AgentState {
        AgentState {
            keys: None,
            actions: HashMap::new(),
            chain,
            top_chain_header: None,
        }
    }

    /// getter for a copy of self.keys
    pub fn keys(&self) -> Option<Keys> {
        self.keys.clone()
    }

    /// getter for a copy of self.actions
    /// uniquely maps action executions to the result of the action
    pub fn actions(&self) -> HashMap<ActionWrapper, ActionResponse> {
        self.actions.clone()
    }

    pub fn chain(&self) -> ChainStore<FilesystemStorage> {
        self.chain.clone()
    }

    pub fn top_chain_header(&self) -> Option<ChainHeader> {
        self.top_chain_header.clone()
    }
}

#[derive(Clone, Debug, PartialEq,Serialize,Deserialize)]
/// the agent's response to an action
/// stored alongside the action in AgentState::actions to provide a state history that observers
/// poll and retrieve
// @TODO abstract this to a standard trait
// @see https://github.com/holochain/holochain-rust/issues/196
pub enum ActionResponse {
    Commit(Result<Address, HolochainError>),
    GetEntry(Option<Entry>),
    GetLinks(Result<Vec<Address>, HolochainError>),
    LinkEntries(Result<Entry, HolochainError>),
}

impl ToJson for ActionResponse {
    fn to_json(&self) -> Result<String, HolochainError> {
        match self {
            ActionResponse::Commit(result) => match result {
                Ok(entry_address) => Ok(format!("{{\"address\":\"{}\"}}", entry_address)),
                Err(err) => Ok((*err).to_json()?),
            },
            ActionResponse::GetEntry(result) => match result {
                Some(entry) => Ok(entry.to_json()?),
                None => Ok("".to_string()),
            },
            ActionResponse::GetLinks(result) => match result {
                Ok(hash_list) => Ok(json!(hash_list).to_string()),
                Err(err) => Ok((*err).to_json()?),
            },
            ActionResponse::LinkEntries(result) => match result {
                Ok(entry) => Ok(format!("{{\"address\":\"{}\"}}", entry.address())),
                Err(err) => Ok((*err).to_json()?),
            },
        }
    }
}

pub fn create_new_chain_header(entry: &Entry, agent_state: &AgentState) -> ChainHeader {
    ChainHeader::new(
        &entry.entry_type(),
        &entry.address(),
        // @TODO signatures
        &Signature::from(""),
        &agent_state
            .top_chain_header
            .clone()
            .and_then(|chain_header| Some(chain_header.address())),
        &agent_state
            .chain()
            .iter_type(&agent_state.top_chain_header, &entry.entry_type())
            .nth(0)
            .and_then(|chain_header| Some(chain_header.address())),
        // @TODO timestamp
        &Iso8601::from(""),
    )
}

/// Do a Commit Action against an agent state.
/// Intended for use inside the reducer, isolated for unit testing.
/// callback checks (e.g. validate_commit) happen elsewhere because callback functions cause
/// action reduction to hang
/// @TODO is there a way to reduce that doesn't block indefinitely on callback fns?
/// @see https://github.com/holochain/holochain-rust/issues/222
fn reduce_commit_entry(
    _context: Arc<Context>,
    state: &mut AgentState,
    action_wrapper: &ActionWrapper,
) {
    let action = action_wrapper.action();
    let entry = unwrap_to!(action => Action::Commit);
    let chain_header = create_new_chain_header(&entry, state);

    fn response(
        context: Arc<Context>,
        state: &mut AgentState,
        entry: &Entry,
        chain_header: &ChainHeader,
    ) -> Result<Address, HolochainError> {
        state.chain.content_storage().add(entry)?;
        state.chain.content_storage().add(chain_header)?;
        let eav_store = &mut (*context).eav_storage.clone();
        let eav = EntityAttributeValue::new(
            &chain_header.address(),
            &String::from("chain-header"),
            &entry.address(),
        );
        eav_store.add_eav(&eav)?;
        Ok(entry.address())
    }
    let result = response(_context, state, &entry, &chain_header);
    state.top_chain_header = Some(chain_header);

    state
        .actions
        .insert(action_wrapper.clone(), ActionResponse::Commit(result));
}

/// do a get action against an agent state
/// intended for use inside the reducer, isolated for unit testing
fn reduce_get_entry(
    _context: Arc<Context>,
    state: &mut AgentState,
    action_wrapper: &ActionWrapper,
) {
    let action = action_wrapper.action();
    let address = unwrap_to!(action => Action::GetEntry);
    let result = state
        .chain()
        .content_storage()
        .fetch(&address)
        .expect("could not fetch from CAS");
    // @TODO if the get fails local, do a network get
    // @see https://github.com/holochain/holochain-rust/issues/167

    state.actions.insert(
        action_wrapper.clone(),
        ActionResponse::GetEntry(result.clone()),
    );
}

/// maps incoming action to the correct handler
fn resolve_reducer(action_wrapper: &ActionWrapper) -> Option<AgentReduceFn> {
    match action_wrapper.action() {
        Action::Commit(_) => Some(reduce_commit_entry),
        Action::GetEntry(_) => Some(reduce_get_entry),
        _ => None,
    }
}

/// Reduce Agent's state according to provided Action
pub fn reduce(
    context: Arc<Context>,
    old_state: Arc<AgentState>,
    action_wrapper: &ActionWrapper,
) -> Arc<AgentState> {
    let handler = resolve_reducer(action_wrapper);
    match handler {
        Some(f) => {
            let mut new_state: AgentState = (*old_state).clone();
            f(context, &mut new_state, &action_wrapper);
            Arc::new(new_state)
        }
        None => old_state,
    }
}

#[cfg(test)]
pub mod tests {
    extern crate tempfile;
    use super::{reduce_commit_entry, reduce_get_entry, ActionResponse, AgentState};
    use action::tests::{test_action_wrapper_commit, test_action_wrapper_get};
    use agent::chain_store::tests::test_chain_store;
    use holochain_core_types::{
        cas::content::AddressableContent,
        entry::{test_entry, test_entry_address},
        error::HolochainError,
        json::ToJson,
    };
    use instance::tests::test_context;
    use std::{collections::HashMap, sync::Arc};
    use agent::chain_store::ChainStore;
    use serde_json;
    use holochain_cas_implementations::cas::file::FilesystemStorage;
    use self::tempfile::tempdir;

    /// dummy agent state
    pub fn test_agent_state() -> AgentState {
        AgentState::new(test_chain_store())
    }

    /// dummy action response for a successful commit as test_entry()
    pub fn test_action_response_commit() -> ActionResponse {
        ActionResponse::Commit(Ok(test_entry_address()))
    }

    /// dummy action response for a successful get as test_entry()
    pub fn test_action_response_get() -> ActionResponse {
        ActionResponse::GetEntry(Some(test_entry()))
    }

    #[test]
    /// smoke test for building a new AgentState
    fn agent_state_new() {
        test_agent_state();
    }

    #[test]
    /// test for the agent state keys getter
    fn agent_state_keys() {
        assert_eq!(None, test_agent_state().keys());
    }

    #[test]
    /// test for the agent state actions getter
    fn agent_state_actions() {
        assert_eq!(HashMap::new(), test_agent_state().actions());
    }

    #[test]
    /// test for reducing commit entry
    fn test_reduce_commit_entry() {
        let mut state = test_agent_state();
        let action_wrapper = test_action_wrapper_commit();

        reduce_commit_entry(test_context("bob"), &mut state, &action_wrapper);

        assert_eq!(
            state.actions().get(&action_wrapper),
            Some(&test_action_response_commit()),
        );
    }

    #[test]
    /// test for reducing get entry
    fn test_reduce_get_entry() {
        let mut state = test_agent_state();
        let context = test_context("foo");

        let aw1 = test_action_wrapper_get();
        reduce_get_entry(Arc::clone(&context), &mut state, &aw1);

        // nothing has been committed so the get must be None
        assert_eq!(
            state.actions().get(&aw1),
            Some(&ActionResponse::GetEntry(None)),
        );

        // do a round trip
        reduce_commit_entry(
            Arc::clone(&context),
            &mut state,
            &test_action_wrapper_commit(),
        );

        let aw2 = test_action_wrapper_get();
        reduce_get_entry(Arc::clone(&context), &mut state, &aw2);

        assert_eq!(state.actions().get(&aw2), Some(&test_action_response_get()),);
    }

    #[test]
    /// test response to json
    fn test_commit_response_to_json() {
        assert_eq!(
            format!("{{\"address\":\"{}\"}}", test_entry_address()),
            ActionResponse::Commit(Ok(test_entry_address()))
                .to_json()
                .unwrap(),
        );
        assert_eq!(
            "{\"error\":\"some error\"}",
            ActionResponse::Commit(Err(HolochainError::new("some error")))
                .to_json()
                .unwrap(),
        );
    }

    #[test]
    fn test_get_response_to_json() {
        assert_eq!(
            "{\"value\":\"test entry value\",\"entry_type\":{\"App\":\"testEntryType\"}}",
            ActionResponse::GetEntry(Some(test_entry().clone()))
                .to_json()
                .unwrap(),
        );
        assert_eq!("", ActionResponse::GetEntry(None).to_json().unwrap());
    }

    #[test]
    fn test_get_links_response_to_json() {
        assert_eq!(
            format!("[\"{}\"]", test_entry_address()),
            ActionResponse::GetLinks(Ok(vec![test_entry().address()]))
                .to_json()
                .unwrap(),
        );
        assert_eq!(
            "{\"error\":\"some error\"}",
            ActionResponse::GetLinks(Err(HolochainError::new("some error")))
                .to_json()
                .unwrap(),
        );
    }

    #[test]
    pub fn serialize_round_trip_agent_state()
    {
        let agent = test_agent_state();
        let json = serde_json::to_string(&agent).unwrap();
        let agent : AgentState = serde_json::from_str(&json).unwrap();
        println!("json encrypted{:}",json);
    }

    #[test]
    fn test_link_entries_response_to_json() {
        assert_eq!(
            format!("{{\"address\":\"{}\"}}", test_entry_address()),
            ActionResponse::LinkEntries(Ok(test_entry()))
                .to_json()
                .unwrap(),
        );
        assert_eq!(
            "{\"error\":\"some error\"}",
            ActionResponse::LinkEntries(Err(HolochainError::new("some error")))
                .to_json()
                .unwrap(),
        );
    }
}
