//! Display capture source backed by the host operating system.

mod backend;

use crate::handle::{CastHandle, Source};
use std::fmt;
use std::num::NonZeroU32;
use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

/// High-level display source that feeds one [`CastHandle`].
#[derive(Debug, Clone)]
pub struct DisplayCapture {
    /// Shared capture state.
    inner: Arc<DisplayCaptureInner>,
}

impl DisplayCapture {
    /// Starts capture on the selected display with default options.
    pub fn start(display: impl Into<Display>) -> Result<Self, DisplayCaptureError> {
        Self::start_with_options(display, DisplayCaptureOptions::default())
    }

    /// Starts capture on the selected display with explicit options.
    pub fn start_with_options(
        display: impl Into<Display>,
        options: DisplayCaptureOptions,
    ) -> Result<Self, DisplayCaptureError> {
        Self::from_handle(
            CastHandle::with_redraw_interval(redraw_interval(options.fps_cap)),
            display,
            options,
        )
    }

    /// Attaches one display source to an existing handle.
    pub fn from_handle(
        handle: CastHandle<DisplayCaptureSource>,
        display: impl Into<Display>,
        options: DisplayCaptureOptions,
    ) -> Result<Self, DisplayCaptureError> {
        let display = display.into();
        let worker = backend::spawn(handle.clone(), display, options)?;

        Ok(Self {
            inner: Arc::new(DisplayCaptureInner {
                handle,
                display,
                worker: Some(worker),
            }),
        })
    }

    /// Returns the display currently bound to the capture.
    pub fn display(&self) -> Display {
        self.inner.display
    }
}

impl AsRef<CastHandle<DisplayCaptureSource>> for DisplayCapture {
    fn as_ref(&self) -> &CastHandle<DisplayCaptureSource> {
        &self.inner.handle
    }
}

impl Deref for DisplayCapture {
    type Target = CastHandle<DisplayCaptureSource>;

    fn deref(&self) -> &Self::Target {
        &self.inner.handle
    }
}

/// Source marker used by OS-backed display capture.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct DisplayCaptureSource;

/// One physical or virtual display exposed by the host operating system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Display(NonZeroU32);

impl Display {
    /// Builds one display identifier from a non-zero host display id.
    pub const fn new(display_id: NonZeroU32) -> Self {
        Self(display_id)
    }

    /// Returns the host display id used by the capture backends.
    pub const fn id(self) -> NonZeroU32 {
        self.0
    }
}

impl From<NonZeroU32> for Display {
    fn from(display_id: NonZeroU32) -> Self {
        Self::new(display_id)
    }
}

impl fmt::Display for Display {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "display:{}", self.id())
    }
}

/// Typed options used when starting one display capture session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DisplayCaptureOptions {
    /// Maximum redraw cadence requested by the capture session.
    pub(crate) fps_cap: NonZeroU32,
    /// Whether the cursor should be visible in frames.
    pub(crate) shows_cursor: bool,
    /// Whether click indicators should be visible in frames.
    pub(crate) shows_click_indicators: bool,
    /// Whether this app should be excluded from capture when supported.
    pub(crate) excludes_self: bool,
}

impl DisplayCaptureOptions {
    /// Builds the default display-capture options.
    pub fn new() -> Self {
        Self {
            fps_cap: NonZeroU32::new(30).expect("default FPS cap is non-zero"),
            shows_cursor: true,
            shows_click_indicators: false,
            excludes_self: false,
        }
    }

    /// Sets the maximum frame rate preferred by the capture session.
    pub fn with_fps_cap(mut self, fps_cap: NonZeroU32) -> Self {
        self.fps_cap = fps_cap;
        self
    }

    /// Sets whether captured frames should include the cursor.
    pub fn with_shows_cursor(mut self, shows_cursor: bool) -> Self {
        self.shows_cursor = shows_cursor;
        self
    }

