use error::{ZomeApiError, ZomeApiResult};
use globals::*;
pub use holochain_wasm_utils::api_serialization::validation::*;
use holochain_wasm_utils::{
    api_serialization::{
        commit::{CommitEntryArgs, CommitEntryResult},
        get_entry::{GetEntryArgs, GetEntryOptions, GetEntryResult, GetResultStatus},
        get_links::{GetLinksArgs, GetLinksResult},
        link_entries::{LinkEntriesArgs, LinkEntriesResult},
        HashEntryArgs, QueryArgs, QueryResult, ZomeFnCallArgs,
    },
    holochain_core_types::hash::HashString,
    memory_allocation::*,
    memory_serialization::*,
};
use serde::de::DeserializeOwned;
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
  pub static ref AGENT_ADDRESS: &'static HashString = &GLOBALS.agent_address;

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
/// # Examples
/// ```rust
/// pub fn handle_some_function(content: String) -> serde_json::Value {
///     // ...
///     hdk::debug("write a message to the logs");
///     // ...
/// }
/// ```
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

/// Call an exposed function from another zome.
/// Arguments for the called function are passed as `serde_json::Value`.
/// Returns the value that's returned by the given function as a json str.
/// # Examples
/// In order to utilize `call`, you must have at least two separate Zomes.
/// Here are two Zome examples, where one performs a `call` into the other.
///
/// This first one, is the one that is called into, with the Zome name `summer`.
/// ```rust
/// #[macro_use]
/// extern crate hdk;
/// extern crate serde;
/// #[macro_use]
/// extern crate serde_derive;
/// #[macro_use]
/// extern crate serde_json;
///
/// fn handle_sum(num1: u32, num2: u32) -> serde_json::Value {
///     let sum = num1 + num2;
///     return json!({"sum": format!("{}",sum)});
/// }
///
/// define_zome! {
///     entries: []
///
///     genesis: || {
///         Ok(())
///     }
///
///     functions: {
///         main (Public) {
///             sum: {
///                 inputs: |num1: u32, num2: u32|,
///                 outputs: |sum: serde_json::Value|,
///                 handler: handle_sum
///             }
///         }
///     }
/// }
/// ```
///
/// This second one, is the one that performs the call into the `summer` Zome.
/// ```rust
/// #[macro_use]
/// extern crate hdk;
/// extern crate serde;
/// #[macro_use]
/// extern crate serde_derive;
/// #[macro_use]
/// extern crate serde_json;
///
/// use hdk::holochain_core_types::hash::HashString;
///
/// fn handle_check_sum(num1: u32, num2: u32) -> serde_json::Value {
///     #[derive(Serialize)]
///     struct SumInput {
///         num1: u32,
///         num2: u32,
///     };
///     let call_input = SumInput {
///         num1: num1,
///         num2: num2,
///     };
///     let maybe_result = hdk::call(
///         "summer",
///         "main",
///         "sum",
///         serde_json::to_value(call_input).unwrap()
///     );
///     match maybe_result {
///         Ok(result) => serde_json::from_str(&result).unwrap(),
///         Err(hdk_error) => hdk_error.to_json(),
///     }
/// }
///
/// define_zome! {
///     entries: []
///
///     genesis: || {
///         Ok(())
///     }
///
///     functions: {
///         main (Public) {
///             check_sum: {
///                 inputs: |num1: u32, num2: u32|,
///                 outputs: |sum: serde_json::Value|,
///                 handler: handle_check_sum
///             }
///         }
///     }
/// }
/// ```
pub fn call<S: Into<String>>(
    zome_name: S,
    cap_name: S,
    fn_name: S,
    fn_args: serde_json::Value,
) -> ZomeApiResult<String> {
    let mut mem_stack: SinglePageStack;
    unsafe {
        mem_stack = G_MEM_STACK.unwrap();
    }

    // Put args in struct and serialize into memory
    let input = ZomeFnCallArgs {
        zome_name: zome_name.into(),
        cap_name: cap_name.into(),
        fn_name: fn_name.into(),
        fn_args: fn_args.to_string(),
    };
    let maybe_allocation_of_input = store_as_json(&mut mem_stack, input.clone());
    if let Err(err_code) = maybe_allocation_of_input {
        return Err(ZomeApiError::Internal(err_code.to_string()));
    }
    let allocation_of_input = maybe_allocation_of_input.unwrap();

    // Call WASMI-able commit
    let encoded_allocation_of_result: u32;
    unsafe {
        encoded_allocation_of_result = hc_call(allocation_of_input.encode() as u32);
    }
    // Deserialize complex result stored in memory and check for ERROR in encoding
    let result = load_string(encoded_allocation_of_result as u32);

    if let Err(err_str) = result {
        return Err(ZomeApiError::Internal(err_str));
    }
    let output = result.unwrap();

    // Free result & input allocations and all allocations made inside commit()
    mem_stack
        .deallocate(allocation_of_input)
        .expect("deallocate failed");
    // Done
    Ok(output)
}

