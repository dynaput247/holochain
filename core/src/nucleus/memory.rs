
use std::rc::Rc;
use std::cell::RefCell;

use wasmi::{
  MemoryRef, ModuleRef,
};


//--------------------------------------------------------------------------------------------------
// WASM Memory Manager
//--------------------------------------------------------------------------------------------------

// #[derive(Clone, Debug)]
//pub struct MemoryManagerRef(Rc<MemoryPageManager>);
//
//impl ::std::ops::Deref for MemoryManagerRef {
//  type Target = MemoryPageManager;
//  fn deref(&self) -> &MemoryPageManager {
//    &self.0
//  }
//}


#[derive(Clone, Debug)]
pub struct MemoryPageManager {
  end: u16,
  wasm_memory : MemoryRef,
  // allocations : Vec<MemoryAllocation> // for debugging only?
}

impl MemoryPageManager {

  pub fn new(wasm_instance : ModuleRef)
    -> /*RefCell<MemoryPageManager>*/ // MemoryManagerRef
    MemoryPageManager
  {
    // get wasm memory reference from module
    let wasm_memory = wasm_instance
      .export_by_name("memory")
      .expect("all modules compiled with rustc should have an export named 'memory'; qed")
      .as_memory()
      .expect("in module generated by rustc export named 'memory' should be a memory; qed")
      .clone();
    let page_manager = MemoryPageManager {
      end: 0,
      wasm_memory : wasm_memory.clone(),
      // allocations : Vec::new(),
    };
    // RefCell::new(page_manager)
    // MemoryManagerRef(Rc::new(page_manager))
    page_manager
  }

  pub fn allocate(&mut self, size: u16) -> Result<MemoryAllocation, &str> {
    if (self.end + size) as u32 >= 65536 {
      return Err("Out of memory");
    }
    let r = self.end;
    self.end += size;
    let allocation = MemoryAllocation {mem_offset: r, mem_len: size};
    // self.allocations.push(allocation);
    Ok(allocation)
  }


  pub fn malloc(&mut self, size: u16) -> Result<u16, &str> {
    let res = self.allocate(size);
    if let Ok(mem_buf) = res {
      return Ok(mem_buf.mem_offset);
    }
    Err("out of memory")
  }


  /// Write data on stack
  pub fn write(&mut self, data : Vec<u8>) -> Result<MemoryAllocation, &str> {
    let data_len = data.len();
    if data_len > 65536 {
      return Err("data length provided is bigger than 64KiB")
    }

    let mut mem_buf = MemoryAllocation{mem_len: 0, mem_offset: 0};

    {
      let res = self.allocate(data_len as u16);
      if res.is_err() {
        return Err("Not enough free memory available");
      }
      mem_buf = res.unwrap();
    }

    self.wasm_memory
      .set(mem_buf.mem_offset as u32, &data)
      .expect("memory should be writable");
    Ok(mem_buf)
  }


  // MemoryAllocation is garanteed to be valid (does not overflow page size)
  pub fn read(&self, allocation : &MemoryAllocation) -> Vec<u8> {
    return
      self.wasm_memory
      .get(allocation.mem_offset as u32, allocation.mem_len as usize)
      .expect("Successfully retrieve the result")
  }

  pub fn free(&mut self) {
    // TODO
  }

}




//--------------------------------------------------------------------------------------------------
// Memory Allocation
//--------------------------------------------------------------------------------------------------

#[derive(Copy, Clone, Debug)]
pub struct MemoryAllocation {
  pub mem_offset : u16,
  pub mem_len : u16,
}

impl MemoryAllocation {
  pub fn new(input : u32) -> Self {
    let allocation = MemoryAllocation {
      mem_offset : (input >> 16) as u16,
      mem_len : (input % 65536) as u16,
    };
    assert!(allocation.mem_len > 0);
    assert!((allocation.mem_offset as u32 + allocation.mem_len as u32) <= 65535);
    allocation
  }

  pub fn encode(&self) -> u32 {
    ((self.mem_offset as u32) << 16) + self.mem_len as u32
  }
}


pub fn encode_mem_buffer(mem_offset : u16, mem_len : u16) -> u32 {
  ((mem_offset as u32) << 16) + mem_len as u32
}
