use std::thread::JoinHandle;
use iced::widget::{Column, button, column};
use maolan_engine::{init, client::Client, message::Message};

struct Maolan {
    client: Client,
    handles: Vec<JoinHandle<()>>,
}

impl Default for Maolan {
    fn default() -> Self {
        let (client, handle) = init();
        Self {client, handles: vec![handle]}
    }
}

impl Maolan {
    fn update(&mut self, message: Message) {
        match message {
            Message::Quit => {
                self.client.quit();
                self.join();
                std::process::exit(0);
            }
            _ => {}
        }
    }

    fn view(&self) -> Column<'_, Message> {
        column![
            button("quit").on_press(Message::Quit),
        ]
    }

    fn join(&mut self) {
        let handle = self.handles.remove(0);
        let _ = handle.join();
    }
}

fn main() -> iced::Result {
    iced::run("Maolan", Maolan::update, Maolan::view)
}
