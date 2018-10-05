use holochain_wasm_utils::{
    memory_allocation::{SinglePageAllocation, SinglePageStack, U16_MAX},
    error::RibosomeErrorCode,
};

use wasmi::{MemoryRef, ModuleRef};

//--------------------------------------------------------------------------------------------------
// WASM Memory Manager
//--------------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
/// Struct for managing a WASM Memory Instance as a single page memory stack
pub struct SinglePageManager {
    stack: SinglePageStack,
    wasm_memory: MemoryRef,
}

/// A Memory Manager limited to one memory page that works like a stack
/// With this Memory Manager, Host and WASM pass around only a i32.
/// That i32 is the last memory allocation on the stack: a i16 offset and a i16 length
/// (which fits with the 64KiB sized of a memory Page).
/// Complex Input arguments should be stored on the latest allocation on the stack.
/// Complex Output arguments can be stored anywhere on stack.
/// ErrorCode passing is also made possible by convention:
/// using i16 offset as error code and i16 length to zero to indicate its an error code.
///
/// In the future we could do same with i32 -> i64 and handle multiple memory Pages.
#[allow(unknown_lints)]
#[allow(cast_lossless)]
impl SinglePageManager {
    pub fn new(wasm_instance: &ModuleRef) -> Self {
        // get wasm memory reference from module
        let wasm_memory = wasm_instance
            .export_by_name("memory")
            .expect("all modules compiled with rustc should have an export named 'memory'; qed")
            .as_memory()
            .expect("in module generated by rustc export named 'memory' should be a memory; qed")
            .clone();

        return SinglePageManager {
            stack: SinglePageStack::default(),
            wasm_memory: wasm_memory.clone(),
        };
    }

    /// Allocate on stack without writing in it
    pub fn allocate(&mut self, length: u16) -> Result<SinglePageAllocation, RibosomeErrorCode> {
        if self.stack.top() as u32 + length as u32 > U16_MAX {
            return Err(RibosomeErrorCode::OutOfMemory);
        }
        let offset = self.stack.allocate(length);
        SinglePageAllocation::new(offset, length)
    }

    /// Write data on top of stack
    pub fn write(&mut self, data: &[u8]) -> Result<SinglePageAllocation, RibosomeErrorCode> {
        let data_len = data.len();
        if data_len > 65536 {
            return Err(RibosomeErrorCode::OutOfMemory);
        }

        // scope for mutable borrow of self
        let mem_buf: SinglePageAllocation;
        {
            let res = self.allocate(data_len as u16);
            if res.is_err() {
                return Err(RibosomeErrorCode::OutOfMemory);
            }
            mem_buf = res.unwrap();
        }

        self.wasm_memory
            .set(mem_buf.offset() as u32, &data)
            .expect("memory should be writable");
        Ok(mem_buf)
    }

    /// Read data somewhere in stack
    pub fn read(&self, allocation: SinglePageAllocation) -> Vec<u8> {
        return self
            .wasm_memory
            .get(allocation.offset() as u32, allocation.length() as usize)
            .expect("Successfully retrieve the result");
    }
}
