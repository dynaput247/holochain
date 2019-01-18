use std::collections::HashMap;
use std::io::Error;
use error::HolochainResult;
use hyper::{Body, Request,
	server::{
		Server
	},
	rt::Future,
};
use holochain_core_types::error::HolochainError;
// use futures;
use config::{UiInterfaceConfiguration, UiBundle};
use tokio::runtime::Runtime;
use hyper_staticfile::{Static, StaticFuture};
use tokio::prelude::future;

pub fn notify(msg: String) {
    println!("{}", msg);
}

/// Hyper `Service` implementation that serves all requests.
struct StaticService {
    static_: Static,
}

impl StaticService {
    fn new(path: String) -> Self {
        StaticService {
            static_: Static::new(path),
        }
    }
}

impl hyper::service::Service for StaticService {
    type ReqBody = Body;
    type ResBody = Body;
    type Error = Error;
    type Future = StaticFuture<Body>;

    fn call(&mut self, req: Request<Body>) -> StaticFuture<Body> {
        self.static_.serve(req)
    }
}

struct StaticServer {
	#[allow(dead_code)]
	shutdown_signal: Option<futures::channel::oneshot::Sender<()>>,
	config: UiInterfaceConfiguration,
	#[allow(dead_code)]
	bundle_config: UiBundle,
	running: bool,
}

impl StaticServer {

	pub fn from_configs(bundle_config: UiBundle, config: UiInterfaceConfiguration) -> Self {
		StaticServer {
			shutdown_signal: None,
			config,
			bundle_config,
			running: false,
		}
	}


	pub fn start(&mut self) -> HolochainResult<()> {
		let addr = ([127, 0, 0, 1], self.config.port).into();

		// let (tx, rx) = futures::channel::oneshot::channel::<()>();
		// self.shutdown_signal = Some(tx);
		
        let server = Server::bind(&addr)
	        .serve(|| future::ok::<_, Error>(StaticService::new("".to_string())))
	        // .with_graceful_shutdown(rx)
	        .map_err(|e| eprintln!("server error: {}", e));
		
		let mut rt = Runtime::new()
			.map_err(|e| HolochainError::ErrorGeneric(format!("Could not start tokio runtime, {}", e)))?;

 		println!("Listening on http://{}", addr);
        rt.spawn(server);
        self.running = true;
		Ok(())
	}

	// pub fn stop(&mut self) -> HolochainResult<()> {
	// 	if let Some(shutdown_signal) = self.shutdown_signal {
	// 		shutdown_signal.send(())
	// 		.and_then(|_| {
	// 			self.running = false;
	// 			self.shutdown_signal = None;
	// 			Ok(())
	// 		});
	// 	}
	// 	Err(HolochainError::ErrorGeneric("server is already stopped".into()).into())
	// }
}




pub struct StaticServerBuilder {
	servers: HashMap<String, StaticServer>
}

impl StaticServerBuilder {

	pub fn new() -> Self {
		StaticServerBuilder {
			servers: HashMap::new()
		}
	}

	pub fn with_interface_configs(mut self, configs: Vec<(UiBundle, UiInterfaceConfiguration)>) -> Self {
		for (bundle_config, config) in configs {
			let id = config.id.clone();
			self.servers.insert(id.clone(), StaticServer::from_configs(bundle_config, config));
		}
		self
	}

	pub fn start_all(&mut self) -> HolochainResult<()> {
		notify("Starting all servers".into());
		self.servers.iter_mut().for_each(|(id, server)| {
			server.start().expect(&format!("Couldnt start server {}", id));
			notify(format!("Server started for \"{}\"", id))
		});
		Ok(())
	}

}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    pub fn test_build_server() {

    	let test_bundle_config = UiBundle {
    		id: "bundle id".to_string(),
    		root_dir: "".to_string(),
    		hash: "Qmsdasdasd".to_string(),
    	};

    	let test_config = UiInterfaceConfiguration {
    		id: "an id".to_string(),
    		bundle: "a bundle".to_string(),
    		port: 3000,
    		dna_interface: "interface".to_string(),
    	};

    	let mut builder = StaticServerBuilder::new().with_interface_configs(vec![(test_bundle_config, test_config)]);
    	assert_eq!(
    		builder.start_all(),
    		Ok(())
    	);

    }
}
