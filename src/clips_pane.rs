use crate::consts::state_ids::METRONOME_TRACK_ID;
use crate::message::Message;
use crate::state::{AudioClip, MIDIClip, SessionMatrix, Track};
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
        session: &SessionMatrix,
        unused_audio: &[AudioClip],
        unused_midi: &[MIDIClip],
        selected: &HashSet<String>,
    ) -> Element<'static, Message> {
        let mut list = column![].spacing(4);
        let mut seen_clip_ids = HashSet::new();

        for track in tracks.iter().filter(|t| t.name != METRONOME_TRACK_ID) {
            let track_selected = selected.contains(&track.name);
            list = list.push(track_header(&track.name, track_selected));

            let track_clip_entries = unique_track_clip_entries(
                track,
                tracks,
                session,
                unused_audio,
                unused_midi,
                track_selected,
                &mut seen_clip_ids,
            );
            if track_clip_entries.is_empty() {
                list = list.push(clip_row("(no clips)", track_selected, true));
            } else {
                for entry in track_clip_entries {
                    list = list.push(pane_clip_row(
                        entry.source_track_name.as_deref(),
                        &entry.id,
                        &entry.name,
                        entry.kind,
                        entry.track_selected,
                    ));
                }
            }
        }

        list = list.push(unused_header());
        let unused_clip_rows =
            unique_unused_clip_rows(unused_audio, unused_midi, &mut seen_clip_ids);
        if unused_clip_rows.is_empty() {
            list = list.push(clip_row("(no unused clips)", false, true));
        } else {
            for row in unused_clip_rows {
                list = list.push(row);
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct ClipPaneEntry {
    source_track_name: Option<String>,
    id: String,
    name: String,
    kind: Kind,
    track_selected: bool,
}

fn unique_track_clip_entries(
    track: &Track,
    tracks: &[Track],
    session: &SessionMatrix,
    unused_audio: &[AudioClip],
    unused_midi: &[MIDIClip],
    track_selected: bool,
    seen_clip_ids: &mut HashSet<String>,
) -> Vec<ClipPaneEntry> {
    let mut rows = Vec::new();
    for clip in &track.audio.clips {
        if seen_clip_ids.insert(clip.id.clone()) {
            rows.push(ClipPaneEntry {
                source_track_name: Some(track.name.clone()),
                id: clip.id.clone(),
                name: clip.name.clone(),
                kind: Kind::Audio,
                track_selected,
            });
        }
    }
    for clip in &track.midi.clips {
        if seen_clip_ids.insert(clip.id.clone()) {
            rows.push(ClipPaneEntry {
                source_track_name: Some(track.name.clone()),
                id: clip.id.clone(),
                name: clip.name.clone(),
                kind: Kind::MIDI,
                track_selected,
            });
        }
    }
    if let Some(slots) = session.slots.get(&track.name) {
        for slot in slots {
            let Some(clip_ref) = slot.clip.as_ref() else {
                continue;
            };
            if !seen_clip_ids.insert(clip_ref.clip_id.clone()) {
                continue;
            }
            if let Some(entry) = resolve_slot_clip_entry(
                &clip_ref.clip_id,
                slot.clip_name.as_deref(),
                tracks,
                unused_audio,
                unused_midi,
                track_selected,
            ) {
                rows.push(entry);
            }
        }
    }
    rows
}

fn resolve_slot_clip_entry(
    clip_id: &str,
    fallback_name: Option<&str>,
    tracks: &[Track],
    unused_audio: &[AudioClip],
    unused_midi: &[MIDIClip],
    track_selected: bool,
) -> Option<ClipPaneEntry> {
    for track in tracks {
        if let Some(clip) = track.audio.clips.iter().find(|clip| clip.id == clip_id) {
            return Some(ClipPaneEntry {
                source_track_name: Some(track.name.clone()),
                id: clip.id.clone(),
                name: clip.name.clone(),
                kind: Kind::Audio,
                track_selected,
            });
        }
        if let Some(clip) = track.midi.clips.iter().find(|clip| clip.id == clip_id) {
            return Some(ClipPaneEntry {
                source_track_name: Some(track.name.clone()),
                id: clip.id.clone(),
                name: clip.name.clone(),
                kind: Kind::MIDI,
                track_selected,
            });
        }
    }
    if let Some(clip) = unused_audio.iter().find(|clip| clip.id == clip_id) {
        return Some(ClipPaneEntry {
            source_track_name: None,
            id: clip.id.clone(),
            name: clip.name.clone(),
            kind: Kind::Audio,
            track_selected,
        });
    }
    if let Some(clip) = unused_midi.iter().find(|clip| clip.id == clip_id) {
        return Some(ClipPaneEntry {
            source_track_name: None,
            id: clip.id.clone(),
            name: clip.name.clone(),
            kind: Kind::MIDI,
            track_selected,
        });
    }
    fallback_name.map(|name| ClipPaneEntry {
        source_track_name: None,
        id: clip_id.to_string(),
        name: name.to_string(),
        kind: Kind::Audio,
        track_selected,
    })
}

fn unique_unused_clip_rows(
    unused_audio: &[AudioClip],
    unused_midi: &[MIDIClip],
    seen_clip_ids: &mut HashSet<String>,
) -> Vec<Element<'static, Message>> {
    let mut rows = Vec::new();
    for clip in unused_audio {
        if seen_clip_ids.insert(clip.id.clone()) {
            rows.push(pane_clip_row(
                None,
                &clip.id,
                &clip.name,
                Kind::Audio,
                false,
            ));
        }
    }
    for clip in unused_midi {
        if seen_clip_ids.insert(clip.id.clone()) {
            rows.push(pane_clip_row(None, &clip.id, &clip.name, Kind::MIDI, false));
        }
    }
    rows
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::SlotClipRef;

    fn track(name: &str) -> Track {
        Track::new(name.to_string(), 1.0, 2, 2, 1, 1)
    }

    fn slot_ref(clip_id: &str) -> SlotClipRef {
        SlotClipRef {
            clip_id: clip_id.to_string(),
            launch_mode: crate::state::LaunchMode::Toggle,
            launch_quantization: crate::state::LaunchQuantization::Bar,
            loop_enabled: true,
            loop_start_samples: 0,
            loop_end_samples: 0,
        }
    }

    #[test]
    fn live_slot_clip_from_unused_pool_is_listed_under_live_track() {
        let tracks = vec![track("Intro")];
        let mut session = SessionMatrix::default();
        session.ensure_track_slots("Intro");
        let slot = session.slot_mut("Intro", 0).unwrap();
        slot.clip = Some(slot_ref("clip-a"));
        slot.clip_name = Some("slot fallback".to_string());
        let unused_audio = vec![AudioClip {
            id: "clip-a".to_string(),
            name: "audio/intro.wav".to_string(),
            ..AudioClip::default()
        }];
        let mut seen = HashSet::new();

        let rows = unique_track_clip_entries(
            &tracks[0],
            &tracks,
            &session,
            &unused_audio,
            &[],
            false,
            &mut seen,
        );

        assert_eq!(
            rows,
            vec![ClipPaneEntry {
                source_track_name: None,
                id: "clip-a".to_string(),
                name: "audio/intro.wav".to_string(),
                kind: Kind::Audio,
                track_selected: false,
            }]
        );
    }

    #[test]
    fn timeline_clip_dedupes_matching_live_slot_reference() {
        let mut tracks = vec![track("Synth")];
        tracks[0].midi.clips.push(MIDIClip {
            id: "clip-m".to_string(),
            name: "midi/synth.mid".to_string(),
            ..MIDIClip::default()
        });
        let mut session = SessionMatrix::default();
        session.ensure_track_slots("Synth");
        let slot = session.slot_mut("Synth", 0).unwrap();
        slot.clip = Some(slot_ref("clip-m"));
        slot.clip_name = Some("slot fallback".to_string());
        let mut seen = HashSet::new();

        let rows =
            unique_track_clip_entries(&tracks[0], &tracks, &session, &[], &[], true, &mut seen);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].source_track_name.as_deref(), Some("Synth"));
        assert_eq!(rows[0].id, "clip-m");
        assert_eq!(rows[0].kind, Kind::MIDI);
        assert!(rows[0].track_selected);
    }
}
