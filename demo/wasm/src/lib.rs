#![feature(wasm_import_memory, custom_attribute)]
#![wasm_import_memory]

extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;

use serde::Deserialize;

extern "C" {
    fn print(i: i32);
}

#[no_mangle]
pub extern "C" fn _call(input_data: *mut u8, input_len: usize) -> i32 {
    unsafe {
        print(input_len as i32);
        for i in 0..input_len {
            //print(i as i32);
            //print(888);
            print(*input_data.offset(i as isize) as i32);
        }
    }

    return 0; //a * a;
}

use std::ffi::CStr;
use std::os::raw::c_char;

fn make_internal<T: Deserialize>(data: *mut c_char) -> T {
    let c_str = unsafe { CStr::from_ptr(data) };
    let actual_str = c_str.to_str().unwrap(); // Don't unwrap ever in real life
    serde_json::from_str(actual_str).unwrap() // OMG you're still doing it! Have you learned nothing?!
}

fn make_external<T: Serialize>(data: *mut c_char, params_len: usize, internal: T) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn test_dispatch(data: *mut c_char, params_len: usize) -> i32 {
    let input = make_internal(data);
    let output = test(input);
    // make_external(data, params_len, output)
}

#[derive(Deserialize)]
struct InputStruct {
    input_int_val: u8,
    input_str_val: String,
}

#[derive(Serialize)]
struct OutputStruct {
    input_int_val_plus2: u8,
    input_str_val_plus_dog: String,
}

fn test(input: InputStruct) -> OutputStruct {}
