//! The cpu module contains the privileged mode, registers, and CPU.

use alloc::{collections::BTreeMap, string::String};
use alloc::{string::ToString, vec::Vec};
use core::cmp::PartialEq;
use core::fmt;

use crate::{
    bus::{Bus, DRAM_BASE},
    csr::*,
    dram::DRAM_SIZE,
    exception::Exception,
    interrupt::Interrupt,
};

/// The number of registers.
pub const REGISTERS_COUNT: usize = 32;

/// 8 bits. 1 byte.
pub const BYTE: u8 = 8;
/// 16 bits. 2 bytes.
pub const HALFWORD: u8 = 16;
/// 32 bits. 4 bytes.
pub const WORD: u8 = 32;

macro_rules! inst_count {
    ($cpu:ident, $inst_name:expr) => {
        if $cpu.is_count {
            *$cpu.inst_counter.entry($inst_name.to_string()).or_insert(0) += 1;
        }
    };
}

/// Access type that is used in the virtual address translation process. It decides which exception
/// should raises (InstructionPageFault, LoadPageFault or StoreAMOPageFault).
#[derive(Debug, PartialEq, PartialOrd)]
pub enum AccessType {
    /// Raises the exception InstructionPageFault. It is used for an instruction fetch.
    Instruction,
    /// Raises the exception LoadPageFault.
    Load,
    /// Raises the exception StoreAMOPageFault.
    Store,
}

/// The privileged mode.
#[derive(Debug, PartialEq, PartialOrd, Eq, Copy, Clone)]
pub enum Mode {
    User = 0b00,
    Machine = 0b11,
    Debug,
}

/// The integer registers.
#[derive(Debug)]
pub struct XRegisters {
    xregs: [u32; REGISTERS_COUNT],
}

impl XRegisters {
    /// Create a new `XRegisters` object.
    pub fn new() -> Self {
        let mut xregs = [0; REGISTERS_COUNT];
        // The stack pointer is set in the default maximum memory size + the start address of dram.
        xregs[2] = DRAM_BASE + DRAM_SIZE;
        Self { xregs }
    }

    /// Read the value from a register.
    pub fn read(&self, index: u32) -> u32 {
        self.xregs[index as usize]
    }

    /// Write the value to a register.
    pub fn write(&mut self, index: u32, value: u32) {
        // Register x0 is hardwired with all bits equal to 0.
        if index != 0 {
            self.xregs[index as usize] = value;
        }
    }
}

impl fmt::Display for XRegisters {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let abi = [
            "zero", " ra ", " sp ", " gp ", " tp ", " t0 ", " t1 ", " t2 ", " s0 ", " s1 ", " a0 ",
            " a1 ", " a2 ", " a3 ", " a4 ", " a5 ", " a6 ", " a7 ", " s2 ", " s3 ", " s4 ", " s5 ",
            " s6 ", " s7 ", " s8 ", " s9 ", " s10", " s11", " t3 ", " t4 ", " t5 ", " t6 ",
        ];
        let mut output = String::from("");
        for i in (0..REGISTERS_COUNT).step_by(4) {
            output = format!(
                "{}\n{}",
                output,
                format!(
                    "x{:02}({})={:>#18x} x{:02}({})={:>#18x} x{:02}({})={:>#18x} x{:02}({})={:>#18x}",
                    i,
                    abi[i],
                    self.read(i as u32),
                    i + 1,
                    abi[i + 1],
                    self.read(i as u32 + 1),
                    i + 2,
                    abi[i + 2],
                    self.read(i as u32 + 2),
                    i + 3,
                    abi[i + 3],
                    self.read(i as u32 + 3),
                )
            );
        }
        write!(f, "{}", output)
    }
}

/// The CPU to contain registers, a program counter, status, and a privileged mode.
pub struct Cpu {
    /// 64-bit integer registers.
    pub xregs: XRegisters,
    /// Program counter.
    pub pc: u32,
    /// Control and status registers (CSR).
    pub state: State,
    /// Privilege level.
    pub mode: Mode,
    /// System bus.
    pub bus: Bus,
    /// A set of bytes that subsumes the bytes in the addressed word used in
    /// load-reserved/store-conditional instructions.
    reservation_set: Vec<u32>,
    /// Idle state. True when WFI is called, and becomes false when an interrupt happens.
    pub idle: bool,
    /// Counter of each instructions for debug.
    pub inst_counter: BTreeMap<String, u32>,
    /// The count flag. Count the number of each instruction executed.
    pub is_count: bool,
    /// Previous instruction. This is for debug.
    pub pre_inst: u32,
}

