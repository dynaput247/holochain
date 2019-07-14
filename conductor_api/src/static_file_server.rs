use config::{InterfaceConfiguration, UiBundleConfiguration, UiInterfaceConfiguration};
use error::HolochainResult;
use hyper::{
    http::{response::Builder, uri},
    Body, Request, Response,
};

pub const DNA_CONFIG_ROUTE: &str = "/_dna_connections.json";

pub fn redirect_request_to_root<T>(req: &mut Request<T>) {
    let mut original_parts: uri::Parts = req.uri().to_owned().into();
    original_parts.path_and_query = Some("/".parse().unwrap());
    *req.uri_mut() = uri::Uri::from_parts(original_parts).unwrap();
}

pub fn dna_connections_response(config: &Option<InterfaceConfiguration>) -> Response<Body> {
    let interface = match config {
        Some(config) => json!(config),
        None => serde_json::Value::Null,
    };
    Builder::new()
        .body(json!({ "dna_interface": interface }).to_string().into())
        .expect("unable to build response")
}

pub trait ConductorStaticFileServer {
    fn from_configs(
        config: UiInterfaceConfiguration,
        bundle_config: UiBundleConfiguration,
        connected_dna_interface: Option<InterfaceConfiguration>,
    ) -> Self;
    fn start(&mut self) -> HolochainResult<()>;
    fn stop(&mut self) -> HolochainResult<()>;
}
