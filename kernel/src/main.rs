#![no_std]
#![no_main]
#![feature(const_mut_refs)]
// we're never actually passing any data beyond Rust, and the same compiler version at that, so this is fine
#![allow(improper_ctypes_definitions)]

#[allow(unused_imports)]
#[macro_use]
extern crate alloc;

use alloc::vec::Vec;
use driver_interface::{Driver, DriverResource};
use log::{info, trace};
use platform_esp::{BoardData, PlatformData};

cfg_if::cfg_if! {
    if #[cfg(feature = "platform-esp")] {
        pub use platform_esp as platform;
    } else {
        compile_error!("no platform feature enabled");
    }
}

pub mod allocator;
pub mod console;
pub mod task;

/// Called by the arch layer once early initialization is done.
/// This is where we start the kernel.
///
/// The following state is expected to be set up by the arch layer before calling us:
/// - system clocks and timers
/// - logging infrastructure
/// - heap allocator
#[no_mangle]
extern "Rust" fn kernel_init(platform_data: PlatformData, board_data: BoardData) -> ! {
    // initialize logging
    info!("OmphalOS kernel starting...");

    // initialize the heap allocator
    allocator::init!(
        64 * 1024, // 64K words = 256KB heap
        platform_data.external_ram_start,
        platform_data.external_ram_size
    );

    // load drivers
    let drivers = board_data.init();
    info!("{} drivers detected for this board", drivers.len());

    // start drivers
    let mut all_resources: Vec<DriverResource> = Vec::new();
    for mut driver in drivers.into_iter() {
        info!("starting driver: {}", driver.name());
        match driver.start() {
            Ok(mut resources) => {
                info!(
                    "driver started successfully - exposed resources: {:?}",
                    resources
                );

                all_resources.append(&mut resources);
            }
            Err(e) => panic!("driver failed to start: {:?}", e),
        }
    }

    // initialize kernel task
    trace!("kernel task init");
    task::init_kernel_task();

    // do we have any display resources? if so, clear the screen
    for resource in all_resources.into_iter() {
        match resource {
            DriverResource::Display(display) => {
                console::init(display).expect("failed to init console")
            }
        }
    }

    info!("init done");

    // info!("creating a test thread!");
    // let test_thread = task::Thread::create(test_task, core::ptr::null_mut());
    // info!("creating a test process");
    // task::with_task_manager(|task_manager| {
    //     task_manager.create_process("test_task".to_string(), test_thread);
    // });
    // info!("we created it :)");

    loop {}
}

// extern "C" {
//     fn test_task(_: *mut core::ffi::c_void) -> ();
// }