/// Attempts to commit an entry to your local source chain. The entry
/// will have to pass the defined validation rules for that entry type.
/// If the entry type is defined as public, will also publish the entry to the DHT.
/// Returns either an address of the committed entry as a string, or an error.
/// # Examples
/// ```rust
/// pub fn handle_create_post(content: String) -> serde_json::Value {
///     let maybe_address = hdk::commit_entry("post", json!({
///         "content": content,
///         "date_created": "now"
///     }));
///     match maybe_address {
///         Ok(post_address) => json!({"address": post_address}),
///         Err(hdk_error) => hdk_error.to_json(),
///     }
/// }
/// ```
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
/// # Examples
/// ```rust
/// pub fn handle_get_post(post_address: HashString) -> serde_json::Value {
///     // get_entry returns a Result<Option<T>, ZomeApiError>
///     // where T is the type that you used to commit the entry, in this case a Blog
///     // It's a ZomeApiError if something went wrong (i.e. wrong type in deserialization)
///     // Otherwise its a Some(T) or a None
///     let result : Result<Option<Post>,ZomeApiError> = hdk::get_entry(post_address);
///     match result {
///         // In the case we don't get an error
///         // it might be an entry ...
///         Ok(Some(post)) => json!(post),
///         Ok(None) =>  json!({}),
///         Err(err) => json!({"error deserializing post": err.to_string()}),
///     }
/// }
/// ```
pub fn get_entry<T>(address: HashString) -> Result<Option<T>, ZomeApiError>
where
    T: DeserializeOwned,
{
    let res = get_entry_result(address, GetEntryOptions {});
    match res {
        Ok(result) => match result.status {
            GetResultStatus::Found => {
                let maybe_entry_value: Result<T, _> = serde_json::from_str(&result.entry);
                match maybe_entry_value {
                    Ok(entry_value) => Ok(Some(entry_value)),
                    Err(err) => Err(ZomeApiError::Internal(err.to_string())),
                }
            }
            GetResultStatus::NotFound => Ok(None),
        },
        Err(err) => Err(err),
    }
}

