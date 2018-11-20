
use holochain_cas_implementations::{
    cas::file::FilesystemStorage, eav::file::EavFileStorage, path::create_path_if_not_exists,
};
use holochain_core::context::Context;
use holochain_core_types::{dna::Dna, error::HolochainError, json::JsonString};
use Holochain;

use holochain_core::{logger::Logger, persister::SimplePersister};
use holochain_core_types::agent::Agent;
use std::{
    collections::HashMap,
    convert::TryFrom,
    fs::File,
    io::prelude::*,
    sync::{Arc, Mutex, RwLock},
};
use holochain_net::p2p_network::P2pNetwork;
use thread;


use config::{Configuration, StorageConfiguration};
use interface::{Interface, InstanceMap, DispatchRpc};
use jsonrpc::JsonRpc;


/// Main representation of the container.
/// Holds a `HashMap` of Holochain instances referenced by ID.

/// A primary point in this struct is
/// `load_config(&mut self, config: &Configuration) -> Result<(), String>`
/// which takes a `config::Configuration` struct and tries to instantiate all configured instances.
/// While doing so it has to load DNA files referenced in the configuration.
/// In order to not bind this code to the assumption that there is a filesystem
/// and also enable easier testing,
/// a DnaLoader has to be injected on creation.
/// This is a closure that returns a Dna object for a given path string.
pub struct Container {
    pub instances: InstanceMap,
    config: Configuration,
    interfaces: HashMap<String, Box<Interface<Container>>>,
    dna_loader: DnaLoader,
}

type DnaLoader = Arc<Box<FnMut(&String) -> Result<Dna, HolochainError> + Send>>;

impl DispatchRpc for Container {
    fn instances(&self) -> &InstanceMap { &self.instances }
    
    /// Dispatch to the correct Holochain and `call` it based on the JSONRPC method
    fn dispatch_rpc(&self, rpc: JsonRpc) -> Result<JsonString, HolochainError> {
        let matches: Vec<&str> = rpc.method.split('/').collect();
        let result = if let [instance_id, zome, cap, func] = matches.as_slice() {
            let key = instance_id.to_string();
            self.instances()
                .get(&key)
                .ok_or(format!("No instance for agent/dna pair: {:?}", key))
                .and_then(|hc| {
                    hc.lock().unwrap().call(zome, cap, func, &rpc.params.to_string()).map_err(|e| e.to_string())
                })
        } else {
            Err(format!("bad rpc method: {}", rpc.method))
        };
        result.map_err(HolochainError::ErrorGeneric)
    }
}

impl Container {
    /// Creates a new instance with the default DnaLoader that actually loads files.
    pub fn with_config(config: Configuration) -> Self {
        Container {
            instances: HashMap::new(),
            interfaces: HashMap::new(),
            config,
            dna_loader: Arc::new(Box::new(Self::load_dna)),
        }
    }

    pub fn start_interfaces(&mut self) {
        
    }


    /// Starts all instances
    pub fn start_all(&mut self) {
        let _ = self.instances.iter_mut().for_each(|(id, hc)| {
            println!("Starting instance \"{}\"...", id);
            match hc.lock().unwrap().start() {
                Ok(()) => println!("ok"),
                Err(err) => println!("Error: {}", err),
            }
        });
    }

    /// Stops all instances
    pub fn stop_all(&mut self) {
        let _ = self.instances.iter_mut().for_each(|(id, hc)| {
            println!("Stopping instance \"{}\"...", id);
            match hc.lock().unwrap().stop() {
                Ok(()) => println!("ok"),
                Err(err) => println!("Error: {}", err),
            }
        });
    }

    /// Stop and clear all instances
    pub fn shutdown(&mut self) {
        self.stop_all();
        self.instances = HashMap::new();
    }

    /// Tries to create all instances configured in the given Configuration object.
    /// Calls `Configuration::check_consistency()` first and clears `self.instances`.
    pub fn load_config(&mut self, config: &Configuration) -> Result<(), String> {
        let _ = config.check_consistency()?;
        self.shutdown();
        let id_instance_pairs = config
            .instance_ids()
            .clone()
            .into_iter()
            .map(|id| {
                (
                    id.clone(),
                    instantiate_from_config(&id, config, &mut self.dna_loader),
                )
            })
            .collect::<Vec<_>>();

        let errors = id_instance_pairs
            .into_iter()
            .filter_map(|(id, maybe_holochain)| match maybe_holochain {
                Ok(holochain) => {
                    self.instances.insert(id.clone(), Mutex::new(holochain));
                    None
                }
                Err(error) => Some(format!(
                    "Error while trying to create instance \"{}\": {}",
                    id, error
                )),
            })
            .collect::<Vec<_>>();

        if errors.len() == 0 {
            Ok(())
        } else {
            Err(errors.iter().nth(0).unwrap().clone())
        }
    }

    /// Default DnaLoader that actually reads files from the filesystem
    #[cfg_attr(tarpaulin, skip)] // This function is mocked in tests
    fn load_dna(file: &String) -> Result<Dna, HolochainError> {
        let mut f = File::open(file)?;
        let mut contents = String::new();
        f.read_to_string(&mut contents)?;
        Dna::try_from(JsonString::from(contents))
    }
}

