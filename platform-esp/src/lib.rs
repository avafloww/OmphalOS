#![no_std]
#![feature(asm_experimental_arch)]

#[macro_use]
extern crate alloc;
extern crate static_cell;

use core::arch::asm;
use esp_backtrace as _;
use esp_hal::{
    clock::{ClockControl, Clocks},
    cpu_control::CpuControl,
    gpio::Io,
    peripherals::Peripherals,
    prelude::*,
    rtc_cntl::Rtc,
    system::SystemControl,
    timer::{timg::TimerGroup, ErasedTimer},
};
use kernel_logger::init_logger;
use log::debug;
use static_cell::StaticCell;

pub use board::BoardData;
pub use esp_hal;

mod board;
pub mod context;

/// Platform-specific data that is passed to the kernel at boot time.
pub struct PlatformData {
    pub external_ram_start: usize,
    pub external_ram_size: usize,
}

/// The first kernel code that runs on the Xtensa architecture.
/// It sets up early architecture-specific initialization and then calls `kernel_init`.
#[entry]
fn main() -> ! {
    static CLOCKS: StaticCell<Clocks> = StaticCell::new();

    let peripherals = Peripherals::take();

    let system = SystemControl::new(peripherals.SYSTEM);
    let clocks = CLOCKS.init(ClockControl::max(system.clock_control).freeze());

    let timer_group0 = TimerGroup::new(peripherals.TIMG0, &clocks);
    let mut wdt0 = timer_group0.wdt;

    let timer_group1 = TimerGroup::new(peripherals.TIMG1, &clocks);
    let mut wdt1 = timer_group1.wdt;

    // Disable the RTC and TIMG watchdog timers
    let mut rtc = Rtc::new(peripherals.LPWR);
    rtc.rwdt.disable();
    wdt0.disable();
    wdt1.disable();

    // park the inactive core
    let mut cpu_control = CpuControl::new(peripherals.CPU_CTRL);
    unsafe { cpu_control.park_core(esp_hal::Cpu::AppCpu) };

    // save timer0 for multitasking
    let erased_timer: ErasedTimer = timer_group0.timer0.into();
    crate::context::set_timer0(crate::context::TimeBase::new(erased_timer));

    // setup logger
    init_logger(log::LevelFilter::Debug);

    // initialize PSRAM and memory allocator
    let (psram_start, psram_size) = if esp_hal::psram::PSRAM_BYTES > 0 {
        debug!("psram init");
        esp_hal::psram::init_psram(peripherals.PSRAM);

        (
            esp_hal::psram::psram_vaddr_start(),
            esp_hal::psram::PSRAM_BYTES,
        )
    } else {
        debug!("psram not available");
        (0, 0)
    };

    let io = Io::new(peripherals.GPIO, peripherals.IO_MUX);

    // init board-specific data (nb: this is feature-gated depending on the target board)
    #[cfg(any(feature = "target-lilygo_t_deck", feature = "target-lilygo_t_watch_s3"))]
    let board_data = BoardData {
        clocks,
        io,
        spi2: peripherals.SPI2,
    };

    // init platform-specific data
    let platform_data = PlatformData {
        external_ram_start: psram_start,
        external_ram_size: psram_size,
    };

    unsafe { kernel_init(platform_data, board_data) }
}

pub fn memory_fence() {
    unsafe {
        asm!("memw");
    }
}

extern "Rust" {
    fn kernel_init(platform_data: PlatformData, board_data: BoardData) -> !;
}
