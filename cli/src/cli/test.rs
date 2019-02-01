use crate::{cli::package, error::DefaultResult, util};
use colored::*;
use std::{fs, path::PathBuf};

pub const TEST_DIR_NAME: &str = "test";
pub const DIST_DIR_NAME: &str = "dist";

pub fn test(
    path: &PathBuf,
    tests_folder: &str,
    testfile: &str,
    skip_build: bool,
) -> DefaultResult<()> {
    // create dist folder
    let dist_path = path.join(&DIST_DIR_NAME);

    if !dist_path.exists() {
        fs::create_dir(dist_path.as_path())?;
    }

    if !skip_build {
        // build the package file, within the dist folder
        let bundle_file_path = dist_path.join(package::DEFAULT_BUNDLE_FILE_NAME);
        println!(
            "{} files for testing to file: {:?}",
            "Packaging".green().bold(),
            bundle_file_path
        );
        package(true, Some(bundle_file_path.to_path_buf()))?;
    }

    // build tests
    let tests_path = path.join(&tests_folder);
    ensure!(
        tests_path.exists(),
        "Directory {} does not exist",
        tests_folder
    );

    // npm install, if no node_modules yet
    let node_modules_path = tests_path.join("node_modules");
    if !node_modules_path.exists() {
        // CLI feedback
        println!("{}", "Installing node_modules".green().bold());
        util::run_cmd(
            tests_path.clone(),
            "npm".to_string(),
            &["install", "--silent"],
        )?;
    }

    // execute the built test file using node
    // CLI feedback
    println!("{} tests in {}", "Running".green().bold(), testfile,);
    util::run_cmd(
        path.to_path_buf(),
        "node".to_string(),
        &[testfile.to_string().as_str()],
    )?;

    Ok(())
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::cli::init::{init, tests::gen_dir};
    //    use assert_cmd::prelude::*;
    //    use std::{env, process::Command};

    #[test]
    // flagged as broken for:
    // 1. taking 60+ seconds
    // 2. because `generate_cargo_toml` in cli/src/scaffold/rust.rs sets the
    //    branch to a fixed value rather than develop and currently there's no way to
    //    adjust that on the fly.
    // 3. because holochain-nodejs version doesn't exist yet
    #[cfg(feature = "broken-tests")]
    fn test_command_basic_test() {
        let temp_dir = gen_dir();
        let temp_dir_path = temp_dir.path();
        let temp_dir_path_buf = temp_dir_path.to_path_buf();

        let mut gen_cmd = Command::main_binary().unwrap();

        let _ = init(&temp_dir_path_buf);

        assert!(env::set_current_dir(&temp_dir_path).is_ok());

        // do gen my_zome first, so there's a zome
        gen_cmd
            .args(&["generate", "zomes/my_zome"])
            .assert()
            .success();

        test(&temp_dir_path_buf, &TEST_DIR_NAME, "test/index.js", false)
            .unwrap_or_else(|e| panic!("test call failed: {}", e));

        // check success of packaging step
        assert!(temp_dir_path_buf
            .join(&DIST_DIR_NAME)
            .join(package::DEFAULT_BUNDLE_FILE_NAME)
            .exists());

        // check success of npm install step
        assert!(temp_dir_path_buf
            .join(&TEST_DIR_NAME)
            .join("node_modules")
            .exists());
    }

    #[test]
    fn test_command_no_test_folder() {
        let temp_dir = gen_dir();
        let temp_dir_path = temp_dir.path();
        let temp_dir_path_buf = temp_dir_path.to_path_buf();

        let _ = init(&temp_dir_path_buf);

        let result = test(&temp_dir_path_buf, "west", "test/index.js", false);

        // should err because "west" directory doesn't exist
        assert!(result.is_err());
    }
}
