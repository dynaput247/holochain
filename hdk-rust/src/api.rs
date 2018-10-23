use error::{ZomeApiError, ZomeApiResult};
use globals::*;
pub use holochain_wasm_utils::api_serialization::validation::*;
use holochain_wasm_utils::{
    api_serialization::{
        commit::{CommitEntryArgs, CommitEntryResult},
        get_entry::{GetEntryArgs, GetEntryOptions, GetEntryResult},
    },
    holochain_core_types::hash::HashString,
    memory_allocation::*,
    memory_serialization::*,
};
use serde_json;

//--------------------------------------------------------------------------------------------------
// ZOME API GLOBAL VARIABLES
//--------------------------------------------------------------------------------------------------

lazy_static! {
  /// The `name` property as taken from the DNA.
  pub static ref DNA_NAME: &'static str = &GLOBALS.dna_name;

  /// The hash of the DNA the Zome is embedded within.
  /// This is often useful as a fixed value that is known by all
  /// participants running the DNA.
  pub static ref DNA_HASH: &'static HashString = &GLOBALS.dna_hash;

  /// The identity string used when the chain was first initialized.
  pub static ref AGENT_ID_STR: &'static str = &GLOBALS.agent_id_str;

  /// The hash of your public key.
  /// This is your node address on the DHT.
  /// It can be used for node-to-node messaging with `send` and `receive` functions.
  pub static ref AGENT_KEY_HASH: &'static HashString = &GLOBALS.agent_key_hash;

  /// The hash of the first identity entry on your chain (The second entry on your chain).
  /// This is your peer's identity on the DHT.
  pub static ref AGENT_INITIAL_HASH: &'static HashString = &GLOBALS.agent_initial_hash;

  #[doc(hidden)]
  /// The hash of the most recent identity entry that has been committed to your chain.
  /// Starts with the same value as AGENT_INITIAL_HASH.
  /// After a call to `update_agent` it will have the value of the hash of the newly committed identity entry.
  pub static ref AGENT_LATEST_HASH: &'static HashString = &GLOBALS.agent_latest_hash;
}

//--------------------------------------------------------------------------------------------------
// SYSTEM CONSTS
//--------------------------------------------------------------------------------------------------

// HC.Status
// WARNING keep in sync with CRUDStatus
bitflags! {
  pub struct EntryStatus: u8 {
    const LIVE     = 1 << 0;
    const REJECTED = 1 << 1;
    const DELETED  = 1 << 2;
    const MODIFIED = 1 << 3;
  }
}

// HC.GetMask
bitflags! {
  pub struct GetEntryMask: u8 {
    const ENTRY      = 1 << 0;
    const ENTRY_TYPE = 1 << 1;
    const SOURCES    = 1 << 2;
  }
}
// explicit `Default` implementation
impl Default for GetEntryMask {
    fn default() -> GetEntryMask {
        GetEntryMask::ENTRY
    }
}

// TODOs
//// HC.LinkAction
//pub enum LinkAction {
//    Add,
//    Delete,
//}
//
//// HC.PkgReq
//pub enum PkgRequest {
//    Chain,
//    ChainOption,
//    EntryTypes,
//}
//
//// HC.PkgReq.ChainOpt
//pub enum ChainOption {
//    None,
//    Headers,
//    Entries,
//    Full,
//}
//
//// HC.Bridge
//pub enum BridgeSide {
//    From,
//    To,
//}
//
//// HC.SysEntryType
//// WARNING Keep in sync with SystemEntryType in holochain-rust
//enum SystemEntryType {
//    Dna,
//    Agent,
//    Key,
//    Headers,
//    Deletion,
//}
//
//mod bundle_cancel {
//    // HC.BundleCancel.Reason
//    pub enum Reason {
//        UserCancel,
//        Timeout,
//    }
//    // HC.BundleCancel.Response
//    pub enum Response {
//        Ok,
//        Commit,
//    }
//}

// Allowed input for close_bundle()
pub enum BundleOnClose {
   Commit,
   Discard,
}

//--------------------------------------------------------------------------------------------------
// API FUNCTIONS
//--------------------------------------------------------------------------------------------------

/// Prints a string through the stdout of the running service, and also
/// writes that string to the logger in the execution context
pub fn debug(msg: &str) -> ZomeApiResult<()> {
    let mut mem_stack = unsafe { G_MEM_STACK.unwrap() };
    let maybe_allocation_of_input = store_as_json(&mut mem_stack, msg);
    if let Err(err_code) = maybe_allocation_of_input {
        return Err(ZomeApiError::Internal(err_code.to_string()));
    }
    let allocation_of_input = maybe_allocation_of_input.unwrap();
    unsafe {
        hc_debug(allocation_of_input.encode());
    }
    mem_stack
        .deallocate(allocation_of_input)
        .expect("should be able to deallocate input that has been allocated on memory stack");
    Ok(())
}

/// Not Yet Available
pub fn call<S: Into<String>>(
    _zome_name: S,
    _cap_name: S,
    _function_name: S,
    _arguments: serde_json::Value,
) -> ZomeApiResult<serde_json::Value> {
    Err(ZomeApiError::FunctionNotImplemented)
}

