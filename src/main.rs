mod menu;
mod message;
mod workspace;

use std::process::exit;
use std::sync::LazyLock;
use tracing::{Level, error, info, span};
use tracing_subscriber;
use tracing_subscriber::{
    fmt::{Layer as FmtLayer, writer::MakeWriterExt},
    prelude::*,
};

use iced::Subscription;
use iced::Theme;
use iced::futures::Stream;
use iced::widget::column;

use iced_aw::ICED_AW_FONT_BYTES;

use engine::message::{Action, Message as EngineMessage};
use maolan_engine as engine;
use message::Message;

static CLIENT: LazyLock<engine::client::Client> =
    LazyLock::new(|| engine::client::Client::default());

pub fn main() -> iced::Result {
    let stdout_layer =
        FmtLayer::new().with_writer(std::io::stdout.with_max_level(tracing::Level::INFO));
    // let logfile = tracing_appender::rolling::hourly("./logs", "maolan.log");
    // let (non_blocking_appender, _guard) = tracing_appender::non_blocking(logfile);
    // let file_layer = FmtLayer::new()
    //     .with_ansi(false)
    //     .with_writer(non_blocking_appender);

    tracing_subscriber::registry()
        .with(stdout_layer)
        // .with(file_layer)
        .init();

    let my_span = span!(Level::INFO, "main");
    let _enter = my_span.enter();

    error!("This is an error message");

    iced::application(Maolan::default, Maolan::update, Maolan::view)
        .title("Maolan")
        .theme(Theme::Dark)
        .font(ICED_AW_FONT_BYTES)
        .subscription(Maolan::subscription)
        .run()
}

#[derive(Default)]
struct Maolan {
    menu: menu::MaolanMenu,
    workspace: workspace::MaolanWorkspace,
}

impl Maolan {
    fn update_children(&mut self, message: &message::Message) {
                    self.menu.update(message.clone());
                    self.workspace.update(message.clone());
    }

    fn update(&mut self, message: message::Message) {
        match message {
            Message::Request(ref a) => match a {
                _ => {
                    info!("Maolan::update::request({:?})", a);
                    CLIENT.send(EngineMessage::Request(a.clone()));
                }
            },
            Message::Response(ref a) => match a {
                Action::Quit => {
                    exit(0);
                }
                Action::Error(s) => {
                    error!("Maolan::update::error({s})");
                }
                _ => {
                    info!("Maolan::update::response({:?})", a);
                    self.update_children(&message)
                }
            },
            message::Message::Debug(s) => {
                info!("Maolan::update::debug({s})");
            }
        }
    }

    fn view(&self) -> iced::Element<'_, message::Message> {
        column![self.menu.view(), self.workspace.view()].into()
    }

    fn subscription(&self) -> Subscription<message::Message> {
        fn listener() -> impl Stream<Item = message::Message> {
            use iced::futures::stream;

            stream::unfold(CLIENT.subscribe(), async move |mut receiver| {
                let command = match receiver.recv().await? {
                    EngineMessage::Response(e) => Message::Response(e),
                    _ => {
                        Message::Response(Action::Error("failed to receive in unfold".to_string()))
                    }
                };

                Some((command, receiver))
            })
        }

        Subscription::run(listener)
    }
}
