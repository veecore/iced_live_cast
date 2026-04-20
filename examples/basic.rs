//! Minimal `iced` app showing the intended `iced_live_cast` flow.

use iced::widget::{button, column, container, row, text};
use iced::{Element, Length, Task, Theme};
use iced_live_cast::{
    CastView, Display, DisplayCapture, DisplayCaptureOptions, DisplayCaptureRuntimeError,
};
use std::env;
use std::num::NonZeroU32;

/// Runs the example application.
fn main() -> iced::Result {
    iced::application(BasicExample::boot, BasicExample::update, BasicExample::view)
        .theme(app_theme)
        .run()
}

/// Small application state for the live-capture example.
#[derive(Debug)]
struct BasicExample {
    /// Display the example should try to capture.
    source: Display,
    /// Active live session when one has been started.
    capture: Option<DisplayCapture>,
    /// Last synchronous startup error from `DisplayCapture::start`.
    start_error: Option<String>,
    /// Last runtime error published by the active cast view.
    runtime_error: Option<DisplayCaptureRuntimeError>,
}

/// User actions supported by the example app.
#[derive(Debug, Clone)]
enum Message {
    /// Starts one capture session for the selected display.
    StartPressed,
    /// Pauses or resumes the active session.
    TogglePlaybackPressed,
    /// Stops and drops the active session.
    StopPressed,
    /// Stores one newly reported runtime error from the active cast view.
    RuntimeErrorReported(DisplayCaptureRuntimeError),
}

impl BasicExample {
    /// Builds the initial state for the example app.
    fn boot() -> (Self, Task<Message>) {
        (
            Self {
                source: selected_display(),
                capture: None,
                start_error: None,
                runtime_error: None,
            },
            Task::none(),
        )
    }

    /// Handles button presses for the small example surface.
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::StartPressed => match DisplayCaptureOptions::default()
                .with_self_exclusion(true)
                .start(self.source)
            {
                Ok(capture) => {
                    self.capture = Some(capture);
                    self.start_error = None;
                    self.runtime_error = None;
                }
                Err(error) => {
                    self.capture = None;
                    self.start_error = Some(error.to_string());
                    self.runtime_error = None;
                }
            },
            Message::TogglePlaybackPressed => {
                if let Some(capture) = &self.capture {
                    if capture.is_paused() {
                        capture.resume();
                    } else {
                        capture.pause();
                    }
                }
            }
            Message::StopPressed => {
                if let Some(capture) = &self.capture {
                    capture.stop();
                }
                self.capture = None;
                self.runtime_error = None;
            }
            Message::RuntimeErrorReported(error) => {
                self.runtime_error = Some(error);
            }
        }

        Task::none()
    }

    /// Renders the example app and one live preview area.
    fn view(&self) -> Element<'_, Message> {
        let start = button(text("Start")).on_press(Message::StartPressed);

        let pause_label = if self
            .capture
            .as_ref()
            .is_some_and(|capture| capture.is_paused())
        {
            "Resume"
        } else {
            "Pause"
        };

        let pause = if self.capture.is_some() {
            button(text(pause_label)).on_press(Message::TogglePlaybackPressed)
        } else {
            button(text(pause_label))
        };

        let stop = if self.capture.is_some() {
            button(text("Stop")).on_press(Message::StopPressed)
        } else {
            button(text("Stop"))
        };

        let controls = row![start, pause, stop].spacing(12);
        let status = text(status_line(self));
        let source = text(self.source.to_string());

        let preview: Element<'_, _> = if let Some(capture) = &self.capture {
            container(
                CastView::new(capture)
                    .on_error(|error| Some(Message::RuntimeErrorReported(error.clone())))
                    .width(Length::Fill)
                    .height(420),
            )
            .width(Length::Fill)
            .height(420)
            .into()
        } else {
            container(text("Press Start to begin live capture."))
                .width(Length::Fill)
                .height(420)
                .center_x(Length::Fill)
                .center_y(420)
                .into()
        };

        container(
            column![
                text("iced_live_cast basic example").size(30),
                source,
                status,
                controls,
                preview
            ]
            .spacing(16)
            .max_width(920),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .padding(24)
        .into()
    }
}

/// Returns the theme used by the example app.
fn app_theme(_: &BasicExample) -> Theme {
    Theme::TokyoNight
}

/// Chooses the display source from the first CLI argument or falls back to `1`.
fn selected_display() -> Display {
    env::args()
        .nth(1)
        .and_then(|value| value.parse::<u32>().ok())
        .and_then(NonZeroU32::new)
        .map(Display::new)
        .unwrap_or_else(|| Display::new(NonZeroU32::new(1).expect("1 is non-zero")))
}

/// Formats the current status line shown above the preview.
fn status_line(app: &BasicExample) -> String {
    if let Some(error) = &app.start_error {
        return format!("Start failed: {error}");
    }

    let Some(capture) = &app.capture else {
        return String::from("Idle");
    };

    if let Some(error) = &app.runtime_error {
        return format!("Streaming error: {error}");
    }

    if capture.is_stopped() {
        return String::from("Stopped");
    }

    if capture.is_paused() {
        return format!("Paused for {}", capture.display());
    }

    format!("Streaming for {}", capture.display())
}
