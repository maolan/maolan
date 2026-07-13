#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(miri)]
fn main() {}

#[cfg(not(miri))]
mod add_track;
#[cfg(not(miri))]
mod apply_template;
#[cfg(not(miri))]
mod audio_defaults;
#[cfg(not(miri))]
mod clip_rename;
#[cfg(not(miri))]
mod config;
#[cfg(not(miri))]
mod connections;
#[cfg(not(miri))]
mod consts;
#[cfg(not(miri))]
mod gui;
#[cfg(not(miri))]
mod hw;
#[cfg(not(miri))]
mod icon;
#[cfg(not(miri))]
mod menu;
#[cfg(not(miri))]
mod message;
#[cfg(not(miri))]
mod midi;
#[cfg(not(miri))]
mod modulator_target_dialog;
#[cfg(not(miri))]
mod modulators_pane;
#[cfg(not(miri))]
mod platform_caps;
#[cfg(not(miri))]
mod plugin_blocklist;
#[cfg(not(miri))]
mod plugin_host;
#[cfg(not(miri))]
mod session_view;
#[cfg(not(miri))]
mod shortcuts_pane;
#[cfg(not(miri))]
mod state;
#[cfg(not(miri))]
mod style;
#[cfg(not(miri))]
mod template_save;
#[cfg(not(miri))]
mod toolbar;
#[cfg(not(miri))]
mod track;
#[cfg(not(miri))]
mod ui_timing;
#[cfg(not(miri))]
mod widget;
#[cfg(not(miri))]
mod workspace;

#[cfg(not(miri))]
pub use track::marker as track_marker;
#[cfg(not(miri))]
pub use track::rename as track_rename;
#[cfg(not(miri))]
pub use track::template_save as track_template_save;

#[cfg(not(miri))]
use gui::Maolan;
#[cfg(not(miri))]
use iced::window;
#[cfg(not(miri))]
use iced::{Pixels, Settings, Theme};
#[cfg(not(miri))]
use iced_fonts::LUCIDE_FONT_BYTES;
#[cfg(not(miri))]
use tracing_subscriber::{
    fmt::{Layer as FmtLayer, writer::MakeWriterExt},
    prelude::*,
};

#[cfg(not(miri))]
pub fn main() -> iced::Result {
    let log_level = parse_log_level_from_env();
    if let Some(level) = log_level {
        let layer = FmtLayer::new().with_writer(std::io::stderr.with_max_level(level));
        tracing_subscriber::registry().with(layer).init();
    }

    let _enter = tracing::info_span!("main").entered();

    run_app()
}

#[cfg(not(miri))]
fn parse_log_level_from_env() -> Option<tracing::Level> {
    let args: Vec<String> = std::env::args().collect();
    if let Some(pos) = args.iter().position(|a| a == "--log-level") {
        if pos + 1 < args.len() {
            match args[pos + 1].as_str() {
                "none" => None,
                "info" => Some(tracing::Level::INFO),
                "warning" => Some(tracing::Level::WARN),
                "error" => Some(tracing::Level::ERROR),
                "debug" => Some(tracing::Level::DEBUG),
                _other => None,
            }
        } else {
            None
        }
    } else {
        None
    }
}

#[cfg(not(miri))]
fn run_app() -> iced::Result {
    let config = config::Config::load().unwrap_or_default();

    let icon = window::icon::from_file_data(crate::consts::main::ICON_BYTES, None).ok();

    let settings = Settings {
        default_text_size: Pixels(config.font_size),
        ..Default::default()
    };

    iced::application(Maolan::new, Maolan::update, Maolan::view)
        .title(Maolan::title)
        .settings(settings)
        .theme(Theme::Dark)
        .font(LUCIDE_FONT_BYTES)
        .font(icon::metronome::LUCIDE_METRONOME_FONT_BYTES)
        .subscription(Maolan::subscription)
        .window(window::Settings {
            icon,
            exit_on_close_request: false,
            ..Default::default()
        })
        .run()
}
