use failure::Error;
use colored::*;
use crate::{
    cli::{package, scaffold::Scaffold},
    config_files::Build,
    error::DefaultResult,
    util,
};
use holochain_wasm_utils::wasm_target_dir;
use std::{
    process::Command,
    fs::{self, OpenOptions},
    io::{Read, Seek, SeekFrom, Write, ErrorKind},
    path::Path,
};
use toml::{self, value::Value};

pub const CARGO_FILE_NAME: &str = "Cargo.toml";
pub const LIB_RS_PATH: &str = "src/lib.rs";

pub struct RustScaffold {
    build_template: Build,
    package_name: String,
}

/// Given existing Cargo.toml string, pull out some values and return a new
/// string with values pulled from template
fn generate_cargo_toml(name: &str, contents: &str) -> DefaultResult<String> {
    let config: Value = toml::from_str(contents)?;

    let authors_default = Value::from("[\"TODO\"]");
    let edition_default = Value::from("\"TODO\"");
    let version_default = String::from("branch = \"develop\"");
    let maybe_package = config.get("package");

    let name = Value::from(name);
    let authors = maybe_package
        .and_then(|p| p.get("authors"))
        .unwrap_or(&authors_default);
    let edition = maybe_package
        .and_then(|p| p.get("edition"))
        .unwrap_or(&edition_default);

    interpolate_cargo_template(&name, authors, edition, version_default)
}

/// Use the Cargo.toml.template file and interpolate values into the placeholders
/// TODO: consider using an actual templating engine such as https://github.com/Keats/tera
fn interpolate_cargo_template(
    name: &Value,
    authors: &Value,
    edition: &Value,
    version: String,
) -> DefaultResult<String> {
    let template = include_str!("rust/Cargo.template.toml");
    Ok(template
        .replace("<<NAME>>", toml::to_string(name)?.as_str())
        .replace("<<AUTHORS>>", toml::to_string(authors)?.as_str())
        .replace("<<EDITION>>", toml::to_string(edition)?.as_str())
        .replace("<<VERSION>>", &version))
}

impl RustScaffold {
    pub fn new(package_name: String) -> RustScaffold {
        let target_dir = wasm_target_dir(&package_name, "");
        let artifact_name = format!(
            "{}/wasm32-unknown-unknown/release/{}.wasm",
            &target_dir, &package_name,
        );
        RustScaffold {
            build_template: Build::with_artifact(artifact_name).cmd(
                "cargo",
                &[
                    "build",
                    "--release",
                    "--target=wasm32-unknown-unknown",
                    &format!("--target-dir={}", target_dir),
                ],
            ),
            package_name: package_name,
        }
    }

    /// Modify Cargo.toml in place, using pieces of the original
    fn rewrite_cargo_toml(&self, base_path: &Path) -> DefaultResult<()> {
        let cargo_file_path = base_path.join(CARGO_FILE_NAME);
        let mut cargo_file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(cargo_file_path)?;
        let mut contents = String::new();
        cargo_file.read_to_string(&mut contents)?;

        // create new Cargo.toml using pieces of the original
        let new_toml = generate_cargo_toml(self.package_name.as_str(), contents.as_str())?;
        cargo_file.seek(SeekFrom::Start(0))?;
        cargo_file.write_all(new_toml.as_bytes())?;
        Ok(())
    }

    /// Completely rewrite src/lib.rs with custom scaffold file
    fn rewrite_lib_rs(&self, base_path: &Path) -> DefaultResult<()> {
        let file_path = base_path.join(LIB_RS_PATH);
        let mut cargo_file = OpenOptions::new()
            .truncate(true)
            .write(true)
            .open(file_path)?;
        let contents = include_str!("./rust/lib.rs");
        cargo_file.write_all(contents.as_bytes())?;
        Ok(())
    }
}

impl Scaffold for RustScaffold {
    fn gen<P: AsRef<Path>>(&self, base_path: P) -> DefaultResult<()> {

        // First, check whether they have `cargo` installed
        let mut check_cargo = Command::new("cargo");
        match check_cargo.status() {
            Ok(_) => {},
            Err(e) => {
                match e.kind() {
                    ErrorKind::NotFound => {
                        println!("This command requires the `cargo` command, which is part of the Rust toolchain.");
                        println!("Before you can generate a Rust based Zome, you must install Rust.");
                        println!("As a first step, get Rust installed by using rustup https://rustup.rs/.");
                        println!("Holochain requires you use the nightly-2019-01-24 toolchain.");
                        println!("With Rust already installed switch to it by running the following commands:");
                        println!("$ rustup toolchain install nightly-2019-01-24");
                        println!("$ rustup default nightly-2019-01-24");
                        println!("Having taken those steps, retry this command.");
                        // early exit with Ok, since this is the graceful exit
                        return Ok(())
                    },
                    // convert from a std::io::Error into a failure::Error
                    // and actually return that error since it's something
                    // different than just not finding `cargo`
                    _ => return Err(Error::from(e))
                }
            }
        };

        fs::create_dir_all(&base_path)?;

        // use cargo to initialise a library Rust crate without any version control
        util::run_cmd(
            base_path.as_ref().to_path_buf(),
            "cargo".into(),
            &["init", "--lib", "--vcs", "none"],
        )?;

        // immediately rewrite the generated Cargo file, using some values
        // and throwing away the rest
        self.rewrite_cargo_toml(base_path.as_ref())?;

        // and clobber the autogenerated lib.rs with our own boilerplate
        self.rewrite_lib_rs(base_path.as_ref())?;

        // create and fill in a build file appropriate for Rust
        let build_file_path = base_path.as_ref().join(package::BUILD_CONFIG_FILE_NAME);
        self.build_template.save_as(build_file_path)?;

        // CLI feedback
        println!(
            "{} {:?} Zome",
            "Generated".green().bold(),
            self.package_name
        );

        Ok(())
    }
}
