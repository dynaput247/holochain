/*!
hc_api provides a library for container applications to instantiate and run holochain applications.

# Examples

``` rust
extern crate hc_core;
extern crate hc_api;
extern crate hc_dna;
extern crate hc_agent;

use hc_api::*;
use hc_dna::Dna;
use hc_agent::Agent;

// instantiate a new app

// need to get to something like this:
//let dna = hc_dna::from_package_file("mydna.hcpkg");

// but for now:
let dna = Dna::new();
let agent = Agent::from_string("bob");
let mut hc = Holochain::new(dna,agent).unwrap();

// start up the app
hc.start().expect("couldn't start the app");

// call a function in the app
hc.call("some_fn");

// get the state
{
    let state = hc.state();

    // do some other stuff with the state here
    // ...
}

// stop the app
hc.stop().expect("couldn't stop the app");

```
*/

extern crate hc_agent;
extern crate hc_core;
extern crate hc_dna;

use hc_agent::Agent as HCAgent;
use hc_dna::Dna;

#[derive(Clone)]
pub struct Context {
    agent: HCAgent,
}

/// contains a Holochain application instance
#[derive(Clone)]
pub struct Holochain {
    instance: hc_core::instance::Instance,
    context: Context,
    active: bool,
}

use hc_core::error::HolochainError;
use hc_core::nucleus::fncall;
use hc_core::nucleus::Action::*;
use hc_core::state::Action::*;
use hc_core::state::State;

impl Holochain {
    pub fn new(dna: Dna, agent: HCAgent) -> Result<Self, HolochainError> {
        let mut instance = hc_core::instance::Instance::new();
        let action = Nucleus(InitApplication(dna.clone()));
        instance.dispatch(action);
        instance.consume_next_action()?;
        let app = Holochain {
            instance: instance,
            context: Context { agent: agent },
            active: false,
        };
        Ok(app)
    }

    pub fn call(&mut self, fn_name: &str) -> Result<(), HolochainError> {
        if !self.active {
            return Err(HolochainError::InstanceNotActive);
        }
        let call_data = fncall::Call::new(fn_name);
        let action = Nucleus(Call(call_data));
        self.instance.dispatch(action.clone());
        self.instance.consume_next_action()
    }

    pub fn active(&self) -> bool {
        self.active
    }

    pub fn start(&mut self) -> Result<(), HolochainError> {
        if self.active {
            return Err(HolochainError::InstanceActive);
        }
        self.active = true;
        Ok(())
    }

    pub fn stop(&mut self) -> Result<(), HolochainError> {
        if !self.active {
            return Err(HolochainError::InstanceNotActive);
        }
        self.active = false;
        Ok(())
    }

    pub fn state(&mut self) -> Result<&State, HolochainError> {
        Ok(self.instance.state())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_instantiate() {
        let dna = Dna::new();
        let agent = HCAgent::from_string("bob");
        let result = Holochain::new(dna.clone(), agent.clone());
        let hc = result.clone().unwrap();
        assert!(!hc.active);
        assert_eq!(hc.context.agent, agent);

        match result {
            Ok(hc) => {
                assert_eq!(hc.instance.state().nucleus().dna(), Some(dna));
            }
            Err(_) => assert!(false),
        };
    }

    #[test]
    fn can_start_and_stop() {
        let dna = Dna::new();
        let agent = HCAgent::from_string("bob");
        let mut hc = Holochain::new(dna.clone(), agent).unwrap();
        assert!(!hc.clone().active());

        // stop when not active returns error
        let result = hc.stop();
        match result {
            Err(HolochainError::InstanceNotActive) => assert!(true),
            Ok(_) => assert!(false),
            Err(_) => assert!(false),
        }

        let result = hc.start();
        match result {
            Ok(_) => assert!(true),
            Err(_) => assert!(false),
        }
        assert!(hc.active());

        // start when active returns error
        let result = hc.start();
        match result {
            Err(HolochainError::InstanceActive) => assert!(true),
            Ok(_) => assert!(false),
            Err(_) => assert!(false),
        }

        let result = hc.stop();
        match result {
            Ok(_) => assert!(true),
            Err(_) => assert!(false),
        }
        assert!(!hc.active());
    }

    #[test]
    fn can_call() {
        let dna = Dna::new();
        let agent = HCAgent::from_string("bob");
        let mut hc = Holochain::new(dna.clone(), agent).unwrap();
        let result = hc.call("bogusfn");
        match result {
            Err(HolochainError::InstanceNotActive) => assert!(true),
            Ok(_) => assert!(false),
            Err(_) => assert!(false),
        }

        hc.start().expect("couldn't start");

        // always returns not implemented error for now!
        let result = hc.call("bogusfn");
        match result {
            Err(HolochainError::NotImplemented) => assert!(true),
            Ok(_) => assert!(false),
            Err(_) => assert!(false),
        };
    }

    #[test]
    fn can_get_state() {
        let dna = Dna::new();
        let agent = HCAgent::from_string("bob");
        let mut hc = Holochain::new(dna.clone(), agent).unwrap();

        let result = hc.state();
        match result {
            Ok(state) => {
                assert_eq!(state.nucleus().dna(), Some(dna));
            }
            Err(_) => assert!(false),
        };
    }
}
