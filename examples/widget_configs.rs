//! Small visual probe for `CastView` builder configuration.

use iced::widget::{column, container, scrollable, text};
use iced::{ContentFit, Element, Length, Task, Theme};
use iced_live_cast::{CastHandle, CastView, FilterMethod, Frame};

/// Probe frame width.
const FRAME_WIDTH: u32 = 640;
/// Probe frame height.
const FRAME_HEIGHT: u32 = 360;
/// Card width used to exaggerate fit behavior.
const FIT_CARD_WIDTH: f32 = 620.0;
/// Card height used to exaggerate fit behavior.
const FIT_CARD_HEIGHT: f32 = 300.0;
/// Card width used by the general examples.
const CARD_WIDTH: f32 = 620.0;
/// Card height used by the general examples.
const CARD_HEIGHT: f32 = 240.0;

/// Runs the widget-configuration example.
fn main() -> iced::Result {
    iced::application(
        ConfigExample::boot,
        ConfigExample::update,
        ConfigExample::view,
    )
    .theme(app_theme)
    .run()
}

/// Static application state for the widget-configuration probe.
#[derive(Debug)]
struct ConfigExample {
    /// Shared handle rendered by most config cards.
    primary_handle: CastHandle,
    /// Low-resolution handle used to make filtering differences obvious.
    filter_handle: CastHandle,
}

impl ConfigExample {
    /// Builds the static example state.
    fn boot() -> (Self, Task<()>) {
        let primary_handle = CastHandle::new();
        primary_handle.present(config_probe_frame());

        let filter_handle = CastHandle::new();
        filter_handle.present(filter_probe_frame());

        (
            Self {
                primary_handle,
                filter_handle,
            },
            Task::none(),
        )
    }

    /// Handles the no-op message surface for the static example.
    fn update(&mut self, _message: ()) -> Task<()> {
        Task::none()
    }

    /// Renders the set of widget-configuration cards.
    fn view(&self) -> Element<'_, ()> {
        let cards = column![
            config_card(
                "Contain",
                "16:9 source inside a taller framed card",
                FIT_CARD_WIDTH,
                FIT_CARD_HEIGHT,
                CastView::new(&self.primary_handle)
                    .width(FIT_CARD_WIDTH)
                    .height(FIT_CARD_HEIGHT)
                    .content_fit(ContentFit::Contain),
            ),
            config_card(
                "Cover",
                "Same card, but filled edge-to-edge",
                FIT_CARD_WIDTH,
                FIT_CARD_HEIGHT,
                CastView::new(&self.primary_handle)
                    .width(FIT_CARD_WIDTH)
                    .height(FIT_CARD_HEIGHT)
                    .content_fit(ContentFit::Cover),
            ),
            config_card(
                "Crop",
                "Center-focused crop in source pixels",
                CARD_WIDTH,
                CARD_HEIGHT,
                CastView::new(&self.primary_handle)
                    .width(CARD_WIDTH)
                    .height(CARD_HEIGHT)
                    .crop(iced::Rectangle {
                        x: 160,
                        y: 60,
                        width: 320,
                        height: 240,
                    }),
            ),
            config_card(
                "Rotate + Radius",
                "Rotation with rounded edges",
                CARD_WIDTH,
                CARD_HEIGHT,
                CastView::new(&self.primary_handle)
                    .width(CARD_WIDTH)
                    .height(CARD_HEIGHT)
                    .rotation(0.14f32)
                    .border_radius(24),
            ),
            config_card(
                "Linear",
                "Low-res source enlarged with linear filtering",
                CARD_WIDTH,
                CARD_HEIGHT,
                CastView::new(&self.filter_handle)
                    .width(CARD_WIDTH)
                    .height(CARD_HEIGHT),
            ),
            config_card(
                "Nearest",
                "Same low-res source with nearest-neighbor filtering",
                CARD_WIDTH,
                CARD_HEIGHT,
                CastView::new(&self.filter_handle)
                    .width(CARD_WIDTH)
                    .height(CARD_HEIGHT)
                    .filter_method(FilterMethod::Nearest),
            ),
            config_card(
                "Opacity + Scale",
                "Softer presentation with slight zoom",
                CARD_WIDTH,
                CARD_HEIGHT,
                CastView::new(&self.primary_handle)
                    .width(CARD_WIDTH)
                    .height(CARD_HEIGHT)
                    .opacity(0.82)
                    .scale(1.08),
            ),
        ]
        .spacing(20);

