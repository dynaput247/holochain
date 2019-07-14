use conductor::base::notify;
use config::{InterfaceConfiguration, UiBundleConfiguration, UiInterfaceConfiguration};
use error::HolochainResult;
use holochain_core_types::error::HolochainError;
use static_file_server::{dna_connections_response, ConductorStaticFileServer, DNA_CONFIG_ROUTE};

use std::{
    net::SocketAddr,
    sync::mpsc::{self, Sender},
};

use nickel::{
    hyper::uri::RequestUri, HttpRouter, Middleware, MiddlewareResult, Mountable, Nickel, Request,
    Response, StaticFilesHandler,
};

pub struct NickelStaticServer {
    shutdown_signal: Option<Sender<()>>,
    config: UiInterfaceConfiguration,
    bundle_config: UiBundleConfiguration,
    connected_dna_interface: Option<InterfaceConfiguration>,
    running: bool,
}

impl ConductorStaticFileServer for NickelStaticServer {
    fn from_configs(
        config: UiInterfaceConfiguration,
        bundle_config: UiBundleConfiguration,
        connected_dna_interface: Option<InterfaceConfiguration>,
    ) -> Self {
        Self {
            shutdown_signal: None,
            config,
            bundle_config,
            connected_dna_interface,
            running: false,
        }
    }

    fn start(&mut self) -> HolochainResult<()> {
        let (tx, rx) = mpsc::channel();

        self.shutdown_signal = Some(tx);
        self.running = true;

        {
            let mut server = Nickel::new();

            let static_file_handler =
                StaticFilesHandler::new(self.bundle_config.root_dir.to_owned());
            server.mount("/", static_file_handler.clone());

            // if required, reroute failed routes to index.html
            // This is required for SPAs with virtual routing
            server.mount("/", FallbackFileRouteHandler::new(static_file_handler));

            // provide a virtual route for inspecting the configed DNA interfaces for this UI
            // let connected_dna_interface = ;
            let connected_dna_interface = self.connected_dna_interface.clone();
            server.get(
                DNA_CONFIG_ROUTE,
                middleware! { |_|
                    dna_connections_response(&connected_dna_interface)
                },
            );

            let addr = SocketAddr::from(([127, 0, 0, 1], self.config.port));

            notify(format!(
                "About to serve path \"{}\" at http://{}",
                &self.bundle_config.root_dir, &addr
            ));

            server
                .listen(addr)
                .map_err(|e| notify(format!("server error: {}", e)))
                .expect("Could not start static file server");

            notify(format!("Listening on http://{}", addr));
            // block waiting for a shutdown signal after which the server goes out of scope
            rx.recv().unwrap();
        };

        Ok(())
    }

    fn stop(&mut self) -> HolochainResult<()> {
        match self.shutdown_signal.clone() {
            Some(shutdown_signal) => {
                shutdown_signal.send(()).unwrap();
                Ok(())
            }
            None => Err(HolochainError::ErrorGeneric("server is already stopped".into()).into()),
        }
    }
}

#[derive(Clone)]
struct FallbackFileRouteHandler(StaticFilesHandler);

impl FallbackFileRouteHandler {
    pub fn new(static_file_handler: StaticFilesHandler) -> Self {
        Self(static_file_handler)
    }
}

impl<D> Middleware<D> for FallbackFileRouteHandler {
    fn invoke<'mw, 'conn>(
        &'mw self,
        req: &mut Request<'mw, 'conn, D>,
        res: Response<'mw, D>,
    ) -> MiddlewareResult<'mw, D> {
        req.origin.uri = RequestUri::AbsolutePath("/".to_string());
        self.0.invoke(req, res)
    }
}
