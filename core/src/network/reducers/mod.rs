pub mod publish;
pub mod receive;
pub mod init;

use crate::network::state::NetworkState;
use std::sync::Arc;
use crate::action::ActionWrapper;
use crate::context::Context;
use crate::network::reducers::init::reduce_init;
use crate::network::reducers::publish::reduce_publish;
use crate::action::NetworkReduceFn;
use crate::action::Action;

/// maps incoming action to the correct handler
fn resolve_reducer(action_wrapper: &ActionWrapper) -> Option<NetworkReduceFn> {
    match action_wrapper.action() {
        crate::action::Action::Publish(_) => Some(reduce_publish),
        Action::InitNetwork(_) => Some(reduce_init),
        _ => None,
    }
}


pub fn reduce(
    context: Arc<Context>,
    old_state: Arc<NetworkState>,
    action_wrapper: &ActionWrapper,
) -> Arc<NetworkState> {
    let handler = resolve_reducer(action_wrapper);
    match handler {
        Some(f) => {
            let mut new_state: NetworkState = (*old_state).clone();
            f(context, &mut new_state, &action_wrapper);
            Arc::new(new_state)
        }
        None => old_state,
    }
}
