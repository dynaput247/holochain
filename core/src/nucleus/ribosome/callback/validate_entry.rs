extern crate serde_json;
use context::Context;
use holochain_core_types::{
    dna::wasm::DnaWasm,
    entry::{entry_type::EntryType, Entry, ToEntry},
    error::HolochainError,
    link::link_add::LinkAddEntry,
    validation::ValidationData,
};
use holochain_wasm_utils::api_serialization::validation::{
    EntryValidationArgs,
    LinkValidationArgs,
};
use nucleus::{
    ribosome::{
        self,
        callback::CallbackResult,
    },
    ZomeFnCall,
};
use std::sync::Arc;
use super::links_utils;

pub fn validate_entry(
    entry: Entry,
    validation_data: ValidationData,
    context: Arc<Context>,
) -> Result<CallbackResult, HolochainError> {
    match entry.entry_type() {
        EntryType::App(app_entry_type) => Ok(validate_app_entry(
            entry.clone(),
            app_entry_type.clone(),
            validation_data,
            context,
        )?),
        EntryType::Dna => Ok(CallbackResult::Pass),
        EntryType::LinkAdd => Ok(validate_link_entry(
            entry.clone(),
            validation_data,
            context,
        )?),
        _ => Ok(CallbackResult::NotImplemented),
    }
}

fn validate_link_entry(
    entry: Entry,
    validation_data: ValidationData,
    context: Arc<Context>,
) -> Result<CallbackResult, HolochainError> {
    let link_add_entry = LinkAddEntry::from_entry(&entry);
    let link = link_add_entry.link().clone();
    let (base, target) = links_utils::get_link_entries(&link, &context)?;
    let link_definition_path = links_utils::find_link_definition_in_dna(
        &base.entry_type(),
        link.tag(),
        &target.entry_type(),
        &context,
    ).ok_or(HolochainError::NotImplemented)?;

    let wasm = context.get_wasm(&link_definition_path.zome_name)
        .expect("Couldn't get WASM for zome");

    let params = LinkValidationArgs {
        entry_type: link_definition_path.entry_type_name,
        link,
        direction:  link_definition_path.direction,
        validation_data,
    };
    let call = ZomeFnCall::new(
        &link_definition_path.zome_name,
        "no capability, since this is an entry validation call",
        "__hdk_validate_link",
        params,
    );
    Ok(run_validation_callback(
        context.clone(),
        call,
        &wasm,
        context.get_dna().unwrap().name.clone(),
    ))
}

fn validate_app_entry(
    entry: Entry,
    app_entry_type: String,
    validation_data: ValidationData,
    context: Arc<Context>,
) -> Result<CallbackResult, HolochainError> {
    let dna = context.get_dna().expect("Callback called without DNA set!");
    let zome_name = dna.get_zome_name_for_entry_type(&app_entry_type);
    if zome_name.is_none() {
        return Ok(CallbackResult::NotImplemented);
    }

    let zome_name = zome_name.unwrap();
    match context.get_wasm(&zome_name) {
        Some(wasm) => {
            let validation_call =
                build_validation_call(entry, app_entry_type, zome_name, validation_data)?;
            Ok(run_validation_callback(
                context.clone(),
                validation_call,
                &wasm,
                dna.name.clone(),
            ))
        }
        None => Ok(CallbackResult::NotImplemented),
    }
}

fn build_validation_call(
    entry: Entry,
    entry_type: String,
    zome_name: String,
    validation_data: ValidationData,
) -> Result<ZomeFnCall, HolochainError> {
    let params = EntryValidationArgs {
        entry_type,
        entry: entry.to_string(),
        validation_data,
    };

    Ok(ZomeFnCall::new(
        &zome_name,
        "no capability, since this is an entry validation call",
        "__hdk_validate_app_entry",
        params,
    ))
}

fn run_validation_callback(
    context: Arc<Context>,
    fc: ZomeFnCall,
    wasm: &DnaWasm,
    dna_name: String,
) -> CallbackResult {
    match ribosome::run_dna(
        &dna_name,
        context,
        wasm.code.clone(),
        &fc,
        Some(fc.clone().parameters.into_bytes()),
    ) {
        Ok(call_result) => match call_result.is_null() {
            true => CallbackResult::Pass,
            false => CallbackResult::Fail(call_result.to_string()),
        },
        // TODO: have "not matching schema" be its own error
        Err(HolochainError::RibosomeFailed(error_string)) => {
            if error_string == "Argument deserialization failed" {
                CallbackResult::Fail(String::from("JSON object does not match entry schema"))
            } else {
                CallbackResult::Fail(error_string)
            }
        }
        Err(error) => CallbackResult::Fail(error.to_string()),
    }
}
