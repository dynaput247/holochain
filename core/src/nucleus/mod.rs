extern crate holochain_dna;
extern crate snowflake;

use holochain_dna::Dna;

pub mod ribosome;

//use self::ribosome::*;
use error::HolochainError;
use state;
use std::collections::HashMap;
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;
use std::thread;
use instance::Observer;
use holochain_dna::zome::capabilities::RegisteredCapabilityNames;
use holochain_dna::zome::capabilities::RegisteredFunctionNames;

#[derive(Clone, Debug, PartialEq, Default)]
pub struct NucleusState {
    dna: Option<Dna>,
    initialized: bool,
    ribosome_calls: HashMap<FunctionCall, Option<Result<String, HolochainError>>>,
}

impl NucleusState {
    pub fn new() -> Self {
        NucleusState {
            dna: None,
            initialized: false,
            ribosome_calls: HashMap::new(),
        }
    }

    pub fn dna(&self) -> Option<Dna> {
        self.dna.clone()
    }
    pub fn initialized(&self) -> bool {
        self.initialized
    }
    pub fn ribosome_call_result(
        &self,
        function_call: &FunctionCall,
    ) -> Option<Result<String, HolochainError>> {
        match self.ribosome_calls.get(function_call) {
            None => None,
            Some(value) => value.clone(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FunctionCall {
    id: snowflake::ProcessUniqueId,
    pub zome: String,
    pub capability: String,
    pub function: String,
    pub parameters: String,
}

impl FunctionCall {
    pub fn new<S>(zome: S, capability: S, function: S, parameters: S) -> Self
    where
        S: Into<String>,
    {
        FunctionCall {
            id: snowflake::ProcessUniqueId::new(),
            zome: zome.into(),
            capability: capability.into(),
            function: function.into(),
            parameters: parameters.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntrySubmission {
    pub zome_name: String,
    pub type_name: String,
    pub content:   String,
}

impl EntrySubmission {
    pub fn new<S>(zome_name: S, type_name: S, content: S) -> Self
        where
          S: Into<String>,
    {
        EntrySubmission {
            zome_name: zome_name.into(),
            type_name: type_name.into(),
            content: content.into(),
        }
    }
}


/// Dispatch ExecuteZoneFunction to Instance and block until call has finished.
/// for test only??
pub fn call_and_wait_for_result(
    call: FunctionCall,
    instance: &mut super::instance::Instance)
  -> Result<String, HolochainError>
{
    let call_action = super::state::Action::Nucleus(Action::ExecuteZomeFunction(call.clone()));

    // Dispatch action with observer closure that waits for a result in the state
    let (sender, receiver) = channel();
    instance.dispatch_with_observer(call_action, move |state: &super::state::State| {
        if let Some(result) = state.nucleus().ribosome_call_result(&call) {
            sender
                .send(result.clone())
                .expect("local channel to be open");
            true
        } else {
            false
        }
    });

    // Block until we got that result through the channel:
    receiver.recv().expect("local channel to work")
}

#[derive(Clone, Debug, PartialEq)]
pub struct FunctionResult {
    call: FunctionCall,
    result: Result<String, HolochainError>,
}

impl FunctionResult {
    fn new(call: FunctionCall, result: Result<String, HolochainError>) -> Self {
        FunctionResult { call, result }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    InitApplication(Dna),
    ExecuteZomeFunction(FunctionCall),
    ReturnZomeFunctionResult(FunctionResult),
    ValidateEntry(EntrySubmission),
}

use ::instance::DISPATCH_WITHOUT_CHANNELS;

/// Reduce state of Nucleus according to action.
/// Note: Can't block when dispatching action here because we are inside the reduce's mutex
pub fn reduce(
    old_state: Arc<NucleusState>,
    action: &state::Action,
    action_channel: &Sender<state::ActionWrapper>,
    observer_channel: &Sender<Observer>)
-> Arc<NucleusState>
{
    match *action {
        state::Action::Nucleus(ref nucleus_action) => {
            let mut new_state: NucleusState = (*old_state).clone();
            match *nucleus_action {

                // Initialize Nucleus
                Action::InitApplication(ref dna) => {
                    if !new_state.initialized {

                        // Set DNA
                        new_state.dna = Some(dna.clone());

                        //  Call each Zome's genesis() with ExecuteZomeFunction Action

                        for zome in dna.clone().zomes {
                            // Make ExecuteZomeFunction Action
                            let call = FunctionCall::new(
                                zome.name,
                                RegisteredCapabilityNames::LifeCycle.as_str().to_string(),
                                RegisteredFunctionNames::Genesis.as_str().to_string(),
                                "".to_string(),
                            );
                            let action = super::state::Action::Nucleus(Action::ExecuteZomeFunction(call));



                            // Dispatch Action with Observer so it can finish asynchronously (outside of mutex)
                            // Wrap Action
                            let wrapper = ::state::ActionWrapper::new(action);
                            let wrapper_clone = wrapper.clone();

                            // Create observer
                            // Done when action is part of state's history
                            let closure = move |state: &::state::State| {
                                if state.history.contains(&wrapper_clone) {
                                    new_state.initialized = true;
                                    true
                                } else {
                                    false
                                }
                            };
                            let observer = Observer {
                                sensor: Box::new(closure),
                                done: false,
                            };

                            // Send observer to instance
                            observer_channel
                              .send(observer)
                              .unwrap_or_else(|_| panic!(DISPATCH_WITHOUT_CHANNELS));

                            // Send action to instance
                            action_channel
                              .send(wrapper)
                              .unwrap_or_else(|_| panic!(DISPATCH_WITHOUT_CHANNELS));

                            // TODO - Have one 'initialized' boolean per Zome because init can fail mid step
                            // Maybe Zome's should have own states and reduce() ?
                        }
                        // new_state.initialized = true;
                    }
                }

                // Execute an exposed Zome function in a seperate thread and send the result in
                // a ReturnZomeFunctionResult Action on success or failure
                Action::ExecuteZomeFunction(ref fc) => {
                    let function_call = fc.clone();
                    let mut zome_capability_found = false;
                    if let Some(ref dna) = new_state.dna {
                        if let Some(ref wasm) =
                            dna.get_wasm_for_capability(&fc.zome, &fc.capability)
                        {
                            new_state.ribosome_calls.insert(fc.clone(), None);

                            let action_channel = action_channel.clone();
                            let tx_observer = observer_channel.clone();
                            let code = wasm.code.clone();
                            thread::spawn(move || {
                                let result: FunctionResult;
                                match ribosome::call(&action_channel, &tx_observer, code, &function_call.function.clone()) {
                                    Ok(runtime) => {
                                        result = FunctionResult::new(
                                            function_call,
                                            Ok(runtime.result.to_string()),
                                        );
                                    }

                                    Err(ref error) => {
                                        result = FunctionResult::new(
                                            function_call,
                                            Err(HolochainError::ErrorGeneric(format!("{}", error))),
                                        );
                                    }
                                }

                                action_channel
                                    .send(state::ActionWrapper::new(state::Action::Nucleus(
                                        Action::ReturnZomeFunctionResult(result),
                                    )))
                                    .expect("action channel to be open in reducer");
                            });
                            zome_capability_found = true;
                        }
                    }
                    if !zome_capability_found {
                        let result = FunctionResult::new(
                            fc.clone(),
                            Err(HolochainError::ErrorGeneric(format!(
                                "Zome or capability not found {}/{}",
                                &fc.zome, &fc.capability
                            ))),
                        );
                        action_channel
                            .send(state::ActionWrapper::new(state::Action::Nucleus(
                                Action::ReturnZomeFunctionResult(result),
                            )))
                            .expect("action channel to be open in reducer");
                    }
                }

                // Store the Result in the ribosome_calls hashmap
                Action::ReturnZomeFunctionResult(ref result) => {
                    new_state
                        .ribosome_calls
                        .insert(result.call.clone(), Some(result.result.clone()));
                }

              // Validate an Entry by calling its validation function
              Action::ValidateEntry(ref es) =>
              {
                  println!("NucleusState::Commit: Entry = {}", es.content);
                  let mut _has_entry_type = false;

                  // must have entry_type
                  if let Some(ref dna) = new_state.dna
                  {
                      if let Some(ref _wasm) = dna.get_validation_bytecode_for_entry_type(&es.zome_name, &es.type_name)
                      {
                          // FIXME DDD
                          // Do same thing as Action::ExecuteZomeFunction
                          _has_entry_type = true;
                      }
                  }


              }

            }
            Arc::new(new_state)
        }
        _ => old_state,
    }
}

#[cfg(test)]
mod tests {
    use super::super::nucleus::Action::*;
    use super::super::state::Action::*;
    use super::*;
    use std::sync::mpsc::channel;

    #[test]
    fn can_instantiate_nucleus_state() {
        let state = NucleusState::new();
        assert_eq!(state.dna, None);
        assert_eq!(state.initialized, false);
    }

    #[test]
    fn can_reduce_initialize_action() {
        let dna = Dna::new();
        let action = Nucleus(InitApplication(dna));
        let state = Arc::new(NucleusState::new()); // initialize to bogus value
        let (sender, _receiver) = channel::<state::ActionWrapper>();
        let (tx_observer, _observer) = channel::<Observer>();
        let reduced_state = reduce(state.clone(), &action, &sender.clone(), &tx_observer.clone());
        assert!(reduced_state.initialized, true);

        // on second reduction it still works.
        let second_reduced_state = reduce(reduced_state.clone(), &action, &sender.clone(), &tx_observer.clone());
        assert_eq!(second_reduced_state, reduced_state);
    }

    #[test]
    fn can_reduce_execfn_action() {
        let call = FunctionCall::new(
            "myZome".to_string(),
            "public".to_string(),
            "bogusfn".to_string(),
            "".to_string(),
        );

        let action = Nucleus(ExecuteZomeFunction(call));
        let state = Arc::new(NucleusState::new()); // initialize to bogus value
        let (sender, _receiver) = channel::<state::ActionWrapper>();
        let (tx_observer, _observer) = channel::<Observer>();
        let reduced_state = reduce(state.clone(), &action, &sender, &tx_observer);
        assert_eq!(state, reduced_state);
    }
}
