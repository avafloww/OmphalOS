use alloc::boxed::Box;
use core::convert::Infallible;
use embedded_graphics::{pixelcolor::*, prelude::*, primitives::Rectangle};

pub trait DisplayResource: core::fmt::Debug {
    fn bounding_box(&self) -> Rectangle;
    fn draw_iter(
        &mut self,
        pixels: &mut dyn Iterator<Item = Pixel<Rgb565>>,
    ) -> Result<(), Infallible>;
}

impl Dimensions for dyn DisplayResource {
    fn bounding_box(&self) -> Rectangle {
        DisplayResource::bounding_box(self)
    }
}

impl DrawTarget for dyn DisplayResource {
    type Color = Rgb565;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = embedded_graphics::Pixel<Self::Color>>,
    {
        // convert the pixel iterator to a Pixel<Rgb565> iterator
        let mut pixels = pixels.into_iter().map(|Pixel(p, c)| Pixel(p, c.into()));
        DisplayResource::draw_iter(self, &mut pixels)?;

        Ok(())
    }
}

impl Dimensions for Box<dyn DisplayResource> {
    fn bounding_box(&self) -> Rectangle {
        DisplayResource::bounding_box(&**self)
    }
}

impl DrawTarget for Box<dyn DisplayResource> {
    type Color = Rgb565;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = embedded_graphics::Pixel<Self::Color>>,
    {
        // convert the pixel iterator to a Pixel<Rgb565> iterator
        let mut pixels = pixels.into_iter().map(|Pixel(p, c)| Pixel(p, c.into()));
        DisplayResource::draw_iter(&mut **self, &mut pixels)?;

        Ok(())
    }
}
