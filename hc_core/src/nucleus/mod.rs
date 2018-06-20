extern crate hc_dna;
use hc_dna::Dna;

pub mod fncall;
pub mod ribosome;

//use self::ribosome::*;
use state;
use std::rc::Rc;
use std::sync::mpsc::Sender;
use std::thread;

#[derive(Clone, Debug, PartialEq, Default)]
pub struct NucleusState {
    dna: Option<Dna>,
    initialized: bool,
}

impl NucleusState {
    pub fn new() -> Self {
        NucleusState {
            dna: None,
            initialized: false,
        }
    }

    pub fn dna(&self) -> Option<Dna> {
        self.dna.clone()
    }

    pub fn initialized(&self) -> bool {
        self.initialized
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct FunctionCall {
    capability: String,
    name: String,
    parameters: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FunctionResult {
    call: FunctionCall,
    result: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    InitApplication(DNA),
    ExecuteZomeFunction(FunctionCall),
    ZomeFunctionResult(FunctionResult),
    Call(fncall::Call),
}

pub fn reduce(
    old_state: Rc<NucleusState>,
    action: &state::Action,
    action_channel: &Sender<state::Action>,
) -> Rc<NucleusState> {
    match *action {
        state::Action::Nucleus(ref nucleus_action) => {
            let mut new_state: NucleusState = (*old_state).clone();
            match *nucleus_action {
                Action::InitApplication(ref dna) => {
                    if !new_state.initialized {
                        new_state.dna = Some(dna.clone());
                        new_state.initialized = true;
                    }
                }

                Action::ExecuteZomeFunction(ref fc) => {
                    let function_call = fc.clone();
                    let wasm = new_state.dna.clone().map(|d| {
                        d.wasm_for_zome_function(&function_call.capability, &function_call.name)
                    });
                    let action_channel = action_channel.clone();
                    thread::spawn(move || {
                        match ribosome::call(wasm.unwrap(), &function_call.name.clone()) {
                            Ok(runtime) => {
                                let mut result = FunctionResult {
                                    call: function_call,
                                    result: runtime.result.to_string(),
                                };

                                action_channel
                                    .send(state::Action::Nucleus(Action::ZomeFunctionResult(
                                        result,
                                    )))
                                    .expect("action channel to be open in reducer");
                            }

                            Err(ref _error) => {}
                        }
                    });
                }

                Action::ZomeFunctionResult(ref _result) => {}
                Action::Call(_) => {}
            }
            Rc::new(new_state)
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
        let state = Rc::new(NucleusState::new()); // initialize to bogus value
        let (sender, _receiver) = channel::<state::Action>();
        let reduced_state = reduce(state.clone(), &action, &sender.clone());
        assert!(reduced_state.initialized, true);

        // on second reduction it still works.
        let second_reduced_state = reduce(reduced_state.clone(), &action, &sender.clone());
        assert_eq!(second_reduced_state, reduced_state);
    }

    #[test]
    fn can_reduce_call_action() {
        let call = fncall::Call::new("bogusfn");
        let action = Nucleus(Call(call));
        let state = Rc::new(NucleusState::new()); // initialize to bogus value
        let (sender, _receiver) = channel::<state::Action>();
        let reduced_state = reduce(state.clone(), &action, &sender);
        assert_eq!(state, reduced_state);
    }
}
