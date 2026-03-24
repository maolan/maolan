use crate::message::{AddTrack, Message};
use iced::{
    Alignment, Border, Color, Element, Length,
    widget::{Id, button, column, container, pick_list, row, text, text_input},
};
use maolan_engine::message::Action;
use maolan_widgets::numeric_input::number_input;

#[derive(Debug)]
pub struct AddTrackView {
    name: String,
    count: usize,
    audio_ins: usize,
    audio_outs: usize,
    midi_ins: usize,
    midi_outs: usize,
    available_templates: Vec<String>,
    selected_template: Option<String>,
}

impl AddTrackView {
    pub fn name_input_id() -> Id {
        Id::new("add-track-name-input")
    }

    fn create_message(&self) -> Option<Message> {
        if self.name.trim().is_empty() {
            return None;
        }
        Some(Message::AddTrack(AddTrack::Submit))
    }

    pub fn create_messages(&self) -> Vec<Message> {
        let base_name = self.name.trim();
        if base_name.is_empty() {
            return Vec::new();
        }

        let template_name = self
            .selected_template
            .clone()
            .unwrap_or_else(|| "empty".to_string());
        let count = self.count.max(1);

        (0..count)
            .map(|index| {
                let name = if count == 1 {
                    base_name.to_string()
                } else {
                    format!("{base_name} {}", index + 1)
                };
                if template_name == "empty" {
                    Message::Request(Action::AddTrack {
                        name,
                        audio_ins: self.audio_ins,
                        midi_ins: self.midi_ins,
                        audio_outs: self.audio_outs,
                        midi_outs: self.midi_outs,
                    })
                } else {
                    Message::AddTrackFromTemplate {
                        name,
                        template: template_name.clone(),
                        audio_ins: self.audio_ins,
                        midi_ins: self.midi_ins,
                        audio_outs: self.audio_outs,
                        midi_outs: self.midi_outs,
                    }
                }
            })
            .collect()
    }

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

