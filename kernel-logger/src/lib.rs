#![no_std]

static mut PRINT_HOOK: Option<fn(core::fmt::Arguments)> = None;

pub const RESET: &str = "\u{001B}[0m";
pub const RED: &str = "\u{001B}[31m";
pub const GREEN: &str = "\u{001B}[32m";
pub const YELLOW: &str = "\u{001B}[33m";
pub const BLUE: &str = "\u{001B}[34m";
pub const CYAN: &str = "\u{001B}[35m";

pub fn init_logger(level: log::LevelFilter) {
    unsafe {
        log::set_logger_racy(&KernelLogger).unwrap();
        log::set_max_level_racy(level);
    }
}

pub fn set_print_hook(hook: fn(core::fmt::Arguments)) {
    unsafe {
        PRINT_HOOK = Some(hook);
    }
}

struct KernelLogger;

impl log::Log for KernelLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    #[allow(unused)]
    fn log(&self, record: &log::Record) {
        let color = match record.level() {
            log::Level::Error => RED,
            log::Level::Warn => YELLOW,
            log::Level::Info => GREEN,
            log::Level::Debug => BLUE,
            log::Level::Trace => CYAN,
        };

        #[cfg(feature = "platform-esp")]
        esp_println::println!("{}{} - {}{}", color, record.level(), record.args(), RESET);
        unsafe { PRINT_HOOK }.map(|hook| {
            hook(format_args!(
                "{}{} - {}{}",
                color,
                record.level(),
                record.args(),
                RESET
            ))
        });
    }

    fn flush(&self) {}
}
