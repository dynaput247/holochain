//! CAS Implementations
//!
//! (CAS == Content Addressable Storage)
//!
//! This crate contains implementations for the CAS and EAV traits
//! which are defined but not implemented in the core_types crate.

extern crate futures;
extern crate holochain_core_types;
#[macro_use]
extern crate lazy_static;
extern crate riker;
extern crate riker_default;
extern crate riker_patterns;
extern crate snowflake;
extern crate walkdir;

extern crate uuid;

extern crate serde;
extern crate serde_json;

pub mod actor;
pub mod cas;
pub mod eav;
pub mod path;
