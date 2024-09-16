use riscv::bus::DRAM_BASE;
use riscv::cpu::REGISTERS_COUNT;
use riscv::dram::DRAM_SIZE;
use riscv::emulator::Emulator;

pub const DEFAULT_SP: u32 = DRAM_BASE + DRAM_SIZE;

/// Create registers for x0-x31 with expected values.
pub fn create_xregs(non_zero_regs: Vec<(usize, u32)>) -> [u32; REGISTERS_COUNT] {
    let mut xregs = [0; REGISTERS_COUNT];

    // Based on XRegisters::new().
    xregs[2] = DEFAULT_SP;

    for pair in non_zero_regs.iter() {
        xregs[pair.0] = pair.1;
    }
    xregs
}

/// Start a test and check if the registers are expected.
pub fn run(emu: &mut Emulator, data: Vec<u8>, expected_xregs: &[u32; 32]) {
    let len = data.len() as u32;

    emu.is_debug = true;

    emu.initialize_dram(data);
    emu.initialize_pc(DRAM_BASE);

    emu.test_start(DRAM_BASE, DRAM_BASE + len);

    for (i, e) in expected_xregs.iter().enumerate() {
        assert_eq!(*e, emu.cpu.xregs.read(i as u32), "fails at {}", i);
    }
}
