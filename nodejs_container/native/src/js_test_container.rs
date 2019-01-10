use holochain_container_api::{
    config::{load_configuration, Configuration},
    container::Container as RustContainer,
};
use holochain_core::signal::signal_channel;
use holochain_core_types::{cas::content::Address, dna::capabilities::CapabilityCall};
use neon::{context::Context, prelude::*};
use std::sync::{
    mpsc::{sync_channel, SyncSender},
    Arc, Mutex,
};

use crate::waiter::{CallBlockingTask, ControlMsg, MainBackgroundTask};

pub struct TestContainer {
    container: RustContainer,
    sender_tx: Option<SyncSender<SyncSender<ControlMsg>>>,
    is_running: Arc<Mutex<bool>>,
}

declare_types! {

    // A TestContainer can be initialized either by:
    // - an Object representation of a Configuration struct
    // - a string representing TOML
    pub class JsTestContainer for TestContainer {
        init(mut cx) {
            let config_arg: Handle<JsValue> = cx.argument(0)?;
            let config: Configuration = if config_arg.is_a::<JsObject>() {
                neon_serde::from_value(&mut cx, config_arg)?
            } else if config_arg.is_a::<JsString>() {
                let toml_str: String = neon_serde::from_value(&mut cx, config_arg)?;
                load_configuration(&toml_str).expect("Could not load TOML config")
            } else {
                panic!("Invalid type specified for config, must be object or string");
            };
            let container = RustContainer::from_config(config);
            let is_running = Arc::new(Mutex::new(false));

            Ok(TestContainer { container, sender_tx: None, is_running })
        }

        // Start the backing Container and spawn a MainBackgroundTask
        method start(mut cx) {
            let js_callback: Handle<JsFunction> = cx.argument(0)?;
            let mut this = cx.this();

            let (signal_tx, signal_rx) = signal_channel();
            let (sender_tx, sender_rx) = sync_channel(1);

            let start_result: Result<(), String> = {
                let guard = cx.lock();
                let tc = &mut *this.borrow_mut(&guard);
                tc.sender_tx = Some(sender_tx);
                {
                    let mut is_running = tc.is_running.lock().unwrap();
                    *is_running = true;
                }
                let background_task = MainBackgroundTask::new(signal_rx, sender_rx, tc.is_running.clone());
                background_task.schedule(js_callback);
                tc.container.load_config_with_signal(Some(signal_tx)).and_then(|_| {
                    tc.container.start_all_instances().map_err(|e| e.to_string())
                })
            };

            start_result.or_else(|e| {
                let error_string = cx.string(format!("unable to start container: {}", e));
                cx.throw(error_string)
            })?;

            Ok(cx.undefined().upcast())
        }

        // Stop the backing container and break the listening loop in the MainBackgroundTask
        method stop(mut cx) {
            let mut this = cx.this();

            let stop_result: Result<(), String> = {
                let guard = cx.lock();
                let tc = &mut *this.borrow_mut(&guard);

                let mut is_running = tc.is_running.lock().unwrap();
                // This causes MainBackgroundTask to eventually terminate
                *is_running = false;

                let result = tc.container.shutdown().map_err(|e| e.to_string());
                result
            };

            stop_result.or_else(|e| {
                let error_string = cx.string(format!("unable to stop container: {}", e));
                cx.throw(error_string)
            })?;

            Ok(cx.undefined().upcast())
        }

        method call(mut cx) {
            let instance_id = cx.argument::<JsString>(0)?.to_string(&mut cx)?.value();
            let zome = cx.argument::<JsString>(1)?.to_string(&mut cx)?.value();
            let cap_name = cx.argument::<JsString>(2)?.to_string(&mut cx)?.value();
            let fn_name = cx.argument::<JsString>(3)?.to_string(&mut cx)?.value();
            let params = cx.argument::<JsString>(4)?.to_string(&mut cx)?.value();

            let mut this = cx.this();

            let call_result = {
                let guard = cx.lock();
                let tc = &mut *this.borrow_mut(&guard);
                let cap = Some(CapabilityCall::new(
                    cap_name.to_string(),
                    Address::from(""), //FIXME
                    None,
                ));
                let instance_arc = tc.container.instances().get(&instance_id)
                    .expect(&format!("No instance with id: {}", instance_id));
                let mut instance = instance_arc.write().unwrap();
                instance.call(&zome, cap, &fn_name, &params)
            };

            let res_string = call_result.or_else(|e| {
                let error_string = cx.string(format!("unable to call zome function: {:?}", &e));
                cx.throw(error_string)
            })?;

            let result_string: String = res_string.into();

            Ok(cx.string(result_string).upcast())
        }

        // This sets up the state of MainBackgroundTask to listen for the next ExecuteZomeFunction
        // action and does its magic of observing incoming actions to invoke the callback once it
        // determines that all test-relevant network activity has completed
        method register_callback(mut cx) {
            let js_callback: Handle<JsFunction> = cx.argument(0)?;
            let this = cx.this();
            {
                let guard = cx.lock();
                let tc = &*this.borrow(&guard);

                let (tx, rx) = sync_channel(0);
                let task = CallBlockingTask { rx };
                task.schedule(js_callback);
                tc
                    .sender_tx
                    .as_ref()
                    .expect("Container sender channel not initialized")
                    .send(tx)
                    .expect("Could not send to sender channel");
            }
            Ok(cx.undefined().upcast())
        }

        // Fetch the agent address from within the instance
        method agent_id(mut cx) {
            let instance_id = cx.argument::<JsString>(0)?.to_string(&mut cx)?.value();
            let this = cx.this();
            let result = {
                let guard = cx.lock();
                let tc = this.borrow(&guard);
                let instance = tc.container.instances().get(&instance_id)
                    .expect(&format!("No instance with id: {}", instance_id))
                    .read().unwrap();
                let out = instance.context().state().ok_or("No state?".to_string())
                    .and_then(|state| state
                        .agent().get_agent_address()
                        .map_err(|e| e.to_string()));
                out
            };

            let hash = result.or_else(|e: String| {
                let error_string = cx.string(format!("unable to call zome function: {:?}", &e));
                cx.throw(error_string)
            })?;
            Ok(cx.string(hash.to_string()).upcast())
        }
    }
}
