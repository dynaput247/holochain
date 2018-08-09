use action::{Action, ActionWrapper, Signal};
use agent::keys::Keys;
use chain::Chain;
use error::HolochainError;
use hash_table::{entry::Entry, memory::MemTable, pair::Pair};
use instance::Observer;
use std::{
    collections::HashMap,
    rc::Rc,
    sync::{mpsc::Sender, Arc},
};
use action::AgentReduceFn;

#[derive(Clone, Debug, PartialEq, Default)]
/// struct to track the internal state of an agent exposed to reducers/observers
pub struct AgentState {
    keys: Option<Keys>,
    // @TODO how should this work with chains/HTs?
    // @see https://github.com/holochain/holochain-rust/issues/137
    // @see https://github.com/holochain/holochain-rust/issues/135
    top_pair: Option<Pair>,
    /// every action and the result of that action
    // @TODO this will blow up memory, implement as some kind of dropping/FIFO with a limit?
    // @see https://github.com/holochain/holochain-rust/issues/166
    actions: HashMap<Action, ActionResponse>,
}

impl AgentState {
    /// builds a new, empty AgentState
    pub fn new() -> AgentState {
        AgentState {
            keys: None,
            top_pair: None,
            actions: HashMap::new(),
        }
    }

    /// getter for a copy of self.keys
    pub fn keys(&self) -> Option<Keys> {
        self.keys.clone()
    }

    /// getter for a copy of self.top_pair
    /// should be used with a source chain for validation/safety
    pub fn top_pair(&self) -> Option<Pair> {
        self.top_pair.clone()
    }

    /// getter for a copy of self.actions
    /// uniquely maps action executions to the result of the action
    pub fn actions(&self) -> HashMap<Action, ActionResponse> {
        self.actions.clone()
    }
}

#[derive(Clone, Debug, PartialEq)]
/// the agent's response to an action
/// stored alongside the action in AgentState::actions to provide a state history that observers
/// poll and retrieve
pub enum ActionResponse {
    Commit(Result<Pair, HolochainError>),
    Get(Option<Pair>),
}

impl ActionResponse {
    /// serialize data or error to JSON
    // @TODO implement this as a round tripping trait
    // @see https://github.com/holochain/holochain-rust/issues/193
    pub fn to_json(&self) -> String {
        match self {
            ActionResponse::Commit(result) => match result {
                Ok(pair) => format!("{{\"hash\":\"{}\"}}", pair.entry().key()),
                Err(err) => (*err).to_json(),
            },
            ActionResponse::Get(result) => match result {
                Some(pair) => pair.to_json(),
                None => "".to_string(),
            },
        }
    }
}

/// do a commit action against an agent state
/// intended for use inside the reducer, isolated for unit testing
/// lifecycle checks (e.g. validate_commit) happen elsewhere because lifecycle functions cause
/// action reduction to hang
/// @TODO is there a way to reduce that doesn't block indefinitely on lifecycle fns?
fn reduce_commit(
    state: &mut AgentState,
    action: &Action,
    _action_channel: &Sender<ActionWrapper>,
    _observer_channel: &Sender<Observer>,
) {
    let signal = action.signal();
    let entry = unwrap_to!(signal => Signal::Commit);

    // add entry to source chain
    // @TODO this does nothing!
    // it needs to get something stateless from the agent state that points to
    // something stateful that can handle an entire hash table (e.g. actor)
    // @see https://github.com/holochain/holochain-rust/issues/135
    // @see https://github.com/holochain/holochain-rust/issues/148
    let mut chain = Chain::new(Rc::new(MemTable::new()));

    state
        .actions
        .insert(action.clone(), ActionResponse::Commit(chain.push(&entry)));
}

/// do a get action against an agent state
/// intended for use inside the reducer, isolated for unit testing
fn reduce_get(
    state: &mut AgentState,
    action: &Action,
    _action_channel: &Sender<ActionWrapper>,
    _observer_channel: &Sender<Observer>,
) {
    let signal = action.signal();
    let key = unwrap_to!(signal => Signal::Get);

    // get pair from source chain
    // @TODO this does nothing!
    // it needs to get something stateless from the agent state that points to
    // something stateful that can handle an entire hash table (e.g. actor)
    // @see https://github.com/holochain/holochain-rust/issues/135
    // @see https://github.com/holochain/holochain-rust/issues/148

    // drop in a dummy entry for testing
    let mut chain = Chain::new(Rc::new(MemTable::new()));
    let e = Entry::new("testEntryType", "test entry content");
    chain.push(&e).unwrap();

    // @TODO if the get fails local, do a network get
    // @see https://github.com/holochain/holochain-rust/issues/167

    let result = chain.get_entry(&key).unwrap();
    state
        .actions
        .insert(action.clone(), ActionResponse::Get(result.clone()));
}

/// maps incoming action to the correct handler
fn resolve_reducer(
    action: &Action,
) -> Option<AgentReduceFn> {
    match action.signal() {
        Signal::Commit(_) => Some(reduce_commit),
        Signal::Get(_) => Some(reduce_get),
        _ => None,
    }
}

/// Reduce Agent's state according to provided Action
pub fn reduce(
    old_state: Arc<AgentState>,
    action: &Action,
    action_channel: &Sender<ActionWrapper>,
    observer_channel: &Sender<Observer>,
) -> Arc<AgentState> {
    let handler = resolve_reducer(action);
    match handler {
        Some(f) => {
            let mut new_state: AgentState = (*old_state).clone();
            f(&mut new_state, &action, action_channel, observer_channel);
            Arc::new(new_state)
        }
        None => old_state,
    }
}

#[cfg(test)]
pub mod tests {
    use super::{reduce_commit, reduce_get, ActionResponse, AgentState};
    use action::{tests::test_action_commit, Action, Signal};
    use hash::tests::test_hash;
    use hash_table::pair::tests::test_pair;
    use instance::tests::test_instance_blank;
    use std::collections::HashMap;

    /// dummy agent state
    pub fn test_agent_state() -> AgentState {
        AgentState::new()
    }

    /// dummy action response for a successful commit as test_pair()
    pub fn test_action_response_commit() -> ActionResponse {
        ActionResponse::Commit(Ok(test_pair()))
    }

    /// dummy action response for a successful get as test_pair()
    pub fn test_action_response_get() -> ActionResponse {
        ActionResponse::Get(Some(test_pair()))
    }

    /// dummy action for a get of test_hash()
    pub fn test_action_get() -> Action {
        Action::new(&Signal::Get(test_hash()))
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
    /// test for the agent state top pair getter
    fn agent_state_top_pair() {
        assert_eq!(None, test_agent_state().top_pair());
    }

    #[test]
    /// test for the agent state actions getter
    fn agent_state_actions() {
        assert_eq!(HashMap::new(), test_agent_state().actions());
    }

    #[test]
    /// test for action commit
    fn test_reduce_commit() {
        let mut state = test_agent_state();
        let action = test_action_commit();

        let instance = test_instance_blank();

        reduce_commit(
            &mut state,
            &action,
            &instance.action_channel().clone(),
            &instance.observer_channel().clone(),
        );

        assert_eq!(
            state.actions().get(&action),
            Some(&test_action_response_commit()),
        );
    }

    #[test]
    /// test for action get
    fn test_reduce_get() {
        let mut state = test_agent_state();
        let action = test_action_get();

        let instance = test_instance_blank();

        reduce_get(
            &mut state,
            &action,
            &instance.action_channel().clone(),
            &instance.observer_channel().clone(),
        );

        assert_eq!(
            state.actions().get(&action),
            Some(&test_action_response_get()),
        );
    }
}
