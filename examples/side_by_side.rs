//! Side-by-side renderer probe for comparing `iced::Image` and [`CastView`].

use iced::widget::{button, column, container, image, row, text};
use iced::{ContentFit, Element, Length, Task, Theme};
use iced_live_cast::{CastHandle, CastView, Frame};

/// Logical width used by the shared probe frame.
const FRAME_WIDTH: u32 = 512;
/// Logical height used by the shared probe frame.
const FRAME_HEIGHT: u32 = 288;

/// Runs the side-by-side renderer probe.
fn main() -> iced::Result {
    iced::application(ProbeApp::boot, ProbeApp::update, ProbeApp::view)
        .theme(app_theme)
        .run()
}

/// Small application state for the side-by-side comparison.
#[derive(Debug)]
struct ProbeApp {
    /// Shared live-cast handle rendered by the custom path.
    handle: CastHandle,
    /// Same frame rendered through `iced::Image`.
    reference: iced::widget::image::Handle,
    /// Sequence number used to build alternate probe frames.
    variant: u32,
}

/// User actions supported by the renderer probe.
#[derive(Debug, Clone, Copy)]
enum Message {
    /// Rebuilds the probe with a slightly different synthetic frame.
    NextPatternPressed,
}

impl ProbeApp {
    /// Builds the initial probe state with one shared frame.
    fn boot() -> (Self, Task<Message>) {
        let handle = CastHandle::new();
        let frame = probe_frame(0);
        let reference = frame.to_handle();
        handle.present(frame);

        (
            Self {
                handle,
                reference,
                variant: 0,
            },
            Task::none(),
        )
    }

    /// Rebuilds the shared frame when the user asks for a fresh pattern.
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::NextPatternPressed => {
                self.variant = self.variant.wrapping_add(1);

                let frame = probe_frame(self.variant);
                self.reference = frame.to_handle();
                self.handle.present(frame);
            }
        }

        Task::none()
    }

    /// Renders the side-by-side comparison.
    fn view(&self) -> Element<'_, Message> {
        let left = column![
            text("iced::Image").size(24),
            text("Reference path"),
            container(
                image(self.reference.clone())
                    .width(FRAME_WIDTH)
                    .height(FRAME_HEIGHT)
                    .content_fit(ContentFit::Contain),
            )
            .width(FRAME_WIDTH)
            .height(FRAME_HEIGHT),
        ]
        .spacing(12)
        .width(Length::Shrink);

        let right = column![
            text("CastView").size(24),
            text("Custom live renderer"),
            container(
                CastView::new(&self.handle)
                    .width(FRAME_WIDTH)
                    .height(FRAME_HEIGHT)
            )
            .width(FRAME_WIDTH)
            .height(FRAME_HEIGHT),
        ]
        .spacing(12)
        .width(Length::Shrink);

        container(
            column![
                text("iced_live_cast side-by-side probe").size(30),
                text(
                    "Both panes are rendering the same pixels. If they diverge, the custom renderer is the culprit."
                ),
                button(text("Next pattern")).on_press(Message::NextPatternPressed),
                row![left, right].spacing(24),
            ]
            .spacing(20)
            .max_width(1200),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .padding(24)
        .into()
    }
}

/// Returns the theme used by the renderer probe.
fn app_theme(_: &ProbeApp) -> Theme {
    Theme::TokyoNight
}

/// Builds one synthetic RGBA frame with strong colors and neutral ramps.
fn probe_frame(variant: u32) -> Frame {
    let mut pixels = Vec::with_capacity((FRAME_WIDTH * FRAME_HEIGHT * 4) as usize);

    for y in 0..FRAME_HEIGHT {
        for x in 0..FRAME_WIDTH {
            let pixel = if y < FRAME_HEIGHT / 2 {
                color_bar_pixel(x, variant)
            } else {
                grayscale_pixel(x, y, variant)
            };

            pixels.extend_from_slice(&pixel);
        }
    }

    Frame::from_rgba_owned(FRAME_WIDTH, FRAME_HEIGHT, pixels)
        .expect("probe frame should always validate")
}

/// Returns one RGBA pixel for the top half of the probe image.
fn color_bar_pixel(x: u32, variant: u32) -> [u8; 4] {
    let band = ((x + variant * 13) * 8 / FRAME_WIDTH).min(7);

    let rgb = match band {
        0 => [255, 255, 255],
        1 => [255, 0, 0],
        2 => [0, 255, 0],
        3 => [0, 0, 255],
        4 => [255, 255, 0],
        5 => [0, 255, 255],
        6 => [255, 0, 255],
        _ => [0, 0, 0],
    };

    [rgb[0], rgb[1], rgb[2], u8::MAX]
}

/// Returns one RGBA pixel for the lower grayscale and contrast area.
fn grayscale_pixel(x: u32, y: u32, variant: u32) -> [u8; 4] {
    let ramp = ((x + variant * 5) * 255 / FRAME_WIDTH.saturating_sub(1).max(1)) as u8;
    let stripe = if ((y / 12) + variant).is_multiple_of(2) {
        ramp
    } else {
        ramp.saturating_sub(24)
    };

    [stripe, stripe, stripe, u8::MAX]
}
