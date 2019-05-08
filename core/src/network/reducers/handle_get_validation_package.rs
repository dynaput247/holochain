use crate::{action::ActionWrapper, network::state::NetworkState};

pub fn reduce_handle_get_validation_package(
    network_state: &mut NetworkState,
    action_wrapper: &ActionWrapper,
) {
    let action = action_wrapper.action();
    let (address, maybe_validation_package) =
        unwrap_to!(action => crate::action::Action::HandleGetValidationPackage);

    network_state
        .get_validation_package_results
        .insert(address.clone(), Some(Ok(maybe_validation_package.clone())));
}
