//! Ready-made frame sources.

/// Display capture source backed by the host operating system.
pub mod display;

pub use display::{
    Display, DisplayCapture, DisplayCaptureError, DisplayCaptureOptions,
    DisplayCaptureRuntimeError, DisplayCaptureSource,
};
