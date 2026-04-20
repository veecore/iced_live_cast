//! Shared benchmark fixtures for `iced_live_cast`.
#![allow(dead_code)]

use iced_live_cast::{CastHandle, Frame};

/// One benchmark frame profile used to keep related measurements consistent.
#[derive(Clone, Copy)]
pub struct FrameProfile {
    /// Human-readable benchmark label.
    pub name: &'static str,
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
}

impl FrameProfile {
    /// Returns the tightly packed byte length for this profile.
    pub fn packed_len(self) -> usize {
        self.width as usize * self.height as usize * 4
    }
}

/// Common frame sizes we care about while iterating on the crate.
pub const FRAME_PROFILES: [FrameProfile; 2] = [
    FrameProfile {
        name: "720p",
        width: 1280,
        height: 720,
    },
    FrameProfile {
        name: "1080p",
        width: 1920,
        height: 1080,
    },
];

/// Builds deterministic RGBA pixels for one packed frame.
pub fn rgba_pixels(profile: FrameProfile) -> Vec<u8> {
    (0..profile.packed_len())
        .map(|index| (index % 251) as u8)
        .collect()
}

/// Builds deterministic BGRA pixels for one packed frame.
pub fn bgra_pixels(profile: FrameProfile) -> Vec<u8> {
    let mut rgba = rgba_pixels(profile);

    for chunk in rgba.chunks_exact_mut(4) {
        chunk.swap(0, 2);
    }

    rgba
}

/// Builds deterministic BGRA pixels with row padding to exercise the stride path.
pub fn padded_bgra_parts(profile: FrameProfile) -> (u32, Vec<u8>) {
    let row_len = profile.width as usize * 4;
    let padded_row_len = row_len + 64;
    let mut pixels = vec![0; padded_row_len * profile.height as usize];

    for (row_index, row) in pixels.chunks_exact_mut(padded_row_len).enumerate() {
        for (column, byte) in row[..row_len].iter_mut().enumerate() {
            *byte = ((row_index + column) % 251) as u8;
        }
    }

    (padded_row_len as u32, pixels)
}

/// Builds one padded BGRA frame.
pub fn padded_bgra_frame(profile: FrameProfile) -> Frame {
    let (bytes_per_row, pixels) = padded_bgra_parts(profile);

    Frame::new(profile.width, profile.height, bytes_per_row, pixels)
        .expect("padded benchmark frame should validate")
}

/// Builds one tightly packed BGRA frame.
pub fn packed_bgra_frame(profile: FrameProfile) -> Frame {
    Frame::from_bgra(profile.width, profile.height, bgra_pixels(profile))
        .expect("packed benchmark frame should validate")
}

/// Builds one tightly packed RGBA frame and lets the crate normalize it into BGRA.
pub fn packed_rgba_frame(profile: FrameProfile) -> Frame {
    Frame::from_rgba_owned(profile.width, profile.height, rgba_pixels(profile))
        .expect("packed benchmark frame should validate")
}

/// Builds one handle seeded with a packed BGRA frame.
pub fn seeded_handle(profile: FrameProfile) -> CastHandle {
    let handle: CastHandle = CastHandle::new();
    handle.present(packed_bgra_frame(profile));
    handle
}