    pub fn update(&mut self, message: &Message) {
        if let Message::AddTrack(a) = message {
            match a {
                AddTrack::Name(name) => {
                    self.name = name.clone();
                }
                AddTrack::Count(count) => {
                    self.count = (*count).max(1);
                }
                AddTrack::AudioIns(ins) => {
                    self.audio_ins = *ins;
                }
                AddTrack::MIDIIns(ins) => {
                    self.midi_ins = *ins;
                }
                AddTrack::AudioOuts(outs) => {
                    self.audio_outs = *outs;
                }
                AddTrack::MIDIOuts(outs) => {
                    self.midi_outs = *outs;
                }
                AddTrack::TemplateSelected(template) => {
                    if template == "empty" {
                        self.selected_template = Some(template.clone());
                        // Reset to defaults when empty is selected
                        self.audio_ins = 1;
                        self.audio_outs = 1;
                        self.midi_ins = 0;
                        self.midi_outs = 0;
                    } else {
                        self.selected_template = Some(template.clone());
                        // Load template to get ins/outs
                        if let Some((audio_ins, audio_outs, midi_ins, midi_outs)) =
                            Self::load_template_config(template)
                        {
                            self.audio_ins = audio_ins;
                            self.audio_outs = audio_outs;
                            self.midi_ins = midi_ins;
                            self.midi_outs = midi_outs;
                        }
                    }
                }
                AddTrack::Submit => {}
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let create_message = self.create_message();
        let create = if let Some(message) = create_message.clone() {
            button("Create").on_press(message)
        } else {
            button("Create")
        };

        // Build template options with "empty" as first option
        let mut template_options = vec!["empty".to_string()];
        template_options.extend(self.available_templates.clone());

        let selected_display = self.selected_template.as_deref().unwrap_or("empty");
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
                    .id(Self::name_input_id())
                    .on_input(|name: String| Message::AddTrack(AddTrack::Name(name)))
                    .on_submit_maybe(create_message)
                    .width(Length::Fixed(200.0)),
            ]
            .spacing(10),
            row![
                text("Tracks:"),
                number_input(&self.count, 1..=128, |count: usize| {
                    Message::AddTrack(AddTrack::Count(count))
                })
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

        container(column![text("Add Track"), col.align_x(Alignment::End).spacing(10)].spacing(10))
            .style(|_theme| container::Style {
                border: Border {
                    color: Color::from_rgba(0.34, 0.42, 0.56, 0.72),
                    width: 1.0,
                    ..Border::default()
                },
                ..crate::style::app_background()
            })
            .padding(12)
            .width(Length::Fixed(320.0))
            .height(Length::Fill)
            .into()
    }
}

impl Default for AddTrackView {
    fn default() -> Self {
        Self {
            count: 1,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::{LazyLock, Mutex},
        time::{SystemTime, UNIX_EPOCH},
    };

    static ENV_GUARD: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    #[test]
    fn update_applies_basic_add_track_fields() {
        let mut view = AddTrackView::default();

        view.update(&Message::AddTrack(AddTrack::Name("Lead".to_string())));
        view.update(&Message::AddTrack(AddTrack::Count(3)));
        view.update(&Message::AddTrack(AddTrack::AudioIns(2)));
        view.update(&Message::AddTrack(AddTrack::AudioOuts(4)));
        view.update(&Message::AddTrack(AddTrack::MIDIIns(1)));
        view.update(&Message::AddTrack(AddTrack::MIDIOuts(5)));

        assert_eq!(view.name, "Lead");
        assert_eq!(view.count, 3);
        assert_eq!(view.audio_ins, 2);
        assert_eq!(view.audio_outs, 4);
        assert_eq!(view.midi_ins, 1);
        assert_eq!(view.midi_outs, 5);
    }

    #[test]
    fn update_clamps_track_count_to_at_least_one() {
        let mut view = AddTrackView::default();

        view.update(&Message::AddTrack(AddTrack::Count(0)));

        assert_eq!(view.count, 1);
    }

    #[test]
    fn update_selecting_empty_template_resets_default_io() {
        let mut view = AddTrackView {
            audio_ins: 7,
            audio_outs: 8,
            midi_ins: 9,
            midi_outs: 10,
            ..AddTrackView::default()
        };

        view.update(&Message::AddTrack(AddTrack::TemplateSelected(
            "empty".to_string(),
        )));

        assert_eq!(view.selected_template.as_deref(), Some("empty"));
        assert_eq!(view.audio_ins, 1);
        assert_eq!(view.audio_outs, 1);
        assert_eq!(view.midi_ins, 0);
        assert_eq!(view.midi_outs, 0);
    }

    #[test]
    fn update_selecting_template_loads_io_from_template_file() {
        let _guard = ENV_GUARD.lock().expect("lock env guard");
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temp_home = std::env::temp_dir().join(format!("maolan_add_track_test_{unique}"));
        let template_dir = temp_home.join(".config/maolan/track_templates/Band");
        fs::create_dir_all(&template_dir).expect("create template dir");
        fs::write(
            template_dir.join("track.json"),
            r#"{"track":{"audio":{"ins":2,"outs":3},"midi":{"ins":4,"outs":5}}}"#,
        )
        .expect("write template file");

        let old_home = std::env::var("HOME").ok();
        unsafe {
            std::env::set_var("HOME", &temp_home);
        }

        let mut view = AddTrackView::default();
        view.update(&Message::AddTrack(AddTrack::TemplateSelected(
            "Band".to_string(),
        )));

        if let Some(home) = old_home {
            unsafe {
                std::env::set_var("HOME", home);
            }
        } else {
            unsafe {
                std::env::remove_var("HOME");
            }
        }
        let _ = fs::remove_dir_all(&temp_home);

        assert_eq!(view.selected_template.as_deref(), Some("Band"));
        assert_eq!(view.audio_ins, 2);
        assert_eq!(view.audio_outs, 3);
        assert_eq!(view.midi_ins, 4);
        assert_eq!(view.midi_outs, 5);
    }

    #[test]
    fn update_ignores_non_add_track_messages() {
        let mut view = AddTrackView::default();

        view.update(&Message::Cancel);

        assert_eq!(view.name, "");
        assert_eq!(view.count, 1);
        assert_eq!(view.selected_template.as_deref(), Some("empty"));
    }
}
