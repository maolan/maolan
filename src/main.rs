mod menu;
mod message;

use std::sync::LazyLock;

use iced::Theme;
use iced::Subscription;
use iced::futures::Stream;

use iced_aw::ICED_AW_FONT_BYTES;

use maolan_engine as engine;

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
            message::Message::Echo(ref s) => {
                println!("Maolan::update::echo({s})");
                CLIENT.echo(s.clone());
            }
            message::Message::Debug(ref s) => {
                println!("Maolan::update::debug({s})");
            }
            _ => {}
        }
        self.menu.update(message)
    }

    fn view(&self) -> iced::Element<'_, message::Message> {
        self.menu.view()
    }

    fn subscription(&self) -> Subscription<message::Message> {
        fn listener() -> impl Stream<Item = message::Message> {
            use iced::futures::stream;

            stream::unfold(CLIENT.subscribe(), async move |mut receiver| {
                let command = match receiver.recv().await? {
                    engine::message::Message::Echo(s) => message::Message::Debug(s),
                    _ => message::Message::Error("failed to receive in unfold".to_string()),
                };

                Some((command, receiver))
            })
        }

        Subscription::run(listener)
    }
}