impl<'a> TryFrom<&'a Configuration> for Container {
    type Error = HolochainError;
    fn try_from(config: &'a Configuration) -> Result<Self, Self::Error> {
        let mut container = Container::with_config(config);
        container
            .load_config(config)
            .map_err(|string| HolochainError::ConfigError(string))?;
        Ok(container)
    }
}

/// Creates one specific Holochain instance from a given Configuration,
/// id string and DnaLoader.
fn instantiate_from_config(
    id: &String,
    config: &Configuration,
    dna_loader: &mut DnaLoader,
) -> Result<Holochain, String> {
    let _ = config.check_consistency()?;

    config
        .instance_by_id(&id)
        .ok_or(String::from("Instance not found in config"))
        .and_then(|instance_config| {
            let agent_config = config.agent_by_id(&instance_config.agent).unwrap();
            let dna_config = config.dna_by_id(&instance_config.dna).unwrap();
            let dna = Arc::get_mut(dna_loader).unwrap()(&dna_config.file).map_err(|_| {
                HolochainError::ConfigError(format!(
                    "Could not load DNA file \"{}\"",
                    dna_config.file
                ))
            })?;

            let context: Context = match instance_config.storage {
                StorageConfiguration::File { path } => create_context(&agent_config.id, &path)
                    .map_err(|hc_err| format!("Error creating context: {}", hc_err.to_string())),
                _ => Err("Only file storage supported currently".to_string()),
            }?;

            Holochain::new(dna, Arc::new(context)).map_err(|hc_err| hc_err.to_string())
        })
}

#[derive(Clone, Debug)]
struct NullLogger {}

impl Logger for NullLogger {
    fn log(&mut self, _msg: String) {}
}

fn create_context(_: &String, path: &String) -> Result<Context, HolochainError> {
    let agent = Agent::generate_fake("c+bob");
    let cas_path = format!("{}/cas", path);
    let eav_path = format!("{}/eav", path);
    create_path_if_not_exists(&cas_path)?;
    create_path_if_not_exists(&eav_path)?;

    let res = P2pNetwork::new(
        Box::new(|_r| Ok(())),
        &json!({
            "backend": "mock"
        }).into(),
    ).unwrap();

    let file_storage = Arc::new(RwLock::new(FilesystemStorage::new(&cas_path)?));

    Context::new(
        agent,
        Arc::new(Mutex::new(NullLogger {})),
        Arc::new(Mutex::new(SimplePersister::new(file_storage.clone()))),
        file_storage.clone(),
        Arc::new(RwLock::new(EavFileStorage::new(eav_path)?)),
        Arc::new(Mutex::new(res)),
    )
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use config::load_configuration;

    pub fn test_dna_loader() -> DnaLoader {
        let loader = Box::new(|_path: &String| Ok(Dna::new()))
            as Box<FnMut(&String) -> Result<Dna, HolochainError> + Send>;
        Arc::new(loader)
    }

    fn test_toml<'a>() -> &'a str {
        r#"
    [[agents]]
    id = "test agent"
    name = "Holo Tester"
    key_file = "holo_tester.key"

    [[dnas]]
    id = "app spec rust"
    file = "app_spec.hcpkg"
    hash = "Qm328wyq38924y"

    [[instances]]
    id = "app spec instance"
    dna = "app spec rust"
    agent = "test agent"
    [instances.logger]
    type = "simple"
    file = "app_spec.log"
    [instances.storage]
    type = "file"
    path = "tmp-storage"

    "#
    }

    //#[test]
    // TODO
    // Deactivating this test because tests running in parallel creating Holochain instances
    // currently fail with:
    // "Error creating context: Failed to create actor in system: Failed to create actor.
    // Cause: An actor at the same path already exists"
    // This needs to be fixed in another PR.
    // #[cfg_attr(tarpaulin, skip)]
    // fn test_instantiate_from_config() {
    //     let config = load_configuration::<Configuration>(test_toml()).unwrap();
    //     let maybe_holochain = instantiate_from_config(
    //         &"app spec instance".to_string(),
    //         &config,
    //         &mut test_dna_loader(),
    //     );
    //
    //     assert_eq!(maybe_holochain.err(), None);
    // }

    #[test]
    fn test_container_load_config() {
        let config = load_configuration::<Configuration>(test_toml()).unwrap();

        let mut container = Container {
            instances: HashMap::new(),
            interfaces: HashMap::new(),
            dna_loader: test_dna_loader(),
        };

        assert!(container.load_config(&config).is_ok());
        assert_eq!(container.instances.len(), 1);

        container.start_all();
        container.stop_all();
    }

    #[test]
    fn test_container_try_from_configuration() {
        let config = load_configuration::<Configuration>(test_toml()).unwrap();

        let maybe_container = Container::try_from(&config);

        assert!(maybe_container.is_err());
        assert_eq!(
            maybe_container.err().unwrap(),
            HolochainError::ConfigError(
                "Error while trying to create instance \"app spec instance\": Could not load DNA file \"app_spec.hcpkg\"".to_string()
            )
        );
    }
}