    /// Sets whether captured frames should include click indicators.
    pub fn with_shows_click_indicators(mut self, shows_click_indicators: bool) -> Self {
        self.shows_click_indicators = shows_click_indicators;
        self
    }

    /// Sets whether this app should be excluded when the backend supports it.
    pub fn with_self_exclusion(mut self, excludes_self: bool) -> Self {
        self.excludes_self = excludes_self;
        self
    }

    /// Starts one display capture session using these options.
    pub fn start(self, display: impl Into<Display>) -> Result<DisplayCapture, DisplayCaptureError> {
        DisplayCapture::start_with_options(display, self)
    }
}

impl Default for DisplayCaptureOptions {
    fn default() -> Self {
        Self::new()
    }
}

/// Failure returned when one display capture session cannot start cleanly.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
#[error("{kind}")]
pub struct DisplayCaptureError {
    /// Stable error classification used for inspection without exposing variants.
    kind: DisplayCaptureErrorKind,
}

/// Private classification for display-capture startup failures.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
enum DisplayCaptureErrorKind {
    /// The current platform does not provide one supported backend.
    #[error("display capture is not available on this platform")]
    UnsupportedPlatform,
    /// The backend could not list or access display sources.
    #[error("display capture could not list shareable content: {message}")]
    SourceUnavailable {
        /// Human-readable source listing failure.
        message: Arc<str>,
    },
    /// The requested display could not be resolved by the backend.
    #[error("{message}")]
    DisplayUnavailable {
        /// Human-readable display resolution failure.
        message: Arc<str>,
    },
    /// The current process could not be excluded from capture when requested.
    #[error("{message}")]
    SelfExclusionUnavailable {
        /// Human-readable self-exclusion failure.
        message: Arc<str>,
    },
    /// The backend could not start one capture session.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    #[error("{message}")]
    StartFailed {
        /// Human-readable backend startup failure.
        message: Arc<str>,
    },
}

impl DisplayCaptureError {
    /// Builds the unsupported-platform error.
    pub(crate) fn unsupported_platform() -> Self {
        Self {
            kind: DisplayCaptureErrorKind::UnsupportedPlatform,
        }
    }

    /// Builds the source-unavailable error.
    pub(crate) fn source_unavailable(message: impl Into<Arc<str>>) -> Self {
        Self {
            kind: DisplayCaptureErrorKind::SourceUnavailable {
                message: message.into(),
            },
        }
    }

    /// Builds the display-unavailable error.
    pub(crate) fn display_unavailable(message: impl Into<Arc<str>>) -> Self {
        Self {
            kind: DisplayCaptureErrorKind::DisplayUnavailable {
                message: message.into(),
            },
        }
    }

    /// Builds the self-exclusion-unavailable error.
    pub(crate) fn self_exclusion_unavailable(message: impl Into<Arc<str>>) -> Self {
        Self {
            kind: DisplayCaptureErrorKind::SelfExclusionUnavailable {
                message: message.into(),
            },
        }
    }

    /// Builds the backend-start failure.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    pub(crate) fn start_failed(message: impl Into<Arc<str>>) -> Self {
        Self {
            kind: DisplayCaptureErrorKind::StartFailed {
                message: message.into(),
            },
        }
    }

    /// Returns whether the current platform does not provide one supported backend.
    pub fn is_unsupported_platform(&self) -> bool {
        matches!(self.kind, DisplayCaptureErrorKind::UnsupportedPlatform)
    }

    /// Returns whether the backend could not list or access display sources.
    pub fn is_source_unavailable(&self) -> bool {
        matches!(self.kind, DisplayCaptureErrorKind::SourceUnavailable { .. })
    }

    /// Returns whether the requested display could not be resolved.
    pub fn is_display_unavailable(&self) -> bool {
        matches!(
            self.kind,
            DisplayCaptureErrorKind::DisplayUnavailable { .. }
        )
    }

    /// Returns whether self-exclusion was requested but unavailable.
    pub fn is_self_exclusion_unavailable(&self) -> bool {
        matches!(
            self.kind,
            DisplayCaptureErrorKind::SelfExclusionUnavailable { .. }
        )
    }

    /// Returns whether the backend could not start one capture session.
    pub fn is_start_failed(&self) -> bool {
        matches!(self.kind, DisplayCaptureErrorKind::StartFailed { .. })
    }
}