/// Attempts to commit an entry to your local source chain. The entry
/// will have to pass the defined validation rules for that entry type. Returns either an
/// address of the committed entry as a string, or an error.
pub fn commit_entry(
    entry_type_name: &str,
    entry_value: serde_json::Value,
) -> ZomeApiResult<HashString> {
    let mut mem_stack: SinglePageStack;
    unsafe {
        mem_stack = G_MEM_STACK.unwrap();
    }

    // Put args in struct and serialize into memory
    let input = CommitEntryArgs {
        entry_type_name: entry_type_name.to_string(),
        entry_value: entry_value.to_string(),
    };
    let maybe_allocation_of_input = store_as_json(&mut mem_stack, input);
    if let Err(err_code) = maybe_allocation_of_input {
        return Err(ZomeApiError::Internal(err_code.to_string()));
    }
    let allocation_of_input = maybe_allocation_of_input.unwrap();

    // Call WASMI-able commit
    let encoded_allocation_of_result: u32;
    unsafe {
        encoded_allocation_of_result = hc_commit_entry(allocation_of_input.encode() as u32);
    }
    // Deserialize complex result stored in memory and check for ERROR in encoding
    let result = load_json(encoded_allocation_of_result as u32);

    if let Err(err_str) = result {
        return Err(ZomeApiError::Internal(err_str));
    }
    let output: CommitEntryResult = result.unwrap();

    // Free result & input allocations and all allocations made inside commit()
    mem_stack
        .deallocate(allocation_of_input)
        .expect("deallocate failed");

    if output.validation_failure.len() > 0 {
        Err(ZomeApiError::ValidationFailed(output.validation_failure))
    } else {
        Ok(HashString::from(output.address))
    }
}

/// Retrieves an entry from the local chain or the DHT, by looking it up using
/// its address.
pub fn get_entry(
    address: HashString,
    _options: GetEntryOptions,
) -> ZomeApiResult<GetEntryResult> {
    let mut mem_stack: SinglePageStack;
    unsafe {
        mem_stack = G_MEM_STACK.unwrap();
    }

    // Put args in struct and serialize into memory
    let input = GetEntryArgs {
        address: address,
    };
    let maybe_allocation_of_input = store_as_json(&mut mem_stack, input);
    if let Err(err_code) = maybe_allocation_of_input {
        return Err(ZomeApiError::Internal(err_code.to_string()));
    }
    let allocation_of_input = maybe_allocation_of_input.unwrap();

    // Call WASMI-able get_entry
    let encoded_allocation_of_result: u32;
    unsafe {
        encoded_allocation_of_result = hc_get_entry(allocation_of_input.encode() as u32);
    }
    // Deserialize complex result stored in memory and check for ERROR in encoding
    let result = load_json(encoded_allocation_of_result as u32);
    if let Err(err_str) = result {
        return Err(ZomeApiError::Internal(err_str));
    }
    let result: GetEntryResult = result.unwrap();

    // Free result & input allocations and all allocations made inside commit()
    mem_stack
        .deallocate(allocation_of_input)
        .expect("deallocate failed");

    Ok(result)
}

/// Not Yet Available
// Returns an application property, which are defined by the app developer.
// It returns values from the DNA file that you set as properties of your application
// (e.g. Name, Language, Description, Author, etc.).
pub fn property<S: Into<String>>(_name: S) -> ZomeApiResult<String> {
    Err(ZomeApiError::FunctionNotImplemented)
}

/// Not Yet Available
pub fn make_hash<S: Into<String>>(
    _entry_type: S,
    _entry_data: serde_json::Value,
) -> ZomeApiResult<HashString> {
    // FIXME
    Err(ZomeApiError::FunctionNotImplemented)
}

/// Not Yet Available
pub fn sign<S: Into<String>>(_doc: S) -> ZomeApiResult<String> {
    Err(ZomeApiError::FunctionNotImplemented)
}

/// Not Yet Available
pub fn verify_signature<S: Into<String>>(
    _signature: S,
    _data: S,
    _pub_key: S,
) -> ZomeApiResult<bool> {
    Err(ZomeApiError::FunctionNotImplemented)
}

/// Not Yet Available
pub fn update_entry<S: Into<String>>(
    _entry_type: S,
    _entry: serde_json::Value,
    _replaces: HashString,
) -> ZomeApiResult<HashString> {
    // FIXME
    Err(ZomeApiError::FunctionNotImplemented)
}

/// Not Yet Available
pub fn update_agent() -> ZomeApiResult<HashString> {
    Err(ZomeApiError::FunctionNotImplemented)
}

/// Not Yet Available
pub fn remove_entry<S: Into<String>>(_entry: HashString, _message: S) -> ZomeApiResult<HashString> {
    Err(ZomeApiError::FunctionNotImplemented)
}

/// Not Yet Available
pub fn link_entries<S: Into<String>>(
    _base: HashString,
    _target: HashString,
    _tag: S,
) -> ZomeApiResult<()> {
    Err(ZomeApiError::FunctionNotImplemented)
}

/// Not Yet Available
pub fn get_links<S: Into<String>>(_base: HashString, _tag: S) -> ZomeApiResult<Vec<HashString>> {
    Err(ZomeApiError::FunctionNotImplemented)
}

/// Not Yet Available
pub fn query() -> ZomeApiResult<Vec<String>> {
    Err(ZomeApiError::FunctionNotImplemented)
}

/// Not Yet Available
pub fn send(_to: HashString, _message: serde_json::Value) -> ZomeApiResult<serde_json::Value> {
    Err(ZomeApiError::FunctionNotImplemented)
}

/// Not Yet Available
pub fn start_bundle(_timeout: usize, _user_param: serde_json::Value) -> ZomeApiResult<()> {
    Err(ZomeApiError::FunctionNotImplemented)
}

/// Not Yet Available
pub fn close_bundle(_action: BundleOnClose) -> ZomeApiResult<()> {
    Err(ZomeApiError::FunctionNotImplemented)
}
