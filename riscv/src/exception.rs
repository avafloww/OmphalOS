//! The exception module contains all the exception kinds and the function to handle exceptions.

use crate::{
    cpu::{Cpu, Mode},
    csr::*,
};

/// All the exception kinds.
#[derive(Debug, PartialEq)]
pub enum Exception {
    /// With the addition of the C extension, no instructions can raise
    /// instruction-address-misaligned exceptions.
    InstructionAddressMisaligned,
    InstructionAccessFault,
    IllegalInstruction(u32),
    Breakpoint,
    LoadAddressMisaligned,
    LoadAccessFault,
    StoreAMOAddressMisaligned,
    StoreAMOAccessFault,
    EnvironmentCallFromUMode,
    EnvironmentCallFromMMode,
    // Stores a trap value (the faulting address) for page fault exceptions.
    InstructionPageFault(u32),
    LoadPageFault(u32),
    StoreAMOPageFault(u32),
}

/// All the trap kinds.
#[derive(Debug)]
pub enum Trap {
    /// The trap is visible to, and handled by, software running inside the execution
    /// environment.
    Contained,
    /// The trap is a synchronous exception that is an explicit call to the execution
    /// environment requesting an action on behalf of software inside the execution environment.
    Requested,
    /// The trap is handled transparently by the execution environment and execution
    /// resumes normally after the trap is handled.
    Invisible,
    /// The trap represents a fatal failure and causes the execution environment to terminate
    /// execution.
    Fatal,
}

impl Exception {
    fn exception_code(&self) -> u32 {
        match self {
            Exception::InstructionAddressMisaligned => 0,
            Exception::InstructionAccessFault => 1,
            Exception::IllegalInstruction(_) => 2,
            Exception::Breakpoint => 3,
            Exception::LoadAddressMisaligned => 4,
            Exception::LoadAccessFault => 5,
            Exception::StoreAMOAddressMisaligned => 6,
            Exception::StoreAMOAccessFault => 7,
            Exception::EnvironmentCallFromUMode => 8,
            Exception::EnvironmentCallFromMMode => 11,
            Exception::InstructionPageFault(_) => 12,
            Exception::LoadPageFault(_) => 13,
            Exception::StoreAMOPageFault(_) => 15,
        }
    }

    fn epc(&self, pc: u32) -> u32 {
        // 3.2.1 Environment Call and Breakpoint
        // "ECALL and EBREAK cause the receiving privilege mode’s epc register to be set to the
        // address of the ECALL or EBREAK instruction itself, not the address of the following
        // instruction."
        match self {
            Exception::Breakpoint
            | Exception::EnvironmentCallFromUMode
            | Exception::EnvironmentCallFromMMode
            // TODO: why page fault needs this?
            | Exception::InstructionPageFault(_)
            | Exception::LoadPageFault(_)
            | Exception::StoreAMOPageFault(_) => pc,
            _ => pc.wrapping_add(4),
        }
    }

    fn trap_value(&self, pc: u32) -> u32 {
        // 3.1.17 Machine Trap Value Register (mtval)
        // 4.1.9 Supervisor Trap Value Register (stval)
        // "When a hardware breakpoint is triggered, or an address-misaligned, access-fault, or
        // page-fault exception occurs on an instruction fetch, load, or store, mtval (stval) is
        // written with the faulting virtual address. On an illegal instruction trap, mtval (stval)
        // may be written with the first XLEN or ILEN bits of the faulting instruction as described
        // below. For other traps, mtval (stval) is set to zero, but a future standard may redefine
        // mtval's (stval's) setting for other traps."
        match self {
            Exception::InstructionAddressMisaligned
            | Exception::InstructionAccessFault
            | Exception::Breakpoint
            | Exception::LoadAddressMisaligned
            | Exception::LoadAccessFault
            | Exception::StoreAMOAddressMisaligned
            | Exception::StoreAMOAccessFault => pc,
            Exception::InstructionPageFault(val)
            | Exception::LoadPageFault(val)
            | Exception::StoreAMOPageFault(val) => *val,
            Exception::IllegalInstruction(val) => *val,
            _ => 0,
        }
    }

    /// Update CSRs and the program counter depending on an exception.
    pub fn take_trap(&self, cpu: &mut Cpu) -> Trap {
        // 1.2 Privilege Levels
        // "Traps that increase privilege level are termed vertical traps, while traps that remain
        // at the same privilege level are termed horizontal traps."

        let exception_pc = self.epc(cpu.pc);
        let previous_mode = cpu.mode;
        let cause = self.exception_code();

        // Handle the trap in M-mode.
        cpu.mode = Mode::Machine;

        // Set the program counter to the machine trap-handler base address (mtvec).
        cpu.pc = (cpu.state.read(MTVEC) & !1) as u32;

        // 3.1.15 Machine Exception Program Counter (mepc)
        // "The low bit of mepc (mepc[0]) is always zero."
        // "When a trap is taken into M-mode, mepc is written with the virtual address of
        // the instruction that was interrupted or that encountered the exception.
        // Otherwise, mepc is never written by the implementation, though it may be
        // explicitly written by software."
        cpu.state.write(MEPC, exception_pc & !1);

        // 3.1.16 Machine Cause Register (mcause)
        // "When a trap is taken into M-mode, mcause is written with a code indicating
        // the event that caused the trap. Otherwise, mcause is never written by the
        // implementation, though it may be explicitly written by software."
        cpu.state.write(MCAUSE, cause);

        // 3.1.17 Machine Trap Value (mtval) Register
        // "When a trap is taken into M-mode, mtval is either set to zero or written with
        // exception-specific information to assist software in handling the trap.
        // Otherwise, mtval is never written by the implementation, though it may be
        // explicitly written by software."
        cpu.state.write(MTVAL, self.trap_value(exception_pc));

        // Set a previous interrupt-enable bit for machine mode (MPIE, 7) to the value
        // of a global interrupt-enable bit for machine mode (MIE, 3).
        cpu.state
            .write_mstatus(MSTATUS_MPIE, cpu.state.read_mstatus(MSTATUS_MIE));
        // Set a global interrupt-enable bit for machine mode (MIE, 3) to 0.
        cpu.state.write_mstatus(MSTATUS_MIE, 0);
        // When a trap is taken from privilege mode y into privilege mode x, xPIE is set
        // to the value of x IE; x IE is set to 0; and xPP is set to y.
        match previous_mode {
            Mode::User => cpu.state.write_mstatus(MSTATUS_MPP, Mode::User as u32),
            Mode::Machine => cpu.state.write_mstatus(MSTATUS_MPP, Mode::Machine as u32),
            _ => panic!("previous privilege mode is invalid"),
        }

        match self {
            Exception::InstructionAddressMisaligned | Exception::InstructionAccessFault => {
                Trap::Fatal
            }
            Exception::IllegalInstruction(_) => Trap::Invisible,
            Exception::Breakpoint => Trap::Requested,
            Exception::LoadAddressMisaligned
            | Exception::LoadAccessFault
            | Exception::StoreAMOAddressMisaligned
            | Exception::StoreAMOAccessFault => Trap::Fatal,
            Exception::EnvironmentCallFromUMode | Exception::EnvironmentCallFromMMode => {
                Trap::Requested
            }
            Exception::InstructionPageFault(_)
            | Exception::LoadPageFault(_)
            | Exception::StoreAMOPageFault(_) => Trap::Invisible,
        }
    }
}
