mod add_track;
mod connections;
mod gui;
mod hw;
mod menu;
mod message;
mod state;
mod style;
mod toolbar;
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
    let stdout_layer =
        FmtLayer::new().with_writer(std::io::stdout.with_max_level(tracing::Level::INFO));

    tracing_subscriber::registry().with(stdout_layer).init();

    let my_span = span!(Level::INFO, "main");
    let _enter = my_span.enter();
    let settings = Settings {
        default_text_size: Pixels(16.0),
        ..Default::default()
    };

    iced::application(Maolan::default, Maolan::update, Maolan::view)
        .title("Maolan")
        .settings(settings)
        .theme(Theme::Dark)
        .font(LUCIDE_FONT_BYTES)
        .subscription(Maolan::subscription)
        .run()
}
