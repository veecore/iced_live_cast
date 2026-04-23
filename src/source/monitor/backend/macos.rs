//! ScreenCaptureKit backend for monitor capture.

use super::BackendMonitor;
use crate::frame::Frame;
use crate::handle::CastHandle;
use crate::source::monitor::{
    MonitorCaptureError, MonitorCaptureOptions, MonitorCaptureRuntimeError, MonitorCaptureSource,
};
use screencapturekit::cm::{CMSampleBuffer, CMTime};
use screencapturekit::dispatch_queue::{DispatchQoS, DispatchQueue};
use screencapturekit::error::SCError;
use screencapturekit::prelude::PixelFormat as CapturePixelFormat;
use screencapturekit::shareable_content::{SCDisplay, SCRunningApplication, SCShareableContent};
use screencapturekit::stream::configuration::SCStreamConfiguration;
use screencapturekit::stream::content_filter::SCContentFilter;
use screencapturekit::stream::output_trait::SCStreamOutputTrait;
use screencapturekit::stream::output_type::SCStreamOutputType;
use screencapturekit::stream::SCStream;
use screencapturekit::FourCharCode;
use std::num::NonZeroU32;

/// Running ScreenCaptureKit worker.
pub(crate) struct WorkerHandle {
    /// Active ScreenCaptureKit stream.
    stream: SCStream,
}

impl WorkerHandle {
    /// Stops the ScreenCaptureKit worker and waits for teardown.
    pub(crate) fn stop(self) {
        let _ = self.stream.stop_capture();
    }
}

/// Enumerates every currently available ScreenCaptureKit monitor target.
pub(crate) fn all() -> Result<Vec<BackendMonitor>, MonitorCaptureError> {
    SCShareableContent::get()
        .map_err(source_unavailable)?
        .displays()
        .into_iter()
        .map(monitor_from_sc_display)
        .collect()
}

/// Spawns one ScreenCaptureKit worker for the requested monitor.
pub(crate) fn spawn(
    handle: CastHandle<MonitorCaptureSource>,
    monitor: BackendMonitor,
    options: MonitorCaptureOptions,
) -> Result<WorkerHandle, MonitorCaptureError> {
    let os_crate_config = config(monitor, options)?;
    let stream = run(handle, os_crate_config).map_err(capture_api_error)?;

    Ok(WorkerHandle { stream })
}

/// ScreenCaptureKit objects needed by the worker thread.
struct OsCrateConfig {
    /// ScreenCaptureKit content filter.
    filter: SCContentFilter,
    /// ScreenCaptureKit stream configuration.
    stream: SCStreamConfiguration,
}

/// Output handler that copies frames into the shared capture handle.
struct FrameHandler {
    /// Shared handle state updated by the output callback.
    handle: CastHandle<MonitorCaptureSource>,
}

impl SCStreamOutputTrait for FrameHandler {
    fn did_output_sample_buffer(&self, sample_buffer: CMSampleBuffer, of_type: SCStreamOutputType) {
        if of_type != SCStreamOutputType::Screen
            || self.handle.is_paused()
            || self.handle.is_stopped()
        {
            return;
        }

        let Some(pixel_buffer) = sample_buffer.image_buffer() else {
            return;
        };

        let Ok(guard) = pixel_buffer.lock_read_only() else {
            self.handle
                .report_error(MonitorCaptureRuntimeError::frame_access(
                    "ScreenCaptureKit could not lock the frame buffer for read-only access",
                ));
            return;
        };

        let actual_format = FourCharCode::from_u32(guard.pixel_format());
        let expected_format = FourCharCode::from(CapturePixelFormat::BGRA);

        if actual_format != expected_format {
            self.handle
                .report_error(MonitorCaptureRuntimeError::unsupported_pixel_format(
                    format!(
                        "ScreenCaptureKit produced an unsupported pixel format: {}",
                        actual_format.display()
                    ),
                ));
            return;
        }

        // ScreenCaptureKit owns the mapped pixel buffer, so we copy the bytes once
        // for the shared handle and trust the OS-reported BGRA layout on this hot path.
        let frame = unsafe {
            Frame::new_unchecked(
                guard.width() as u32,
                guard.height() as u32,
                guard.bytes_per_row() as u32,
                guard.as_slice().to_vec(),
            )
        };

        self.handle.present(frame);
    }
}

