use super::meta::{Track, TrackType};
use crate::message::Message;
use iced::{
    Element,
    widget::{column, text},
};
use maolan_engine::message::Action;

#[derive(Debug, Default)]
pub struct Tracks {
    tracks: Vec<Track>,
}

impl Tracks {
    fn update_children(&mut self, message: Message) {
        match message {
            _ => {
                for track in &mut self.tracks {
                    track.update(message.clone());
                }
            }
        }
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::Response(Ok(ref a)) => match a {
                Action::AddAudioTrack{name, ins, audio_outs, midi_outs} => {
                    self.tracks.push(Track::new(
                        name.clone(),
                        0.0,
                        ins.clone(),
                        TrackType::Audio,
                        audio_outs.clone(),
                        midi_outs.clone(),
                    ));
                }
                Action::AddMIDITrack{name, midi_outs, audio_outs} => {
                    self.tracks.push(Track::new(
                        name.clone(),
                        0.0,
                        0,
                        TrackType::MIDI,
                        audio_outs.clone(),
                        midi_outs.clone(),
                    ));
                }
                _ => {}
            },
            _ => {
                self.update_children(message);
            }
        }

    }

    pub fn view(&self) -> Element<'_, Message> {
        let mut result = column![];
        for track in &self.tracks {
            result = result.push(text(track.name.clone()));
        }
        result.into()
    }
}
