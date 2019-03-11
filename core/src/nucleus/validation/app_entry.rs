use crate::{
    context::Context,
    nucleus::{
        actions::run_validation_callback::run_validation_callback,
        validation::{ValidationError, ValidationResult},
        CallbackFnCall,
    },
    workflows::get_entry_result::get_entry_result_workflow,
    network::entry_with_header::{EntryWithHeader,fetch_entry_with_header}
};
use holochain_core_types::{
    cas::content::AddressableContent,
    entry::{entry_type::AppEntryType, Entry},
    validation::ValidationData,
    error::HolochainError
};
use holochain_wasm_utils::api_serialization::{validation::EntryValidationArgs,get_entry::GetEntryArgs};
use std::sync::Arc;

use futures_util::try_future::TryFutureExt;

pub async fn validate_app_entry(
    entry: Entry,
    app_entry_type: AppEntryType,
    validation_data: ValidationData,
    context: &Arc<Context>,
) -> ValidationResult {
    let dna = context.get_dna().expect("Callback called without DNA set!");
    let EntryWithHeader{entry ,header: old_entry_header} = fetch_entry_with_header(&entry.address(),&context).map_err(|_|{
            ValidationError::Fail("Entry not found in dht chain".to_string())
        })?;
    
    let zome_name = dna
        .get_zome_name_for_app_entry_type(&app_entry_type)
        .ok_or(ValidationError::NotImplemented)?;
    if old_entry_header.link_update_delete().is_some()
    {
        let expected_link_update = old_entry_header.link_update_delete().expect("Should unwrap link_update_delete with no problems");
        let entry_args = &GetEntryArgs {
        address: expected_link_update.clone(),
        options: Default::default()};
        let result = await!(get_entry_result_workflow(&context,entry_args).map_err(|_|{
            ValidationError::Fail("Could not get entry for link_update_delete".to_string())
        }))?;
        let latest = result.latest().ok_or(ValidationError::Fail("Could not find entry for link_update_delete".to_string()))?;
        await!(run_call_back(context.clone(), entry, &zome_name, validation_data))
    }
    else 
    {
        await!(run_call_back(context.clone(), entry, &zome_name, validation_data))
    }

    
}

async fn run_call_back(context:Arc<Context>,entry:Entry,zome_name:&String,validation_data: ValidationData)-> ValidationResult
{
    let params = EntryValidationArgs {
        validation_data: validation_data.clone().entry_validation,
    };

    let call = CallbackFnCall::new(&zome_name, "__hdk_validate_app_entry", params);

    await!(run_validation_callback(entry.address(), call, &context))
}
