//! The bus module contains the system bus which can access the memroy or memory-mapped peripheral
//! devices.

use alloc::vec::Vec;

use crate::dram::{Dram, DRAM_SIZE};
use crate::exception::Exception;

// QEMU virt machine:
// https://github.com/qemu/qemu/blob/master/hw/riscv/virt.c#L46-L63

/// The address which DRAM starts.
pub const DRAM_BASE: u32 = 0x10000;
/// The address which DRAM ends.
const DRAM_END: u32 = DRAM_BASE + DRAM_SIZE;

/// The system bus.
pub struct Bus {
    dram: Dram,
}

impl Bus {
    /// Create a new bus object.
    pub fn new() -> Bus {
        Self { dram: Dram::new() }
    }

    /// Set the binary data to the memory.
    pub fn initialize_dram(&mut self, data: Vec<u8>) {
        self.dram.initialize(data);
    }

    /// Load a `size`-bit data from the device that connects to the system bus.
    pub fn read(&mut self, addr: u32, size: u8) -> Result<u32, Exception> {
        match addr {
            DRAM_BASE..=DRAM_END => self.dram.read(addr, size),
            _ => Err(Exception::LoadAccessFault),
        }
    }

    /// Store a `size`-bit data to the device that connects to the system bus.
    pub fn write(&mut self, addr: u32, value: u32, size: u8) -> Result<(), Exception> {
        match addr {
            DRAM_BASE..=DRAM_END => self.dram.write(addr, value, size),
            _ => Err(Exception::StoreAMOAccessFault),
        }
    }
}
