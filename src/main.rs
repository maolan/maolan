use iced::{
    Element,
    widget::{button, column, text},
};
use maolan_engine::{client::Client, init, message::Message, message::Track as MessageTrack};
use std::thread::JoinHandle;

#[derive(Default)]
struct Track {
    name: String,
    // channels: usize,
    // flavor: String,
}

impl Track {
    pub fn new(name: String, _channels: usize, _flavor: String) -> Self {
        Self {
            name,
            // channels,
            // flavor,
        }
    }
}

#[derive(Default)]
struct State {
    tracks: Vec<Track>,
}

struct Maolan {
    client: Client,
    handles: Vec<JoinHandle<()>>,
    state: State,
}

impl Default for Maolan {
    fn default() -> Self {
        let (client, handle) = init();
        Self {
            client,
            handles: vec![handle],
            state: State::default(),
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
            Message::Add(t) => {
                match t {
                    MessageTrack::Audio(name, channels) => {
                        self.client.add_audio_track(name.clone(), channels);
                        self.state.tracks.push(Track::new(name, channels, "audio".to_string()));
                    }
                    MessageTrack::MIDI(name) => {
                        self.client.add_midi_track(name.clone());
                        self.state.tracks.push(Track::new(name, 0, "midi".to_string()));
                    }
                }
            }
            _ => {}
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let mut result = column![
            button("quit").on_press(Message::Quit),
            button("add").on_press(Message::Add(MessageTrack::MIDI("random".to_string()))),
        ];
        for track in &self.state.tracks {
            println!("track {}", track.name);
            result = result.push(text(track.name.clone()));
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
