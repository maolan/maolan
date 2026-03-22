mod add_track;
mod audio_defaults;
mod clip_rename;
mod config;
mod connections;
mod consts;
mod gui;
mod hw;
mod menu;
mod message;
mod platform_caps;
mod plugins;
mod state;
mod style;
mod template_save;
mod toolbar;
mod track;
mod ui_timing;
mod widget;
mod workspace;

pub use track::group as track_group;
pub use track::marker as track_marker;
pub use track::rename as track_rename;
pub use track::template_save as track_template_save;

use gui::Maolan;
use iced::window;
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
    // winit picks Wayland whenever WAYLAND_DISPLAY exists and does not fallback to X11.
    unsafe {
        std::env::remove_var("WAYLAND_DISPLAY");
        std::env::remove_var("WAYLAND_SOCKET");
    }
}

fn run_app() -> iced::Result {
    let config = config::Config::load().unwrap_or_default();

    let icon = window::icon::from_file_data(crate::consts::main::ICON_BYTES, None).ok();

    let settings = Settings {
        default_text_size: Pixels(config.font_size),
        ..Default::default()
    };

    iced::application(Maolan::default, Maolan::update, Maolan::view)
        .title(Maolan::title)
        .settings(settings)
        .theme(Theme::Dark)
        .font(LUCIDE_FONT_BYTES)
        .subscription(Maolan::subscription)
        .window(window::Settings {
            icon,
            exit_on_close_request: false,
            ..Default::default()
        })
        .run()
}
