//! Monitor capture source backed by the host operating system.

mod backend;

use crate::handle::{CastHandle, Source};
use std::fmt;
use std::num::NonZeroU32;
use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

/// High-level monitor source that feeds one [`CastHandle`].
#[derive(Debug, Clone)]
pub struct MonitorCapture {
    /// Shared capture state.
    inner: Arc<MonitorCaptureInner>,
}

impl MonitorCapture {
    /// Starts capture on the selected monitor with default options.
    pub fn start(monitor: impl Into<Monitor>) -> Result<Self, MonitorCaptureError> {
        Self::start_with_options(monitor, MonitorCaptureOptions::default())
    }

    /// Starts capture on the selected monitor with explicit options.
    pub fn start_with_options(
        monitor: impl Into<Monitor>,
        options: MonitorCaptureOptions,
    ) -> Result<Self, MonitorCaptureError> {
        Self::from_handle(
            CastHandle::with_redraw_interval(redraw_interval(options.fps_cap)),
            monitor,
            options,
        )
    }

    /// Attaches one monitor source to an existing handle.
    pub fn from_handle(
        handle: CastHandle<MonitorCaptureSource>,
        monitor: impl Into<Monitor>,
        options: MonitorCaptureOptions,
    ) -> Result<Self, MonitorCaptureError> {
        let monitor = monitor.into();
        let worker = backend::spawn(handle.clone(), monitor.target(), options)?;

        Ok(Self {
            inner: Arc::new(MonitorCaptureInner {
                handle,
                monitor,
                worker: Some(worker),
            }),
        })
    }

    /// Returns the monitor currently bound to the capture.
    pub fn monitor(&self) -> Monitor {
        self.inner.monitor
    }
}

impl AsRef<CastHandle<MonitorCaptureSource>> for MonitorCapture {
    fn as_ref(&self) -> &CastHandle<MonitorCaptureSource> {
        &self.inner.handle
    }
}

impl Deref for MonitorCapture {
    type Target = CastHandle<MonitorCaptureSource>;

    fn deref(&self) -> &Self::Target {
        &self.inner.handle
    }
}

/// Source marker used by OS-backed monitor capture.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct MonitorCaptureSource;

/// One backend-native monitor target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Monitor {
    /// Backend-native monitor target.
    inner: backend::BackendMonitor,
}

impl Monitor {
    /// Builds one monitor target from a persisted host monitor identifier.
    pub const fn new(monitor_id: NonZeroU32) -> Self {
        Self {
            inner: backend::BackendMonitor::new(monitor_id),
        }
    }

    /// Enumerates every currently available monitor.
    pub fn all() -> Result<Vec<Self>, MonitorCaptureError> {
        backend::all().map(|monitors| monitors.into_iter().map(|inner| Self { inner }).collect())
    }

    /// Returns the persisted host monitor identifier.
    pub const fn id(&self) -> NonZeroU32 {
        self.inner.id()
    }

    /// Returns the backend-native monitor target used by the worker.
    pub(crate) const fn target(self) -> backend::BackendMonitor {
        self.inner
    }
}

impl From<NonZeroU32> for Monitor {
    fn from(monitor_id: NonZeroU32) -> Self {
        Self::new(monitor_id)
    }
}

impl fmt::Display for Monitor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "monitor:{}", self.id())
    }
}

/// Typed options used when starting one monitor capture session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MonitorCaptureOptions {
    /// Maximum redraw cadence requested by the capture session.
    pub(crate) fps_cap: NonZeroU32,
    /// Whether the cursor should be visible in frames.
    pub(crate) shows_cursor: bool,
    /// Whether click indicators should be visible in frames.
    pub(crate) shows_click_indicators: bool,
    /// Whether this app should be excluded from capture when supported.
    pub(crate) excludes_self: bool,
}

impl MonitorCaptureOptions {
    /// Builds the default monitor-capture options.
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

    /// Starts one monitor capture session using these options.
    pub fn start(self, monitor: impl Into<Monitor>) -> Result<MonitorCapture, MonitorCaptureError> {
        MonitorCapture::start_with_options(monitor, self)
    }
}

impl Default for MonitorCaptureOptions {
    fn default() -> Self {
        Self::new()
    }
}

/// Failure returned when one monitor capture session cannot start cleanly.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
#[error("{message}")]
pub struct MonitorCaptureError {
    /// Human-readable backend failure.
    message: Arc<str>,
    /// Stable classification exposed as one small flag.
    flag: MonitorCaptureErrorFlag,
}

/// Stable startup classifications for monitor capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum MonitorCaptureErrorFlag {
    /// The current platform does not provide one supported backend.
    UnsupportedPlatform,
    /// The backend could not list or access monitor sources.
    SourceUnavailable,
    /// The requested monitor could not be resolved by the backend.
    MonitorUnavailable,
    /// The current process could not be excluded from capture when requested.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    SelfExclusionUnavailable,
    /// The backend could not start one capture session.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    StartFailed,
}

