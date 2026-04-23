//! Active monitor presentation surfaces for `iced`.
//!
//! `iced_live_cast` keeps frame capture and live presentation inside one focused
//! crate so host applications do not need to move live frames through their own
//! message loops.
//!
//! The public surface has three layers:
//!
//! - [`CastView`] for rendering a handle inside `iced`
//! - [`handle`] for callers that already own frames and want to push them
//! - [`source`] for ready-made capture sources like monitor capture
//!
//! `CastView` and `Frame::to_handle()` are part of the default crate surface.
//!
//! Run `cargo run --example basic`
//! for a full `iced` app with `main()` showing the built-in monitor source.
//!
//! Run `cargo run --example manual_push`
//! to see the lower-level manual path where your own producer thread feeds frames.

/// Frame values shared by cast handles and sources.
pub mod frame;
/// Lower-level mutable live handle fed by one or more sources.
pub mod handle;
/// Ready-made capture sources like monitor capture.
pub mod source;

mod render;
mod widget;

pub use frame::{Frame, FrameError};
pub use handle::{CastHandle, ManualSource, Source};
pub use iced::widget::image::FilterMethod;
pub use render::{LiveImage, LiveRasterRenderer};
pub use source::{
    Monitor, MonitorCapture, MonitorCaptureError, MonitorCaptureOptions,
    MonitorCaptureRuntimeError, MonitorCaptureSource,
};
pub use widget::CastView;
