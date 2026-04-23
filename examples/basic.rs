//! Minimal `iced` app showing the intended `iced_live_cast` flow.

use iced::widget::{button, column, container, row, text};
use iced::{Element, Length, Task, Theme};
use iced_live_cast::{
    CastView, Monitor, MonitorCapture, MonitorCaptureOptions, MonitorCaptureRuntimeError,
};
use std::env;

/// Runs the example application.
fn main() -> iced::Result {
    iced::application(BasicExample::boot, BasicExample::update, BasicExample::view)
        .theme(app_theme)
        .run()
}

/// Small application state for the live-capture example.
#[derive(Debug)]
struct BasicExample {
    /// Monitor the example should try to capture.
    source: Option<Monitor>,
    /// Active live session when one has been started.
    capture: Option<MonitorCapture>,
    /// Last synchronous startup error from `MonitorCapture::start`.
    start_error: Option<String>,
    /// Last runtime error published by the active cast view.
    runtime_error: Option<MonitorCaptureRuntimeError>,
}

/// User actions supported by the example app.
#[derive(Debug, Clone)]
enum Message {
    /// Starts one capture session for the selected monitor.
    StartPressed,
    /// Pauses or resumes the active session.
    TogglePlaybackPressed,
    /// Stops and drops the active session.
    StopPressed,
    /// Stores one newly reported runtime error from the active cast view.
    RuntimeErrorReported(MonitorCaptureRuntimeError),
}

impl BasicExample {
    /// Builds the initial state for the example app.
    fn boot() -> (Self, Task<Message>) {
        let (source, start_error) = match selected_monitor() {
            Ok(monitor) => (Some(monitor), None),
            Err(error) => (None, Some(error)),
        };

        (
            Self {
                source,
                capture: None,
                start_error,
                runtime_error: None,
            },
            Task::none(),
        )
    }

    /// Handles button presses for the small example surface.
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::StartPressed => {
                let Some(source) = self.source else {
                    return Task::none();
                };

                match MonitorCaptureOptions::default()
                    .with_self_exclusion(true)
                    .start(source)
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
                }
            }
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
        let source = text(source_line(self));

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

/// Chooses one real monitor from the CLI or falls back to the first monitor.
fn selected_monitor() -> Result<Monitor, String> {
    let monitors = Monitor::all().map_err(|error| error.to_string())?;

    if monitors.is_empty() {
        return Err(String::from("No monitors are currently available"));
    }

    let selected = if let Some(value) = env::args().nth(1) {
        if let Ok(monitor_id) = value.parse::<u32>() {
            monitors
                .iter()
                .find(|monitor| monitor.id().get() == monitor_id)
                .copied()
                .or_else(|| {
                    value.parse::<usize>().ok().and_then(|index| {
                        index
                            .checked_sub(1)
                            .and_then(|index| monitors.get(index))
                            .copied()
                    })
                })
                .ok_or_else(|| format!("No monitor matched `{value}`"))?
        } else {
            return Err(format!(
                "Could not parse `{value}` as a monitor id or index"
            ));
        }
    } else {
        monitors[0]
    };

    Ok(selected)
}

/// Formats the source line shown above the preview.
fn source_line(app: &BasicExample) -> String {
    match &app.source {
        Some(monitor) => format!("Selected {}", monitor),
        None => String::from("No monitor selected"),
    }
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
        return format!("Paused for {}", capture.monitor());
    }

    format!("Streaming for {}", capture.monitor())
}
