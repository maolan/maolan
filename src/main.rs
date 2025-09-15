use iced::{
    Element,
    widget::{button, column, text},
};
use maolan_engine::{client::Client, init, message::Message};
use std::thread::JoinHandle;

struct Maolan {
    client: Client,
    handles: Vec<JoinHandle<()>>,
}

impl Default for Maolan {
    fn default() -> Self {
        let (client, handle) = init();
        Self {
            client,
            handles: vec![handle],
        }
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

    fn view(&self) -> Element<'_, Message> {
        let mut result = column![button("quit").on_press(Message::Quit),];
        match self.client.state().read() {
            Ok(state) => {
                for (name, _) in state.audio.tracks.clone() {
                    result = result.push(text(name));
                }
            }
            Err(e) => {
                println!("Error reading state: {e}");
            }
        }
        result.into()
    }

    fn join(&mut self) {
        let handle = self.handles.remove(0);
        match handle.join() {
            Err(_e) => {
                println!("Error joining engine thread");
            }
            _ => {}
        }
    }
}

fn main() -> iced::Result {
    iced::run("Maolan", Maolan::update, Maolan::view)
}
