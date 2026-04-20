//! Shared cast handles fed by one or more frame sources.

use crate::frame::Frame;
use std::convert::Infallible;
use std::fmt;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

/// Marker trait describing one family of frame source and the typed error it reports.
pub trait Source: 'static {
    /// Error reported by this source through one [`CastHandle`].
    type Error: Clone + Send + Sync + 'static;
}

/// Default marker used by callers that push frames manually.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ManualSource<E = Infallible> {
    /// Marker tying this manual source to its error type.
    _marker: PhantomData<fn() -> E>,
}

impl<E> ManualSource<E> {
    /// Builds one manual source marker for the selected error type.
    pub const fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<E> Default for ManualSource<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E> Source for ManualSource<E>
where
    E: Clone + Send + Sync + 'static,
{
    type Error = E;
}

/// Monotonically increasing identifier assigned to each cast handle.
static NEXT_HANDLE_ID: AtomicU64 = AtomicU64::new(1);

/// Cheap, cloneable handle shared by frame sources and the cast view widget.
pub struct CastHandle<S: Source = ManualSource> {
    /// Shared state used by sources and the widget.
    pub(crate) inner: Arc<CastHandleInner<S>>,
}

impl<S: Source> CastHandle<S> {
    /// Builds one empty handle with the default redraw cadence.
    pub fn new() -> Self {
        Self::with_redraw_interval(Duration::from_secs_f64(1.0 / 30.0))
    }

    /// Builds one empty handle with an explicit redraw cadence.
    pub fn with_redraw_interval(redraw_interval: Duration) -> Self {
        Self {
            inner: Arc::new(CastHandleInner {
                id: NEXT_HANDLE_ID.fetch_add(1, Ordering::Relaxed),
                redraw_interval,
                generation: AtomicU64::new(0),
                error_generation: AtomicU64::new(0),
                paused: AtomicBool::new(false),
                stopped: AtomicBool::new(false),
                alive: Arc::new(AtomicBool::new(true)),
                latest_frame: RwLock::new(None),
                last_error: RwLock::new(None),
            }),
        }
    }

    /// Presents one fresh frame to the attached view.
    pub fn present(&self, frame: impl Into<Frame>) {
        self.inner.record_frame(frame.into());
    }

    /// Records one source-side error without clearing the last good frame.
    pub fn report_error(&self, error: S::Error) {
        self.inner.record_error(error);
    }

    /// Pauses visible view updates without destroying the shared handle.
    pub fn pause(&self) {
        self.inner.paused.store(true, Ordering::Relaxed);
    }

    /// Resumes visible view updates.
    pub fn resume(&self) {
        self.inner.paused.store(false, Ordering::Relaxed);
    }

    /// Stops the handle and asks attached sources to wind down.
    pub fn stop(&self) {
        self.inner.stopped.store(true, Ordering::Relaxed);
    }

    /// Returns whether the handle is currently paused.
    pub fn is_paused(&self) -> bool {
        self.inner.paused.load(Ordering::Relaxed)
    }

    /// Returns whether the handle has been stopped.
    pub fn is_stopped(&self) -> bool {
        self.inner.stopped.load(Ordering::Relaxed)
    }

    /// Returns the redraw interval preferred by the attached source.
    pub fn redraw_interval(&self) -> Duration {
        self.inner.redraw_interval
    }

    /// Returns the latest frame dimensions when one exists.
    pub fn dimensions(&self) -> Option<(u32, u32)> {
        self.inner.latest_frame.read().ok().and_then(|frame| {
            frame.as_ref().map(|frame| {
                let (width, height) = frame.dimensions();
                (width.max(1), height.max(1))
            })
        })
    }

    /// Returns a clone of the latest available frame.
    pub fn snapshot(&self) -> Option<Frame> {
        self.inner.snapshot()
    }
}

impl<S: Source> CastHandle<S> {
    /// Returns the latest error reported by a source.
    pub fn last_error(&self) -> Option<S::Error> {
        self.inner
            .last_error
            .read()
            .ok()
            .and_then(|error| error.clone())
    }
}

impl<S: Source> Default for CastHandle<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: Source> Clone for CastHandle<S> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<S: Source> fmt::Debug for CastHandle<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CastHandle")
            .field("id", &self.inner.id)
            .field("paused", &self.is_paused())
            .field("stopped", &self.is_stopped())
            .finish_non_exhaustive()
    }
}

impl<S: Source> From<&CastHandle<S>> for CastHandle<S> {
    fn from(handle: &CastHandle<S>) -> Self {
        handle.clone()
    }
}

impl<S: Source> AsRef<CastHandle<S>> for CastHandle<S> {
    fn as_ref(&self) -> &CastHandle<S> {
        self
    }
}