/// Runs the ScreenCaptureKit stream until the handle stops.
fn run(
    handle: CastHandle<MonitorCaptureSource>,
    os_crate_config: OsCrateConfig,
) -> Result<SCStream, SCError> {
    let queue = DispatchQueue::new(
        "dev.iced-live-cast.monitor-stream",
        DispatchQoS::UserInteractive,
    );

    let mut stream = SCStream::new(&os_crate_config.filter, &os_crate_config.stream);
    stream.add_output_handler_with_queue(
        FrameHandler {
            handle: handle.clone(),
        },
        SCStreamOutputType::Screen,
        Some(&queue),
    );

    stream.start_capture()?;

    Ok(stream)
}

/// Builds the ScreenCaptureKit configuration from the selected monitor and options.
fn config(
    monitor: BackendMonitor,
    options: MonitorCaptureOptions,
) -> Result<OsCrateConfig, MonitorCaptureError> {
    let content = SCShareableContent::get().map_err(source_unavailable)?;
    let monitor_id = monitor.id().get();

    let selected_monitor = content
        .displays()
        .into_iter()
        .find(|candidate| candidate.display_id() == monitor_id)
        .ok_or_else(|| {
            MonitorCaptureError::monitor_unavailable(format!(
                "monitor:{} is no longer available",
                monitor.id()
            ))
        })?;

    let current_app = if options.excludes_self {
        Some(current_app(&content)?)
    } else {
        None
    };

    let filter = if let Some(current_app) = current_app.as_ref() {
        SCContentFilter::create()
            .with_display(&selected_monitor)
            .with_excluding_applications(&[current_app], &[])
            .build()
    } else {
        SCContentFilter::create()
            .with_display(&selected_monitor)
            .with_excluding_windows(&[])
            .build()
    };

    let stream = SCStreamConfiguration::new()
        .with_width(selected_monitor.width())
        .with_height(selected_monitor.height())
        .with_pixel_format(CapturePixelFormat::BGRA)
        .with_shows_cursor(options.shows_cursor)
        .with_shows_mouse_clicks(options.shows_click_indicators)
        .with_minimum_frame_interval(&CMTime::new(1, options.fps_cap.get() as i32));

    Ok(OsCrateConfig { filter, stream })
}

/// Returns the current app for self-exclusion when ScreenCaptureKit can resolve it.
fn current_app(content: &SCShareableContent) -> Result<SCRunningApplication, MonitorCaptureError> {
    let current_pid = std::process::id() as i32;

    content
        .applications()
        .into_iter()
        .find(|app| app.process_id() == current_pid)
        .ok_or_else(|| {
            MonitorCaptureError::self_exclusion_unavailable(
                "the current app could not be excluded from ScreenCaptureKit capture",
            )
        })
}

/// Converts the backend crate's startup error into the crate error surface.
fn capture_api_error(error: SCError) -> MonitorCaptureError {
    MonitorCaptureError::start_failed(format!("ScreenCaptureKit could not start capture: {error}"))
}

/// Builds one backend monitor target from one ScreenCaptureKit display.
fn monitor_from_sc_display(display: SCDisplay) -> Result<BackendMonitor, MonitorCaptureError> {
    NonZeroU32::new(display.display_id())
        .map(BackendMonitor::new)
        .ok_or_else(|| {
            MonitorCaptureError::source_unavailable("ScreenCaptureKit returned a zero display id")
        })
}

/// Converts one ScreenCaptureKit source-listing failure into the crate error surface.
fn source_unavailable(error: SCError) -> MonitorCaptureError {
    MonitorCaptureError::source_unavailable(format!(
        "ScreenCaptureKit could not list shareable content: {error}"
    ))
}