impl MonitorCaptureError {
    /// Builds the unsupported-platform error.
    pub(crate) fn unsupported_platform() -> Self {
        Self {
            message: Arc::from("monitor capture is not available on this platform"),
            flag: MonitorCaptureErrorFlag::UnsupportedPlatform,
        }
    }

    /// Builds the source-unavailable error.
    pub(crate) fn source_unavailable(message: impl Into<Arc<str>>) -> Self {
        Self {
            message: message.into(),
            flag: MonitorCaptureErrorFlag::SourceUnavailable,
        }
    }

    /// Builds the monitor-unavailable error.
    pub(crate) fn monitor_unavailable(message: impl Into<Arc<str>>) -> Self {
        Self {
            message: message.into(),
            flag: MonitorCaptureErrorFlag::MonitorUnavailable,
        }
    }

    /// Builds the self-exclusion-unavailable error.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(crate) fn self_exclusion_unavailable(message: impl Into<Arc<str>>) -> Self {
        Self {
            message: message.into(),
            flag: MonitorCaptureErrorFlag::SelfExclusionUnavailable,
        }
    }

    /// Builds the start-failed error.
    pub(crate) fn start_failed(message: impl Into<Arc<str>>) -> Self {
        Self {
            message: message.into(),
            flag: MonitorCaptureErrorFlag::StartFailed,
        }
    }

    /// Returns whether the backend could not list or access monitor sources.
    pub fn is_source_unavailable(&self) -> bool {
        self.flag == MonitorCaptureErrorFlag::SourceUnavailable
    }

    /// Returns whether the requested monitor could not be resolved by the backend.
    pub fn is_monitor_unavailable(&self) -> bool {
        self.flag == MonitorCaptureErrorFlag::MonitorUnavailable
    }

    /// Returns whether self-exclusion was requested but unavailable.
    pub fn is_self_exclusion_unavailable(&self) -> bool {
        self.flag == MonitorCaptureErrorFlag::SelfExclusionUnavailable
    }

    /// Returns whether the backend could not start one capture session.
    pub fn is_start_failed(&self) -> bool {
        self.flag == MonitorCaptureErrorFlag::StartFailed
    }
}

/// Runtime error reported by one active monitor-capture source.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
#[error("{message}")]
pub struct MonitorCaptureRuntimeError {
    /// Human-readable backend failure.
    message: Arc<str>,
    /// Stable classification exposed as one small flag.
    flag: MonitorCaptureRuntimeErrorFlag,
}

/// Stable runtime classifications for monitor capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum MonitorCaptureRuntimeErrorFlag {
    /// The source could not access one delivered frame.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    FrameAccess,
    /// The source delivered one unsupported pixel format.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    UnsupportedPixelFormat,
}

impl MonitorCaptureRuntimeError {
    /// Builds one frame-access failure.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    pub(crate) fn frame_access(message: impl Into<Arc<str>>) -> Self {
        Self {
            message: message.into(),
            flag: MonitorCaptureRuntimeErrorFlag::FrameAccess,
        }
    }

    /// Builds one unsupported-format failure.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    pub(crate) fn unsupported_pixel_format(message: impl Into<Arc<str>>) -> Self {
        Self {
            message: message.into(),
            flag: MonitorCaptureRuntimeErrorFlag::UnsupportedPixelFormat,
        }
    }

    /// Returns whether the source could not access one delivered frame.
    pub fn is_frame_access(&self) -> bool {
        self.flag == MonitorCaptureRuntimeErrorFlag::FrameAccess
    }

    /// Returns whether the source delivered one unsupported pixel format.
    pub fn is_unsupported_pixel_format(&self) -> bool {
        self.flag == MonitorCaptureRuntimeErrorFlag::UnsupportedPixelFormat
    }
}

impl Source for MonitorCaptureSource {
    type Error = MonitorCaptureRuntimeError;
}

/// Shared state for one monitor capture.
#[derive(Debug)]
struct MonitorCaptureInner {
    /// Handle fed by the capture worker.
    handle: CastHandle<MonitorCaptureSource>,
    /// Monitor bound to the worker.
    monitor: Monitor,
    /// Worker owned until the capture is dropped.
    worker: Option<backend::WorkerHandle>,
}

impl Drop for MonitorCaptureInner {
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
    use super::{Monitor, MonitorCapture, MonitorCaptureInner};
    use crate::handle::CastHandle;
    use std::num::NonZeroU32;
    use std::sync::Arc;

    /// Monitor captures should expose their handles through cheap borrowing.
    #[test]
    fn monitor_capture_borrows_its_handle() {
        let capture = MonitorCapture {
            inner: Arc::new(MonitorCaptureInner {
                handle: CastHandle::new(),
                monitor: Monitor::new(NonZeroU32::new(7).expect("test monitor id")),
                worker: None,
            }),
        };

        assert!(std::ptr::eq(
            capture.as_ref().inner.as_ref(),
            capture.inner.handle.inner.as_ref()
        ));
    }
}
