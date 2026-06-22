use crate::message::{ApplyTemplate, Message};
use crate::state::State;
use iced::{
    Alignment, Border, Color, Element, Length,
    widget::{button, column, container, pick_list, row, text},
};

#[derive(Debug)]
pub struct ApplyTemplateView {
    state: State,
}

impl ApplyTemplateView {
    pub fn new(state: State) -> Self {
        Self { state }
    }

    pub fn update(&mut self, message: &Message) {
        if let Message::ApplyTemplate(ApplyTemplate::TemplateSelected(template)) = message
            && let Some(dialog) = &mut self.state.blocking_write().apply_template_dialog
        {
            dialog.selected_template = Some(template.clone());
        }
    }

    fn load_track_template_config(template_name: &str) -> Option<(usize, usize, usize, usize)> {
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

    fn load_group_template_track_count(template_name: &str) -> Option<usize> {
        use std::fs::File;
        use std::io::BufReader;

        let home = std::env::var("HOME").ok()?;
        let template_path = format!(
            "{}/.config/maolan/group_templates/{}/group.json",
            home, template_name
        );

        let file = File::open(template_path).ok()?;
        let reader = BufReader::new(file);
        let json: serde_json::Value = serde_json::from_reader(reader).ok()?;

        let tracks = json.get("tracks")?.as_array()?;
        Some(tracks.len())
    }

    pub fn view(&self) -> Element<'_, Message> {
        let state = self.state.blocking_read();
        let Some(dialog) = &state.apply_template_dialog else {
            return container("").into();
        };

        let target_label = if dialog.is_group {
            format!("Group: {}", dialog.track_name)
        } else {
            format!("Track: {}", dialog.track_name)
        };

        let selected_display = dialog.selected_template.as_deref().unwrap_or("");

        let mut info_text = String::new();
        let mut can_apply = !selected_display.is_empty();

        if dialog.is_group {
            if let Some(count) = Self::load_group_template_track_count(selected_display) {
                info_text = format!("Group template: {} tracks", count);
            } else if !selected_display.is_empty() {
                info_text = "Unable to read group template".to_string();
                can_apply = false;
            }
        } else if let Some(track) = state
            .tracks
            .iter()
            .find(|t| t.name == dialog.track_name)
        {
            if let Some((t_audio_ins, t_audio_outs, t_midi_ins, t_midi_outs)) =
                Self::load_track_template_config(selected_display)
            {
                let existing = (
                    track.primary_audio_ins(),
                    track.primary_audio_outs(),
                    track.midi.ins,
                    track.midi.outs,
                );
                let template = (t_audio_ins, t_audio_outs, t_midi_ins, t_midi_outs);
                let compatible = existing == template;
                can_apply = compatible;
                info_text = format!(
                    "Template: audio {} in / {} out, midi {} in / {} out\nTrack: audio {} in / {} out, midi {} in / {} out\n{}",
                    template.0,
                    template.1,
                    template.2,
                    template.3,
                    existing.0,
                    existing.1,
                    existing.2,
                    existing.3,
                    if compatible {
                        "Compatible"
                    } else {
                        "Incompatible input/output counts"
                    }
                );
            } else if !selected_display.is_empty() {
                info_text = "Unable to read track template".to_string();
                can_apply = false;
            }
        } else {
            info_text = "Target track not found".to_string();
            can_apply = false;
        }

        let apply_button = if can_apply {
            button("Apply").on_press(Message::ApplyTemplate(ApplyTemplate::Submit))
        } else {
            button("Apply")
        };

        let template_options = dialog.available_templates.clone();
        let pick = pick_list(
            template_options,
            dialog.selected_template.clone(),
            |template| Message::ApplyTemplate(ApplyTemplate::TemplateSelected(template)),
        )
        .placeholder("Select a template...")
        .width(Length::Fixed(240.0));

        let col = column![
            text(target_label).size(16),
            row![text("Template:"), pick].spacing(10),
            text(info_text).size(12),
            row![
                apply_button,
                button("Cancel")
                    .on_press(Message::Cancel)
                    .style(button::secondary)
            ]
            .spacing(10),
        ]
        .spacing(10)
        .align_x(Alignment::End);

        container(
            column![text("Apply Template"), col]
                .spacing(10)
                .align_x(Alignment::End),
        )
        .style(|_theme| container::Style {
            border: Border {
                color: Color::from_rgba(0.34, 0.42, 0.56, 0.72),
                width: 1.0,
                ..Border::default()
            },
            ..crate::style::app_background()
        })
        .padding(12)
        .width(Length::Fixed(360.0))
        .height(Length::Fill)
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    #[test]
    fn new_creates_view() {
        let state = Arc::new(RwLock::new(crate::state::StateData::default()));
        let _view = ApplyTemplateView::new(state);
    }

    #[test]
    fn update_sets_selected_template() {
        let state = Arc::new(RwLock::new(crate::state::StateData::default()));
        state.blocking_write().apply_template_dialog = Some(crate::state::ApplyTemplateDialog {
            track_name: "Kick".to_string(),
            is_group: false,
            selected_template: None,
            available_templates: vec![],
        });
        let mut view = ApplyTemplateView::new(state.clone());

        view.update(&Message::ApplyTemplate(ApplyTemplate::TemplateSelected(
            "Drum".to_string(),
        )));

        assert_eq!(
            state
                .blocking_read()
                .apply_template_dialog
                .as_ref()
                .and_then(|d| d.selected_template.as_deref()),
            Some("Drum")
        );
    }

    #[test]
    fn view_returns_empty_when_dialog_closed() {
        let state = Arc::new(RwLock::new(crate::state::StateData::default()));
        let view = ApplyTemplateView::new(state);
        let _element = view.view();
    }
}
