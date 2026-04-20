//! Shared frame types used by sources and the widget renderer.

use iced::widget::image::Handle as ImageHandle;
use std::sync::Arc;
use thiserror::Error;

/// Bytes per pixel used by every frame stored in this crate.
const BYTES_PER_PIXEL: u32 = 4;

/// One immutable BGRA frame ready for presentation.
#[derive(Debug, Clone)]
pub struct Frame {
    /// Width of the frame in pixels.
    width: u32,
    /// Height of the frame in pixels.
    height: u32,
    /// Distance in bytes between adjacent rows.
    bytes_per_row: u32,
    /// Shared BGRA bytes owned by the source or capture handle.
    pixels: Arc<[u8]>,
}

impl Frame {
    /// Builds one tightly packed RGBA frame and converts it into the crate's BGRA storage.
    pub fn from_rgba_owned(
        width: u32,
        height: u32,
        pixels: impl Into<Vec<u8>>,
    ) -> Result<Self, FrameError> {
        let mut pixels = pixels.into();

        for pixel in pixels.chunks_exact_mut(BYTES_PER_PIXEL as usize) {
            pixel.swap(0, 2);
        }

        Self::from_bgra(width, height, pixels)
    }

    /// Builds one tightly packed BGRA frame.
    pub fn from_bgra(
        width: u32,
        height: u32,
        pixels: impl Into<Arc<[u8]>>,
    ) -> Result<Self, FrameError> {
        let bytes_per_row = width
            .checked_mul(BYTES_PER_PIXEL)
            .ok_or_else(FrameError::dimensions_too_large)?;

        Self::new(width, height, bytes_per_row, pixels)
    }

    /// Builds one BGRA frame after validating its dimensions, stride, and byte length.
    pub fn new(
        width: u32,
        height: u32,
        bytes_per_row: u32,
        pixels: impl Into<Arc<[u8]>>,
    ) -> Result<Self, FrameError> {
        let pixels = pixels.into();
        let minimum_row_len = width
            .checked_mul(BYTES_PER_PIXEL)
            .ok_or_else(FrameError::dimensions_too_large)?;

        if bytes_per_row < minimum_row_len {
            return Err(FrameError::stride_too_small(bytes_per_row, minimum_row_len));
        }

        let required_len = (bytes_per_row as usize)
            .checked_mul(height as usize)
            .ok_or_else(FrameError::dimensions_too_large)?;

        if pixels.len() < required_len {
            return Err(FrameError::not_enough_pixels(pixels.len(), required_len));
        }

        Ok(Self {
            width,
            height,
            bytes_per_row,
            pixels,
        })
    }

    /// Builds one BGRA frame without validating its declared dimensions or stride.
    ///
    /// # Safety
    ///
    /// The caller must ensure:
    ///
    /// - `bytes_per_row >= width * 4`
    /// - `pixels.len() >= bytes_per_row * height`
    /// - the bytes are valid BGRA pixels for the declared frame geometry
    ///
    /// Violating these invariants can make later renderer uploads fail or read the
    /// wrong row layout.
    pub unsafe fn new_unchecked(
        width: u32,
        height: u32,
        bytes_per_row: u32,
        pixels: impl Into<Arc<[u8]>>,
    ) -> Self {
        Self {
            width,
            height,
            bytes_per_row,
            pixels: pixels.into(),
        }
    }

    /// Returns the frame width in pixels.
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Returns the frame height in pixels.
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Returns the distance in bytes between adjacent rows.
    pub const fn bytes_per_row(&self) -> u32 {
        self.bytes_per_row
    }

    /// Returns the frame dimensions as `(width, height)`.
    pub const fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Converts the frame into an `iced` image handle.
    pub fn to_handle(&self) -> ImageHandle {
        ImageHandle::from_rgba(self.width, self.height, self.rgba_pixels())
    }

    /// Returns one tightly packed RGBA buffer for interoperability.
    pub fn rgba_pixels(&self) -> Vec<u8> {
        let packed_row_len = self.width as usize * BYTES_PER_PIXEL as usize;
        let mut rgba = vec![0; packed_row_len * self.height as usize];

        for row in 0..self.height as usize {
            let source_row = &self.pixels[row * self.bytes_per_row as usize
                ..row * self.bytes_per_row as usize + packed_row_len];
            let target_row = &mut rgba[row * packed_row_len..(row + 1) * packed_row_len];

            for (target, source) in target_row
                .chunks_exact_mut(BYTES_PER_PIXEL as usize)
                .zip(source_row.chunks_exact(BYTES_PER_PIXEL as usize))
            {
                target[0] = source[2];
                target[1] = source[1];
                target[2] = source[0];
                target[3] = source[3];
            }
        }

        rgba
    }

