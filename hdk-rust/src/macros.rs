#[doc(hidden)]
#[macro_export]
macro_rules! load_json {
    ($encoded_allocation_of_input:ident) => {{
        let maybe_input =
            $crate::holochain_wasm_utils::memory_serialization::load_json($encoded_allocation_of_input);
        if maybe_input.is_err() {
            return $crate::holochain_wasm_utils::holochain_core_types::error::RibosomeErrorCode::ArgumentDeserializationFailed
                as u32;
        }
        maybe_input
    }};
}

/// Every Zome must utilize the `define_zome`
/// macro in the main library file in their Zome.
/// The `define_zome` macro has 3 component parts:
/// 1. entries: an array of [ValidatingEntryType](entry_definition/struct.ValidatingEntryType.html) as returned by using the [entry](macro.entry.html) macro
/// 2. genesis: `genesis` is a callback called by Holochain to every Zome implemented within a DNA.
///     It gets called when a new agent is initializing an instance of the DNA for the first time, and
///     should return `Ok` or an `Err`, depending on whether the agent can join the network or not.
/// 3. functions: `functions` is divided up into `capabilities`, which specify who can access those functions.
///     `functions` must be a tree structure where the first children are `capabilities`
///     and the children of those `capabilities` are actual function definitions.
/// # Examples
///
/// ```rust
/// #[macro_use]
/// extern crate hdk;
/// extern crate serde;
/// #[macro_use]
/// extern crate serde_derive;
/// #[macro_use]
/// extern crate serde_json;
/// extern crate boolinator;
///
/// #[derive(Serialize, Deserialize)]
/// pub struct Post {
///     content: String,
///     date_created: String,
/// }
///
/// fn handle_hash_post(content: String) -> serde_json::Value {
///     let maybe_address = hdk::hash_entry("post", json!({
///         "content": content,
///         "date_created": "now"
///     }));
///     match maybe_address {
///         Ok(address) => {
///             json!({"address": address})
///         }
///         Err(hdk_error) => hdk_error.to_json(),
///     }
/// }
///
/// define_zome! {
///     entries: [
///         entry!(
///             name: "post",
///             description: "",
///             sharing: Sharing::Public,
///             native_type: Post,
///         
///             validation_package: || {
///                 hdk::ValidationPackageDefinition::ChainFull
///             },
///         
///             validation: |post: Post, _ctx: hdk::ValidationData| {
///                 (post.content.len() < 280)
///                     .ok_or_else(|| String::from("Content too long"))
///             }
///         )
///     ]
///
///     genesis: || {
///         Ok(())
///     }
///
///     functions: {
///         // "main" is the name of the capability
///         // "Public" is the access setting of the capability
///         main (Public) {
///             // the name of this function, "hash_post" is the
///             // one to give while performing a `call` method to this function.
///             // the name of the handler function must be different than the
///             // name of the Zome function.
///             hash_post: {
///                 inputs: |content: String|,
///                 outputs: |post: serde_json::Value|,
///                 handler: handle_hash_post
///             }
///         }
///     }
/// }
/// ```
#[macro_export]
macro_rules! define_zome {
    (
        entries : [
            $( $entry_expr:expr ),*
        ]

        genesis : || {
            $genesis_expr:expr
        }

        functions : {
            $(
                $cap:ident ( $vis:ident ) {
                    $(
                        $zome_function_name:ident : {
                            inputs: | $( $input_param_name:ident : $input_param_type:ty ),* |,
                            outputs: | $( $output_param_name:ident : $output_param_type:ty ),* |,
                            handler: $handler_path:path
                        }
                    )+
                }
            )*
        }

    ) => {
        #[no_mangle]
        #[allow(unused_variables)]
        pub extern "C" fn zome_setup(zd: &mut $crate::meta::ZomeDefinition) {
            $(
                zd.define($entry_expr);
            )*
        }

        #[no_mangle]
        pub extern "C" fn genesis(encoded_allocation_of_input: u32) -> u32 {
            $crate::global_fns::init_global_memory(encoded_allocation_of_input);

            fn execute() -> Result<(), String> {
                $genesis_expr
            }

            $crate::global_fns::store_and_return_output(execute())
        }

        use $crate::holochain_dna::zome::capabilities::Capability;
        use std::collections::HashMap;

        #[no_mangle]
        #[allow(unused_imports)]
        pub fn __list_capabilities() -> HashMap<String, Capability> {

            use $crate::holochain_dna::zome::capabilities::{Capability, Membrane, CapabilityType, FnParameter, FnDeclaration};
            use std::collections::HashMap;

            let return_value: HashMap<String, Capability> = {
                let mut cap_map = HashMap::new();

                $(
                    {
                        let mut capability = Capability::new();
                        capability.cap_type = CapabilityType { membrane: Membrane::$vis };
                        capability.functions = vec![
                            $(
                                FnDeclaration {
                                    name: stringify!($zome_function_name).into(),
                                    inputs: vec![
                                        $(
                                            FnParameter::new(stringify!($input_param_name), stringify!($input_param_type))
                                        ),*
                                    ],
                                    outputs: vec![
                                        $(
                                            FnParameter::new(stringify!($output_param_name), stringify!($output_param_type))
                                        ),*
                                    ]
                                }

                            ),+
                        ];

                        cap_map.insert(stringify!($cap).into(), capability);
                    }
                ),*

                cap_map
            };

            return_value
        }

        $(
            $(
                #[no_mangle]
                pub extern "C" fn $zome_function_name(encoded_allocation_of_input: u32) -> u32 {
                    $crate::global_fns::init_global_memory(encoded_allocation_of_input);

                    // Macro'd InputStruct
                    #[derive(Deserialize)]
                    struct InputStruct {
                        $($input_param_name : $input_param_type),*
                    }

                    // #[derive(Serialize)]
                    // struct OutputStruct {
                    //     $( $output_param_name:ident : $output_param_type:ty ),*
                    // }

                    // Deserialize input
                    let maybe_input = load_json!(encoded_allocation_of_input);
                    let input: InputStruct = maybe_input.unwrap();

                    // Macro'd function body
                    fn execute(params: InputStruct) -> impl ::serde::Serialize {
                        let InputStruct { $($input_param_name),* } = params;

                        $handler_path($($input_param_name),*)
                    }

                    // Execute inner function
                    let output_obj = execute(input);

                    $crate::global_fns::store_and_return_output(output_obj)
                }
            )+
        )*
    };
}
