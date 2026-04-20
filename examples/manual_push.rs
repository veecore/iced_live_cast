//! Minimal `iced` app showing manual frame pushing through [`CastHandle`].

use iced::widget::{button, column, container, row, text};
use iced::{Element, Length, Task, Theme};
use iced_live_cast::{CastHandle, CastView, Frame};
use std::thread;
use std::time::Duration;

/// Runs the manual-push example application.
fn main() -> iced::Result {
    iced::application(
        ManualExample::boot,
        ManualExample::update,
        ManualExample::view,
    )
    .theme(app_theme)
    .run()
}

/// Small application state for the manual frame-source example.
#[derive(Debug)]
struct ManualExample {
    /// Shared handle rendered by the active cast view.
    handle: CastHandle,
}

/// User actions supported by the manual example.
#[derive(Debug, Clone, Copy)]
enum Message {
    /// Pauses or resumes the manual source.
    TogglePlaybackPressed,
    /// Stops the source permanently.
    StopPressed,
}

impl ManualExample {
    /// Builds the initial state and starts one synthetic frame producer.
    fn boot() -> (Self, Task<Message>) {
        let handle = CastHandle::new();
        spawn_source(handle.clone());

        (Self { handle }, Task::none())
    }

    /// Handles the pause and stop controls for the manual source.
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::TogglePlaybackPressed => {
                if self.handle.is_paused() {
                    self.handle.resume();
                } else {
                    self.handle.pause();
                }
            }
            Message::StopPressed => self.handle.stop(),
        }

        Task::none()
    }

    /// Renders the manual source controls and the live cast view.
    fn view(&self) -> Element<'_, Message> {
        let pause_label = if self.handle.is_paused() {
            "Resume"
        } else {
            "Pause"
        };
        let pause = if self.handle.is_stopped() {
            button(text(pause_label))
        } else {
            button(text(pause_label)).on_press(Message::TogglePlaybackPressed)
        };
        let stop = if self.handle.is_stopped() {
            button(text("Stopped"))
        } else {
            button(text("Stop")).on_press(Message::StopPressed)
        };

        container(
            column![
                text("iced_live_cast manual push example").size(30),
                text(status_line(&self.handle)),
                row![pause, stop].spacing(12),
                container(CastView::new(&self.handle).width(Length::Fill).height(420))
                    .width(Length::Fill)
                    .height(420),
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

/// Returns the theme used by the manual example.
fn app_theme(_: &ManualExample) -> Theme {
    Theme::TokyoNight
}

/// Starts one background producer that feeds synthetic frames into the handle.
fn spawn_source(handle: CastHandle) {
    thread::spawn(move || {
        let mut tick = 0u32;

        while !handle.is_stopped() {
            handle.present(animated_frame(tick));
            tick = tick.wrapping_add(1);
            thread::sleep(Duration::from_millis(33));
        }
    });
}

/// Returns a human-readable playback status for the manual source.
fn status_line(handle: &CastHandle) -> &'static str {
    if handle.is_stopped() {
        "Stopped"
    } else if handle.is_paused() {
        "Paused"
    } else {
        "Streaming synthetic frames from a manual producer thread"
    }
}

/// Builds one animated RGBA frame with moving gradients and stripes.
fn animated_frame(tick: u32) -> Frame {
    let width = 960u32;
    let height = 540u32;
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);

    for y in 0..height {
        for x in 0..width {
            let red = ((x + tick * 3) % 256) as u8;
            let green = ((y + tick * 2) % 256) as u8;
            let blue = (((x / 8) ^ (y / 8) ^ tick) % 256) as u8;

            pixels.extend_from_slice(&[red, green, blue, u8::MAX]);
        }
    }

    Frame::from_rgba_owned(width, height, pixels).expect("manual example frame should validate")
}
