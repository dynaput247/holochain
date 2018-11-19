use action::ActionWrapper;
use agent::{
    chain_store::ChainStore,
    state::{AgentState, AgentStateSnapshot},
};
use context::Context;
use dht::dht_store::DhtStore;
use holochain_core_types::{
    cas::storage::ContentAddressableStorage,
    dna::Dna,
    entry::{entry_type::EntryType, Entry, SerializedEntry, ToEntry},
    error::{HcResult, HolochainError},
};
use nucleus::state::NucleusState;
use std::{
    collections::HashSet,
    convert::TryInto,
    sync::{Arc, RwLock},
};

/// The Store of the Holochain instance Object, according to Redux pattern.
/// It's composed of all sub-module's state slices.
/// To plug in a new module, its state slice needs to be added here.
#[derive(Clone, PartialEq, Debug)]
pub struct State {
    nucleus: Arc<NucleusState>,
    agent: Arc<AgentState>,
    dht: Arc<DhtStore>,
    // @TODO eventually drop stale history
    // @see https://github.com/holochain/holochain-rust/issues/166
    pub history: HashSet<ActionWrapper>,
}

impl State {
    pub fn new(context: Arc<Context>) -> Self {
        // @TODO file table
        // @see https://github.com/holochain/holochain-rust/pull/246

        let cas = &(*context).file_storage;
        let eav = context.eav_storage.clone();
        State {
            nucleus: Arc::new(NucleusState::new()),
            agent: Arc::new(AgentState::new(ChainStore::new(cas.clone()))),
            dht: Arc::new(DhtStore::new(cas.clone(), eav)),
            history: HashSet::new(),
        }
    }

    pub fn new_with_agent(context: Arc<Context>, agent_state: Arc<AgentState>) -> Self {
        // @TODO file table
        // @see https://github.com/holochain/holochain-rust/pull/246

        let cas = context.file_storage.clone();
        let eav = context.eav_storage.clone();

        fn get_dna(
            agent_state: &Arc<AgentState>,
            cas: Arc<RwLock<dyn ContentAddressableStorage>>,
        ) -> Result<Dna, HolochainError> {
            let dna_entry_header = agent_state
                .chain()
                .iter_type(&agent_state.top_chain_header(), &EntryType::Dna)
                .last()
                .ok_or(HolochainError::ErrorGeneric(
                    "No DNA entry found in source chain while creating state from agent"
                        .to_string(),
                ))?;
            let json = (*cas.read().unwrap()).fetch(dna_entry_header.entry_address())?;
            let serialized_entry: SerializedEntry = json.map(|e| e.try_into()).ok_or(
                HolochainError::ErrorGeneric(
                    "No DNA entry found in storage while creating state from agent".to_string(),
                ),
            )??;
            let entry: Entry = serialized_entry.into();
            Ok(Dna::from_entry(&entry))
        }

        let mut nucleus_state = NucleusState::new();
        nucleus_state.dna = get_dna(&agent_state, cas.clone()).ok();
        State {
            nucleus: Arc::new(nucleus_state),
            agent: agent_state,
            dht: Arc::new(DhtStore::new(cas.clone(), eav.clone())),
            history: HashSet::new(),
        }
    }

    pub fn reduce(&self, context: Arc<Context>, action_wrapper: ActionWrapper) -> Self {
        let mut new_state = State {
            nucleus: ::nucleus::reduce(
                Arc::clone(&context),
                Arc::clone(&self.nucleus),
                &action_wrapper,
            ),
            agent: ::agent::state::reduce(
                Arc::clone(&context),
                Arc::clone(&self.agent),
                &action_wrapper,
            ),
            dht: ::dht::dht_reducers::reduce(
                Arc::clone(&context),
                Arc::clone(&self.dht),
                &action_wrapper,
            ),
            history: self.history.clone(),
        };

        new_state.history.insert(action_wrapper);
        new_state
    }

    pub fn nucleus(&self) -> Arc<NucleusState> {
        Arc::clone(&self.nucleus)
    }

    pub fn agent(&self) -> Arc<AgentState> {
        Arc::clone(&self.agent)
    }

    pub fn dht(&self) -> Arc<DhtStore> {
        Arc::clone(&self.dht)
    }

    pub fn try_from_agent_snapshot(
        context: Arc<Context>,
        snapshot: AgentStateSnapshot,
    ) -> HcResult<State> {
        let agent_state = AgentState::new_with_top_chain_header(
            ChainStore::new(context.file_storage.clone()),
            snapshot.top_chain_header().clone(),
        );
        Ok(State::new_with_agent(
            context.clone(),
            Arc::new(agent_state),
        ))
    }
}

pub fn test_store(context: Arc<Context>) -> State {
    State::new(context)
}
