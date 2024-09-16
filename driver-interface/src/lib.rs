#![no_std]
extern crate alloc;

use alloc::{boxed::Box, vec::Vec};

pub mod display;

/// The driver interface trait.
/// All drivers must implement this trait, either directly or through a subtrait.
pub trait Driver: core::any::Any + 'static {
    /// The error type that the driver may return.
    type Error: core::fmt::Debug;

    const NAME: &'static str;
    fn name(&self) -> &'static str {
        Self::NAME
    }

    /// Start the driver with the specified configuration.
    /// A driver may optionally return one or more resources that it creates.
    fn start(&mut self) -> Result<Vec<DriverResource>, Self::Error>;

    /// Stop the driver.
    /// This method is optional and may not be implemented by all drivers.
    fn stop(&mut self) -> Result<(), Self::Error> {
        unimplemented!("BUG: stop() not implemented for driver {}", Self::NAME);
    }
}

#[derive(Debug)]
pub enum DriverResource {
    Display(Box<dyn display::DisplayResource>),
}
