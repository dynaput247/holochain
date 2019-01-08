extern crate metadeps;

use std::env;
use std::path::Path;

// fn prefix_dir(env_name: &str, dir: &str) -> Option<String> {
//     env::var(env_name).ok().or_else(|| {
//         env::var("LIBZMQ_PREFIX").ok()
//             .map(|prefix| Path::new(&prefix).join(dir))
//             .and_then(|path| path.to_str().map(|p| p.to_owned()))
//     })
// }

fn prefix_dir(dir: &str) -> Option<String> {
    env::var("CARGO_MANIFEST_DIR").ok()
        .map(|prefix| Path::new(&prefix).join("vendor").join("zmq").join(dir))
        .and_then(|path| path.to_str().map(|p| p.to_owned()))
}

fn main() {
    // println!("fooo!");
    // let dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    // let prefix_dir = format!("{}/zmq-sys/vendor/zmq", dir);
    // #[cfg(not(windows))]
    println!("cargo:rustc-link-search=native={}", &prefix_dir("lib").unwrap());
    // #[cfg(windows)]
    // println!("cargo:rustc-link-search=native={}", &prefix_dir("bin").unwrap());
    println!("cargo:rustc-env=PATH={}:{}", env::var("PATH").unwrap(), prefix_dir("bin").unwrap());
    println!("cargo:include={}", &prefix_dir("include").unwrap());

    // let lib_path = prefix_dir("LIBZMQ_LIB_DIR", "lib");
    // let include = prefix_dir("LIBZMQ_INCLUDE_DIR", "include");
    //
    // match (lib_path, include) {
    //     (Some(lib_path), Some(include)) => {
    //         println!("cargo:rustc-link-search=native={}", lib_path);
    //         println!("cargo:include={}", include);
    //     }
    //     (Some(_), None) => {
    //         panic!("Unable to locate libzmq include directory.")
    //     }
    //     (None, Some(_)) => {
    //         panic!("Unable to locate libzmq library directory.")
    //     }
    //     (None, None) => {
    //         if let Err(e) = metadeps::probe() {
    //             panic!("Unable to locate libzmq:\n{}", e);
    //         }
    //     }
    // }
}