/// Runtime error reported by one active display-capture source.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
#[error("{kind}")]
pub struct DisplayCaptureRuntimeError {
    /// Stable error classification used for inspection without exposing variants.
    kind: DisplayCaptureRuntimeErrorKind,
}

/// Private classification for display-capture runtime failures.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
enum DisplayCaptureRuntimeErrorKind {
    /// The source could not access one delivered frame.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    #[error("{message}")]
    FrameAccess {
        /// Human-readable frame-access failure.
        message: Arc<str>,
    },
    /// The source delivered one unsupported pixel format.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    #[error("{message}")]
    UnsupportedPixelFormat {
        /// Human-readable unsupported-format failure.
        message: Arc<str>,
    },
}

impl DisplayCaptureRuntimeError {
    /// Builds one frame-access failure.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    pub(crate) fn frame_access(message: impl Into<Arc<str>>) -> Self {
        Self {
            kind: DisplayCaptureRuntimeErrorKind::FrameAccess {
                message: message.into(),
            },
        }
    }

    /// Builds one unsupported-format failure.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    pub(crate) fn unsupported_pixel_format(message: impl Into<Arc<str>>) -> Self {
        Self {
            kind: DisplayCaptureRuntimeErrorKind::UnsupportedPixelFormat {
                message: message.into(),
            },
        }
    }

    /// Returns whether the source could not access one delivered frame.
    pub fn is_frame_access(&self) -> bool {
        matches!(
            self.kind,
            DisplayCaptureRuntimeErrorKind::FrameAccess { .. }
        )
    }

    /// Returns whether the source delivered one unsupported pixel format.
    pub fn is_unsupported_pixel_format(&self) -> bool {
        matches!(
            self.kind,
            DisplayCaptureRuntimeErrorKind::UnsupportedPixelFormat { .. }
        )
    }
}

impl Source for DisplayCaptureSource {
    type Error = DisplayCaptureRuntimeError;
}

/// Shared state for one display capture.
#[derive(Debug)]
struct DisplayCaptureInner {
    /// Handle fed by the capture worker.
    handle: CastHandle<DisplayCaptureSource>,
    /// Display bound to the worker.
    display: Display,
    /// Worker owned until the capture is dropped.
    worker: Option<backend::WorkerHandle>,
}

impl Drop for DisplayCaptureInner {
    fn drop(&mut self) {
        self.handle.stop();

        if let Some(worker) = self.worker.take() {
            worker.stop();
        }
    }
}

/// Returns the redraw interval implied by the configured frame-rate cap.
fn redraw_interval(fps_cap: NonZeroU32) -> Duration {
    Duration::from_secs_f64(1.0 / fps_cap.get() as f64)
}

#[cfg(test)]
mod tests {
    use super::{Display, DisplayCapture, DisplayCaptureInner};
    use crate::handle::CastHandle;
    use std::num::NonZeroU32;
    use std::sync::Arc;

    /// Display captures should expose their handles through cheap borrowing.
    #[test]
    fn display_capture_borrows_its_handle() {
        let capture = DisplayCapture {
            inner: Arc::new(DisplayCaptureInner {
                handle: CastHandle::new(),
                display: Display::new(NonZeroU32::new(7).expect("test display id")),
                worker: None,
            }),
        };

        assert!(std::ptr::eq(
            capture.as_ref().inner.as_ref(),
            capture.inner.handle.inner.as_ref()
        ));
    }
}