        let content = column![
            text("iced_live_cast widget configs").size(30),
            text("One config per row, with a tiny low-res source for the filtering checks."),
            cards,
        ]
        .spacing(20)
        .max_width(760);

        scrollable(
            column![container(content)
                .width(Length::Fill)
                .center_x(Length::Fill),]
            .padding(24),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }
}

/// Builds one labeled configuration card.
fn config_card<'a>(
    title: &'a str,
    description: &'a str,
    width: f32,
    height: f32,
    view: CastView<'a, (), iced_live_cast::ManualSource, Theme>,
) -> Element<'a, ()> {
    container(
        column![
            text(title).size(22),
            text(description),
            preview_surface(width, height, view),
        ]
        .spacing(10),
    )
    .width(Length::Fill)
    .into()
}

/// Wraps one preview in a visible frame so fit behavior is easy to see.
fn preview_surface(
    width: f32,
    height: f32,
    view: CastView<(), iced_live_cast::ManualSource, Theme>,
) -> Element<()> {
    container(view)
        .width(width)
        .height(height)
        .style(container::rounded_box)
        .into()
}

/// Returns the theme used by the config example.
fn app_theme(_: &ConfigExample) -> Theme {
    Theme::TokyoNight
}

/// Builds one synthetic frame with structure that makes fit, crop, and
/// filtering differences easy to notice.
fn config_probe_frame() -> Frame {
    let mut pixels = Vec::with_capacity((FRAME_WIDTH * FRAME_HEIGHT * 4) as usize);

    for y in 0..FRAME_HEIGHT {
        for x in 0..FRAME_WIDTH {
            let pixel = if y < FRAME_HEIGHT / 2 {
                top_half_pixel(x, y)
            } else {
                bottom_half_pixel(x, y)
            };

            pixels.extend_from_slice(&pixel);
        }
    }

    Frame::from_rgba_owned(FRAME_WIDTH, FRAME_HEIGHT, pixels)
        .expect("widget-config probe frame should validate")
}

/// Builds one tiny frame with crisp pixel structure for the filtering cards.
fn filter_probe_frame() -> Frame {
    let width = 56u32;
    let height = 32u32;
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);

    for y in 0..height {
        for x in 0..width {
            let pixel = if x % 8 < 4 {
                if y % 8 < 4 {
                    [255, 255, 255, u8::MAX]
                } else {
                    [32, 32, 32, u8::MAX]
                }
            } else if y % 8 < 4 {
                [255, 72, 64, u8::MAX]
            } else {
                [64, 96, 255, u8::MAX]
            };

            pixels.extend_from_slice(&pixel);
        }
    }

    Frame::from_rgba_owned(width, height, pixels).expect("filter probe frame should validate")
}

/// Returns one RGBA pixel for the top half of the config probe.
fn top_half_pixel(x: u32, y: u32) -> [u8; 4] {
    let band = (x * 6 / FRAME_WIDTH).min(5);
    let rgb: [u8; 3] = match band {
        0 => [255, 255, 255],
        1 => [255, 64, 48],
        2 => [88, 255, 72],
        3 => [72, 120, 255],
        4 => [255, 236, 72],
        _ => [216, 72, 255],
    };

    let stripe: u8 = if ((x / 12) + (y / 12)).is_multiple_of(2) {
        0
    } else {
        18
    };

    [
        rgb[0].saturating_sub(stripe),
        rgb[1].saturating_sub(stripe),
        rgb[2].saturating_sub(stripe),
        u8::MAX,
    ]
}

/// Returns one RGBA pixel for the bottom half of the config probe.
fn bottom_half_pixel(x: u32, y: u32) -> [u8; 4] {
    let ramp = (x * 255 / FRAME_WIDTH.saturating_sub(1).max(1)) as u8;
    let stripe = if ((y / 16) + (x / 32)).is_multiple_of(2) {
        ramp
    } else {
        ramp.saturating_sub(28)
    };

    [stripe, stripe, stripe, u8::MAX]
}
