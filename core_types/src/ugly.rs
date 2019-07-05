use std::fmt::Debug;

use std::sync::mpsc::{Sender, SyncSender};

pub fn lax_send<T: Clone + Debug>(tx: Sender<T>, val: T, _failure_reason: &str) -> bool {
    match tx.send(val.clone()) {
        Ok(()) => true,
        Err(_) => {
            // println!("[lax_send]\n{}\n{:?}\n", _failure_reason, val);
            false
        }
    }
}

pub fn lax_send_sync<T: Clone + Debug>(tx: SyncSender<T>, val: T, _failure_reason: &str) -> bool {
    match tx.send(val.clone()) {
        Ok(()) => true,
        Err(_) => {
            // println!("[lax_send_sync]\n{}\n{:?}\n", _failure_reason, val);
            false
        }
    }
}
