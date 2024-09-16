use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, Ordering},
};

use alloc::{
    boxed::Box,
    string::{String, ToString},
    vec::Vec,
};
use driver_interface::display::DisplayResource;
use embedded_graphics::{
    draw_target::DrawTarget,
    mono_font::{ascii::FONT_8X13, MonoFont, MonoTextStyle},
    pixelcolor::Rgb565,
    prelude::{Point, RgbColor},
    text::Text,
    Drawable,
};
use log::{debug, info};
use ringbuf::{
    traits::{Consumer, Producer},
    HeapRb,
};
use static_cell::StaticCell;

use crate::task;

static mut CONSOLE_WRITER: Option<ConsoleWriter> = None;

const CONSOLE_FONT: MonoFont = FONT_8X13;
const CHAR_WIDTH: u16 = 8;
const LINE_HEIGHT: u16 = 16;

pub fn init(mut display: Box<dyn DisplayResource>) -> Result<(), ()> {
    static CONSOLE: StaticCell<Console> = StaticCell::new();

    display.clear(Rgb565::BLACK).unwrap();
    let style = MonoTextStyle::new(&CONSOLE_FONT, Rgb565::WHITE);
    Text::new("Hello world!", Point::new(0, 16), style)
        .draw(&mut display)
        .unwrap();
    Text::new("This is a test", Point::new(0, 32), style)
        .draw(&mut display)
        .unwrap();

    let console = CONSOLE.init(Console::new(display));
    task::with_task_manager(|tm| unsafe {
        tm.create_process(
            "console".to_string(),
            task::Thread::create(
                core::mem::transmute::<*const (), extern "C" fn()>(run as *const ()),
                console as *mut Console as *mut (),
            ),
        );
    });

    Ok(())
}

#[derive(Debug)]
struct Console {
    display: Box<dyn DisplayResource>,
    lines: Vec<String>,
    row_count: u16,
    col_count: u16,
    dirty: AtomicBool,
}

impl Console {
    fn new(display: Box<dyn DisplayResource>) -> Self {
        match unsafe { CONSOLE_WRITER.replace(ConsoleWriter::new()) } {
            Some(_) => panic!("console writer already initialized"),
            None => {}
        }

        let display_size = display.bounding_box().size;
        let row_count = display_size.height as u16 / LINE_HEIGHT;
        let col_count = display_size.width as u16 / CHAR_WIDTH;

        Console {
            display,
            lines: Vec::new(),
            row_count,
            col_count,
            dirty: AtomicBool::new(true),
        }
    }

    fn write(&mut self, s: &str) {
        let mut line = String::new();
        let mut i = 0;
        for c in s.chars() {
            if i == self.col_count - 1 || c == '\n' {
                self.lines.push(line);
                i = 0;
                line = String::new();
            } else {
                line.push(c);
                i += 1;
            }
        }
        self.lines.push(line);

        while self.lines.len() > self.row_count as usize {
            self.lines.remove(0);
        }
    }

    fn draw(&mut self) {
        self.display.clear(Rgb565::BLACK).unwrap();
        let style = MonoTextStyle::new(&CONSOLE_FONT, Rgb565::WHITE);
        for (i, line) in self.lines.iter().enumerate() {
            Text::new(
                line,
                Point::new(0, (i + 1) as i32 * LINE_HEIGHT as i32),
                style,
            )
            .draw(&mut self.display)
            .unwrap();
        }
    }
}

struct ConsoleWriter {
    buffer: UnsafeCell<HeapRb<String>>,
}

impl ConsoleWriter {
    fn new() -> Self {
        ConsoleWriter {
            buffer: UnsafeCell::new(HeapRb::new(16)),
        }
    }

    fn write(&self, s: String) {
        // SAFETY: ringbuf allows concurrent writes
        unsafe {
            self.buffer
                .get()
                .as_mut()
                .unwrap()
                .try_push(s)
                .expect("console write buffer full")
        };
    }

    fn flush(&self, console: &mut Console) {
        // SAFETY: ringbuf allows concurrent reads
        let buffer = unsafe { self.buffer.get().as_mut().unwrap() };
        let mut changed = false;

        while let Some(s) = buffer.try_pop() {
            console.write(&s);
            changed = true;
        }

        if changed {
            console.dirty.store(true, Ordering::Relaxed);
        }
    }
}

extern "C" fn run(console: &mut Console) {
    critical_section::with(|_| kernel_logger::set_print_hook(klog_print));

    info!("console task started");
    debug!("writer: {:?}", console);
    loop {
        critical_section::with(|_| {
            unsafe {
                CONSOLE_WRITER.as_ref().unwrap().flush(console);
            }

            if console.dirty.load(Ordering::Relaxed) {
                console.draw();
                console.dirty.store(false, Ordering::Relaxed);
            }
        });
    }
}

fn klog_print(args: core::fmt::Arguments) {
    let queue = unsafe { CONSOLE_WRITER.as_ref().unwrap() };
    let mut line = String::new();
    core::fmt::write(&mut line, args).unwrap();
    queue.write(line);
}
