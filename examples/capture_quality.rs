//! Saves one captured frame to disk so source quality can be inspected
//! outside the live renderer.

use iced_live_cast::{Display, DisplayCapture, DisplayCaptureOptions, Frame};
use image::{ImageBuffer, Rgba, RgbaImage};
use std::env;
use std::fs;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

/// Waits for one captured frame, writes PNG variants, and prints alpha stats.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let capture = DisplayCapture::start_with_options(
        args.display,
        DisplayCaptureOptions::default().with_self_exclusion(args.exclude_self),
    )?;

    let frame = wait_for_frame(&capture, args.timeout)?;
    capture.stop();

    fs::create_dir_all(&args.output_dir)?;

    let exact_rgba = frame.rgba_pixels();
    let opaque_rgba = force_opaque(&exact_rgba);

    save_rgba_png(
        args.output_dir.join("capture-exact.png"),
        frame.width(),
        frame.height(),
        exact_rgba.clone(),
    )?;
    save_rgba_png(
        args.output_dir.join("capture-opaque.png"),
        frame.width(),
        frame.height(),
        opaque_rgba,
    )?;

    let stats = alpha_stats(&exact_rgba);

    println!(
        "captured {}x{} frame from {}",
        frame.width(),
        frame.height(),
        args.display
    );
    println!(
        "saved {}",
        args.output_dir.join("capture-exact.png").display()
    );
    println!(
        "saved {}",
        args.output_dir.join("capture-opaque.png").display()
    );
    println!(
        "alpha: min={}, max={}, translucent_pixels={}, transparent_pixels={}",
        stats.min_alpha, stats.max_alpha, stats.translucent_pixels, stats.transparent_pixels
    );
    println!("If exact looks bad but opaque looks normal, alpha/blending is the culprit.");

    Ok(())
}

/// Command-line arguments for the capture-quality smoke test.
struct Args {
    /// Display to capture.
    display: Display,
    /// Whether the running app should be excluded when supported.
    exclude_self: bool,
    /// Maximum time to wait for the first frame.
    timeout: Duration,
    /// Directory where PNG smoke-test outputs are written.
    output_dir: PathBuf,
}

impl Args {
    /// Parses the smoke-test arguments from the current process arguments.
    fn parse() -> Self {
        let mut args = env::args().skip(1);
        let display = args
            .next()
            .and_then(|value| value.parse::<u32>().ok())
            .and_then(NonZeroU32::new)
            .map(Display::new)
            .unwrap_or_else(default_display);
        let output_dir = args
            .next()
            .map(PathBuf::from)
            .unwrap_or_else(default_output_dir);
        let exclude_self = args
            .next()
            .map(|value| matches!(value.as_str(), "1" | "true" | "yes"))
            .unwrap_or(false);

        Self {
            display,
            exclude_self,
            timeout: Duration::from_secs(5),
            output_dir,
        }
    }
}

/// Summary of alpha values found in one RGBA frame.
struct AlphaStats {
    /// Minimum alpha channel value found in the frame.
    min_alpha: u8,
    /// Maximum alpha channel value found in the frame.
    max_alpha: u8,
    /// Number of pixels with alpha strictly between zero and fully opaque.
    translucent_pixels: usize,
    /// Number of pixels with zero alpha.
    transparent_pixels: usize,
}

/// Waits for the first available frame or returns a timeout error.
fn wait_for_frame(
    capture: &DisplayCapture,
    timeout: Duration,
) -> Result<Frame, Box<dyn std::error::Error>> {
    let started = Instant::now();

    loop {
        if let Some(frame) = capture.snapshot() {
            return Ok(frame);
        }

        if let Some(error) = capture.last_error() {
            return Err(error.into());
        }

        if started.elapsed() >= timeout {
            return Err(format!(
                "timed out waiting for the first frame from {}",
                capture.display()
            )
            .into());
        }

        thread::sleep(Duration::from_millis(16));
    }
}

/// Writes one packed RGBA image to disk as a PNG.
fn save_rgba_png(
    path: impl AsRef<Path>,
    width: u32,
    height: u32,
    pixels: Vec<u8>,
) -> Result<(), Box<dyn std::error::Error>> {
    let image: RgbaImage = ImageBuffer::<Rgba<u8>, _>::from_raw(width, height, pixels)
        .ok_or("RGBA buffer did not match the captured frame dimensions")?;

    image.save(path)?;
    Ok(())
}

/// Returns one RGBA buffer with all alpha values forced fully opaque.
fn force_opaque(rgba: &[u8]) -> Vec<u8> {
    let mut opaque = rgba.to_vec();

    for pixel in opaque.chunks_exact_mut(4) {
        pixel[3] = u8::MAX;
    }

    opaque
}

/// Calculates simple alpha statistics for one RGBA frame.
fn alpha_stats(rgba: &[u8]) -> AlphaStats {
    let mut min_alpha = u8::MAX;
    let mut max_alpha = u8::MIN;
    let mut translucent_pixels = 0;
    let mut transparent_pixels = 0;

    for pixel in rgba.chunks_exact(4) {
        let alpha = pixel[3];
        min_alpha = min_alpha.min(alpha);
        max_alpha = max_alpha.max(alpha);

        if alpha == 0 {
            transparent_pixels += 1;
        } else if alpha < u8::MAX {
            translucent_pixels += 1;
        }
    }

    AlphaStats {
        min_alpha,
        max_alpha,
        translucent_pixels,
        transparent_pixels,
    }
}

/// Returns the default display used by the smoke test.
fn default_display() -> Display {
    Display::new(NonZeroU32::new(1).expect("1 is non-zero"))
}

/// Returns the default output directory used by the smoke test.
fn default_output_dir() -> PathBuf {
    PathBuf::from("target/iced_live_cast/smoke")
}
