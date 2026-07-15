use crate::consts::state_ids::METRONOME_TRACK_ID;
use crate::message::Message;
use crate::state::{AudioClip, MIDIClip, Track};
use iced::{
    Background, Border, Color, Element, Length,
    widget::{column, container, scrollable, text},
};
use iced_drop::droppable;
use maolan_engine::kind::Kind;
use std::collections::HashSet;

pub struct ClipsPane;

impl ClipsPane {
    pub fn view(
        tracks: &[Track],
        unused_audio: &[AudioClip],
        unused_midi: &[MIDIClip],
        selected: &HashSet<String>,
    ) -> Element<'static, Message> {
        let mut list = column![].spacing(4);

        for track in tracks.iter().filter(|t| t.name != METRONOME_TRACK_ID) {
            let track_selected = selected.contains(&track.name);
            list = list.push(track_header(&track.name, track_selected));

            if track.audio.clips.is_empty() && track.midi.clips.is_empty() {
                list = list.push(clip_row("(no clips)", track_selected, true));
            } else {
                for clip in &track.audio.clips {
                    list = list.push(pane_clip_row(
                        Some(&track.name),
                        &clip.id,
                        &clip.name,
                        Kind::Audio,
                        track_selected,
                    ));
                }
                for clip in &track.midi.clips {
                    list = list.push(pane_clip_row(
                        Some(&track.name),
                        &clip.id,
                        &clip.name,
                        Kind::MIDI,
                        track_selected,
                    ));
                }
            }
        }

        list = list.push(unused_header());
        if unused_audio.is_empty() && unused_midi.is_empty() {
            list = list.push(clip_row("(no unused clips)", false, true));
        } else {
            for clip in unused_audio {
                list = list.push(pane_clip_row(
                    None,
                    &clip.id,
                    &clip.name,
                    Kind::Audio,
                    false,
                ));
            }
            for clip in unused_midi {
                list = list.push(pane_clip_row(None, &clip.id, &clip.name, Kind::MIDI, false));
            }
        }

        container(
            column![
                text("Clips").size(16),
                scrollable(list).height(Length::Fill),
            ]
            .spacing(10),
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
        .width(Length::Fixed(320.0))
        .height(Length::Fill)
        .into()
    }
}

fn unused_header() -> Element<'static, Message> {
    text("▸ Unused").size(13).into()
}

fn clip_display_name(name: &str) -> &str {
    let stripped = name
        .strip_prefix("audio/")
        .or_else(|| name.strip_prefix("midi/"))
        .unwrap_or(name);
    for suffix in [".wav", ".midi", ".mid"] {
        if let Some(stem) = stripped.strip_suffix(suffix) {
            return stem;
        }
    }
    stripped
}

fn pane_clip_row(
    source_track_name: Option<&str>,
    id: &str,
    name: &str,
    kind: Kind,
    track_selected: bool,
) -> Element<'static, Message> {
    let name = clip_display_name(name);
    let display = if name.trim().is_empty() {
        "(unnamed)".to_string()
    } else {
        format!("  • {}", name)
    };
    let mut label = text(display).size(11);
    if track_selected {
        label = label.color(Color::from_rgb(1.0, 0.95, 0.6));
    }
    let item: Element<'static, Message> = if track_selected {
        container(label)
            .style(|_theme| container::Style {
                background: Some(Background::Color(Color::from_rgba(0.35, 0.4, 0.22, 0.45))),
                border: Border {
                    radius: 4.0.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            })
            .padding([2, 4])
            .into()
    } else {
        label.into()
    };
    let clip_id = id.to_string();
    let source_track_name = source_track_name.map(str::to_string);
    droppable(item)
        .on_drag(move |_point, _rect| Message::PaneClipDragStart {
            source_track_name: source_track_name.clone(),
            clip_id: clip_id.clone(),
            kind,
        })
        .on_drop(move |point, _rect| Message::PaneClipDropped { point })
        .into()
}

fn track_header(name: &str, selected: bool) -> Element<'static, Message> {
    let label = text(format!("▸ {}", name)).size(13);
    if selected {
        container(label)
            .style(|_theme| container::Style {
                background: Some(Background::Color(Color::from_rgba(0.25, 0.32, 0.45, 0.55))),
                border: Border {
                    radius: 4.0.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            })
            .padding([2, 4])
            .into()
    } else {
        label.into()
    }
}

fn clip_row(name: &str, track_selected: bool, dim: bool) -> Element<'static, Message> {
    let display = if name.trim().is_empty() {
        "(unnamed)".to_string()
    } else {
        format!("  • {}", name)
    };
    let mut label = text(display).size(11);
    if dim {
        label = label.color(Color::from_rgba(0.6, 0.6, 0.6, 1.0));
    }

    if track_selected {
        container(label.color(Color::from_rgb(1.0, 0.95, 0.6)))
            .style(|_theme| container::Style {
                background: Some(Background::Color(Color::from_rgba(0.35, 0.4, 0.22, 0.45))),
                border: Border {
                    radius: 4.0.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            })
            .padding([2, 4])
            .into()
    } else {
        label.into()
    }
}