impl Cpu {
    /// Create a new `Cpu` object.
    pub fn new() -> Cpu {
        Cpu {
            xregs: XRegisters::new(),
            pc: 0,
            state: State::new(),
            mode: Mode::Machine,
            bus: Bus::new(),
            reservation_set: Vec::new(),
            idle: false,
            inst_counter: BTreeMap::new(),
            is_count: false,
            pre_inst: 0,
        }
    }

    fn debug(&self, _inst: u32, _name: &str) {
        /*
        if (((0x20_0000_0000 & self.pc) >> 37) == 1) && (self.pc & 0xf0000000_00000000) == 0 {
            println!(
                "[user]    {} pc: {:#x}, inst: {:#x}, is_inst 16? {} x[0x1c] {:#x}",
                name,
                self.pc,
                inst,
                // Check if an instruction is one of the compressed instructions.
                inst & 0b11 == 0 || inst & 0b11 == 1 || inst & 0b11 == 2,
                self.xregs.read(0x1c),
            );
            return;
        }

        if (self.pc & 0xf0000000_00000000) != 0 {
            return;
            /*
            println!(
                "[kernel]  {} pc: {:#x}, inst: {:#x}, is_inst 16? {} x[0x1c] {:#x}",
                name,
                self.pc,
                inst,
                // Check if an instruction is one of the compressed instructions.
                inst & 0b11 == 0 || inst & 0b11 == 1 || inst & 0b11 == 2,
                self.xregs.read(0x1c),
            );
            return;
            */
        }

