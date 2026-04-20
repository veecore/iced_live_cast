//! Compares `iced::Image` and `CastView` side by side for the same pixels.

use iced::widget::{button, column, container, row, text, Image};
use iced::{ContentFit, Element, Length, Task, Theme};
use iced_live_cast::{CastHandle, CastView, FitMode, Frame};
use image::DynamicImage;
use std::env;
use std::path::{Path, PathBuf};

/// Runs the still-image renderer probe.
fn main() -> iced::Result {
    iced::application(StillExample::boot, StillExample::update, StillExample::view)
        .theme(app_theme)
        .run()
}

/// Small application state for the renderer probe.
#[derive(Debug)]
struct StillExample {
    /// Frame used by both rendering paths.
    frame: Option<Frame>,
    /// Cast handle used by the active cast-view path.
    handle: Option<CastHandle>,
    /// Human-readable status for the current probe input.
    status: String,
    /// Human-readable description of the image source.
    source_label: String,
}

/// User actions supported by the renderer probe.
#[derive(Debug, Clone, Copy)]
enum Message {
    /// Pauses or resumes the capture-view path.
    TogglePlaybackPressed,
}

impl StillExample {
    /// Builds the initial renderer-probe state.
    fn boot() -> (Self, Task<Message>) {
        let (frame, source_label, status) = match selected_input() {
            Input::File(path) => match load_frame(&path) {
                Ok(frame) => (
                    Some(frame),
                    path.display().to_string(),
                    format!("Loaded {}", path.display()),
                ),
                Err(error) => (
                    None,
                    path.display().to_string(),
                    format!("Load failed: {error}"),
                ),
            },
            Input::Synthetic => {
                let frame = synthetic_frame();
                (
                    Some(frame),
                    String::from("Synthetic opaque test card"),
                    String::from(
                        "Synthetic card loaded. If Image looks right and CastView looks dark, the bug is in our cast-view path.",
                    ),
                )
            }
        };

        let handle = frame.as_ref().map(|frame| {
            let handle = CastHandle::new();
            handle.present(frame.clone());
            handle
        });

        (
            Self {
                frame,
                handle,
                status,
                source_label,
            },
            Task::none(),
        )
    }

    /// Handles pause and resume for the capture-view path.
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::TogglePlaybackPressed => {
                if let Some(handle) = &self.handle {
                    if handle.is_paused() {
                        handle.resume();
                    } else {
                        handle.pause();
                    }
                }
            }
        }

        Task::none()
    }

    /// Renders both still-image paths side by side.
    fn view(&self) -> Element<'_, Message> {
        let pause_label = if self
            .handle
            .as_ref()
            .is_some_and(|handle| handle.is_paused())
        {
            "Resume CastView"
        } else {
            "Pause CastView"
        };

        let pause = if self.handle.is_some() {
            button(text(pause_label)).on_press(Message::TogglePlaybackPressed)
        } else {
            button(text(pause_label))
        };

        let comparison = row![
            preview_panel("iced::Image", self.iced_image_preview()),
            preview_panel("CastView", self.capture_view_preview()),
        ]
        .spacing(24)
        .width(Length::Fill);

        container(
            column![
                text("iced_live_cast renderer probe").size(30),
                text(&self.source_label),
                text(&self.status),
                row![pause].spacing(12),
                comparison,
            ]
            .spacing(16)
            .max_width(1280),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .padding(24)
        .into()
    }

    /// Renders the stock `iced::Image` path for the current frame.
    fn iced_image_preview(&self) -> Element<'_, Message> {
        let Some(frame) = &self.frame else {
            return empty_preview("No frame loaded.");
        };

        Image::new(frame.to_handle())
            .width(Length::Fill)
            .height(Length::Fill)
            .content_fit(ContentFit::Contain)
            .into()
    }

    /// Renders the active `CastView` path for the current frame.
    fn capture_view_preview(&self) -> Element<'_, Message> {
        let Some(handle) = &self.handle else {
            return empty_preview("No frame loaded.");
        };

        CastView::new(handle)
            .width(Length::Fill)
            .height(Length::Fill)
            .fit_mode(FitMode::Contain)
            .into()
    }
}

/// Identifies which still-image source should be used by the probe.
enum Input {
    /// Load one still image from disk.
    File(PathBuf),
    /// Use the built-in synthetic test card.
    Synthetic,
}

/// Returns the theme used by the still-image example.
fn app_theme(_: &StillExample) -> Theme {
    Theme::TokyoNight
}

/// Builds one labeled preview panel.
fn preview_panel<'a>(title: &'a str, body: Element<'a, Message>) -> Element<'a, Message> {
    container(column![text(title).size(20), container(body).height(480)].spacing(12))
        .width(Length::FillPortion(1))
        .padding(16)
        .into()
}

/// Builds an empty preview placeholder.
fn empty_preview<'a>(message: &'a str) -> Element<'a, Message> {
    container(text(message))
        .width(Length::Fill)
        .height(480)
        .center_x(Length::Fill)
        .center_y(480)
        .into()
}

/// Loads one image file and converts it into a frame with opaque RGBA pixels.
fn load_frame(path: &Path) -> Result<Frame, Box<dyn std::error::Error>> {
    let image = image::open(path)?;
    let rgba = as_rgba8(image);
    let width = rgba.width();
    let height = rgba.height();
    let pixels = rgba.into_raw();

    Ok(Frame::from_rgba_owned(width, height, pixels)?)
}

/// Converts one dynamic image into tightly packed RGBA pixels.
fn as_rgba8(image: DynamicImage) -> image::RgbaImage {
    image.to_rgba8()
}

/// Chooses the probe input from the first CLI argument or falls back to a synthetic card.
fn selected_input() -> Input {
    env::args()
        .nth(1)
        .map(PathBuf::from)
        .map(Input::File)
        .unwrap_or(Input::Synthetic)
}

/// Builds one synthetic opaque frame with obvious colors and gradients.
fn synthetic_frame() -> Frame {
    let width = 1200u32;
    let height = 720u32;
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);

    for y in 0..height {
        for x in 0..width {
            let pixel = if y < height / 2 {
                match (x * 6) / width {
                    0 => [255, 255, 255, 255],
                    1 => [255, 0, 0, 255],
                    2 => [0, 255, 0, 255],
                    3 => [0, 0, 255, 255],
                    4 => [255, 255, 0, 255],
                    _ => [0, 0, 0, 255],
                }
            } else {
                let value = ((x as f32 / (width - 1) as f32) * 255.0).round() as u8;
                [value, value, value, 255]
            };

            pixels.extend_from_slice(&pixel);
        }
    }

    Frame::from_rgba_owned(width, height, pixels).expect("synthetic frame should validate")
}
