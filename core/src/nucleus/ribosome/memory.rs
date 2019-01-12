
use wasmi::{MemoryRef, ModuleRef};
use holochain_wasm_utils::memory::MemoryInt;
use holochain_wasm_utils::memory::allocation::WasmAllocation;
use holochain_wasm_utils::memory::stack::WasmStack;
use holochain_wasm_utils::memory::allocation::AllocationResult;
use holochain_wasm_utils::memory::MemoryBits;
use holochain_wasm_utils::memory::allocation::Length;
use holochain_wasm_utils::memory::allocation::AllocationError;

//--------------------------------------------------------------------------------------------------
// WASM Memory Manager
//--------------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
/// Struct for managing a WASM Memory Instance as a single page memory stack
pub struct SinglePageManager {
    stack: WasmStack,
    wasm_memory: MemoryRef,
}

/// A Memory Manager limited to one wasm memory page that works like a stack.
/// With this Memory Manager, the WASM host (i.e. the Ribosome) and WASM module (i.e. the Zome)
/// only need to pass around an i32 to communicate any data.
/// That i32 is the last memory allocation on the stack:
/// it is split in an i16 'offset' in the upper bits and an i16 'length' in the lower bits.
/// This fits with the 64KiB sized of a memory Page.
/// Complex input arguments should be stored on the latest allocation on the stack.
/// Complex output arguments can be stored anywhere on stack.
/// Since zero sized allocations are not allowed,
/// it is possible to pass around a return and/or error code with the following convention:
/// using the i16 'offset' as return code and i16 'length' set to zero
/// to indicate its a return code.
/// Return code of 0 means success, while any other value means a failure and gives the error code.
/// In the future, to handle bigger memory needs, we could do same with an i64 instead
/// and handle multiple memory Pages.
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
            stack: WasmStack::default(),
            wasm_memory,
        };
    }

    /// Allocate on stack without writing in it
    pub fn allocate(&mut self, length: Length) -> AllocationResult {
        let allocation = self.stack.next_allocation(length)?;
        let top = self.stack.allocate(allocation)?;
        Ok(WasmAllocation::new(MemoryInt::from(top).into(), length)?)
    }

    /// Write data on top of stack
    pub fn write(&mut self, data: &[u8]) -> AllocationResult {
        if data.len() as MemoryBits > WasmAllocation::max() {
            return Err(AllocationError::OutOfBounds);
        }

        if data.is_empty() {
            return Err(AllocationError::ZeroLength);
        }

        // scope for mutable borrow of self
        let mem_buf = self.allocate(MemoryInt::from(data.len() as u16).into())?;

        self.wasm_memory
            .set(u32::from(mem_buf.offset()), &data)
            .expect("memory should be writable");

        Ok(mem_buf)
    }

    /// Read data somewhere in stack
    pub fn read(&self, allocation: WasmAllocation) -> Vec<u8> {
        self.wasm_memory
            .get(MemoryBits::from(allocation.offset()), MemoryInt::from(allocation.length()) as usize)
            .expect("Successfully retrieve the result")
    }
}
