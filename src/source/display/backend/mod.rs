//! Backend workers for display capture.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

use crate::handle::CastHandle;
use crate::source::display::{
    Display, DisplayCaptureError, DisplayCaptureOptions, DisplayCaptureSource,
};

/// Running backend worker owned by one display capture.
pub(crate) enum WorkerHandle {
    /// Worker thread used by the macOS backend.
    #[cfg(target_os = "macos")]
    MacOS(macos::WorkerHandle),
    /// Windows Graphics Capture control handle.
    #[cfg(target_os = "windows")]
    Windows(windows::WorkerHandle),
}

impl std::fmt::Debug for WorkerHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            #[cfg(target_os = "macos")]
            Self::MacOS(_) => f.write_str("WorkerHandle::MacOS(..)"),
            #[cfg(target_os = "windows")]
            Self::Windows(_) => f.write_str("WorkerHandle::Windows(..)"),
        }
    }
}

impl WorkerHandle {
    /// Stops the worker and waits for it to wind down.
    pub(crate) fn stop(self) {
        match self {
            #[cfg(target_os = "macos")]
            Self::MacOS(handle) => handle.stop(),
            #[cfg(target_os = "windows")]
            Self::Windows(handle) => handle.stop(),
        }
    }
}

/// Spawns the platform-native worker for the selected display.
pub(crate) fn spawn(
    handle: CastHandle<DisplayCaptureSource>,
    display: Display,
    options: DisplayCaptureOptions,
) -> Result<WorkerHandle, DisplayCaptureError> {
    #[cfg(target_os = "macos")]
    {
        return macos::spawn(handle, display, options).map(WorkerHandle::MacOS);
    }

    #[cfg(target_os = "windows")]
    {
        return windows::spawn(handle, display, options).map(WorkerHandle::Windows);
    }

    #[allow(unreachable_code)]
    Err(DisplayCaptureError::unsupported_platform())
}
