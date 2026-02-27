mod add_track;
mod clip_rename;
mod connections;
mod gui;
mod hw;
mod menu;
mod message;
mod plugins;
mod state;
mod style;
mod toolbar;
mod track_rename;
mod ui_timing;
mod widget;
mod workspace;

use gui::Maolan;
use iced::{Pixels, Settings, Theme};
use iced_fonts::LUCIDE_FONT_BYTES;
use tracing::{Level, span};
use tracing_subscriber::{
    fmt::{Layer as FmtLayer, writer::MakeWriterExt},
    prelude::*,
};

pub fn main() -> iced::Result {
    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
    prefer_x11_backend();

    let debug_logging = std::env::args().any(|arg| arg == "--debug");
    if debug_logging {
        let stdout_layer =
            FmtLayer::new().with_writer(std::io::stdout.with_max_level(tracing::Level::INFO));
        tracing_subscriber::registry().with(stdout_layer).init();
        let my_span = span!(Level::INFO, "main");
        let _enter = my_span.enter();
        return run_app();
    }

    run_app()
}

#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
fn prefer_x11_backend() {
    let keep_wayland = std::env::var("MAOLAN_USE_WAYLAND")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false);

    if !keep_wayland {
        // winit picks Wayland whenever WAYLAND_DISPLAY exists and does not fallback to X11.
        unsafe {
            std::env::remove_var("WAYLAND_DISPLAY");
            std::env::remove_var("WAYLAND_SOCKET");
        }
    }
}

fn run_app() -> iced::Result {
    let settings = Settings {
        default_text_size: Pixels(16.0),
        ..Default::default()
    };

    iced::application(Maolan::default, Maolan::update, Maolan::view)
        .title(Maolan::title)
        .settings(settings)
        .theme(Theme::Dark)
        .font(LUCIDE_FONT_BYTES)
        .subscription(Maolan::subscription)
        .run()
}
