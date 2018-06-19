// In this example we execute a contract funciton exported as "_call"

extern crate wabt;
extern crate wasmi;

use self::wasmi::{
    Error as InterpreterError, Externals, FuncInstance, FuncRef, ImportsBuilder,
    ModuleImportResolver, ModuleInstance, RuntimeArgs, RuntimeValue, Signature, Trap, ValueType,
};

#[derive(Clone)]
pub struct Runtime<'a> {
    print_output: Vec<u32>,
    pub result: &'a str,
}

#[allow(dead_code)]
pub fn call(wasm: Vec<u8>, function_name: &str) -> Result<Runtime, InterpreterError> {
    let module = wasmi::Module::from_buffer(wasm).unwrap();

    const PRINT_FUNC_INDEX: usize = 0;

    impl<'a> Externals for Runtime<'a> {
        fn invoke_index(
            &mut self,
            index: usize,
            args: RuntimeArgs,
        ) -> Result<Option<RuntimeValue>, Trap> {
            match index {
                PRINT_FUNC_INDEX => {
                    let arg: u32 = args.nth(0);
                    self.print_output.push(arg);
                    Ok(None)
                }
                _ => panic!("unknown function index"),
            }
        }
    }

    struct RuntimeModuleImportResolver;

    impl ModuleImportResolver for RuntimeModuleImportResolver {
        fn resolve_func(
            &self,
            field_name: &str,
            _signature: &Signature,
        ) -> Result<FuncRef, InterpreterError> {
            let func_ref = match field_name {
                "print" => FuncInstance::alloc_host(
                    Signature::new(&[ValueType::I32][..], None),
                    PRINT_FUNC_INDEX,
                ),
                _ => {
                    return Err(InterpreterError::Function(format!(
                        "host module doesn't export function with name {}",
                        field_name
                    )))
                }
            };
            Ok(func_ref)
        }
    }

    let mut imports = ImportsBuilder::new();
    imports.push_resolver("env", &RuntimeModuleImportResolver);

    let main = ModuleInstance::new(&module, &imports)
        .expect("Failed to instantiate module")
        .assert_no_start();

    let memory = main
        .export_by_name("memory")
        .expect("all modules compiled with rustc should have an export named 'memory'; qed")
        .as_memory()
        .expect("in module generated by rustc export named 'memory' should be a memory; qed")
        .clone();

    let parameters = vec![6u8, 7u8, 8u8];
    memory
        .set(0, &parameters)
        .expect("memory should be writable");

    let mut runtime = Runtime {
        print_output: vec![],
        result: "",
    };
    main.invoke_export(function_name, &[], &mut runtime)?;
    Ok(runtime.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    fn test_wasm() -> Vec<u8> {
        use std::io::prelude::*;
        let mut file = File::open(
            "src/nucleus/wasm-test/target/wasm32-unknown-unknown/release/wasm_ribosome_test.wasm",
        ).unwrap();
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).unwrap();
        return buf;
    }

    #[test]
    fn test_print() {
        let runtime = call(test_wasm(), "test_print").expect("test_print should be callable");
        assert_eq!(runtime.print_output.len(), 1);
    }
}
