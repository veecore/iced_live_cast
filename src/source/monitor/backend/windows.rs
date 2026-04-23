//! Windows Graphics Capture backend for monitor capture.

use super::BackendMonitor;
use crate::frame::Frame;
use crate::handle::CastHandle;
use crate::source::monitor::{
    MonitorCaptureError, MonitorCaptureOptions, MonitorCaptureRuntimeError, MonitorCaptureSource,
};
use std::num::NonZeroU32;
use windows_capture::capture::{
    CaptureControl, Context, GraphicsCaptureApiError, GraphicsCaptureApiHandler,
};
use windows_capture::frame::Frame as WindowsFrame;
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};

/// Running Windows Graphics Capture worker.
pub(crate) struct WorkerHandle {
    /// Capture control returned by the backend crate.
    control: CaptureControl<FrameHandler, FrameHandlerError>,
}

impl WorkerHandle {
    /// Stops the Windows capture worker and waits for teardown.
    pub(crate) fn stop(self) {
        let _ = self.control.stop();
    }
}

/// Enumerates every currently available Windows monitor target.
pub(crate) fn all() -> Result<Vec<BackendMonitor>, MonitorCaptureError> {
    Monitor::enumerate()
        .map_err(source_unavailable)?
        .into_iter()
        .map(monitor_from_backend_monitor)
        .collect()
}

/// Spawns one Windows Graphics Capture worker for the requested monitor.
pub(crate) fn spawn(
    handle: CastHandle<MonitorCaptureSource>,
    monitor: BackendMonitor,
    options: MonitorCaptureOptions,
) -> Result<WorkerHandle, MonitorCaptureError> {
    let settings = os_crate_config(monitor, options, handle)?;
    let control = FrameHandler::start_free_threaded(settings).map_err(capture_api_error)?;

    Ok(WorkerHandle { control })
}

/// Small callback handler that forwards frames into the shared capture handle.
struct FrameHandler {
    /// Handle updated by arriving frames.
    handle: CastHandle<MonitorCaptureSource>,
}

/// Zero-sized callback error used to stop the backend without carrying payloads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FrameHandlerError;

impl GraphicsCaptureApiHandler for FrameHandler {
    /// Flags passed from [`Settings`] into the handler.
    type Flags = CastHandle<MonitorCaptureSource>;
    /// Error surfaced by the handler back into the control handle.
    type Error = FrameHandlerError;

    /// Builds the frame handler from the configured handle.
    fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
        Ok(Self { handle: ctx.flags })
    }

    /// Pushes each arriving frame into the shared handle.
    fn on_frame_arrived(
        &mut self,
        frame: &mut WindowsFrame<'_>,
        capture_control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        if self.handle.is_stopped() {
            capture_control.stop();
            return Ok(());
        }

        if self.handle.is_paused() {
            return Ok(());
        }

        let format = frame.color_format();
        let width = frame.width();
        let height = frame.height();
        let mut buffer = match frame.buffer() {
            Ok(buffer) => buffer,
            Err(error) => {
                let error = MonitorCaptureRuntimeError::frame_access(format!(
                    "Windows Graphics Capture could not map the frame: {error}"
                ));
                self.handle.report_error(error);
                return Err(FrameHandlerError);
            }
        };

        if format != ColorFormat::Bgra8 {
            let error = MonitorCaptureRuntimeError::unsupported_pixel_format(format!(
                "unsupported Windows Graphics Capture pixel format: {format:?}"
            ));
            self.handle.report_error(error);
            return Err(FrameHandlerError);
        }

        // Windows Graphics Capture reports BGRA frames directly, so we keep that
        // layout and avoid per-frame validation in the hot callback.
        let snapshot = unsafe {
            Frame::new_unchecked(
                width,
                height,
                buffer.row_pitch(),
                buffer.as_raw_buffer().to_vec(),
            )
        };

        self.handle.present(snapshot);
        Ok(())
    }

    /// Stops the handle when the monitor stream closes.
    fn on_closed(&mut self) -> Result<(), Self::Error> {
        self.handle.stop();
        Ok(())
    }
}

/// Builds the Windows crate settings from the public monitor capture options.
fn os_crate_config(
    monitor: BackendMonitor,
    options: MonitorCaptureOptions,
    handle: CastHandle<MonitorCaptureSource>,
) -> Result<Settings<CastHandle<MonitorCaptureSource>, Monitor>, MonitorCaptureError> {
    let monitor = raw_monitor(monitor)?;

    Ok(Settings::new(
        monitor,
        cursor_capture(options),
        DrawBorderSettings::Default,
        SecondaryWindowSettings::Default,
        MinimumUpdateIntervalSettings::Custom(std::time::Duration::from_secs_f64(
            1.0 / options.fps_cap.get() as f64,
        )),
        DirtyRegionSettings::ReportAndRender,
        ColorFormat::Bgra8,
        handle,
    ))
}

/// Converts the source options into the backend cursor setting.
fn cursor_capture(options: MonitorCaptureOptions) -> CursorCaptureSettings {
    if options.shows_cursor {
        CursorCaptureSettings::WithCursor
    } else {
        CursorCaptureSettings::WithoutCursor
    }
}

/// Resolves one persisted monitor target into a Windows monitor.
fn raw_monitor(monitor: BackendMonitor) -> Result<Monitor, MonitorCaptureError> {
    Monitor::from_index(monitor.id().get() as usize).map_err(|error| {
        MonitorCaptureError::monitor_unavailable(format!(
            "monitor:{} is no longer available: {error}",
            monitor.id()
        ))
    })
}

/// Converts the backend crate's startup error into the crate error surface.
fn capture_api_error(error: GraphicsCaptureApiError<FrameHandlerError>) -> MonitorCaptureError {
    MonitorCaptureError::start_failed(format!(
        "Windows Graphics Capture could not start: {error:?}"
    ))
}

/// Builds one backend monitor target from one Windows monitor.
fn monitor_from_backend_monitor(monitor: Monitor) -> Result<BackendMonitor, MonitorCaptureError> {
    let index = monitor.index().map_err(source_unavailable)?;
    let id = NonZeroU32::new(u32::try_from(index).map_err(|_| {
        MonitorCaptureError::source_unavailable(
            "Windows Graphics Capture returned an invalid monitor index",
        )
    })?)
    .ok_or_else(|| {
        MonitorCaptureError::source_unavailable(
            "Windows Graphics Capture returned a zero monitor index",
        )
    })?;

    Ok(BackendMonitor::new(id))
}

/// Converts one Windows source-listing failure into the crate error surface.
fn source_unavailable(error: impl std::fmt::Display) -> MonitorCaptureError {
    MonitorCaptureError::source_unavailable(format!(
        "Windows Graphics Capture could not enumerate monitors: {error}"
    ))
}
