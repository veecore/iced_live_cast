//! Backend workers for monitor capture.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

use crate::handle::CastHandle;
use crate::source::monitor::{MonitorCaptureError, MonitorCaptureOptions, MonitorCaptureSource};
use std::num::NonZeroU32;

/// Backend-native monitor identifier persisted across app runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct BackendMonitor {
    /// Persisted host monitor identifier.
    id: NonZeroU32,
}

impl BackendMonitor {
    /// Builds one backend monitor from a persisted monitor identifier.
    pub(crate) const fn new(id: NonZeroU32) -> Self {
        Self { id }
    }

    /// Returns the persisted host monitor identifier.
    pub(crate) const fn id(self) -> NonZeroU32 {
        self.id
    }
}

/// Running backend worker owned by one monitor capture.
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

/// Enumerates every currently available monitor target.
pub(crate) fn all() -> Result<Vec<BackendMonitor>, MonitorCaptureError> {
    #[cfg(target_os = "macos")]
    {
        return macos::all();
    }

    #[cfg(target_os = "windows")]
    {
        return windows::all();
    }

    #[allow(unreachable_code)]
    Err(MonitorCaptureError::unsupported_platform())
}

/// Spawns the platform-native worker for the selected monitor.
pub(crate) fn spawn(
    handle: CastHandle<MonitorCaptureSource>,
    monitor: BackendMonitor,
    options: MonitorCaptureOptions,
) -> Result<WorkerHandle, MonitorCaptureError> {
    #[cfg(target_os = "macos")]
    {
        return macos::spawn(handle, monitor, options).map(WorkerHandle::MacOS);
    }

    #[cfg(target_os = "windows")]
    {
        return windows::spawn(handle, monitor, options).map(WorkerHandle::Windows);
    }

    #[allow(unreachable_code)]
    Err(MonitorCaptureError::unsupported_platform())
}
