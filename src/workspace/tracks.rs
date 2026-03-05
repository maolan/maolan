use crate::{
    message::{Message, TrackAutomationTarget},
    state::State,
    style,
};
use iced::{
    Background, Border, Color, Element, Length,
    widget::{Column, Space, button, column, container, mouse_area, row, text},
};
use iced_aw::ContextMenu;
use iced_drop::droppable;
use iced_fonts::lucide::{audio_waveform, disc};
use maolan_engine::message::{Action, TrackMidiLearnTarget};
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct Tracks {
    state: State,
}

impl Tracks {
    pub fn new(state: State) -> Self {
        Self { state }
    }

    fn trim_with_ellipsis(value: &str, max_chars: usize) -> String {
        if value.chars().count() <= max_chars {
            return value.to_string();
        }
        if max_chars <= 3 {
            return "...".to_string();
        }
        let trimmed: String = value.chars().take(max_chars - 3).collect();
        format!("{trimmed}...")
    }

    pub fn view(&self) -> Element<'_, Message> {
        let (tracks, selected, width, hovered_resize_track) = {
            let state = self.state.blocking_read();
            (
                state.tracks.clone(),
                state.selected.clone(),
                state.tracks_width,
                state.hovered_track_resize_handle.clone(),
            )
        };
        let track_width_px = match width {
            Length::Fixed(v) => v,
            _ => 200.0,
        };
        let track_names: Vec<String> = tracks.iter().map(|t| t.name.clone()).collect();
        let mut vca_follower_counts: HashMap<String, usize> = HashMap::new();
        for track in &tracks {
            if let Some(master) = track.vca_master.as_ref() {
                *vca_follower_counts.entry(master.clone()).or_default() += 1;
            }
        }

