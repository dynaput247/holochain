#![feature(try_from)]
extern crate holochain_container_api;
extern crate holochain_core;
extern crate holochain_core_types;
extern crate tempfile;
extern crate test_utils;
#[macro_use]
extern crate serde_json;
#[macro_use]
extern crate serde_derive;
extern crate hdk;
extern crate holochain_wasm_utils;
#[macro_use]
extern crate holochain_core_types_derive;

use hdk::error::ZomeApiError;
use holochain_container_api::*;
use holochain_core_types::{
    cas::content::Address,
    dna::zome::{
        capabilities::{Capability, FnDeclaration, Membrane},
        entry_types::{EntryTypeDef, LinksTo},
    },
    entry::{
        entry_type::{test_app_entry_type, AppEntryType, EntryType},
        AppEntryValue, Entry,
    },
    error::{CoreError, HcResult, HolochainError, ZomeApiInternalResult},
    hash::HashString,
    json::JsonString,
};
use holochain_wasm_utils::api_serialization::get_links::GetLinksResult;
use std::sync::{Arc, Mutex};
use test_utils::*;

pub fn create_test_cap_with_fn_names(fn_names: Vec<&str>) -> Capability {
    let mut capability = Capability::new();
    capability.cap_type.membrane = Membrane::Public;

    for fn_name in fn_names {
        let mut fn_decl = FnDeclaration::new();
        fn_decl.name = String::from(fn_name);
        capability.functions.push(fn_decl);
    }
    capability
}

#[derive(DefaultJson, Serialize, Deserialize, Debug)]
struct EntryStruct {
    stuff: String,
}

fn example_valid_entry() -> Entry {
    Entry::App(
        AppEntryType::from(test_app_entry_type()),
        AppEntryValue::from(EntryStruct {
            stuff: "non fail".into(),
        }),
    )
}

fn example_valid_entry_params() -> String {
    format!(
        "{{\"entry\":{}}}",
        String::from(JsonString::from(example_valid_entry())),
    )
}

fn example_valid_entry_address() -> Address {
    Address::from("QmefcRdCAXM2kbgLW2pMzqWhUvKSDvwfFSVkvmwKvBQBHd")
}

fn start_holochain_instance() -> (Holochain, Arc<Mutex<TestLogger>>) {
    // Setup the holochain instance
    let wasm =
        create_wasm_from_file("wasm-test/target/wasm32-unknown-unknown/release/test_globals.wasm");
    let capabability = create_test_cap_with_fn_names(vec![
        "check_global",
        "check_commit_entry",
        "check_commit_entry_macro",
        "check_get_entry_result",
        "check_get_entry",
        "send_tweet",
        "commit_validation_package_tester",
        "link_two_entries",
        "links_roundtrip",
        "link_validation",
        "check_query",
        "check_app_entry_address",
        "check_sys_entry_address",
        "check_call",
        "check_call_with_args",
    ]);
    let mut dna = create_test_dna_with_cap("test_zome", "test_cap", &capabability, &wasm);

    // TODO: construct test DNA using the auto-generated JSON feature
    // The code below is fragile!
    // We have to manually construct a Dna struct that reflects what we defined using define_zome!
    // in wasm-test/src/lib.rs.
    // In a production setting, hc would read the auto-generated JSON to make sure the Dna struct
    // matches up. We should do the same in test.
    {
        let entry_types = &mut dna.zomes.get_mut("test_zome").unwrap().entry_types;
        entry_types.insert(
            EntryType::from("validation_package_tester"),
            EntryTypeDef::new(),
        );

        let test_entry_type = &mut entry_types
            .get_mut(&EntryType::from("testEntryType"))
            .unwrap();
        test_entry_type.links_to.push(LinksTo {
            target_type: String::from("testEntryType"),
            tag: String::from("test-tag"),
        });
    }

    {
        let entry_types = &mut dna.zomes.get_mut("test_zome").unwrap().entry_types;
        let mut link_validator = EntryTypeDef::new();
        link_validator.links_to.push(LinksTo {
            target_type: String::from("link_validator"),
            tag: String::from("longer"),
        });
        entry_types.insert(EntryType::from("link_validator"), link_validator);
    }

    let (context, test_logger) = test_context_and_logger("alex");
    let mut hc =
        Holochain::new(dna.clone(), context).expect("could not create new Holochain instance.");

    // Run the holochain instance
    hc.start().expect("couldn't start");
    (hc, test_logger)
}