/// Shared state used by one cast handle and its attached sources.
pub(crate) struct CastHandleInner<S: Source> {
    /// Stable identifier used to keep one GPU texture entry alive across frames.
    ///
    /// This is a resource-cache key, not one frame-cache key. New frames overwrite
    /// the same texture entry instead of allocating renderer state every redraw.
    pub id: u64,
    /// Redraw cadence preferred by the source feeding this handle.
    pub redraw_interval: Duration,
    /// Generation counter bumped whenever a fresh frame replaces the current one.
    pub generation: AtomicU64,
    /// Generation counter bumped whenever one source reports a new error.
    pub error_generation: AtomicU64,
    /// Flag telling the view to temporarily stop advancing frames.
    pub paused: AtomicBool,
    /// Flag telling the view and attached sources to shut down.
    pub stopped: AtomicBool,
    /// Shared liveness flag used to drop renderer cache entries when handles die.
    pub alive: Arc<AtomicBool>,
    /// Latest frame available to the view.
    pub latest_frame: RwLock<Option<Frame>>,
    /// Latest error reported by any attached source.
    pub last_error: RwLock<Option<S::Error>>,
}

impl<S: Source> CastHandleInner<S> {
    /// Returns the current frame generation.
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }

    /// Returns the current error generation.
    pub fn error_generation(&self) -> u64 {
        self.error_generation.load(Ordering::Relaxed)
    }

    /// Returns a cloneable snapshot of the latest frame.
    pub fn snapshot(&self) -> Option<Frame> {
        self.latest_frame
            .read()
            .ok()
            .and_then(|frame| frame.clone())
    }

    /// Returns whether the handle is currently paused.
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    /// Returns whether the handle has been stopped.
    pub fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::Relaxed)
    }

    /// Records one fresh frame when the handle is actively advancing.
    pub fn record_frame(&self, frame: Frame) {
        if self.is_stopped() || self.is_paused() {
            return;
        }

        if let Ok(mut slot) = self.latest_frame.write() {
            *slot = Some(frame);
        }

        self.generation.fetch_add(1, Ordering::Relaxed);

        if let Ok(mut error) = self.last_error.write() {
            *error = None;
        }
    }

    /// Records one source failure without clearing the last good frame.
    pub fn record_error(&self, error: S::Error) {
        if let Ok(mut slot) = self.last_error.write() {
            *slot = Some(error);
        }

        self.error_generation.fetch_add(1, Ordering::Relaxed);
    }
}

impl<S: Source> Drop for CastHandleInner<S> {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Relaxed);
        self.stopped.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::{CastHandle, ManualSource};
    use crate::frame::Frame;

    /// Active handles should advance generations when new frames arrive.
    #[test]
    fn generation_advances_for_active_frames() {
        let handle = CastHandle::<ManualSource<()>>::new();

        handle.present(sample_frame());

        assert_eq!(handle.inner.generation(), 1);
    }

    /// Paused handles should ignore incoming frames until they resume.
    #[test]
    fn paused_handles_ignore_frames() {
        let handle = CastHandle::<ManualSource<()>>::new();

        handle.pause();
        handle.present(sample_frame());
        handle.resume();
        handle.present(sample_frame());

        assert_eq!(handle.inner.generation(), 1);
    }

    /// Stopping a handle should flip the stopped flag immediately.
    #[test]
    fn stop_marks_handle_as_stopped() {
        let handle = CastHandle::<ManualSource<()>>::new();

        handle.stop();

        assert!(handle.is_stopped());
    }

    /// Presenting validated frames should update the shared snapshot.
    #[test]
    fn present_updates_the_handle() {
        let handle = CastHandle::<ManualSource<()>>::new();

        handle.present(sample_frame());

        assert!(handle.snapshot().is_some());
    }

    /// Default manual handles should work without spelling the source marker.
    #[test]
    fn default_manual_source_can_present_frames() {
        let handle: CastHandle = CastHandle::new();

        handle.present(sample_frame());

        assert!(handle.snapshot().is_some());
    }

    /// Recording one error should advance the error generation.
    #[test]
    fn error_generation_advances_for_reported_errors() {
        let handle = CastHandle::<ManualSource<&'static str>>::new();

        handle.report_error("boom");

        assert_eq!(handle.inner.error_generation(), 1);
    }

    /// Fresh frames should clear the last reported source error.
    #[test]
    fn presenting_a_frame_clears_the_last_error() {
        let handle = CastHandle::<ManualSource<&'static str>>::new();

        handle.report_error("boom");
        handle.present(sample_frame());

        assert_eq!(handle.last_error(), None);
    }

    /// Handles should keep distinct ids so renderer cache entries do not collide.
    #[test]
    fn handles_receive_distinct_ids() {
        let first = CastHandle::<ManualSource<()>>::new();
        let second = CastHandle::<ManualSource<()>>::new();

        assert_ne!(first.inner.id, second.inner.id);
    }

    /// Builds one sample frame for handle tests.
    fn sample_frame() -> Frame {
        Frame::from_bgra(2, 2, vec![255; 16]).expect("sample frame should validate")
    }
}