        let result = Column::with_children(tracks.into_iter().enumerate().map(|(index, track)| {
            let selected = selected.contains(&track.name);
            let height = track.height;
            let is_resize_hovered = hovered_resize_track.as_deref() == Some(track.name.as_str());
            let vca_follower_count = vca_follower_counts.get(&track.name).copied().unwrap_or(0);
            let vca_master_label = track.vca_master.clone();
            let midi_learn_vol = track.midi_learn_volume.clone();
            let midi_learn_bal = track.midi_learn_balance.clone();
            let midi_learn_mute = track.midi_learn_mute.clone();
            let midi_learn_solo = track.midi_learn_solo.clone();
            let midi_learn_arm = track.midi_learn_arm.clone();
            let midi_learn_input_monitor = track.midi_learn_input_monitor.clone();
            let midi_learn_disk_monitor = track.midi_learn_disk_monitor.clone();
            let aux_sends = track.aux_sends.clone();
            let vca_candidates: Vec<String> = track_names
                .iter()
                .filter(|candidate| **candidate != track.name)
                .cloned()
                .collect();
            let layout = track.lane_layout();
            let lane_h = layout.lane_height.max(12.0);
            let has_visible_automation = track.automation_lanes.iter().any(|lane| lane.visible);
            let mut lane_rows: Column<'_, Message> = column![];
            for lane in track.automation_lanes.iter().filter(|lane| lane.visible) {
                lane_rows = lane_rows.push(
                    container(text(format!("Auto {}", lane.target)).size(11))
                        .width(Length::Fill)
                        .height(Length::Fixed(lane_h))
                        .padding(4)
                        .style(|_theme| container::Style {
                            background: Some(Background::Color(Color {
                                r: 0.28,
                                g: 0.2,
                                b: 0.12,
                                a: 0.55,
                            })),
                            ..container::Style::default()
                        }),
                );
            }

            let max_name_chars = (((track_width_px - 90.0) / 7.0).floor() as i32).clamp(8, 64);
            let mut title = format!(
                "▾ {}{}",
                Self::trim_with_ellipsis(&track.name, max_name_chars as usize),
                if track.frozen { " [FRZ]" } else { "" }
            );
            if let Some(master) = vca_master_label.as_ref() {
                title.push_str(&format!(" [VCA<-{}]", master));
            }
            if vca_follower_count > 0 {
                title.push_str(&format!(" [VCA x{}]", vca_follower_count));
            }
            if let Some(binding) = midi_learn_vol.as_ref() {
                title.push_str(&format!(" [CC{}:{}->Vol]", binding.channel + 1, binding.cc));
            }
            if let Some(binding) = midi_learn_bal.as_ref() {
                title.push_str(&format!(" [CC{}:{}->Bal]", binding.channel + 1, binding.cc));
            }
            if let Some(binding) = midi_learn_mute.as_ref() {
                title.push_str(&format!(
                    " [CC{}:{}->Mute]",
                    binding.channel + 1,
                    binding.cc
                ));
            }
            if let Some(binding) = midi_learn_solo.as_ref() {
                title.push_str(&format!(
                    " [CC{}:{}->Solo]",
                    binding.channel + 1,
                    binding.cc
                ));
            }
            if let Some(binding) = midi_learn_arm.as_ref() {
                title.push_str(&format!(" [CC{}:{}->Arm]", binding.channel + 1, binding.cc));
            }
            if let Some(binding) = midi_learn_input_monitor.as_ref() {
                title.push_str(&format!(
                    " [CC{}:{}->InMon]",
                    binding.channel + 1,
                    binding.cc
                ));
            }
            if let Some(binding) = midi_learn_disk_monitor.as_ref() {
                title.push_str(&format!(
                    " [CC{}:{}->Disk]",
                    binding.channel + 1,
                    binding.cc
                ));
            }

            let track_controls = row![
                button("R")
                    .padding(3)
                    .style(move |theme, _state| { style::arm::style(theme, track.armed) })
                    .on_press(Message::Request(Action::TrackToggleArm(track.name.clone()))),
                button("M")
                    .padding(3)
                    .style(move |theme, _state| { style::mute::style(theme, track.muted) })
                    .on_press(Message::Request(Action::TrackToggleMute(
                        track.name.clone()
                    ))),
                button("S")
                    .padding(3)
                    .style(move |theme, _state| { style::solo::style(theme, track.soloed) })
                    .on_press(Message::Request(Action::TrackToggleSolo(
                        track.name.clone()
                    ))),
                button(audio_waveform())
                    .padding(3)
                    .style(move |theme, _state| { style::input::style(theme, track.input_monitor) })
                    .on_press(Message::Request(Action::TrackToggleInputMonitor(
                        track.name.clone()
                    ))),
                button(disc())
                    .padding(3)
                    .style(move |theme, _state| { style::disk::style(theme, track.disk_monitor) })
                    .on_press(Message::Request(Action::TrackToggleDiskMonitor(
                        track.name.clone()
                    ))),
            ]
            .spacing(4.0);

            let track_ui: Column<'_, Message> = column![
                row![
                    text(title),
                    Space::new().width(Length::Fill),
                ]
                .height(Length::Fixed(layout.header_height)),
                track_controls,
                lane_rows.height(Length::Fill),
                mouse_area(
                    container("")
                        .width(Length::Fill)
                        .height(Length::Fixed(3.0))
                        .style(move |_theme| {
                            use container::Style;
                            Style {
                                background: Some(Background::Color(Color {
                                    r: 0.5,
                                    g: 0.5,
                                    b: 0.5,
                                    a: if is_resize_hovered { 0.8 } else { 0.5 },
                                })),
                                ..Style::default()
                            }
                        }),
                )
                .on_enter(Message::TrackResizeHover(track.name.clone(), true))
                .on_exit(Message::TrackResizeHover(track.name.clone(), false))
                .on_press(Message::TrackResizeStart(track.name.clone())),
            ]
            .spacing(2.0);

            {
                let track_name_for_menu = track.name.clone();
                let track_is_frozen = track.frozen;
                let track_has_visible_automation = has_visible_automation;
                let track_automation_mode = track.automation_mode;
                let track_vca_master = track.vca_master.clone();
                let track_vca_candidates = vca_candidates.clone();
                let track_midi_learn_vol = track.midi_learn_volume.clone();
                let track_midi_learn_bal = track.midi_learn_balance.clone();
                let track_midi_learn_mute = track.midi_learn_mute.clone();
                let track_midi_learn_solo = track.midi_learn_solo.clone();
                let track_midi_learn_arm = track.midi_learn_arm.clone();
                let track_midi_learn_input_monitor = track.midi_learn_input_monitor.clone();
                let track_midi_learn_disk_monitor = track.midi_learn_disk_monitor.clone();
                let track_with_mouse = mouse_area(
                    container(track_ui)
                        .id(track.name.clone())
                        .width(Length::Fill)
                        .height(Length::Fixed(height))
                        .padding(5)
                        .style(move |_theme| container::Style {
                            background: if selected {
                                Some(Background::Color(Color {
                                    r: 1.0,
                                    g: 1.0,
                                    b: 1.0,
                                    a: 0.1,
                                }))
                            } else {
                                Some(Background::Color(Color {
                                    r: 0.0,
                                    g: 0.0,
                                    b: 0.0,
                                    a: 0.0,
                                }))
                            },
                            border: Border {
                                color: Color {
                                    r: 0.0,
                                    g: 0.0,
                                    b: 0.0,
                                    a: 1.0,
                                },
                                width: 1.0,
                                radius: 5.0.into(),
                            },
                            ..container::Style::default()
                        }),
                )
                .on_press(Message::SelectTrack(track.name.clone()))
                .on_double_click(Message::OpenTrackPlugins(track.name.clone()));

                let track_with_context = ContextMenu::new(track_with_mouse, move || {
                    let mut menu = column![
                        button("Automation: Volume").on_press(Message::TrackAutomationAddLane {
                            track_name: track_name_for_menu.clone(),
                            target: TrackAutomationTarget::Volume,
                        }),
                        button("Automation: Balance").on_press(Message::TrackAutomationAddLane {
                            track_name: track_name_for_menu.clone(),
                            target: TrackAutomationTarget::Balance,
                        }),
                        button("Automation: Mute").on_press(Message::TrackAutomationAddLane {
                            track_name: track_name_for_menu.clone(),
                            target: TrackAutomationTarget::Mute,
                        }),
                        button("Rename")
                            .on_press(Message::TrackRenameShow(track_name_for_menu.clone())),
                        button(if track_has_visible_automation {
                            "Hide Automation (A-)"
                        } else {
                            "Show Automation (A+)"
                        })
                        .on_press(Message::TrackAutomationToggle {
                            track_name: track_name_for_menu.clone(),
                        }),
                        button(text(format!(
                            "Automation Mode: {}",
                            track_automation_mode
                        )))
                        .on_press(Message::TrackAutomationCycleMode {
                            track_name: track_name_for_menu.clone(),
                        }),
                        button(if track_is_frozen {
                            "Unfreeze"
                        } else {
                            "Freeze"
                        })
                        .on_press(Message::TrackFreezeToggle {
                            track_name: track_name_for_menu.clone(),
                        }),
                        if track_is_frozen {
                            button("Flatten").on_press(Message::TrackFreezeFlatten {
                                track_name: track_name_for_menu.clone(),
                            })
                        } else {
                            button("Flatten")
                        },
                        button("Save as template")
                            .on_press(Message::TrackTemplateSaveShow(track_name_for_menu.clone())),
                    ];
                    if let Some(master) = track_vca_master.as_ref() {
                        menu =
                            menu.push(button(text(format!("VCA: Unassign ({master})"))).on_press(
                                Message::TrackSetVcaMaster {
                                    track_name: track_name_for_menu.clone(),
                                    master_track: None,
                                },
                            ));
                    }
                    for send in &aux_sends {
                        menu = menu.push(
                            button(text(format!(
                                "Send {} Level -1dB ({:.1})",
                                send.aux_track, send.level_db
                            )))
                            .on_press(
                                Message::TrackAuxSendLevelAdjust {
                                    track_name: track_name_for_menu.clone(),
                                    aux_track: send.aux_track.clone(),
                                    delta_db: -1.0,
                                },
                            ),
                        );
                        menu = menu.push(
                            button(text(format!(
                                "Send {} Level +1dB ({:.1})",
                                send.aux_track, send.level_db
                            )))
                            .on_press(
                                Message::TrackAuxSendLevelAdjust {
                                    track_name: track_name_for_menu.clone(),
                                    aux_track: send.aux_track.clone(),
                                    delta_db: 1.0,
                                },
                            ),
                        );
                        menu = menu.push(
                            button(text(format!(
                                "Send {} Pan L ({:.2})",
                                send.aux_track, send.pan
                            )))
                            .on_press(
                                Message::TrackAuxSendPanAdjust {
                                    track_name: track_name_for_menu.clone(),
                                    aux_track: send.aux_track.clone(),
                                    delta: -0.1,
                                },
                            ),
                        );
                        menu = menu.push(
                            button(text(format!(
                                "Send {} Pan R ({:.2})",
                                send.aux_track, send.pan
                            )))
                            .on_press(
                                Message::TrackAuxSendPanAdjust {
                                    track_name: track_name_for_menu.clone(),
                                    aux_track: send.aux_track.clone(),
                                    delta: 0.1,
                                },
                            ),
                        );
                        menu = menu.push(
                            button(text(format!(
                                "Send {} Mode: {}",
                                send.aux_track,
                                if send.pre_fader {
                                    "Pre-Fader"
                                } else {
                                    "Post-Fader"
                                }
                            )))
                            .on_press(
                                Message::TrackAuxSendTogglePrePost {
                                    track_name: track_name_for_menu.clone(),
                                    aux_track: send.aux_track.clone(),
                                },
                            ),
                        );
                    }
                    menu = menu.push(button("MIDI Learn Volume").on_press(
                        Message::TrackMidiLearnArm {
                            track_name: track_name_for_menu.clone(),
                            target: TrackMidiLearnTarget::Volume,
                        },
                    ));
                    menu = menu.push(button("MIDI Learn Balance").on_press(
                        Message::TrackMidiLearnArm {
                            track_name: track_name_for_menu.clone(),
                            target: TrackMidiLearnTarget::Balance,
                        },
                    ));
                    menu = menu.push(button("MIDI Learn Mute").on_press(
                        Message::TrackMidiLearnArm {
                            track_name: track_name_for_menu.clone(),
                            target: TrackMidiLearnTarget::Mute,
                        },
                    ));
                    menu = menu.push(button("MIDI Learn Solo").on_press(
                        Message::TrackMidiLearnArm {
                            track_name: track_name_for_menu.clone(),
                            target: TrackMidiLearnTarget::Solo,
                        },
                    ));
                    menu = menu.push(button("MIDI Learn Arm").on_press(
                        Message::TrackMidiLearnArm {
                            track_name: track_name_for_menu.clone(),
                            target: TrackMidiLearnTarget::Arm,
                        },
                    ));
                    menu = menu.push(button("MIDI Learn Input Monitor").on_press(
                        Message::TrackMidiLearnArm {
                            track_name: track_name_for_menu.clone(),
                            target: TrackMidiLearnTarget::InputMonitor,
                        },
                    ));
                    menu = menu.push(button("MIDI Learn Disk Monitor").on_press(
                        Message::TrackMidiLearnArm {
                            track_name: track_name_for_menu.clone(),
                            target: TrackMidiLearnTarget::DiskMonitor,
                        },
                    ));
                    if track_midi_learn_vol.is_some() {
                        menu = menu.push(button("Clear MIDI Learn Volume").on_press(
                            Message::TrackMidiLearnClear {
                                track_name: track_name_for_menu.clone(),
                                target: TrackMidiLearnTarget::Volume,
                            },
                        ));
                    }
                    if track_midi_learn_bal.is_some() {
                        menu = menu.push(button("Clear MIDI Learn Balance").on_press(
                            Message::TrackMidiLearnClear {
                                track_name: track_name_for_menu.clone(),
                                target: TrackMidiLearnTarget::Balance,
                            },
                        ));
                    }
                    if track_midi_learn_mute.is_some() {
                        menu = menu.push(button("Clear MIDI Learn Mute").on_press(
                            Message::TrackMidiLearnClear {
                                track_name: track_name_for_menu.clone(),
                                target: TrackMidiLearnTarget::Mute,
                            },
                        ));
                    }
                    if track_midi_learn_solo.is_some() {
                        menu = menu.push(button("Clear MIDI Learn Solo").on_press(
                            Message::TrackMidiLearnClear {
                                track_name: track_name_for_menu.clone(),
                                target: TrackMidiLearnTarget::Solo,
                            },
                        ));
                    }
                    if track_midi_learn_arm.is_some() {
                        menu = menu.push(button("Clear MIDI Learn Arm").on_press(
                            Message::TrackMidiLearnClear {
                                track_name: track_name_for_menu.clone(),
                                target: TrackMidiLearnTarget::Arm,
                            },
                        ));
                    }
                    if track_midi_learn_input_monitor.is_some() {
                        menu = menu.push(button("Clear MIDI Learn Input Monitor").on_press(
                            Message::TrackMidiLearnClear {
                                track_name: track_name_for_menu.clone(),
                                target: TrackMidiLearnTarget::InputMonitor,
                            },
                        ));
                    }
                    if track_midi_learn_disk_monitor.is_some() {
                        menu = menu.push(button("Clear MIDI Learn Disk Monitor").on_press(
                            Message::TrackMidiLearnClear {
                                track_name: track_name_for_menu.clone(),
                                target: TrackMidiLearnTarget::DiskMonitor,
                            },
                        ));
                    }
                    for master in &track_vca_candidates {
                        menu = menu.push(button(text(format!("VCA -> {master}"))).on_press(
                            Message::TrackSetVcaMaster {
                                track_name: track_name_for_menu.clone(),
                                master_track: Some(master.clone()),
                            },
                        ));
                    }
                    menu.into()
                });

                droppable(track_with_context)
                    .on_drag(move |_, _| Message::TrackDrag(index))
                    .on_drop(Message::TrackDropped)
                    .into()
            }
        }));
        mouse_area(result.width(width))
            .on_press(Message::DeselectAll)
            .into()
    }
}
