//! Ready-made frame sources.

/// Monitor capture source backed by the host operating system.
pub mod monitor;

pub use monitor::{
    Monitor, MonitorCapture, MonitorCaptureError, MonitorCaptureOptions,
    MonitorCaptureRuntimeError, MonitorCaptureSource,
};