        println!(
            "[machine] {} pc: {:#x}, inst: {:#x}, is_inst 16? {} x[0x1c] {:#x}",
            name,
            self.pc,
            inst,
            // Check if an instruction is one of the compressed instructions.
            inst & 0b11 == 0 || inst & 0b11 == 1 || inst & 0b11 == 2,
            self.xregs.read(0x1c),
        );
        */
    }

    /// Reset CPU states.
    pub fn reset(&mut self) {
        self.pc = 0;
        self.mode = Mode::Machine;
        self.state.reset();
        for i in 0..REGISTERS_COUNT {
            self.xregs.write(i as u32, 0);
        }
    }

    /// Check interrupt flags for all devices that can interrupt.
    pub fn check_pending_interrupt(&mut self) -> Option<Interrupt> {
        // global interrupt: PLIC (Platform Local Interrupt Controller) dispatches global
        //                   interrupts to multiple harts.
        // local interrupt: CLINT (Core Local Interrupter) dispatches local interrupts to a hart
        //                  which directly connected to CLINT.

        // 3.1.6.1 Privilege and Global Interrupt-Enable Stack in mstatus register
        // "When a hart is executing in privilege mode x, interrupts are globally enabled when
        // xIE=1 and globally disabled when xIE=0."
        match self.mode {
            Mode::Machine => {
                // Check if the MIE bit is enabled.
                if self.state.read_mstatus(MSTATUS_MIE) == 0 {
                    return None;
                }
            }
            _ => {}
        }

        // TODO: Take interrupts based on priorities.

        // Check external interrupt for uart and virtio.
        let irq = 0;
        // if self.bus.uart.is_interrupting() {
        //     irq = UART_IRQ;
        // } else if self.bus.virtio.is_interrupting() {
        //     // An interrupt is raised after a disk access is done.
        //     Virtio::disk_access(self).expect("failed to access the disk");
        //     irq = VIRTIO_IRQ;
        // } else {
        //     irq = 0;
        // }

        if irq != 0 {
            // TODO: assume that hart is 0
            // TODO: write a value to MCLAIM if the mode is machine
            self.state.write(MIP, self.state.read(MIP) | SEIP_BIT);
        }

        // 3.1.9 Machine Interrupt Registers (mip and mie)
        // "An interrupt i will be taken if bit i is set in both mip and mie, and if interrupts are
        // globally enabled. By default, M-mode interrupts are globally enabled if the hart’s
        // current privilege mode is less than M, or if the current privilege mode is M and the MIE
        // bit in the mstatus register is set. If bit i in mideleg is set, however, interrupts are
        // considered to be globally enabled if the hart’s current privilege mode equals the
        // delegated privilege mode (S or U) and that mode’s interrupt enable bit (SIE or UIE in
        // mstatus) is set, or if the current privilege mode is less than the delegated privilege
        // mode."
        let pending = self.state.read(MIE) & self.state.read(MIP);

        if (pending & MEIP_BIT) != 0 {
            self.state.write(MIP, self.state.read(MIP) & !MEIP_BIT);
            return Some(Interrupt::MachineExternalInterrupt);
        }
        if (pending & MSIP_BIT) != 0 {
            self.state.write(MIP, self.state.read(MIP) & !MSIP_BIT);
            return Some(Interrupt::MachineSoftwareInterrupt);
        }
        if (pending & MTIP_BIT) != 0 {
            self.state.write(MIP, self.state.read(MIP) & !MTIP_BIT);
            return Some(Interrupt::MachineTimerInterrupt);
        }

        return None;
    }

    /// Read `size`-bit data from the system bus.
    fn read(&mut self, addr: u32, size: u8) -> Result<u32, Exception> {
        let previous_mode = self.mode;

        // 3.1.6.3 Memory Privilege in mstatus Register
        // "When MPRV=1, load and store memory addresses are translated and protected, and
        // endianness is applied, as though the current privilege mode were set to MPP."
        if self.state.read_mstatus(MSTATUS_MPRV) == 1 {
            self.mode = match self.state.read_mstatus(MSTATUS_MPP) {
                0b00 => Mode::User,
                0b11 => Mode::Machine,
                _ => Mode::Debug,
            };
        }

        let result = self.bus.read(addr, size);

        if self.state.read_mstatus(MSTATUS_MPRV) == 1 {
            self.mode = previous_mode;
        }

        result
    }

    /// Write `size`-bit data to the system bus with the translation a virtual address to a physical
    /// address if it is enabled.
    fn write(&mut self, addr: u32, value: u32, size: u8) -> Result<(), Exception> {
        let previous_mode = self.mode;

        // 3.1.6.3 Memory Privilege in mstatus Register
        // "When MPRV=1, load and store memory addresses are translated and protected, and
        // endianness is applied, as though the current privilege mode were set to MPP."
        if self.state.read_mstatus(MSTATUS_MPRV) == 1 {
            self.mode = match self.state.read_mstatus(MSTATUS_MPP) {
                0b00 => Mode::User,
                0b11 => Mode::Machine,
                _ => Mode::Debug,
            };
        }

        // "The SC must fail if a write from some other device to the bytes accessed by the LR can
        // be observed to occur between the LR and SC."
        if self.reservation_set.contains(&addr) {
            self.reservation_set.retain(|&x| x != addr);
        }

        let result = self.bus.write(addr, value, size);

        if self.state.read_mstatus(MSTATUS_MPRV) == 1 {
            self.mode = previous_mode;
        }

        result
    }

    pub fn fetch(&mut self) -> Result<u32, Exception> {
        // The result of the read method can be `Exception::LoadAccessFault`. In fetch(), an error
        // should be `Exception::InstructionAccessFault`.
        match self.bus.read(self.pc, WORD) {
            Ok(value) => Ok(value),
            Err(_) => Err(Exception::InstructionAccessFault),
        }
    }

    /// Execute a cycle on peripheral devices.
    pub fn devices_increment(&mut self) {
        // TODO: mtime in Clint and TIME in CSR should be the same value.
        // Increment the timer register (mtimer) in Clint.
        // self.bus.clint.increment(&mut self.state);
        // Increment the value in the TIME and CYCLE registers in CSR.
        self.state.increment_time();
    }

    /// Execute an instruction. Raises an exception if something is wrong, otherwise, returns
    /// the instruction executed in this cycle.
    pub fn execute(&mut self) -> Result<u32, Exception> {
        // WFI is called and pending interrupts don't exist.
        if self.idle {
            return Ok(0);
        }

        // Fetch.
        let inst = self.fetch()?;
        self.execute_general(inst)?;
        // Add 4 bytes to the program counter.
        self.pc += 4;

        self.pre_inst = inst;
        Ok(inst)
    }

    /// Execute a general-purpose instruction. Raises an exception if something is wrong,
    /// otherwise, returns a fetched instruction. It also increments the program counter by 4 bytes.
    fn execute_general(&mut self, inst: u32) -> Result<(), Exception> {
        // 2. Decode.
        let opcode = inst & 0x0000007f;
        let rd = (inst & 0x00000f80) >> 7;
        let rs1 = (inst & 0x000f8000) >> 15;
        let rs2 = (inst & 0x01f00000) >> 20;
        let funct3 = (inst & 0x00007000) >> 12;
        let funct7 = (inst & 0xfe000000) >> 25;

        // 3. Execute.
        match opcode {
            0x03 => {
                // RV32I and RV64I
                // imm[11:0] = inst[31:20]
                let offset = ((inst as i32) >> 20) as u32;
                let addr = self.xregs.read(rs1).wrapping_add(offset);
                match funct3 {
                    0x0 => {
                        // lb
                        inst_count!(self, "lb");
                        self.debug(inst, "lb");

                        let val = self.read(addr, BYTE)?;
                        self.xregs.write(rd, val as i8 as i32 as u32);
                    }
                    0x1 => {
                        // lh
                        inst_count!(self, "lh");
                        self.debug(inst, "lh");

                        let val = self.read(addr, HALFWORD)?;
                        self.xregs.write(rd, val as i16 as i32 as u32);
                    }
                    0x2 => {
                        // lw
                        inst_count!(self, "lw");
                        self.debug(inst, "lw");

                        let val = self.read(addr, WORD)?;
                        self.xregs.write(rd, val as i32 as u32);
                    }
                    0x4 => {
                        // lbu
                        inst_count!(self, "lbu");
                        self.debug(inst, "lbu");

                        let val = self.read(addr, BYTE)?;
                        self.xregs.write(rd, val);
                    }
                    0x5 => {
                        // lhu
                        inst_count!(self, "lhu");
                        self.debug(inst, "lhu");

                        let val = self.read(addr, HALFWORD)?;
                        self.xregs.write(rd, val);
                    }
                    _ => {
                        return Err(Exception::IllegalInstruction(inst));
                    }
                }
            }
            0x0f => {
                // RV32I and RV64I
                // fence instructions are not supported yet because this emulator executes an
                // instruction sequentially on a single thread.
                // fence.i is a part of the Zifencei extension.
                match funct3 {
                    0x0 => {
                        // fence
                        inst_count!(self, "fence");
                        self.debug(inst, "fence");
                    }
                    0x1 => {
                        // fence.i
                        inst_count!(self, "fence.i");
                        self.debug(inst, "fence.i");
                    }
                    _ => {
                        return Err(Exception::IllegalInstruction(inst));
                    }
                }
            }
            0x13 => {
                // RV32I and RV64I
                // imm[11:0] = inst[31:20]
                let imm = ((inst as i32) >> 20) as u32;
                let funct6 = funct7 >> 1;
                match funct3 {
                    0x0 => {
                        // addi
                        inst_count!(self, "addi");
                        self.debug(inst, "addi");

                        self.xregs.write(rd, self.xregs.read(rs1).wrapping_add(imm));
                    }
                    0x1 => {
                        // slli
                        inst_count!(self, "slli");
                        self.debug(inst, "slli");

                        // shamt size is 5 bits for RV32I and 6 bits for RV64I.
                        let shamt = (inst >> 20) & 0x1f;
                        self.xregs.write(rd, self.xregs.read(rs1) << shamt);
                    }
                    0x2 => {
                        // slti
                        inst_count!(self, "slti");
                        self.debug(inst, "slti");

                        self.xregs.write(
                            rd,
                            if (self.xregs.read(rs1) as i32) < (imm as i32) {
                                1
                            } else {
                                0
                            },
                        );
                    }
                    0x3 => {
                        // sltiu
                        inst_count!(self, "sltiu");
                        self.debug(inst, "sltiu");

                        self.xregs
                            .write(rd, if self.xregs.read(rs1) < imm { 1 } else { 0 });
                    }
                    0x4 => {
                        // xori
                        inst_count!(self, "xori");
                        self.debug(inst, "xori");

                        self.xregs.write(rd, self.xregs.read(rs1) ^ imm);
                    }
                    0x5 => {
                        match funct6 {
                            0x00 => {
                                // srli
                                inst_count!(self, "srli");
                                self.debug(inst, "srli");

                                // shamt size is 5 bits for RV32I and 6 bits for RV64I.
                                let shamt = (inst >> 20) & 0x1f;
                                self.xregs.write(rd, self.xregs.read(rs1) >> shamt);
                            }
                            0x10 => {
                                // srai
                                inst_count!(self, "srai");
                                self.debug(inst, "srai");

                                // shamt size is 5 bits for RV32I and 6 bits for RV64I.
                                let shamt = (inst >> 20) & 0x1f;
                                self.xregs
                                    .write(rd, ((self.xregs.read(rs1) as i32) >> shamt) as u32);
                            }
                            _ => {
                                return Err(Exception::IllegalInstruction(inst));
                            }
                        }
                    }
                    0x6 => {
                        // ori
                        inst_count!(self, "ori");
                        self.debug(inst, "ori");

                        self.xregs.write(rd, self.xregs.read(rs1) | imm);
                    }
                    0x7 => {
                        // andi
                        inst_count!(self, "andi");
                        self.debug(inst, "andi");

                        self.xregs.write(rd, self.xregs.read(rs1) & imm);
                    }
                    _ => {
                        return Err(Exception::IllegalInstruction(inst));
                    }
                }
            }
            0x17 => {
                // RV32I
                // auipc
                inst_count!(self, "auipc");
                self.debug(inst, "auipc");

                // AUIPC forms a 32-bit offset from the 20-bit U-immediate, filling
                // in the lowest 12 bits with zeros.
                // imm[31:12] = inst[31:12]
                let imm = (inst & 0xfffff000) as i32 as u32;
                self.xregs.write(rd, self.pc.wrapping_add(imm));
            }

            0x23 => {
                // RV32I
                // offset[11:5|4:0] = inst[31:25|11:7]
                let offset = (((inst & 0xfe000000) as i32 >> 20) as u32) | ((inst >> 7) & 0x1f);
                let addr = self.xregs.read(rs1).wrapping_add(offset);
                match funct3 {
                    0x0 => {
                        // sb
                        inst_count!(self, "sb");
                        self.debug(inst, "sb");

                        self.write(addr, self.xregs.read(rs2), BYTE)?
                    }
                    0x1 => {
                        // sh
                        inst_count!(self, "sh");
                        self.debug(inst, "sh");

                        self.write(addr, self.xregs.read(rs2), HALFWORD)?
                    }
                    0x2 => {
                        // sw
                        inst_count!(self, "sw");
                        self.debug(inst, "sw");

                        self.write(addr, self.xregs.read(rs2), WORD)?
                    }
                    _ => {
                        return Err(Exception::IllegalInstruction(inst));
                    }
                }
            }
            0x33 => {
                // RV32M
                match (funct3, funct7) {
                    (0x0, 0x00) => {
                        // add
                        inst_count!(self, "add");
                        self.debug(inst, "add");

                        self.xregs
                            .write(rd, self.xregs.read(rs1).wrapping_add(self.xregs.read(rs2)));
                    }
                    (0x0, 0x01) => {
                        // mul
                        inst_count!(self, "mul");
                        self.debug(inst, "mul");

                        self.xregs.write(
                            rd,
                            (self.xregs.read(rs1) as i32).wrapping_mul(self.xregs.read(rs2) as i32)
                                as u32,
                        );
                    }
                    (0x0, 0x20) => {
                        // sub
                        inst_count!(self, "sub");
                        self.debug(inst, "sub");

                        self.xregs
                            .write(rd, self.xregs.read(rs1).wrapping_sub(self.xregs.read(rs2)));
                    }
                    (0x1, 0x00) => {
                        // sll
                        inst_count!(self, "sll");
                        self.debug(inst, "sll");

                        // "SLL, SRL, and SRA perform logical left, logical right, and arithmetic right shifts on the value in
                        // register rs1 by the shift amount held in the lower 5 bits of register rs2."
                        let shamt = self.xregs.read(rs2) & 0x1f;
                        self.xregs.write(rd, self.xregs.read(rs1) << shamt);
                    }
                    (0x1, 0x01) => {
                        // mulh
                        inst_count!(self, "mulh");
                        self.debug(inst, "mulh");

                        // signed × signed
                        self.xregs.write(
                            rd,
                            ((self.xregs.read(rs1) as i32 as i128)
                                .wrapping_mul(self.xregs.read(rs2) as i32 as i128)
                                >> 64) as u32,
                        );
                    }
                    (0x2, 0x00) => {
                        // slt
                        inst_count!(self, "slt");
                        self.debug(inst, "slt");

                        self.xregs.write(
                            rd,
                            if (self.xregs.read(rs1) as i32) < (self.xregs.read(rs2) as i32) {
                                1
                            } else {
                                0
                            },
                        );
                    }
                    (0x2, 0x01) => {
                        // mulhsu
                        inst_count!(self, "mulhsu");
                        self.debug(inst, "mulhsu");

                        // signed × unsigned
                        self.xregs.write(
                            rd,
                            ((self.xregs.read(rs1) as i32 as i128 as u128)
                                .wrapping_mul(self.xregs.read(rs2) as u128)
                                >> 64) as u32,
                        );
                    }
                    (0x3, 0x00) => {
                        // sltu
                        inst_count!(self, "sltu");
                        self.debug(inst, "sltu");

                        self.xregs.write(
                            rd,
                            if self.xregs.read(rs1) < self.xregs.read(rs2) {
                                1
                            } else {
                                0
                            },
                        );
                    }
                    (0x3, 0x01) => {
                        // mulhu
                        inst_count!(self, "mulhu");
                        self.debug(inst, "mulhu");

                        // unsigned × unsigned
                        self.xregs.write(
                            rd,
                            ((self.xregs.read(rs1) as u128)
                                .wrapping_mul(self.xregs.read(rs2) as u128)
                                >> 64) as u32,
                        );
                    }
                    (0x4, 0x00) => {
                        // xor
                        inst_count!(self, "xor");
                        self.debug(inst, "xor");

                        self.xregs
                            .write(rd, self.xregs.read(rs1) ^ self.xregs.read(rs2));
                    }
                    (0x4, 0x01) => {
                        // div
                        inst_count!(self, "div");
                        self.debug(inst, "div");

                        let dividend = self.xregs.read(rs1) as i32;
                        let divisor = self.xregs.read(rs2) as i32;
                        self.xregs.write(
                            rd,
                            if divisor == 0 {
                                // Division by zero
                                // Set DZ (Divide by Zero) flag to 1.
                                self.state.write_bit(FCSR, 3, 1);
                                // "The quotient of division by zero has all bits set"
                                u32::MAX
                            } else if dividend == i32::MIN && divisor == -1 {
                                // Overflow
                                // "The quotient of a signed division with overflow is equal to the
                                // dividend"
                                dividend as u32
                            } else {
                                // "division of rs1 by rs2, rounding towards zero"
                                dividend.wrapping_div(divisor) as u32
                            },
                        );
                    }
                    (0x5, 0x00) => {
                        // srl
                        inst_count!(self, "srl");
                        self.debug(inst, "srl");

                        // "SLL, SRL, and SRA perform logical left, logical right, and arithmetic right shifts on the value in
                        // register rs1 by the shift amount held in the lower 5 bits of register rs2."
                        let shamt = self.xregs.read(rs2) & 0x1f;
                        self.xregs.write(rd, self.xregs.read(rs1) >> shamt);
                    }
                    (0x5, 0x01) => {
                        // divu
                        inst_count!(self, "divu");
                        self.debug(inst, "divu");

                        let dividend = self.xregs.read(rs1);
                        let divisor = self.xregs.read(rs2);
                        self.xregs.write(
                            rd,
                            if divisor == 0 {
                                // Division by zero
                                // Set DZ (Divide by Zero) flag to 1.
                                self.state.write_bit(FCSR, 3, 1);
                                // "The quotient of division by zero has all bits set"
                                u32::MAX
                            } else {
                                // "division of rs1 by rs2, rounding towards zero"
                                dividend.wrapping_div(divisor)
                            },
                        );
                    }
                    (0x5, 0x20) => {
                        // sra
                        inst_count!(self, "sra");
                        self.debug(inst, "sra");

                        // "SLL, SRL, and SRA perform logical left, logical right, and arithmetic right shifts on the value in
                        // register rs1 by the shift amount held in the lower 5 bits of register rs2."
                        let shamt = self.xregs.read(rs2) & 0x1f;
                        self.xregs
                            .write(rd, ((self.xregs.read(rs1) as i32) >> shamt) as u32);
                    }
                    (0x6, 0x00) => {
                        // or
                        inst_count!(self, "or");
                        self.debug(inst, "or");

                        self.xregs
                            .write(rd, self.xregs.read(rs1) | self.xregs.read(rs2));
                    }
                    (0x6, 0x01) => {
                        // rem
                        inst_count!(self, "rem");
                        self.debug(inst, "rem");

                        let dividend = self.xregs.read(rs1) as i32;
                        let divisor = self.xregs.read(rs2) as i32;
                        self.xregs.write(
                            rd,
                            if divisor == 0 {
                                // Division by zero
                                // "the remainder of division by zero equals the dividend"
                                dividend as u32
                            } else if dividend == i32::MIN && divisor == -1 {
                                // Overflow
                                // "the remainder is zero"
                                0
                            } else {
                                // "provide the remainder of the corresponding division
                                // operation"
                                dividend.wrapping_rem(divisor) as u32
                            },
                        );
                    }
                    (0x7, 0x00) => {
                        // and
                        inst_count!(self, "and");
                        self.debug(inst, "and");

                        self.xregs
                            .write(rd, self.xregs.read(rs1) & self.xregs.read(rs2));
                    }
                    (0x7, 0x01) => {
                        // remu
                        inst_count!(self, "remu");
                        self.debug(inst, "remu");

                        let dividend = self.xregs.read(rs1);
                        let divisor = self.xregs.read(rs2);
                        self.xregs.write(
                            rd,
                            if divisor == 0 {
                                // Division by zero
                                // "the remainder of division by zero equals the dividend"
                                dividend
                            } else {
                                // "provide the remainder of the corresponding division
                                // operation"
                                dividend.wrapping_rem(divisor)
                            },
                        );
                    }
                    _ => {
                        return Err(Exception::IllegalInstruction(inst));
                    }
                };
            }
            0x37 => {
                // RV32I
                // lui
                inst_count!(self, "lui");
                self.debug(inst, "lui");

                // "LUI places the U-immediate value in the top 20 bits of the destination
                // register rd, filling in the lowest 12 bits with zeros."
                self.xregs.write(rd, (inst & 0xfffff000) as i32 as u32);
            }
            0x63 => {
                // RV32I
                // imm[12|10:5|4:1|11] = inst[31|30:25|11:8|7]
                let imm = (((inst & 0x80000000) as i32 >> 19) as u32)
                    | ((inst & 0x80) << 4) // imm[11]
                    | ((inst >> 20) & 0x7e0) // imm[10:5]
                    | ((inst >> 7) & 0x1e); // imm[4:1]

                match funct3 {
                    0x0 => {
                        // beq
                        inst_count!(self, "beq");
                        self.debug(inst, "beq");

                        if self.xregs.read(rs1) == self.xregs.read(rs2) {
                            self.pc = self.pc.wrapping_add(imm).wrapping_sub(4);
                        }
                    }
                    0x1 => {
                        // bne
                        inst_count!(self, "bne");
                        self.debug(inst, "bne");

                        if self.xregs.read(rs1) != self.xregs.read(rs2) {
                            self.pc = self.pc.wrapping_add(imm).wrapping_sub(4);
                        }
                    }
                    0x4 => {
                        // blt
                        inst_count!(self, "blt");
                        self.debug(inst, "blt");

                        if (self.xregs.read(rs1) as i32) < (self.xregs.read(rs2) as i32) {
                            self.pc = self.pc.wrapping_add(imm).wrapping_sub(4);
                        }
                    }
                    0x5 => {
                        // bge
                        inst_count!(self, "bge");
                        self.debug(inst, "bge");

                        if (self.xregs.read(rs1) as i32) >= (self.xregs.read(rs2) as i32) {
                            self.pc = self.pc.wrapping_add(imm).wrapping_sub(4);
                        }
                    }
                    0x6 => {
                        // bltu
                        inst_count!(self, "bltu");
                        self.debug(inst, "bltu");

                        if self.xregs.read(rs1) < self.xregs.read(rs2) {
                            self.pc = self.pc.wrapping_add(imm).wrapping_sub(4);
                        }
                    }
                    0x7 => {
                        // bgeu
                        inst_count!(self, "bgeu");
                        self.debug(inst, "bgeu");

                        if self.xregs.read(rs1) >= self.xregs.read(rs2) {
                            self.pc = self.pc.wrapping_add(imm).wrapping_sub(4);
                        }
                    }
                    _ => {
                        return Err(Exception::IllegalInstruction(inst));
                    }
                }
            }
            0x67 => {
                // jalr
                inst_count!(self, "jalr");
                self.debug(inst, "jalr");

                let t = self.pc.wrapping_add(4);

                let offset = (inst as i32) >> 20;
                let target = ((self.xregs.read(rs1) as i32).wrapping_add(offset)) & !1;

                self.pc = (target as u32).wrapping_sub(4);

                self.xregs.write(rd, t);
            }
            0x6F => {
                // jal
                inst_count!(self, "jal");
                self.debug(inst, "jal");

                self.xregs.write(rd, self.pc.wrapping_add(4));

                // imm[20|10:1|11|19:12] = inst[31|30:21|20|19:12]
                let offset = (((inst & 0x80000000) as i32 >> 11) as u32) // imm[20]
                    | (inst & 0xff000) // imm[19:12]
                    | ((inst >> 9) & 0x800) // imm[11]
                    | ((inst >> 20) & 0x7fe); // imm[10:1]

                self.pc = self.pc.wrapping_add(offset).wrapping_sub(4);
            }
            0x73 => {
                // RV32I, RVZicsr, and supervisor ISA
                let csr_addr = ((inst >> 20) & 0xfff) as u16;
                match funct3 {
                    0x0 => {
                        match (rs2, funct7) {
                            (0x0, 0x0) => {
                                // ecall
                                inst_count!(self, "ecall");
                                self.debug(inst, "ecall");

                                // Makes a request of the execution environment by raising an
                                // environment call exception.
                                match self.mode {
                                    Mode::User => {
                                        return Err(Exception::EnvironmentCallFromUMode);
                                    }
                                    Mode::Machine => {
                                        return Err(Exception::EnvironmentCallFromMMode);
                                    }
                                    _ => {
                                        return Err(Exception::IllegalInstruction(inst));
                                    }
                                }
                            }
                            (0x1, 0x0) => {
                                // ebreak
                                inst_count!(self, "ebreak");
                                self.debug(inst, "ebreak");

                                // Makes a request of the debugger by raising a Breakpoint
                                // exception.
                                return Err(Exception::Breakpoint);
                            }
                            (0x2, 0x0) => {
                                // uret
                                inst_count!(self, "uret");
                                self.debug(inst, "uret");
                                panic!("uret: not implemented yet. pc {}", self.pc);
                            }
                            (0x2, 0x18) => {
                                // mret
                                inst_count!(self, "mret");
                                self.debug(inst, "mret");

                                // "The RISC-V Reader" book says:
                                // "Returns from a machine-mode exception handler. Sets the pc to
                                // CSRs[mepc], the privilege mode to CSRs[mstatus].MPP,
                                // CSRs[mstatus].MIE to CSRs[mstatus].MPIE, and CSRs[mstatus].MPIE
                                // to 1; and, if user mode is supported, sets CSRs[mstatus].MPP to
                                // 0".

                                // Set the program counter to the machine exception program
                                // counter (MEPC).
                                self.pc = self.state.read(MEPC).wrapping_sub(4);

                                // Set the current privileged mode depending on a previous
                                // privilege mode for machine  mode (MPP, 11..13).
                                self.mode = match self.state.read_mstatus(MSTATUS_MPP) {
                                    0b00 => {
                                        // If MPP != M-mode, MRET also sets MPRV=0.
                                        self.state.write_mstatus(MSTATUS_MPRV, 0);
                                        Mode::User
                                    }
                                    0b11 => Mode::Machine,
                                    _ => Mode::Debug,
                                };

                                // Read a previous interrupt-enable bit for machine mode (MPIE, 7),
                                // and set a global interrupt-enable bit for machine mode (MIE, 3)
                                // to it.
                                self.state.write_mstatus(
                                    MSTATUS_MIE,
                                    self.state.read_mstatus(MSTATUS_MPIE),
                                );

                                // Set a previous interrupt-enable bit for machine mode (MPIE, 7)
                                // to 1.
                                self.state.write_mstatus(MSTATUS_MPIE, 1);

                                // Set a previous privilege mode for machine mode (MPP, 11..13) to
                                // 0.
                                self.state.write_mstatus(MSTATUS_MPP, Mode::User as u32);
                            }
                            (0x5, 0x8) => {
                                // wfi
                                inst_count!(self, "wfi");
                                self.debug(inst, "wfi");
                                // "provides a hint to the implementation that the current
                                // hart can be stalled until an interrupt might need servicing."
                                self.idle = true;
                            }
                            (_, 0x9) => {
                                // sfence.vma
                                inst_count!(self, "sfence.vma");
                                self.debug(inst, "sfence.vma");
                                // "SFENCE.VMA is used to synchronize updates to in-memory
                                // memory-management data structures with current execution"
                            }
                            (_, 0x11) => {
                                // hfence.bvma
                                inst_count!(self, "hfence.bvma");
                                self.debug(inst, "hfence.bvma");
                            }
                            (_, 0x51) => {
                                // hfence.gvma
                                inst_count!(self, "hfence.gvma");
                                self.debug(inst, "hfence.gvma");
                            }
                            _ => {
                                return Err(Exception::IllegalInstruction(inst));
                            }
                        }
                    }
                    0x1 => {
                        // csrrw
                        inst_count!(self, "csrrw");
                        self.debug(inst, "csrrw");

                        let t = self.state.read(csr_addr);
                        self.state.write(csr_addr, self.xregs.read(rs1));
                        self.xregs.write(rd, t);
                    }
                    0x2 => {
                        // csrrs
                        inst_count!(self, "csrrs");
                        self.debug(inst, "csrrs");

                        let t = self.state.read(csr_addr);
                        self.state.write(csr_addr, t | self.xregs.read(rs1));
                        self.xregs.write(rd, t);
                    }
                    0x3 => {
                        // csrrc
                        inst_count!(self, "csrrc");
                        self.debug(inst, "csrrc");

                        let t = self.state.read(csr_addr);
                        self.state.write(csr_addr, t & (!self.xregs.read(rs1)));
                        self.xregs.write(rd, t);
                    }
                    0x5 => {
                        // csrrwi
                        inst_count!(self, "csrrwi");
                        self.debug(inst, "csrrwi");

                        let zimm = rs1;
                        self.xregs.write(rd, self.state.read(csr_addr));
                        self.state.write(csr_addr, zimm);
                    }
                    0x6 => {
                        // csrrsi
                        inst_count!(self, "csrrsi");
                        self.debug(inst, "csrrsi");

                        let zimm = rs1;
                        let t = self.state.read(csr_addr);
                        self.state.write(csr_addr, t | zimm);
                        self.xregs.write(rd, t);
                    }
                    0x7 => {
                        // csrrci
                        inst_count!(self, "csrrci");
                        self.debug(inst, "csrrci");

                        let zimm = rs1;
                        let t = self.state.read(csr_addr);
                        self.state.write(csr_addr, t & (!zimm));
                        self.xregs.write(rd, t);
                    }
                    _ => {
                        return Err(Exception::IllegalInstruction(inst));
                    }
                }
            }
            _ => {
                return Err(Exception::IllegalInstruction(inst));
            }
        }
        Ok(())
    }
}
