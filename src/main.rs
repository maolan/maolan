mod menu;
mod message;

use std::sync::LazyLock;
use std::process::exit;

use iced::Subscription;
use iced::Theme;
use iced::futures::Stream;

use iced_aw::ICED_AW_FONT_BYTES;

use maolan_engine as engine;
use engine::message::{Action, Message as EngineMessage};
use message::{Message};

static CLIENT: LazyLock<engine::client::Client> =
    LazyLock::new(|| engine::client::Client::default());

pub fn main() -> iced::Result {
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
}

impl Maolan {
    fn update(&mut self, message: message::Message) {
        match message {
            Message::Request(ref a) => {
                match a {
                    _ => {
                        println!("Maolan::update::request({:?})", a);
                        CLIENT.send(EngineMessage::Request(a.clone()));
                    },
                }
            }
            Message::Response(ref a) => {
                match a {
                    Action::Quit => {
                        exit(0);
                    }
                    _ => {
                        println!("Maolan::update::response({:?})", a);
                        self.menu.update(message);
                    },
                }
            }
            message::Message::Debug(ref s) => {
                println!("Maolan::update::debug({s})");
            },
        }
    }

    fn view(&self) -> iced::Element<'_, message::Message> {
        self.menu.view()
    }

    fn subscription(&self) -> Subscription<message::Message> {
        fn listener() -> impl Stream<Item = message::Message> {
            use iced::futures::stream;

            stream::unfold(CLIENT.subscribe(), async move |mut receiver| {
                let command = match receiver.recv().await? {
                    EngineMessage::Response(e) => Message::Response(e),
                    _ => Message::Response(Action::Error("failed to receive in unfold".to_string())),
                };

                Some((command, receiver))
            })
        }

        Subscription::run(listener)
    }
}