    /// Returns the raw BGRA bytes stored by the frame.
    pub(crate) fn pixels(&self) -> &[u8] {
        self.pixels.as_ref()
    }
}

/// Failure returned when raw frame parts do not describe one valid BGRA image.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("{kind}")]
pub struct FrameError {
    /// Stable error classification used for inspection without exposing variants.
    kind: FrameErrorKind,
}

/// Private classification for one frame validation failure.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
enum FrameErrorKind {
    /// Width, height, or stride overflowed the required byte math.
    #[error("frame dimensions are too large to address safely")]
    DimensionsTooLarge,
    #[error(
        "frame stride {bytes_per_row} is smaller than the minimum row length {minimum_row_len}"
    )]
    /// One row cannot fit inside the declared stride.
    StrideTooSmall {
        bytes_per_row: u32,
        minimum_row_len: u32,
    },
    /// The supplied byte buffer is shorter than the described frame.
    #[error(
        "frame pixel buffer length {actual_len} is smaller than the required length {required_len}"
    )]
    NotEnoughPixels {
        actual_len: usize,
        required_len: usize,
    },
}

impl FrameError {
    /// Builds the overflow error.
    fn dimensions_too_large() -> Self {
        Self {
            kind: FrameErrorKind::DimensionsTooLarge,
        }
    }

    /// Builds the short-stride error.
    fn stride_too_small(bytes_per_row: u32, minimum_row_len: u32) -> Self {
        Self {
            kind: FrameErrorKind::StrideTooSmall {
                bytes_per_row,
                minimum_row_len,
            },
        }
    }

    /// Builds the short-buffer error.
    fn not_enough_pixels(actual_len: usize, required_len: usize) -> Self {
        Self {
            kind: FrameErrorKind::NotEnoughPixels {
                actual_len,
                required_len,
            },
        }
    }

    /// Returns whether width, height, or stride overflowed the required byte math.
    pub fn is_dimensions_too_large(&self) -> bool {
        matches!(self.kind, FrameErrorKind::DimensionsTooLarge)
    }

    /// Returns whether the declared stride is shorter than one pixel row.
    pub fn is_stride_too_small(&self) -> bool {
        matches!(self.kind, FrameErrorKind::StrideTooSmall { .. })
    }

    /// Returns whether the supplied byte buffer is shorter than the described frame.
    pub fn is_not_enough_pixels(&self) -> bool {
        matches!(self.kind, FrameErrorKind::NotEnoughPixels { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::{Frame, FrameError};

    /// Packed frames should infer their row stride automatically.
    #[test]
    fn packed_frame_uses_tight_stride() {
        let frame = Frame::from_bgra(4, 2, vec![0; 32]).expect("packed frame should validate");

        assert_eq!(frame.bytes_per_row(), 16);
    }

    /// Strides shorter than one full pixel row should fail clearly.
    #[test]
    fn short_stride_fails() {
        let error = Frame::new(4, 2, 8, vec![0; 16]).expect_err("short stride should be rejected");

        assert_eq!(error, FrameError::stride_too_small(8, 16));
    }

    /// Byte buffers shorter than the described frame should fail clearly.
    #[test]
    fn short_buffer_fails() {
        let error =
            Frame::new(4, 2, 16, vec![0; 31]).expect_err("short pixel buffer should be rejected");

        assert_eq!(error, FrameError::not_enough_pixels(31, 32));
    }

    /// BGRA frames should convert into RGBA bytes without changing alpha.
    #[test]
    fn bgra_converts_to_rgba() {
        let frame = Frame::from_bgra(1, 1, vec![10, 20, 30, 40]).expect("frame should validate");

        assert_eq!(frame.rgba_pixels(), vec![30, 20, 10, 40]);
    }

    /// RGBA frames should be normalized into BGRA storage internally.
    #[test]
    fn rgba_converts_to_bgra_storage() {
        let frame =
            Frame::from_rgba_owned(1, 1, vec![30, 20, 10, 40]).expect("frame should validate");

        assert_eq!(frame.pixels(), &[10, 20, 30, 40]);
    }
}