/// Retrieves an entry and meta data from the local chain or the DHT, by looking it up using
/// its address, and a the full options to specify exactly what data to return
pub fn get_entry_result(
    address: HashString,
    _options: GetEntryOptions,
) -> ZomeApiResult<GetEntryResult> {
    let mut mem_stack: SinglePageStack;
    unsafe {
        mem_stack = G_MEM_STACK.unwrap();
    }

    // Put args in struct and serialize into memory
    let input = GetEntryArgs { address: address };
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

/// Consumes three values, two of which are the addresses of entries, and one of which is a string that defines a
/// relationship between them, called a `tag`. Later, lists of entries can be looked up by using [get_links](fn.get_links.html). Entries
/// can only be looked up in the direction from the `base`, which is the first argument, to the `target`.
/// # Examples
/// ```rust
/// pub fn handle_create_post(content: String) -> serde_json::Value {
///     let maybe_address = hdk::commit_entry("post", json!({
///         "content": content,
///         "date_created": "now"
///     }));
///     match maybe_address {
///         Ok(post_address) => {
///             let link_result = hdk::link_entries(
///                 &HashString::from(AGENT_ADDRESS.to_string()),
///                 &post_address,
///                 "authored_posts"
///             );
///             if link_result.is_err() {
///                 return json!({"link error": link_result.err().unwrap()})
///             }
///             json!({"address": post_address})
///         }
///         Err(hdk_error) => hdk_error.to_json(),
///     }
/// }
/// ```
pub fn link_entries<S: Into<String>>(
    base: &HashString,
    target: &HashString,
    tag: S,
) -> Result<(), ZomeApiError> {
    let mut mem_stack = unsafe { G_MEM_STACK.unwrap() };

    // Put args in struct and serialize into memory
    let input = LinkEntriesArgs {
        base: base.clone(),
        target: target.clone(),
        tag: tag.into(),
    };

    let allocation_of_input = store_as_json(&mut mem_stack, input)
        .map_err(|err_code| ZomeApiError::Internal(err_code.to_string()))?;

    let encoded_allocation_of_result: u32 =
        unsafe { hc_link_entries(allocation_of_input.encode() as u32) };

    // Deserialize complex result stored in memory and check for ERROR in encoding
    let result: LinkEntriesResult = load_json(encoded_allocation_of_result as u32)
        .map_err(|err_str| ZomeApiError::Internal(err_str))?;

    // Free result & input allocations and all allocations made inside commit()
    mem_stack
        .deallocate(allocation_of_input)
        .expect("deallocate failed");

    if result.ok {
        Ok(())
    } else {
        Err(ZomeApiError::Internal(result.error))
    }
}

/// Not Yet Available
// Returns a DNA property, which are defined by the DNA developer.
// They are custom values that are defined in the DNA file
// that can be used in the zome code for defining configurable behaviors.
// (e.g. Name, Language, Description, Author, etc.).
pub fn property<S: Into<String>>(_name: S) -> ZomeApiResult<String> {
    Err(ZomeApiError::FunctionNotImplemented)
}

/// Reconstructs an address of the given entry data.
/// This is the same value that would be returned if `entry_type_name` and `entry_value` were passed
/// to the [commit_entry](fn.commit_entry.html) function and by which it would be retrievable from the DHT using [get_entry](fn.get_entry.html).
/// This is often used to reconstruct an address of a `base` argument when calling [get_links](fn.get_links.html).
/// # Examples
/// ```rust
/// fn handle_hash_post(content: String) -> serde_json::Value {
///     let maybe_address = hdk::hash_entry("post", json!({
///         "content": content,
///         "date_created": "now"
///     }));
///     match maybe_address {
///         Ok(address) => {
///             json!({"address": address})
///         }
///         Err(hdk_error) => hdk_error.to_json(),
///     }
/// }
/// ```
pub fn hash_entry<S: Into<String>>(
    entry_type_name: S,
    entry_value: serde_json::Value,
) -> ZomeApiResult<HashString> {
    let mut mem_stack = unsafe { G_MEM_STACK.unwrap() };
    // Put args in struct and serialize into memory
    let input = HashEntryArgs {
        entry_type_name: entry_type_name.into(),
        entry_value: entry_value.to_string(),
    };
    let allocation_of_input = store_as_json(&mut mem_stack, input)
        .map_err(|err_code| ZomeApiError::Internal(err_code.to_string()))?;
    let encoded_allocation_of_result: u32 =
        unsafe { hc_hash_entry(allocation_of_input.encode() as u32) };
    // Deserialize complex result stored in memory and check for ERROR in encoding
    let result = load_string(encoded_allocation_of_result as u32)
        .map_err(|err_str| ZomeApiError::Internal(err_str))?;
    // Free result & input allocations and all allocations made inside commit()
    mem_stack
        .deallocate(allocation_of_input)
        .expect("deallocate failed");
    Ok(HashString::from(result))
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

/// Consumes two values, the first of which is the address of an entry, `base`, and the second of which is a string, `tag`,
/// used to describe the relationship between the `base` and other entries you wish to lookup. Returns a list of addresses of other
/// entries which matched as being linked by the given `tag`. Links are created in the first place using the Zome API function [link_entries](fn.link_entries.html).
/// Once you have the addresses, there is a good likelihood that you will wish to call [get_entry](fn.get_entry.html) for each of them.
/// # Examples
/// ```rust
/// pub fn handle_posts_by_agent(agent: HashString) -> serde_json::Value {
///     match hdk::get_links(&agent, "authored_posts") {
///         Ok(result) => json!({"post_addresses": result.links}),
///         Err(hdk_error) => hdk_error.to_json(),
///     }
/// }
/// ```
pub fn get_links<S: Into<String>>(base: &HashString, tag: S) -> ZomeApiResult<GetLinksResult> {
    let mut mem_stack = unsafe { G_MEM_STACK.unwrap() };

    // Put args in struct and serialize into memory
    let input = GetLinksArgs {
        entry_address: base.clone(),
        tag: tag.into(),
    };

    let allocation_of_input = store_as_json(&mut mem_stack, input)
        .map_err(|err_code| ZomeApiError::Internal(err_code.to_string()))?;

    let encoded_allocation_of_result: u32 =
        unsafe { hc_get_links(allocation_of_input.encode() as u32) };

    // Deserialize complex result stored in memory and check for ERROR in encoding
    let result: GetLinksResult = load_json(encoded_allocation_of_result as u32)
        .map_err(|err_str| ZomeApiError::Internal(err_str))?;

    // Free result & input allocations and all allocations made inside commit()
    mem_stack
        .deallocate(allocation_of_input)
        .expect("deallocate failed");

    if result.ok {
        Ok(result)
    } else {
        Err(ZomeApiError::Internal(result.error))
    }
}

/// Returns a list of entries from your local source chain, that match a given type.
/// - entry_type_name: Specify type of entry to retrieve
/// - limit: Max number of entries to retrieve, with `0` indicating unlimited
///
/// Once you have the list of addresses of your entries, there is a good likelihood that you will
/// wish to iterate and call [get_entry](fn.get_entry.html) to retrieve each one.
/// Please be aware that the API for this function is subject to change in future versions.
/// # Examples
/// ```rust
/// pub fn handle_my_posts() -> serde_json::Value {
///     match hdk::query("post", 0) {
///         Ok(posts) => json!({"post_addresses": posts}),
///         Err(hdk_error) => hdk_error.to_json(),
///     }
/// }
/// ```
pub fn query(entry_type_name: &str, limit: u32) -> ZomeApiResult<Vec<HashString>> {
    let mut mem_stack = unsafe { G_MEM_STACK.unwrap() };
    // Put args in struct and serialize into memory
    let input = QueryArgs {
        entry_type_name: entry_type_name.to_string(),
        limit: limit,
    };
    let allocation_of_input = store_as_json(&mut mem_stack, input)
        .map_err(|err_code| ZomeApiError::Internal(err_code.to_string()))?;
    let encoded_allocation_of_result: u32 =
        unsafe { hc_query(allocation_of_input.encode() as u32) };
    // Deserialize complex result stored in memory and check for ERROR in encoding
    let result: QueryResult = load_json(encoded_allocation_of_result as u32)
        .map_err(|err_str| ZomeApiError::Internal(err_str))?;
    // Free result & input allocations and all allocations made inside commit()
    mem_stack
        .deallocate(allocation_of_input)
        .expect("deallocate failed");
    Ok(result.hashes)
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
