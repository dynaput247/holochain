use crate::action::ActionWrapper;
use crossbeam_channel::{unbounded, Receiver, Sender};
use holochain_core_types::{error::HolochainError, json::JsonString};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::thread;

#[derive(Clone, Debug, DefaultJson)]
pub enum Signal {
    Internal(ActionWrapper),
    User(JsonString),
}

impl Serialize for Signal {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Signal::Internal(action_wrapper) => serializer.serialize_newtype_variant(
                "Signal",
                0,
                "Internal",
                &format!("{:?}", action_wrapper.action()),
            ),
            Signal::User(msg) => {
                serializer.serialize_newtype_variant("Signal", 1, "User", &msg.to_string())
            }
        }
    }
}

impl<'de> Deserialize<'de> for Signal {
    fn deserialize<D>(_deserializer: D) -> Result<Signal, D::Error>
    where
        D: Deserializer<'de>,
    {
        unimplemented!()
    }
}

pub type SignalSender = Sender<Signal>;
pub type SignalReceiver = Receiver<Signal>;

pub fn signal_channel() -> (SignalSender, SignalReceiver) {
    unbounded()
}

/// Pass on messages from multiple receivers into a single receiver
/// A potentially useful utility, but currently unused.
pub fn _combine_receivers<T>(rxs: Vec<Receiver<T>>) -> Receiver<T>
where
    T: 'static + Send,
{
    let (master_tx, master_rx) = unbounded::<T>();
    for rx in rxs {
        let tx = master_tx.clone();
        thread::spawn(move || {
            while let Ok(item) = rx.recv() {
                tx.send(item).unwrap_or(());
            }
        });
    }
    master_rx
}
