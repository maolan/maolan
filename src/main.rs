mod menu;
mod message;
mod state;
mod style;
mod workspace;

use serde_json::Value;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::PathBuf;
use std::process::exit;
use std::sync::LazyLock;
use tracing::{Level, debug, error, span};
use tracing_subscriber;
use tracing_subscriber::{
    fmt::{Layer as FmtLayer, writer::MakeWriterExt},
    prelude::*,
};

use iced::Subscription;
use iced::Theme;
use iced::futures::{Stream, io};
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
    workspace: workspace::Workspace,
}

impl Maolan {
    fn save(&self, path: String) -> std::io::Result<()> {
        let filename = "session.json";
        let result = self.workspace.json();
        let mut p = PathBuf::from(path.clone());
        p.push(filename);
        fs::create_dir_all(path)?;
        let file = File::create(&p)?;
        serde_json::to_writer_pretty(file, &result)?;
        Ok(())
    }
    fn load(&self, path: String) -> std::io::Result<()> {
        let filename = "session.json";
        let mut p = PathBuf::from(path.clone());
        p.push(filename);
        let file = File::open(&p)?;
        let reader = BufReader::new(file);
        let session: Value = serde_json::from_reader(reader)?;

        if let Some(arr) = session["tracks"].as_array() {
            for track in arr {
                println!("track: {}", track);
                let name = {
                    if let Some(value) = track["name"].as_str() {
                        value.to_string()
                    } else {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "No 'name' in track",
                        ));
                    }
                };
                let ins = {
                    if let Some(value) = track["ins"].as_u64() {
                        value as usize
                    } else {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "No 'ins' in track",
                        ));
                    }
                };
                let audio_outs = {
                    if let Some(value) = track["audio_outs"].as_u64() {
                        value as usize
                    } else {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "No 'audio_outs' in track",
                        ));
                    }
                };
                let midi_outs = {
                    if let Some(value) = track["midi_outs"].as_u64() {
                        value as usize
                    } else {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "No 'midi_outs' in track",
                        ));
                    }
                };
                if track["track_type"] == "Audio" {
                    CLIENT.send(EngineMessage::Request(Action::AddAudioTrack {
                        name,
                        ins,
                        audio_outs,
                        midi_outs,
                    }));
                } else if track["track_type"] == "MIDI" {
                    CLIENT.send(EngineMessage::Request(Action::AddMIDITrack {
                        name,
                        audio_outs,
                        midi_outs,
                    }));
                } else {
                    let track_type = track["track_type"].to_string();
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("Unknown track type '{track_type}'"),
                    ));
                }
                if let Some(value) = track["armed"].as_bool() {
                    if value {
                        CLIENT.send(EngineMessage::Request(Action::TrackToggleArm(track["name"].to_string())));
                    }
                } else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "'armed' is not boolean",
                    ));
                }
                if let Some(value) = track["muted"].as_bool() {
                    if value {
                        CLIENT.send(EngineMessage::Request(Action::TrackToggleMute(track["name"].to_string())));
                    }
                } else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "'muted' is not boolean",
                    ));
                }
                if let Some(value) = track["soloed"].as_bool() {
                    if value {
                        CLIENT.send(EngineMessage::Request(Action::TrackToggleMute(track["name"].to_string())));
                    }
                } else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "'soloed' is not boolean",
                    ));
                }
            }
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "'tracks' is not an array",
            ));
        }
        Ok(())
    }
    fn update_children(&mut self, message: &message::Message) {
        self.menu.update(message.clone());
        self.workspace.update(message.clone());
    }

    fn update(&mut self, message: message::Message) {
        match message {
            Message::Request(ref a) => {
                CLIENT.send(EngineMessage::Request(a.clone()));
                return;
            }
            Message::Response(Ok(ref a)) => match a {
                Action::Quit => {
                    exit(0);
                }
                _ => {}
            },
            Message::Debug(ref s) => {
                debug!("Maolan::update::debug({s})");
            }
            Message::Save(ref path) => match self.save(path.clone()) {
                Err(s) => {
                    error!("{}", s);
                }
                _ => {}
            },
            Message::Open(ref path) => match self.load(path.clone()) {
                Err(s) => {
                    error!("{}", s);
                }
                _ => {}
            },
            _ => {}
        }
        self.update_children(&message);
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
                    _ => Message::Response(Err("failed to receive in unfold".to_string())),
                };

                Some((command, receiver))
            })
        }

        Subscription::run(listener)
    }
}
