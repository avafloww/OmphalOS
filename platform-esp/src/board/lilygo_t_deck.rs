use alloc::vec::Vec;
use core::cell::RefCell;
use critical_section::Mutex;
use driver_display_mipidsi::{ColorOrder, Orientation, Rotation};
use driver_interface::Driver;
use esp_hal::delay::Delay;
use esp_hal::gpio::{Level, Output};
use esp_hal::spi::{master::Spi, SpiMode};
use esp_hal::{clock::Clocks, gpio::Io, peripherals::SPI2, spi::FullDuplexMode};
use fugit::RateExtU32;
use static_cell::StaticCell;

/// Board-specific data that is passed to the kernel at boot time.
pub struct BoardData {
    pub clocks: &'static mut Clocks<'static>,
    pub io: Io,
    pub spi2: esp_hal::peripherals::SPI2,
}

impl BoardData {
    /// Consumes the board data and initializes the board.
    /// Returns a list of drivers that are ready to be started.
    pub fn init(self) -> Vec<impl Driver> {
        static SPI2_BUS: StaticCell<Mutex<RefCell<Spi<SPI2, FullDuplexMode>>>> = StaticCell::new();

        // needs to be pulled high on T-Deck, otherwise we get no peripheral power (i.e. no display)
        let _peripheral_pwr = Output::new(self.io.pins.gpio10, Level::High);

        let spi2_bus = SPI2_BUS.init(Mutex::new(RefCell::new(
            Spi::new(self.spi2, 60u32.MHz(), SpiMode::Mode0, &self.clocks)
                .with_sck(self.io.pins.gpio40)
                .with_mosi(self.io.pins.gpio41)
                .with_miso(self.io.pins.gpio38),
        )));

        // SPI chip select pins
        // set them all to high to start (disabled)
        let display_cs = Output::new(self.io.pins.gpio12, Level::High);
        let _lora_cs = Output::new(self.io.pins.gpio9, Level::High);
        let _sdcard_cs = Output::new(self.io.pins.gpio39, Level::High);

        // initialize display
        let delay = Delay::new(&self.clocks);
        let enable_pin = Output::new(self.io.pins.gpio42, Level::High);
        let dc_pin = Output::new(self.io.pins.gpio11, Level::Low);
        let display = driver_display_mipidsi::init_display(
            spi2_bus,
            dc_pin,
            display_cs,
            delay,
            enable_pin,
            (240, 320),
            ColorOrder::Rgb,
            true,
            Orientation {
                rotation: Rotation::Deg90,
                mirrored: false,
            },
        )
        .unwrap();

        vec![display]
    }
}
