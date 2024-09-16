//! The memory module contains the memory structure and implementation to read/write the memory.

use crate::bus::DRAM_BASE;
use crate::cpu::{BYTE, HALFWORD, WORD};
use crate::exception::Exception;
use alloc::vec::Vec;

/// Default memory size (32KiB).
pub const DRAM_SIZE: u32 = 32 * 1024;

/// The memory used by the emulator.
#[derive(Debug)]
pub struct Dram {
    pub dram: Vec<u8>,
    code_size: u32,
}

impl Dram {
    /// Create a new memory object with default memory size.
    pub fn new() -> Self {
        Self {
            dram: vec![0; DRAM_SIZE as usize],
            code_size: 0,
        }
    }

    /// Set the binary in the memory.
    pub fn initialize(&mut self, binary: Vec<u8>) {
        self.code_size = binary.len() as u32;
        self.dram.splice(..binary.len(), binary.iter().cloned());
    }

    /// Load `size`-bit data from the memory.
    pub fn read(&self, addr: u32, size: u8) -> Result<u32, Exception> {
        match size {
            BYTE => Ok(self.read8(addr)),
            HALFWORD => Ok(self.read16(addr)),
            WORD => Ok(self.read32(addr)),
            _ => return Err(Exception::LoadAccessFault),
        }
    }

    /// Store `size`-bit data to the memory.
    pub fn write(&mut self, addr: u32, value: u32, size: u8) -> Result<(), Exception> {
        match size {
            BYTE => self.write8(addr, value),
            HALFWORD => self.write16(addr, value),
            WORD => self.write32(addr, value),
            _ => return Err(Exception::StoreAMOAccessFault),
        }
        Ok(())
    }

    /// Write a byte to the memory.
    fn write8(&mut self, addr: u32, val: u32) {
        let index = (addr - DRAM_BASE) as usize;
        self.dram[index] = val as u8
    }

    /// Write 2 bytes to the memory with little endian.
    fn write16(&mut self, addr: u32, val: u32) {
        let index = (addr - DRAM_BASE) as usize;
        self.dram[index] = (val & 0xff) as u8;
        self.dram[index + 1] = ((val >> 8) & 0xff) as u8;
    }

    /// Write 4 bytes to the memory with little endian.
    fn write32(&mut self, addr: u32, val: u32) {
        let index = (addr - DRAM_BASE) as usize;
        self.dram[index] = (val & 0xff) as u8;
        self.dram[index + 1] = ((val >> 8) & 0xff) as u8;
        self.dram[index + 2] = ((val >> 16) & 0xff) as u8;
        self.dram[index + 3] = ((val >> 24) & 0xff) as u8;
    }

    /// Read a byte from the memory.
    fn read8(&self, addr: u32) -> u32 {
        let index = (addr - DRAM_BASE) as usize;
        self.dram[index] as u32
    }

    /// Read 2 bytes from the memory with little endian.
    fn read16(&self, addr: u32) -> u32 {
        let index = (addr - DRAM_BASE) as usize;
        return (self.dram[index] as u32) | ((self.dram[index + 1] as u32) << 8);
    }

    /// Read 4 bytes from the memory with little endian.
    fn read32(&self, addr: u32) -> u32 {
        let index = (addr - DRAM_BASE) as usize;
        return (self.dram[index] as u32)
            | ((self.dram[index + 1] as u32) << 8)
            | ((self.dram[index + 2] as u32) << 16)
            | ((self.dram[index + 3] as u32) << 24);
    }
}