#[test]
fn can_use_globals() {
    let (mut hc, _) = start_holochain_instance();
    // Call the exposed wasm function that calls the debug API function for printing all GLOBALS
    let result = hc.call("test_zome", "test_cap", "check_global", r#"{}"#);
    assert_eq!(
        result.clone(),
        Ok(JsonString::from(HashString::from(
            "QmfFVhScc1cVzEqTBVLBr6d2FbsHaM5Cn3ynnvM7CUiJp9"
        ))),
        "result = {:?}",
        result
    );
}

#[test]
fn can_commit_entry() {
    let (mut hc, _) = start_holochain_instance();

    // Call the exposed wasm function that calls the Commit API function
    let result = hc.call(
        "test_zome",
        "test_cap",
        "check_commit_entry",
        &String::from(JsonString::from(example_valid_entry())),
    );
    println!("\t result = {:?}", result);
    assert!(result.is_ok(), "result = {:?}", result);
    assert_eq!(
        result.unwrap(),
        JsonString::from(example_valid_entry_address()),
    );
}
#[test]
fn can_commit_entry_macro() {
    let (mut hc, _) = start_holochain_instance();
    // Call the exposed wasm function that calls the Commit API function
    let result = hc.call(
        "test_zome",
        "test_cap",
        "check_commit_entry_macro",
        &example_valid_entry_params(),
    );
    println!("\t result = {:?}", result);
    assert!(result.is_ok(), "\t result = {:?}", result);
    assert_eq!(
        result.unwrap(),
        JsonString::from(Address::from(
            "QmefcRdCAXM2kbgLW2pMzqWhUvKSDvwfFSVkvmwKvBQBHd"
        )),
    );
}

#[test]
fn can_round_trip() {
    let (mut hc, test_logger) = start_holochain_instance();
    let result = hc.call(
        "test_zome",
        "test_cap",
        "send_tweet",
        r#"{ "author": "bob", "content": "had a boring day" }"#,
    );
    assert_eq!(
        result.unwrap(),
        JsonString::from("{\"first\":\"bob\",\"second\":\"had a boring day\"}"),
    );

    let test_logger = test_logger.lock().unwrap();

    println!("{:?}", *test_logger);
}

#[test]
#[cfg(not(windows))]
fn can_get_entry() {
    let (mut hc, _) = start_holochain_instance();
    // Call the exposed wasm function that calls the Commit API function
    let result = hc.call(
        "test_zome",
        "test_cap",
        "check_commit_entry_macro",
        &example_valid_entry_params(),
    );
    assert!(result.is_ok(), "\t result = {:?}", result);
    assert_eq!(
        result.unwrap(),
        JsonString::from(example_valid_entry_address()),
    );

    let result = hc.call(
        "test_zome",
        "test_cap",
        "check_get_entry_result",
        &String::from(JsonString::from(json!({
            "entry_address": example_valid_entry_address()
        }))),
    );
    assert!(result.is_ok(), "\t result = {:?}", result);
    assert_eq!(result.unwrap(), JsonString::from(example_valid_entry()));

    let result = hc.call(
        "test_zome",
        "test_cap",
        "check_get_entry",
        &String::from(JsonString::from(json!({
            "entry_address": example_valid_entry_address()
        }))),
    );
    println!("\t can_get_entry result = {:?}", result);
    assert!(result.is_ok(), "\t result = {:?}", result);
    assert_eq!(result.unwrap(), JsonString::from(example_valid_entry()),);

    // test the case with a bad address
    let result = hc.call(
        "test_zome",
        "test_cap",
        "check_get_entry_result",
        &String::from(JsonString::from(json!(
                    {"entry_address": Address::from("QmbC71ggSaEa1oVPTeNN7ZoB93DYhxowhKSF6Yia2Vjxxx")}
                ))),
    );
    println!("\t can_get_entry_result result = {:?}", result);
    assert!(result.is_ok(), "\t result = {:?}", result);
    assert_eq!(result.unwrap(), JsonString::null());

    // test the case with a bad address
    let result = hc.call(
        "test_zome",
        "test_cap",
        "check_get_entry",
        &String::from(JsonString::from(json!(
                    {"entry_address": Address::from("QmbC71ggSaEa1oVPTeNN7ZoB93DYhxowhKSF6Yia2Vjxxx")}
                ))),
    );
    println!("\t can_get_entry result = {:?}", result);
    assert!(result.is_ok(), "\t result = {:?}", result);
    assert_eq!(result.unwrap(), JsonString::null());
}

#[test]
#[cfg(not(windows))] // TODO does not work on windows because of different seperator
fn can_invalidate_invalid_commit() {
    let (mut hc, _) = start_holochain_instance();
    // Call the exposed wasm function that calls the Commit API function
    let result = hc.call(
        "test_zome",
        "test_cap",
        "check_commit_entry_macro",
        &json!({"entry":
            Entry::App(
                AppEntryType::from(test_app_entry_type()),
                AppEntryValue::from(EntryStruct {
                    stuff: "FAIL".into(),
                }),
            )
        })
        .to_string(),
    );
    println!("\t result = {:?}", result);
    assert!(result.is_ok(), "result = {:?}", result);
    assert_eq!(
        result.unwrap(),
        JsonString::from("{\"error\":{\"Internal\":\"{\\\"kind\\\":{\\\"ValidationFailed\\\":\\\"FAIL content is not allowed\\\"},\\\"file\\\":\\\"core/src/nucleus/ribosome/runtime.rs\\\",\\\"line\\\":\\\"86\\\"}\"}}"),
    );
}

#[test]
fn has_populated_validation_data() {
    let (mut hc, _) = start_holochain_instance();

    //
    // Add two entries to chain to have something to check ValidationData on
    //
    let result = hc.call(
        "test_zome",
        "test_cap",
        "check_commit_entry_macro",
        &example_valid_entry_params(),
    );
    assert!(result.is_ok(), "\t result = {:?}", result);
    assert_eq!(
        result.unwrap(),
        JsonString::from(example_valid_entry_address()),
    );
    let result = hc.call(
        "test_zome",
        "test_cap",
        "check_commit_entry_macro",
        &example_valid_entry_params(),
    );
    assert!(result.is_ok(), "\t result = {:?}", result);
    assert_eq!(
        result.unwrap(),
        JsonString::from(example_valid_entry_address()),
    );

    //
    // Expect the commit in this zome function to fail with a serialized ValidationData struct
    //
    let result = hc.call(
        "test_zome",
        "test_cap",
        "commit_validation_package_tester",
        r#"{}"#,
    );

    assert!(result.is_ok(), "\t result = {:?}", result);

    //
    // Deactivating this test for now since ordering of contents change non-deterministically
    //
    /*
    assert_eq!(
        JsonString::from("{\"Err\":{\"Internal\":\"{\\\"package\\\":{\\\"chain_header\\\":{\\\"entry_type\\\":{\\\"App\\\":\\\"validation_package_tester\\\"},\\\"entry_address\\\":\\\"QmYQPp1fExXdKfmcmYTbkw88HnCr3DzMSFUZ4ncEd9iGBY\\\",\\\"entry_signature\\\":\\\"\\\",\\\"link\\\":\\\"QmSQqKHPpYZbafF7PXPKx31UwAbNAmPVuSHHxcBoDcYsci\\\",\\\"link_same_type\\\":null,\\\"timestamp\\\":\\\"\\\"},\\\"source_chain_entries\\\":[{\\\"value\\\":\\\"\\\\\\\"non fail\\\\\\\"\\\",\\\"entry_type\\\":\\\"testEntryType\\\"},{\\\"value\\\":\\\"\\\\\\\"non fail\\\\\\\"\\\",\\\"entry_type\\\":\\\"testEntryType\\\"},{\\\"value\\\":\\\"alex\\\",\\\"entry_type\\\":\\\"%agent_id\\\"}],\\\"source_chain_headers\\\":[{\\\"entry_type\\\":{\\\"App\\\":\\\"testEntryType\\\"},\\\"entry_address\\\":\\\"QmXxdzM9uHiSfV1xDwUxMm5jX4rVU8jhtWVaeCzjkFW249\\\",\\\"entry_signature\\\":\\\"\\\",\\\"link\\\":\\\"QmRHUwiUuFJiMyRmKaA1U49fXEnT8qbZMoj2V9maa4Q3JE\\\",\\\"link_same_type\\\":\\\"QmRHUwiUuFJiMyRmKaA1U49fXEnT8qbZMoj2V9maa4Q3JE\\\",\\\"timestamp\\\":\\\"\\\"},{\\\"entry_type\\\":{\\\"App\\\":\\\"testEntryType\\\"},\\\"entry_address\\\":\\\"QmXxdzM9uHiSfV1xDwUxMm5jX4rVU8jhtWVaeCzjkFW249\\\",\\\"entry_signature\\\":\\\"\\\",\\\"link\\\":\\\"QmRYerwRRXYxmYoxq1LTZMVVRfjNMAeqmdELTNDxURtHEZ\\\",\\\"link_same_type\\\":null,\\\"timestamp\\\":\\\"\\\"},{\\\"entry_type\\\":\\\"AgentId\\\",\\\"entry_address\\\":\\\"QmQw3V41bAWkQA9kwpNfU3ZDNzr9YW4p9RV4QHhFD3BkqA\\\",\\\"entry_signature\\\":\\\"\\\",\\\"link\\\":\\\"QmQJxUSfJe2QoxTyEwKQX9ypbkcNv3cw1vasGTx1CUpJFm\\\",\\\"link_same_type\\\":null,\\\"timestamp\\\":\\\"\\\"}],\\\"custom\\\":null},\\\"sources\\\":[\\\"<insert your agent key here>\\\"],\\\"lifecycle\\\":\\\"Chain\\\",\\\"action\\\":\\\"Commit\\\"}\"}}"),
        result.unwrap(),
    );
    */}

#[test]
fn can_link_entries() {
    let (mut hc, _) = start_holochain_instance();

    let result = hc.call("test_zome", "test_cap", "link_two_entries", r#"{}"#);
    assert!(result.is_ok(), "\t result = {:?}", result);
    assert_eq!(result.unwrap(), JsonString::from(r#"{"Ok":null}"#));
}

#[test]
fn can_roundtrip_links() {
    let (mut hc, _) = start_holochain_instance();
    let result = hc.call("test_zome", "test_cap", "links_roundtrip", r#"{}"#);
    assert!(result.is_ok(), "result = {:?}", result);
    let result_string = result.unwrap();

    let address_1 = Address::from("QmdQVqSuqbrEJWC8Va85PSwrcPfAB3EpG5h83C3Vrj62hN");
    let address_2 = Address::from("QmPn1oj8ANGtxS5sCGdKBdSBN63Bb6yBkmWrLc9wFRYPtJ");

    println!("can_roundtrip_links result_string: {:?}", result_string);
    let expected: HcResult<GetLinksResult> = Ok(GetLinksResult::new(vec![
        address_1.clone(),
        address_2.clone(),
    ]));
    let ordering1: bool = result_string == JsonString::from(expected);

    let expected: HcResult<GetLinksResult> = Ok(GetLinksResult::new(vec![address_2, address_1]));
    let ordering2: bool = result_string == JsonString::from(expected);

    assert!(ordering1 || ordering2, "result = {:?}", result_string);
}

#[test]
#[cfg(not(windows))]
fn can_validate_links() {
    let (mut hc, _) = start_holochain_instance();
    let params_ok = r#"{"stuff1": "a", "stuff2": "aa"}"#;
    let result = hc.call("test_zome", "test_cap", "link_validation", params_ok);
    assert!(result.is_ok(), "result = {:?}", result);

    let params_not_ok = r#"{"stuff1": "aaa", "stuff2": "aa"}"#;
    let result = hc.call("test_zome", "test_cap", "link_validation", params_not_ok);
    assert!(result.is_ok(), "result = {:?}", result);
    // Yep, the zome call is ok but what we got back should be a ValidationFailed error,
    // wrapped in a CoreError, wrapped in a ZomeApiError, wrapped in a Result,
    // serialized to JSON :D
    let zome_result: Result<(), ZomeApiError> =
        serde_json::from_str(&result.unwrap().to_string()).unwrap();
    assert!(zome_result.is_err());
    if let ZomeApiError::Internal(error) = zome_result.err().unwrap() {
        let core_error: CoreError = serde_json::from_str(&error).unwrap();
        assert_eq!(
            core_error.kind,
            HolochainError::ValidationFailed("Target stuff is not longer".to_string()),
        );
    } else {
        assert!(false);
    }
}

#[test]
fn can_check_query() {
    let (mut hc, _) = start_holochain_instance();

    let result = hc.call(
        "test_zome",
        "test_cap",
        "check_query",
        r#"{ "entry_type_name": "testEntryType", "limit": "0" }"#,
    );
    assert!(result.is_ok(), "result = {:?}", result);
    assert_eq!(
        result.unwrap(),
        JsonString::from(vec![Address::from(
            "QmPn1oj8ANGtxS5sCGdKBdSBN63Bb6yBkmWrLc9wFRYPtJ",
        )]),
    );
}

#[test]
fn can_check_app_entry_address() {
    let (mut hc, _) = start_holochain_instance();

    let result = hc.call("test_zome", "test_cap", "check_app_entry_address", r#"{}"#);
    assert!(result.is_ok(), "result = {:?}", result);
    assert_eq!(
        result.unwrap(),
        JsonString::from(Address::from(
            "QmSbNw63sRS4VEmuqFBd7kJT6V9pkEpMRMY2LWvjNAqPcJ"
        )),
    );
}

#[test]
fn can_check_sys_entry_address() {
    let (mut hc, _) = start_holochain_instance();

    let _result = hc.call("test_zome", "test_cap", "check_sys_entry_address", r#"{}"#);
    // TODO
    //    assert!(result.is_ok(), "result = {:?}", result);
    //    assert_eq!(
    //        result.unwrap(),
    //        r#"{"result":"QmYmZyvDda3ygMhNnEjx8p9Q1TonHG9xhpn9drCptRT966"}"#,
    //    );
}

#[test]
fn can_check_call() {
    let (mut hc, _) = start_holochain_instance();

    let result = hc.call("test_zome", "test_cap", "check_call", r#"{}"#);
    assert!(result.is_ok(), "result = {:?}", result);
    assert_eq!(
        result.unwrap(),
        JsonString::from(ZomeApiInternalResult::success(Address::from(
            "QmSbNw63sRS4VEmuqFBd7kJT6V9pkEpMRMY2LWvjNAqPcJ"
        ))),
    );
}

#[test]
fn can_check_call_with_args() {
    let (mut hc, _) = start_holochain_instance();

    let result = hc.call("test_zome", "test_cap", "check_call_with_args", r#"{}"#);
    println!("\t result = {:?}", result);
    assert!(result.is_ok(), "\t result = {:?}", result);
    assert_eq!(
        result.unwrap(),
        JsonString::from(ZomeApiInternalResult::success(Address::from(
            "QmefcRdCAXM2kbgLW2pMzqWhUvKSDvwfFSVkvmwKvBQBHd"
        ))),
    );
}
