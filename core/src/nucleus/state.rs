use crate::{
    nucleus::{actions::initialize::Initialization, validation::ValidationResult, ZomeFnCall},
    scheduled_jobs::pending_validations::{PendingValidation, ValidatingWorkflow},
    state::State,
};
use holochain_core_types::{
    cas::content::{Address, AddressableContent, Content},
    dna::Dna,
    error::HolochainError,
    json::JsonString,
    validation::ValidationPackage,
};
use snowflake;
use std::{
    collections::HashMap,
    convert::TryFrom,
    fmt::{self, Debug, Formatter},
    ops::Deref,
    sync::Arc,
};
use wasmi::Module;

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, DefaultJson)]
pub enum NucleusStatus {
    New,
    Initializing,
    Initialized(Initialization),
    InitializationFailed(String),
}

impl Default for NucleusStatus {
    fn default() -> Self {
        NucleusStatus::New
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PendingValidationKey(String);
impl PendingValidationKey {
    pub fn new(address: Address, workflow: ValidatingWorkflow) -> Self {
        PendingValidationKey(format!("{}:{}", workflow, address))
    }
}

/// Wrapper around wasmi::Module since it does not implement Clone, Debug, PartialEq, Eq,
/// which are all needed to add it to the state below.
#[derive(Clone)]
pub struct ModuleArc(Arc<Module>);
impl ModuleArc {
    pub fn new(module: Module) -> Self {
        ModuleArc(Arc::new(module))
    }
}
impl PartialEq for ModuleArc {
    fn eq(&self, _other: &ModuleArc) -> bool {
        //*self == *other
        false
    }
}
impl Eq for ModuleArc {}
impl Deref for ModuleArc {
    type Target = Arc<Module>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl Debug for ModuleArc {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "ModuleMutex")
    }
}

/// The state-slice for the Nucleus.
/// Holds the dynamic parts of the DNA, i.e. zome calls and validation requests.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct NucleusState {
    // Persisted fields:
    pub status: NucleusStatus,
    pub pending_validations: HashMap<PendingValidationKey, PendingValidation>,

    // Transient fields:
    pub dna: Option<Dna>, //DNA is transient here because it is stored in the chain and gets
    //read from there when loading an instance/chain.

    /// WASM modules read from the DNA.
    /// Each Zome brings its own WASM binary that gets read and parsed into a module.
    /// This is a mapping from zome name to a pool (=vector) of according modules.
    pub wasm_modules: HashMap<String, ModuleArc>,

    // @TODO eventually drop stale calls
    // @see https://github.com/holochain/holochain-rust/issues/166
    // @TODO should this use the standard ActionWrapper/ActionResponse format?
    // @see https://github.com/holochain/holochain-rust/issues/196
    pub zome_calls: HashMap<ZomeFnCall, Option<Result<JsonString, HolochainError>>>,
    pub validation_results: HashMap<(snowflake::ProcessUniqueId, Address), ValidationResult>,
    pub validation_packages:
        HashMap<snowflake::ProcessUniqueId, Result<ValidationPackage, HolochainError>>,
}

impl NucleusState {
    pub fn new() -> Self {
        NucleusState {
            dna: None,
            wasm_modules: HashMap::new(),
            status: NucleusStatus::New,
            zome_calls: HashMap::new(),
            validation_results: HashMap::new(),
            validation_packages: HashMap::new(),
            pending_validations: HashMap::new(),
        }
    }

    pub fn zome_call_result(
        &self,
        zome_call: &ZomeFnCall,
    ) -> Option<Result<JsonString, HolochainError>> {
        self.zome_calls
            .get(zome_call)
            .and_then(|value| value.clone())
    }

    pub fn has_initialized(&self) -> bool {
        match self.status {
            NucleusStatus::Initialized(_) => true,
            _ => false,
        }
    }

    pub fn initialization(&self) -> Option<Initialization> {
        match self.status {
            NucleusStatus::Initialized(ref init) => Some(init.clone()),
            _ => None,
        }
    }

    pub fn has_initialization_failed(&self) -> bool {
        match self.status {
            NucleusStatus::InitializationFailed(_) => true,
            _ => false,
        }
    }

    // Getters
    pub fn dna(&self) -> Option<Dna> {
        self.dna.clone()
    }
    pub fn status(&self) -> NucleusStatus {
        self.status.clone()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, DefaultJson)]
pub struct NucleusStateSnapshot {
    pub status: NucleusStatus,
    pub pending_validations: HashMap<PendingValidationKey, PendingValidation>,
}

impl From<&State> for NucleusStateSnapshot {
    fn from(state: &State) -> Self {
        NucleusStateSnapshot {
            status: state.nucleus().status(),
            pending_validations: state.nucleus().pending_validations.clone(),
        }
    }
}

impl From<NucleusStateSnapshot> for NucleusState {
    fn from(snapshot: NucleusStateSnapshot) -> Self {
        NucleusState {
            dna: None,
            wasm_modules: HashMap::new(),
            status: snapshot.status,
            zome_calls: HashMap::new(),
            validation_results: HashMap::new(),
            validation_packages: HashMap::new(),
            pending_validations: snapshot.pending_validations,
        }
    }
}

pub static NUCLEUS_SNAPSHOT_ADDRESS: &'static str = "NucleusState";
impl AddressableContent for NucleusStateSnapshot {
    fn address(&self) -> Address {
        NUCLEUS_SNAPSHOT_ADDRESS.into()
    }

    fn content(&self) -> Content {
        self.to_owned().into()
    }

    fn try_from_content(content: &Content) -> Result<Self, HolochainError> {
        Self::try_from(content.to_owned())
    }
}

#[cfg(test)]
pub mod tests {

    use super::NucleusState;

    /// dummy nucleus state
    pub fn test_nucleus_state() -> NucleusState {
        NucleusState::new()
    }

}
