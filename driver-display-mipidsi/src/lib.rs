#![no_std]
#[macro_use]
extern crate alloc;

use alloc::{boxed::Box, vec::Vec};
use core::cell::RefCell;
use critical_section::Mutex;
use display_interface::WriteOnlyDataCommand;
use display_interface_spi::SPIInterface;
use driver_interface::{display::DisplayResource, Driver, DriverResource};
use embedded_graphics::{draw_target::DrawTarget, prelude::Dimensions, primitives::Rectangle};
use embedded_hal::{delay::DelayNs, digital::OutputPin};
use embedded_hal_bus::spi;
use mipidsi::{models::*, options::ColorInversion, Builder};

#[macro_use]
mod macros;

cfg_if::cfg_if! {
    if #[cfg(feature = "st7789")] {
        use_display_model!(ST7789);
    } else {
        compile_error!("no display model selected");
    }
}

pub use mipidsi::options::ColorOrder;
pub use mipidsi::options::{Orientation, Rotation};

pub trait Delay: DelayNs + Clone + Copy {}
impl<T> Delay for T where T: DelayNs + Clone + Copy {}

#[derive(Debug)]
pub enum Error {
    AlreadyInitialized,
    InitError,
}

pub struct Config<DI, RST, DELAY>
where
    DI: WriteOnlyDataCommand,
    RST: OutputPin,
    DELAY: DelayNs,
{
    interface: Option<DI>,
    reset_pin: Option<RST>,
    delay_source: Option<DELAY>,
    display_size: (u16, u16),
    color_order: ColorOrder,
    invert_colors: bool,
    orientation: Orientation,
}

impl<DI, RST, DELAY> Config<DI, RST, DELAY>
where
    DI: WriteOnlyDataCommand,
    RST: OutputPin,
    DELAY: DelayNs,
{
    pub fn new(
        interface: DI,
        reset_pin: RST,
        delay_source: DELAY,
        display_size: (u16, u16),
        color_order: ColorOrder,
        invert_colors: bool,
        orientation: Orientation,
    ) -> Self {
        Config {
            interface: Some(interface),
            reset_pin: Some(reset_pin),
            delay_source: Some(delay_source),
            display_size,
            color_order,
            invert_colors,
            orientation,
        }
    }
}

pub struct MipiDsiDisplayDriver<DI, RST, DELAY>
where
    DI: WriteOnlyDataCommand,
    RST: OutputPin,
    DELAY: DelayNs,
{
    config: Config<DI, RST, DELAY>,
    draw_target: Option<mipidsi::Display<DI, DisplayModel, RST>>,
}

struct DisplayWrapper<DI, RST>
where
    DI: WriteOnlyDataCommand,
    RST: OutputPin,
{
    display: mipidsi::Display<DI, DisplayModel, RST>,
}

impl<DI, RST> core::fmt::Debug for DisplayWrapper<DI, RST>
where
    DI: WriteOnlyDataCommand,
    RST: OutputPin,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DisplayWrapper").finish()
    }
}

impl<DI, RST> DisplayResource for DisplayWrapper<DI, RST>
where
    DI: WriteOnlyDataCommand,
    RST: OutputPin,
{
    fn bounding_box(&self) -> Rectangle {
        self.display.bounding_box()
    }

    fn draw_iter(
        &mut self,
        pixels: &mut dyn Iterator<
            Item = embedded_graphics::Pixel<embedded_graphics::pixelcolor::Rgb565>,
        >,
    ) -> Result<(), core::convert::Infallible> {
        self.display
            .draw_iter(pixels)
            .expect("failed to draw pixels");
        Ok(())
    }
}

impl<DI, RST, DELAY> Driver for MipiDsiDisplayDriver<DI, RST, DELAY>
where
    DI: WriteOnlyDataCommand + 'static,
    RST: OutputPin + 'static,
    DELAY: DelayNs + 'static,
{
    type Error = Error;

    const NAME: &'static str = "driver-display-mipidsi";

    fn start(&mut self) -> Result<Vec<DriverResource>, Self::Error> {
        let di = self
            .config
            .interface
            .take()
            .ok_or(Error::AlreadyInitialized)?;
        let rst = self
            .config
            .reset_pin
            .take()
            .ok_or(Error::AlreadyInitialized)?;
        let mut delay = self
            .config
            .delay_source
            .take()
            .ok_or(Error::AlreadyInitialized)?;

        let color_inversion = match self.config.invert_colors {
            true => ColorInversion::Inverted,
            false => ColorInversion::Normal,
        };

        let display: mipidsi::Display<DI, DisplayModel, RST> =
            Builder::new(DisplayModel::default(), di.into())
                .display_size(self.config.display_size.0, self.config.display_size.1)
                .color_order(self.config.color_order)
                .invert_colors(color_inversion)
                .reset_pin(rst)
                .orientation(self.config.orientation)
                .init(&mut delay)
                .map_err(|_| Error::InitError)?;

        let _ = self.draw_target.insert(display);

        // convert self into a DisplayResource
        let display_resource = DriverResource::Display(Box::new(DisplayWrapper {
            display: self.draw_target.take().unwrap(),
        }));

        Ok(vec![display_resource])
    }
}

impl<DI, RST, DELAY> MipiDsiDisplayDriver<DI, RST, DELAY>
where
    DI: WriteOnlyDataCommand,
    RST: OutputPin,
    DELAY: DelayNs,
{
    fn create(config: Config<DI, RST, DELAY>) -> Result<Self, Error>
    where
        Self: Sized,
    {
        Ok(MipiDsiDisplayDriver {
            config,
            draw_target: None,
        })
    }
}

pub fn init_display<'a, BUS, DC, CS, DELAY, RST>(
    spi_bus: &'a Mutex<RefCell<BUS>>,
    dc_pin: DC,
    cs_pin: CS,
    delay_source: DELAY,
    reset_pin: RST,
    display_size: (u16, u16),
    color_order: ColorOrder,
    invert_colors: bool,
    orientation: Orientation,
) -> Result<MipiDsiDisplayDriver<impl WriteOnlyDataCommand + 'a, RST, DELAY>, Error>
where
    BUS: embedded_hal::spi::SpiBus,
    DC: OutputPin + 'a,
    CS: OutputPin + 'a,
    DELAY: DelayNs + Copy + 'a,
    RST: OutputPin + 'a,
{
    let spi_device = spi::CriticalSectionDevice::new(spi_bus, cs_pin, delay_source).unwrap();
    let interface = SPIInterface::new(spi_device, dc_pin);
    MipiDsiDisplayDriver::create(Config::new(
        interface,
        reset_pin,
        delay_source,
        display_size,
        color_order,
        invert_colors,
        orientation,
    ))
}
