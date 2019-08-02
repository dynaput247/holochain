use crate::{
    action::RespondQueryPayload,
    context::Context,
    network::{
        actions::get::{get, GetMethod},
        query::{GetLinksNetworkQuery, GetLinksNetworkResult},
    },
};

use holochain_core_types::error::HolochainError;
use holochain_wasm_utils::api_serialization::get_links::{GetLinksArgs, GetLinksResultCount};
use std::sync::Arc;

pub async fn get_link_result_count_workflow<'a>(
    context: Arc<Context>,
    link_args: &'a GetLinksArgs,
) -> Result<GetLinksResultCount, HolochainError> {
    let method = GetMethod::Link(link_args.clone(), GetLinksNetworkQuery::Count);
    let response = await!(get(
        context.clone(),
        method,
        link_args.options.timeout.clone()
    ))?;

    let links_result = match response {
        RespondQueryPayload::Links((link_result, _, _)) => Ok(link_result),
        RespondQueryPayload::Entry(_) => Err(HolochainError::ErrorGeneric(
            "Could not get link".to_string(),
        )),
    }?;

    let links_count = match links_result {
        GetLinksNetworkResult::Count(count) => Ok(count),
        _ => Err(HolochainError::ErrorGeneric(
            "Getting wrong type of GetLinks".to_string(),
        )),
    }?;

    Ok(GetLinksResultCount { count: links_count })
}
