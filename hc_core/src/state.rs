use agent::AgentState;
use error::HolochainError;
use nucleus::NucleusState;
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Agent(::agent::Action),
    Network(::network::Action),
    Nucleus(::nucleus::Action),
}

#[derive(Clone, PartialEq, Debug)]
pub struct State {
    nucleus: Rc<NucleusState>,
    agent: Rc<AgentState>,
}

impl State {
    pub fn new() -> Self {
        State {
            nucleus: Rc::new(NucleusState::new()),
            agent: Rc::new(AgentState::new()),
        }
    }

    pub fn reduce(&mut self, action: &Action) -> Result<Self, HolochainError> {
        let nucleus = ::nucleus::reduce(Rc::clone(&self.nucleus), action)?;

        Ok(State {
            nucleus,
            agent: ::agent::reduce(Rc::clone(&self.agent), action),
        })
    }

    pub fn nucleus(&self) -> Rc<NucleusState> {
        Rc::clone(&self.nucleus)
    }

    pub fn agent(&self) -> Rc<AgentState> {
        Rc::clone(&self.agent)
    }
}

/*
TODO: write macro for DRY reducer functions
macro_rules! reducer {
    ($func_name:ident) => (
        fn reducer(old_state: Rc<$state_type>, action: &_Action) -> Rc<$state_type>  {
            // The `stringify!` macro converts an `ident` into a string.
            println!("You called {:?}()",
                     stringify!($func_name));
        }
    )
}
*/
