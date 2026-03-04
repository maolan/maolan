use crate::message::{AddTrack, Message};
use iced::{
    Alignment, Element, Length,
    widget::{button, column, container, pick_list, row, text, text_input},
};
use iced_aw::number_input;
use maolan_engine::message::Action;

#[derive(Debug)]
pub struct AddTrackView {
    name: String,
    audio_ins: usize,
    audio_outs: usize,
    midi_ins: usize,
    midi_outs: usize,
    available_templates: Vec<String>,
    selected_template: Option<String>,
}

impl AddTrackView {
    pub fn set_available_templates(&mut self, templates: Vec<String>) {
        self.available_templates = templates;
    }

    fn load_template_config(template_name: &str) -> Option<(usize, usize, usize, usize)> {
        use std::fs::File;
        use std::io::BufReader;

        let home = std::env::var("HOME").ok()?;
        let template_path = format!(
            "{}/.config/maolan/track_templates/{}/track.json",
            home, template_name
        );

        let file = File::open(template_path).ok()?;
        let reader = BufReader::new(file);
        let json: serde_json::Value = serde_json::from_reader(reader).ok()?;

        let track = json.get("track")?;
        let audio = track.get("audio")?;
        let midi = track.get("midi")?;

        let audio_ins = audio.get("ins")?.as_u64()? as usize;
        let audio_outs = audio.get("outs")?.as_u64()? as usize;
        let midi_ins = midi.get("ins")?.as_u64()? as usize;
        let midi_outs = midi.get("outs")?.as_u64()? as usize;

        Some((audio_ins, audio_outs, midi_ins, midi_outs))
    }

    pub fn update(&mut self, message: Message) {
        if let Message::AddTrack(a) = message {
            match a {
                AddTrack::Name(name) => {
                    self.name = name;
                }
                AddTrack::AudioIns(ins) => {
                    self.audio_ins = ins;
                }
                AddTrack::MIDIIns(ins) => {
                    self.midi_ins = ins;
                }
                AddTrack::AudioOuts(outs) => {
                    self.audio_outs = outs;
                }
                AddTrack::MIDIOuts(outs) => {
                    self.midi_outs = outs;
                }
                AddTrack::TemplateSelected(template) => {
                    if template == "empty" {
                        self.selected_template = Some(template);
                        // Reset to defaults when empty is selected
                        self.audio_ins = 1;
                        self.audio_outs = 1;
                        self.midi_ins = 0;
                        self.midi_outs = 0;
                    } else {
                        self.selected_template = Some(template.clone());
                        // Load template to get ins/outs
                        if let Some((audio_ins, audio_outs, midi_ins, midi_outs)) =
                            Self::load_template_config(&template)
                        {
                            self.audio_ins = audio_ins;
                            self.audio_outs = audio_outs;
                            self.midi_ins = midi_ins;
                            self.midi_outs = midi_outs;
                        }
                    }
                }
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let template_name = self
            .selected_template
            .clone()
            .unwrap_or_else(|| "empty".to_string());

        let create = if self.name.trim().is_empty() {
            button("Create")
        } else {
            let message = if template_name == "empty" {
                Message::Request(Action::AddTrack {
                    name: self.name.clone(),
                    audio_ins: self.audio_ins,
                    midi_ins: self.midi_ins,
                    audio_outs: self.audio_outs,
                    midi_outs: self.midi_outs,
                })
            } else {
                Message::AddTrackFromTemplate {
                    name: self.name.clone(),
                    template: template_name.clone(),
                    audio_ins: self.audio_ins,
                    midi_ins: self.midi_ins,
                    audio_outs: self.audio_outs,
                    midi_outs: self.midi_outs,
                }
            };
            button("Create").on_press(message)
        };

        // Build template options with "empty" as first option
        let mut template_options = vec!["empty".to_string()];
        template_options.extend(self.available_templates.clone());

        let selected_display = self
            .selected_template
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or("empty");
        let is_empty_template = selected_display == "empty";

        let mut col = column![
            row![
                text("Template:"),
                pick_list(
                    template_options,
                    Some(selected_display.to_string()),
                    |template| Message::AddTrack(AddTrack::TemplateSelected(template))
                )
                .width(Length::Fixed(200.0)),
            ]
            .spacing(10),
            row![
                text("Name:"),
                text_input("Track name", &self.name)
                    .on_input(|name: String| Message::AddTrack(AddTrack::Name(name)))
                    .width(Length::Fixed(200.0)),
            ]
            .spacing(10),
        ];

        // Only show ins/outs inputs if "empty" template is selected
        if is_empty_template {
            col = col.push(
                row![
                    text("Audio inputs:"),
                    number_input(&self.audio_ins, 0..=32, |ins: usize| {
                        Message::AddTrack(AddTrack::AudioIns(ins))
                    })
                ]
                .spacing(10),
            );
            col = col.push(
                row![
                    text("Audio outputs:"),
                    number_input(&self.audio_outs, 0..=32, |outs: usize| {
                        Message::AddTrack(AddTrack::AudioOuts(outs))
                    }),
                ]
                .spacing(10),
            );
            col = col.push(
                row![
                    text("Midi inputs:"),
                    number_input(&self.midi_ins, 0..=32, |ins: usize| {
                        Message::AddTrack(AddTrack::MIDIIns(ins))
                    })
                ]
                .spacing(10),
            );
            col = col.push(
                row![
                    text("Midi outputs:"),
                    number_input(&self.midi_outs, 0..=32, |outs: usize| {
                        Message::AddTrack(AddTrack::MIDIOuts(outs))
                    }),
                ]
                .spacing(10),
            );
        } else {
            // Show read-only information when a template is selected
            col = col.push(
                row![text(format!(
                    "Audio: {} in / {} out, MIDI: {} in / {} out",
                    self.audio_ins, self.audio_outs, self.midi_ins, self.midi_outs
                )),]
                .spacing(10),
            );
        }

        col = col.push(
            row![
                create,
                button("Cancel")
                    .on_press(Message::Cancel)
                    .style(button::secondary)
            ]
            .spacing(10),
        );

        container(col.align_x(Alignment::End).spacing(10))
            .padding(20)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .into()
    }
}

impl Default for AddTrackView {
    fn default() -> Self {
        Self {
            audio_ins: 1,
            audio_outs: 1,
            midi_ins: 0,
            midi_outs: 0,
            name: "".to_string(),
            available_templates: vec![],
            selected_template: Some("empty".to_string()),
        }
    }
}
