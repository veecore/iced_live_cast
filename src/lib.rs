//! Active display presentation surfaces for `iced`.
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
//! Run `cargo run --example basic -- 1`
//! for a full `iced` app with `main()` showing the built-in display source.
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
pub use source::{
    Display, DisplayCapture, DisplayCaptureError, DisplayCaptureOptions,
    DisplayCaptureRuntimeError, DisplayCaptureSource,
};
pub use widget::{CastView, FitMode};
