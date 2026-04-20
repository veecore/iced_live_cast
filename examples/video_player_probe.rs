//! Displays one video file with `iced_video_player` to probe its render path.

use iced::widget::{button, column, container, row, text};
use iced::{ContentFit, Element, Length, Task, Theme};
use iced_video_player::{Video, VideoPlayer};
use std::env;
use std::path::{Path, PathBuf};
use url::Url;

/// Runs the `iced_video_player` probe.
fn main() -> iced::Result {
    configure_runtime_environment();

    iced::application(
        VideoPlayerProbe::boot,
        VideoPlayerProbe::update,
        VideoPlayerProbe::view,
    )
    .theme(app_theme)
    .run()
}

/// Small application state for the video-player probe.
#[derive(Debug)]
struct VideoPlayerProbe {
    /// Human-readable path or label for the current media source.
    source_label: String,
    /// Human-readable status shown above the player.
    status: String,
    /// Loaded video when opening succeeded.
    video: Option<Video>,
    /// Last known playback position in seconds.
    position_seconds: f64,
}

/// Messages emitted by the probe UI and player widget.
#[derive(Debug, Clone, Copy)]
enum Message {
    /// Toggles playback pause.
    TogglePausePressed,
    /// Toggles looping.
    ToggleLoopPressed,
    /// Updates the visible playback position after one fresh frame arrives.
    NewFrame,
    /// Notes that the file reached its end.
    EndOfStream,
}

impl VideoPlayerProbe {
    /// Builds the initial probe state from the first CLI argument.
    fn boot() -> (Self, Task<Message>) {
        let Some(path) = selected_input() else {
            return (
                Self {
                    source_label: String::from("No file provided"),
                    status: String::from(
                        "Pass a local video path. Example: cargo run --example video_player_probe -- /path/to/file.mov",
                    ),
                    video: None,
                    position_seconds: 0.0,
                },
                Task::none(),
            );
        };

        match load_video(&path) {
            Ok(video) => (
                Self {
                    source_label: path.display().to_string(),
                    status: String::from(
                        "If this probe looks normal while our custom path looks dark, the bug is ours.",
                    ),
                    video: Some(video),
                    position_seconds: 0.0,
                },
                Task::none(),
            ),
            Err(error) => (
                Self {
                    source_label: path.display().to_string(),
                    status: format!("Load failed: {error}"),
                    video: None,
                    position_seconds: 0.0,
                },
                Task::none(),
            ),
        }
    }

    /// Handles simple playback controls for the probe.
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::TogglePausePressed => {
                if let Some(video) = self.video.as_mut() {
                    video.set_paused(!video.paused());
                }
            }
            Message::ToggleLoopPressed => {
                if let Some(video) = self.video.as_mut() {
                    video.set_looping(!video.looping());
                }
            }
            Message::NewFrame => {
                if let Some(video) = self.video.as_ref() {
                    self.position_seconds = video.position().as_secs_f64();
                }
            }
            Message::EndOfStream => {
                self.status = String::from("End of stream reached.");
            }
        }

        Task::none()
    }

    /// Renders the file label, controls, and player widget.
    fn view(&self) -> Element<'_, Message> {
        let controls = self.controls();
        let player = self.player_view();

        container(
            column![
                text("iced_video_player renderer probe").size(30),
                text(&self.source_label),
                text(&self.status),
                controls,
                player,
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

    /// Builds the playback controls shown above the player.
    fn controls(&self) -> Element<'_, Message> {
        let pause_label = self
            .video
            .as_ref()
            .map(|video| if video.paused() { "Resume" } else { "Pause" })
            .unwrap_or("Pause");
        let loop_label = self
            .video
            .as_ref()
            .map(|video| {
                if video.looping() {
                    "Disable Loop"
                } else {
                    "Enable Loop"
                }
            })
            .unwrap_or("Enable Loop");

        let pause = if self.video.is_some() {
            button(text(pause_label)).on_press(Message::TogglePausePressed)
        } else {
            button(text(pause_label))
        };
        let looping = if self.video.is_some() {
            button(text(loop_label)).on_press(Message::ToggleLoopPressed)
        } else {
            button(text(loop_label))
        };

        row![
            pause,
            looping,
            text(format!("Position: {:.2}s", self.position_seconds)),
        ]
        .spacing(12)
        .into()
    }

    /// Builds the video player itself or a placeholder when no file loaded.
    fn player_view(&self) -> Element<'_, Message> {
        let Some(video) = self.video.as_ref() else {
            return container(text("No video loaded."))
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into();
        };

        container(
            VideoPlayer::new(video)
                .width(Length::Fill)
                .height(Length::Fill)
                .content_fit(ContentFit::Contain)
                .on_new_frame(Message::NewFrame)
                .on_end_of_stream(Message::EndOfStream),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }
}

/// Returns the theme used by the video probe.
fn app_theme(_: &VideoPlayerProbe) -> Theme {
    Theme::TokyoNight
}

/// Picks the first CLI argument as the video input path.
fn selected_input() -> Option<PathBuf> {
    env::args().nth(1).map(PathBuf::from)
}

/// Configures the Homebrew GStreamer runtime paths used by the example on macOS.
fn configure_runtime_environment() {
    #[cfg(target_os = "macos")]
    {
        set_var_if_missing(
            "GI_TYPELIB_PATH",
            "/opt/homebrew/lib/girepository-1.0:/opt/homebrew/opt/gstreamer/lib/girepository-1.0",
        );
        set_var_if_missing(
            "DYLD_LIBRARY_PATH",
            "/opt/homebrew/lib:/opt/homebrew/opt/gstreamer/lib",
        );
        set_var_if_missing(
            "DYLD_FALLBACK_LIBRARY_PATH",
            "/opt/homebrew/lib:/opt/homebrew/opt/gstreamer/lib",
        );
        set_var_if_missing(
            "GST_PLUGIN_SCANNER",
            "/opt/homebrew/opt/gstreamer/libexec/gstreamer-1.0/gst-plugin-scanner",
        );
        set_var_if_missing(
            "GST_PLUGIN_SYSTEM_PATH_1_0",
            "/opt/homebrew/opt/gstreamer/lib/gstreamer-1.0",
        );
    }
}

/// Sets one environment variable only when the caller has not already provided it.
fn set_var_if_missing(key: &str, value: &str) {
    if env::var_os(key).is_none() {
        // This example is an adapter around external native tooling, so environment
        // variables are the cleanest way to hand Homebrew's runtime paths to GStreamer.
        env::set_var(key, value);
    }
}

/// Loads one local file as an `iced_video_player` video.
fn load_video(path: &Path) -> Result<Video, Box<dyn std::error::Error>> {
    let absolute = path.canonicalize()?;
    let uri = Url::from_file_path(&absolute)
        .map_err(|_| format!("could not convert {} into a file URL", absolute.display()))?;
    let mut video = Video::new(&uri)?;
    video.set_paused(false);

    Ok(video)
}
