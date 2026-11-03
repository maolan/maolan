#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod add_track;
mod apply_template;
mod audio_defaults;
mod clip_rename;
mod clips_pane;
mod config;
mod connections;
mod consts;
mod gui;
mod hw;
mod icon;
mod keyboard_shortcuts;
mod menu;
mod message;
mod midi;
mod modulator_target_dialog;
mod modulators_pane;
mod platform_caps;
mod plugin_blocklist;
mod plugin_host;
mod session_restore;
mod session_view;
mod shortcuts_pane;
mod state;
mod style;
mod template_save;
mod toolbar;
mod track;
mod ui_timing;
mod view_api;
mod widget;
mod workspace;

pub use track::marker as track_marker;
pub use track::rename as track_rename;
pub use track::template_save as track_template_save;

use gui::Maolan;
use maolan_widgets::iced::window;
use maolan_widgets::iced::{Pixels, Settings, Theme};
use maolan_widgets::iced_fonts::LUCIDE_FONT_BYTES;
use tracing_subscriber::{filter::LevelFilter, fmt::Layer as FmtLayer, prelude::*};

pub fn main() -> maolan_widgets::iced::Result {
    let log_level = parse_log_level_from_env();
    if let Some(level) = log_level {
        let layer = FmtLayer::new()
            .with_writer(std::io::stderr)
            .with_filter(LevelFilter::from_level(level));
        tracing_subscriber::registry().with(layer).init();
    }

    let _enter = tracing::info_span!("main").entered();

    run_app()
}

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

fn run_app() -> maolan_widgets::iced::Result {
    let config = config::Config::load().unwrap_or_default();

    let icon = window::icon::from_file_data(crate::consts::main::ICON_BYTES, None).ok();

    let settings = Settings {
        default_text_size: Pixels(config.font_size),
        ..Default::default()
    };

    maolan_widgets::iced::application(Maolan::new, Maolan::update, Maolan::view)
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
