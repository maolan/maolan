mod menu;
mod message;

use std::cell::RefCell;
use tokio::sync::mpsc::UnboundedReceiver as Receiver;
use tokio::task::JoinHandle;

use iced::{Subscription, Theme};

use iced_aw::ICED_AW_FONT_BYTES;

use maolan_engine::{client::Client, init};

pub fn main() -> iced::Result {
    iced::application(Maolan::default, Maolan::update, Maolan::view)
        .title("Maolan")
        .theme(Theme::Dark)
        .font(ICED_AW_FONT_BYTES)
        .run()
}

struct Maolan {
    menu: menu::MaolanMenu,
    receiver: RefCell<Option<Receiver<maolan_engine::message::Message>>>,
    client: Client,
    handles: Vec<JoinHandle<()>>,
}

impl Default for Maolan {
    fn default() -> Self {
        let (client, handle, rx) = init();
        Self {
            client,
            receiver: RefCell::new(Some(rx)),
            handles: vec![handle],
            menu: menu::MaolanMenu::default(),
        }
    }
}

impl Maolan {
    fn update(&mut self, message: message::Message) {
        self.menu.update(message)
    }

    fn view(&self) -> iced::Element<'_, message::Message> {
        self.menu.view()
    }

    async fn join(&mut self) {
        let handle = self.handles.remove(0);
        match handle.await {
            Err(_e) => {
                println!("Error joining engine thread");
            }
            _ => {}
        }
    }

    fn subscription(&self) -> Subscription<message::Message> {
        Subscription::none()
    }
}
