use super::VisibleTrackWindow;
use crate::{
    consts::state_ids::METRONOME_TRACK_ID,
    consts::state_track::TRACK_SUBTRACK_GAP,
    menu,
    message::{Message, MidiLaneChannelSelection, TrackAutomationTarget},
    state::{State, StateData, TrackLaneLayout},
    style,
};
use iced::{
    Alignment, Background, Border, Color, Element, Length, Point, Theme,
    alignment::Horizontal,
    widget::{Column, Space, button, column, container, lazy, mouse_area, pick_list, row, text},
};
use iced_drop::droppable;
use iced_fonts::lucide::{audio_waveform, disc};
use maolan_engine::message::{Action, TrackMidiLearnTarget};
use std::hash::{Hash, Hasher};

#[derive(Debug, Default)]
pub struct Tracks {
    state: State,
}

#[derive(Clone)]
struct VisibleAutomationLane {
    target: TrackAutomationTarget,
    points_len: usize,
}

#[derive(Clone)]
struct TrackViewData {
    name: String,
    height: f32,
    layout: TrackLaneLayout,
    selected: bool,
    resize_hovered: bool,
    armed: bool,
    frozen: bool,
    muted: bool,
    soloed: bool,
    input_monitor: bool,
    disk_monitor: bool,
    audio_ins: usize,
    audio_outs: usize,
    primary_audio_ins: usize,
    primary_audio_outs: usize,
    midi_ins: usize,
    midi_outs: usize,
    midi_lane_channels: Vec<Option<u8>>,
    balance: f32,
    automation_mode: String,
    visible_automation_lanes: Vec<VisibleAutomationLane>,
    midi_learn_vol: bool,
    midi_learn_bal: bool,
    midi_learn_mute: bool,
    midi_learn_solo: bool,
    midi_learn_arm: bool,
    midi_learn_input_monitor: bool,
    midi_learn_disk_monitor: bool,
    vca_master: Option<String>,
}

#[derive(Clone)]
struct VcaStripViewData {
    label: String,
    prev_same: bool,
    next_same: bool,
}

pub(super) fn track_context_menu_overlay(
    state: &StateData,
) -> Option<(Point, Element<'static, Message>)> {
    let menu_state = state.track_context_menu.as_ref()?;
    let track = state
        .tracks
        .iter()
        .find(|track| track.name == menu_state.track_name)?;

    let mut top = 0.0_f32;
    for t in state
        .tracks
        .iter()
        .filter(|track| track.name != METRONOME_TRACK_ID)
    {
        if t.name == track.name {
            break;
        }
        top += t.height;
    }

    let track_name = track.name.clone();
    let has_visible_automation = track.automation_lanes.iter().any(|lane| lane.visible);
    let freeze_supported = track.midi.ins == 0;
    let mut items = vec![
        menu::menu_item(
            "Automation: Volume",
            Message::TrackAutomationAddLane {
                track_name: track_name.clone(),
                target: TrackAutomationTarget::Volume,
            },
        ),
        menu::menu_item(
            "Automation: Balance",
            Message::TrackAutomationAddLane {
                track_name: track_name.clone(),
                target: TrackAutomationTarget::Balance,
            },
        ),
        menu::menu_item(
            "Automation: Mute",
            Message::TrackAutomationAddLane {
                track_name: track_name.clone(),
                target: TrackAutomationTarget::Mute,
            },
        ),
        menu::menu_item("Rename", Message::TrackRenameShow(track_name.clone())),
        menu::menu_item(
            if has_visible_automation {
                "Hide Automation (A-)"
            } else {
                "Show Automation (A+)"
            },
            Message::TrackAutomationToggle {
                track_name: track_name.clone(),
            },
        ),
        menu::menu_item(
            format!("Automation Mode: {}", track.automation_mode),
            Message::TrackAutomationCycleMode {
                track_name: track_name.clone(),
            },
        ),
        menu::menu_item(
            "Save as template",
            Message::TrackTemplateSaveShow(track_name.clone()),
        ),
        menu::menu_item("Add Return", Message::TrackAddReturn(track_name.clone())),
        menu::menu_item("Add Send", Message::TrackAddSend(track_name.clone())),
        menu::menu_item(
            "MIDI Learn Volume",
            Message::TrackMidiLearnArm {
                track_name: track_name.clone(),
                target: TrackMidiLearnTarget::Volume,
            },
        ),
        menu::menu_item(
            "MIDI Learn Balance",
            Message::TrackMidiLearnArm {
                track_name: track_name.clone(),
                target: TrackMidiLearnTarget::Balance,
            },
        ),
        menu::menu_item(
            "MIDI Learn Mute",
            Message::TrackMidiLearnArm {
                track_name: track_name.clone(),
                target: TrackMidiLearnTarget::Mute,
            },
        ),
        menu::menu_item(
            "MIDI Learn Solo",
            Message::TrackMidiLearnArm {
                track_name: track_name.clone(),
                target: TrackMidiLearnTarget::Solo,
            },
        ),
        menu::menu_item(
            "MIDI Learn Arm",
            Message::TrackMidiLearnArm {
                track_name: track_name.clone(),
                target: TrackMidiLearnTarget::Arm,
            },
        ),
        menu::menu_item(
            "MIDI Learn Input Monitor",
            Message::TrackMidiLearnArm {
                track_name: track_name.clone(),
                target: TrackMidiLearnTarget::InputMonitor,
            },
        ),
        menu::menu_item(
            "MIDI Learn Disk Monitor",
            Message::TrackMidiLearnArm {
                track_name: track_name.clone(),
                target: TrackMidiLearnTarget::DiskMonitor,
            },
        ),
    ];

    if freeze_supported {
        items.push(menu::menu_item(
            if track.frozen { "Unfreeze" } else { "Freeze" },
            Message::TrackFreezeToggle {
                track_name: track_name.clone(),
            },
        ));
        if track.frozen {
            items.push(menu::menu_item(
                "Flatten",
                Message::TrackFreezeFlatten {
                    track_name: track_name.clone(),
                },
            ));
        }
    }

    let selected_tracks: Vec<_> = state
        .tracks
        .iter()
        .filter(|candidate| state.selected.contains(candidate.name.as_str()))
        .map(|candidate| candidate.name.clone())
        .collect();
    let can_group_selected =
        selected_tracks.len() > 1 && selected_tracks.iter().any(|name| name == &track.name);

    if can_group_selected {
        items.push(menu::menu_item(
            "Group",
            Message::TrackGroupShow {
                track_name: track_name.clone(),
            },
        ));
    }

    if let Some(master) = track.vca_master.as_ref() {
        items.push(menu::menu_item(
            format!("Group: Ungroup ({master})"),
            Message::TrackSetVcaMaster {
                track_name: track_name.clone(),
                master_track: None,
            },
        ));
    }

    if track.midi_learn_volume.is_some() {
        items.push(menu::menu_item(
            "Clear MIDI Learn Volume",
            Message::TrackMidiLearnClear {
                track_name: track_name.clone(),
                target: TrackMidiLearnTarget::Volume,
            },
        ));
    }
    if track.midi_learn_balance.is_some() {
        items.push(menu::menu_item(
            "Clear MIDI Learn Balance",
            Message::TrackMidiLearnClear {
                track_name: track_name.clone(),
                target: TrackMidiLearnTarget::Balance,
            },
        ));
    }
    if track.midi_learn_mute.is_some() {
        items.push(menu::menu_item(
            "Clear MIDI Learn Mute",
            Message::TrackMidiLearnClear {
                track_name: track_name.clone(),
                target: TrackMidiLearnTarget::Mute,
            },
        ));
    }
    if track.midi_learn_solo.is_some() {
        items.push(menu::menu_item(
            "Clear MIDI Learn Solo",
            Message::TrackMidiLearnClear {
                track_name: track_name.clone(),
                target: TrackMidiLearnTarget::Solo,
            },
        ));
    }
    if track.midi_learn_arm.is_some() {
        items.push(menu::menu_item(
            "Clear MIDI Learn Arm",
            Message::TrackMidiLearnClear {
                track_name: track_name.clone(),
                target: TrackMidiLearnTarget::Arm,
            },
        ));
    }
    if track.midi_learn_input_monitor.is_some() {
        items.push(menu::menu_item(
            "Clear MIDI Learn Input Monitor",
            Message::TrackMidiLearnClear {
                track_name: track_name.clone(),
                target: TrackMidiLearnTarget::InputMonitor,
            },
        ));
    }
    if track.midi_learn_disk_monitor.is_some() {
        items.push(menu::menu_item(
            "Clear MIDI Learn Disk Monitor",
            Message::TrackMidiLearnClear {
                track_name: track_name.clone(),
                target: TrackMidiLearnTarget::DiskMonitor,
            },
        ));
    }

    let panel = container(Column::with_children(items).spacing(2))
        .width(Length::Fill)
        .padding(6)
        .style(|theme| {
            let palette = theme.extended_palette();
            container::Style {
                background: Some(Background::Color(palette.background.weak.color)),
                border: Border {
                    color: palette.background.strong.color,
                    width: 1.0,
                    radius: 6.0.into(),
                },
                ..container::Style::default()
            }
        })
        .into();

    Some((
        Point::new(
            menu_state.anchor.x.max(0.0),
            (top + menu_state.anchor.y).max(0.0),
        ),
        panel,
    ))
}

impl Tracks {
    pub fn new(state: State) -> Self {
        Self { state }
    }

    fn trim_with_ellipsis(value: &str, max_chars: usize) -> String {
        if value.chars().count() <= max_chars {
            return value.to_string();
        }
        value.chars().take(max_chars).collect()
    }

    fn rotate_text_cw(value: &str, max_chars: usize) -> String {
        Self::trim_with_ellipsis(value, max_chars)
            .chars()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn char_slice(value: &str, start: usize, len: usize) -> String {
        value.chars().skip(start).take(len).collect()
    }

    fn format_balance(balance: f32) -> String {
        let balance = balance.clamp(-1.0, 1.0);
        if balance.abs() < 0.005 {
            "C".to_string()
        } else if balance < 0.0 {
            format!("L{}", (-balance * 100.0).round() as i32)
        } else {
            format!("R{}", (balance * 100.0).round() as i32)
        }
    }

    fn track_row_render_hash(
        track: &TrackViewData,
        track_width_px: f32,
        row_index: usize,
        vca: Option<&VcaStripViewData>,
    ) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        row_index.hash(&mut hasher);
        track_width_px.to_bits().hash(&mut hasher);
        track.name.hash(&mut hasher);
        track.height.to_bits().hash(&mut hasher);
        track.layout.header_height.to_bits().hash(&mut hasher);
        track.layout.lane_height.to_bits().hash(&mut hasher);
        track.layout.audio_lanes.hash(&mut hasher);
        track.layout.midi_lanes.hash(&mut hasher);
        track.selected.hash(&mut hasher);
        track.resize_hovered.hash(&mut hasher);
        track.armed.hash(&mut hasher);
        track.frozen.hash(&mut hasher);
        track.muted.hash(&mut hasher);
        track.soloed.hash(&mut hasher);
        track.input_monitor.hash(&mut hasher);
        track.disk_monitor.hash(&mut hasher);
        track.audio_ins.hash(&mut hasher);
        track.audio_outs.hash(&mut hasher);
        track.primary_audio_ins.hash(&mut hasher);
        track.primary_audio_outs.hash(&mut hasher);
        track.midi_ins.hash(&mut hasher);
        track.midi_outs.hash(&mut hasher);
        track.midi_lane_channels.hash(&mut hasher);
        track.balance.to_bits().hash(&mut hasher);
        track.automation_mode.hash(&mut hasher);
        track.midi_learn_vol.hash(&mut hasher);
        track.midi_learn_bal.hash(&mut hasher);
        track.midi_learn_mute.hash(&mut hasher);
        track.midi_learn_solo.hash(&mut hasher);
        track.midi_learn_arm.hash(&mut hasher);
        track.midi_learn_input_monitor.hash(&mut hasher);
        track.midi_learn_disk_monitor.hash(&mut hasher);
        track.visible_automation_lanes.len().hash(&mut hasher);
        for lane in &track.visible_automation_lanes {
            std::mem::discriminant(&lane.target).hash(&mut hasher);
            match lane.target {
                TrackAutomationTarget::Volume
                | TrackAutomationTarget::Balance
                | TrackAutomationTarget::Mute => {}
                TrackAutomationTarget::Lv2Parameter {
                    instance_id,
                    index,
                    min,
                    max,
                } => {
                    instance_id.hash(&mut hasher);
                    index.hash(&mut hasher);
                    min.to_bits().hash(&mut hasher);
                    max.to_bits().hash(&mut hasher);
                }
                TrackAutomationTarget::Vst3Parameter {
                    instance_id,
                    param_id,
                } => {
                    instance_id.hash(&mut hasher);
                    param_id.hash(&mut hasher);
                }
                TrackAutomationTarget::ClapParameter {
                    instance_id,
                    param_id,
                    min,
                    max,
                } => {
                    instance_id.hash(&mut hasher);
                    param_id.hash(&mut hasher);
                    min.to_bits().hash(&mut hasher);
                    max.to_bits().hash(&mut hasher);
                }
            }
            lane.points_len.hash(&mut hasher);
        }
        if let Some(vca) = vca {
            vca.label.hash(&mut hasher);
            vca.prev_same.hash(&mut hasher);
            vca.next_same.hash(&mut hasher);
        }
        hasher.finish()
    }

    fn info_badge(label: String, accent: bool) -> Element<'static, Message> {
        container(text(label).size(9))
            .padding([2, 5])
            .style(move |_theme| container::Style {
                background: Some(Background::Color(if accent {
                    Color::from_rgba(0.34, 0.48, 0.69, 0.88)
                } else {
                    Color::from_rgba(0.16, 0.19, 0.25, 0.95)
                })),
                border: Border {
                    color: Color::from_rgba(0.62, 0.74, 0.9, if accent { 0.55 } else { 0.18 }),
                    width: 1.0,
                    radius: 5.0.into(),
                },
                text_color: Some(if accent {
                    Color::WHITE
                } else {
                    Color::from_rgb(0.82, 0.86, 0.93)
                }),
                ..container::Style::default()
            })
            .into()
    }

    fn render_track_row(
        track: TrackViewData,
        track_width_px: f32,
        index: usize,
        vca: Option<VcaStripViewData>,
    ) -> Element<'static, Message> {
        let selected = track.selected;
        let height = track.height;
        let is_resize_hovered = track.resize_hovered;
        let midi_learn_vol = track.midi_learn_vol;
        let midi_learn_bal = track.midi_learn_bal;
        let midi_learn_mute = track.midi_learn_mute;
        let midi_learn_solo = track.midi_learn_solo;
        let midi_learn_arm = track.midi_learn_arm;
        let midi_learn_input_monitor = track.midi_learn_input_monitor;
        let midi_learn_disk_monitor = track.midi_learn_disk_monitor;
        let layout = track.layout;
        let lane_h = layout.lane_height.max(12.0);
        let has_visible_automation = !track.visible_automation_lanes.is_empty();
        let inline_midi_lane_selector = track.audio_ins == 0 && track.midi_ins > 0;
        let max_name_chars = (((track_width_px - 98.0) / 7.0).floor() as i32).clamp(10, 64);
        let learn_count = [
            midi_learn_vol,
            midi_learn_bal,
            midi_learn_mute,
            midi_learn_solo,
            midi_learn_arm,
            midi_learn_input_monitor,
            midi_learn_disk_monitor,
        ]
        .iter()
        .filter(|bound| **bound)
        .count();
        let resize_handle_height = 6.0;
        let outer_spacing = 6.0;
        let inner_vertical_padding = 8.0;
        let lane_row_count = track
            .midi_ins
            .saturating_sub(usize::from(inline_midi_lane_selector))
            + track.visible_automation_lanes.len();
        let lane_rows_height = if lane_row_count > 0 {
            lane_row_count as f32 * lane_h
                + lane_row_count.saturating_sub(1) as f32 * TRACK_SUBTRACK_GAP
        } else {
            0.0
        };
        let inner_available_height = (height - inner_vertical_padding).max(16.0);
        let body_height = (inner_available_height
            - layout.header_height
            - resize_handle_height
            - outer_spacing
            - lane_rows_height)
            .max(0.0);
        let mut title_badges = row![].spacing(4).align_y(Alignment::Center);
        if track.frozen {
            title_badges = title_badges.push(Self::info_badge("FRZ".to_string(), false));
        }
        if learn_count > 0 {
            title_badges =
                title_badges.push(Self::info_badge(format!("CC {}", learn_count), false));
        }

        let header = mouse_area(
            container(
                row![
                    text(Self::trim_with_ellipsis(
                        &track.name,
                        max_name_chars as usize
                    ))
                    .size(13),
                    Space::new().width(Length::Fill),
                    title_badges,
                ]
                .align_y(Alignment::Center)
                .spacing(6),
            )
            .height(Length::Fixed(layout.header_height))
            .padding([2, 6])
            .style(move |_theme| container::Style {
                background: Some(Background::Color(if selected {
                    Color::from_rgba(0.28, 0.39, 0.56, 0.98)
                } else {
                    Color::from_rgba(0.18, 0.22, 0.30, 0.96)
                })),
                border: Border {
                    color: Color::from_rgba(0.78, 0.87, 0.99, if selected { 0.5 } else { 0.16 }),
                    width: 1.0,
                    radius: 7.0.into(),
                },
                text_color: Some(Color::from_rgb(0.92, 0.95, 1.0)),
                ..container::Style::default()
            }),
        )
        .on_press(Message::SelectTrack(track.name.clone()))
        .on_double_click(Message::OpenTrackPlugins(track.name.clone()));

        let track_name = track.name.clone();
        let controls = row![
            button(
                container(text("R").size(13))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .center_x(Length::Fill)
                    .center_y(Length::Fill)
            )
            .width(Length::Fixed(22.0))
            .height(Length::Fixed(22.0))
            .padding(0)
            .style(move |theme, _state| style::arm::style(theme, track.armed))
            .on_press(Message::Request(Action::TrackToggleArm(track_name.clone()))),
            button(
                container(text("M").size(13))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .center_x(Length::Fill)
                    .center_y(Length::Fill)
            )
            .width(Length::Fixed(22.0))
            .height(Length::Fixed(22.0))
            .padding(0)
            .style(move |theme, _state| style::mute::style(theme, track.muted))
            .on_press(Message::Request(Action::TrackToggleMute(
                track.name.clone()
            ))),
            button(
                container(text("S").size(13))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .center_x(Length::Fill)
                    .center_y(Length::Fill)
            )
            .width(Length::Fixed(22.0))
            .height(Length::Fixed(22.0))
            .padding(0)
            .style(move |theme, _state| style::solo::style(theme, track.soloed))
            .on_press(Message::Request(Action::TrackToggleSolo(
                track.name.clone()
            ))),
            button(
                container(audio_waveform().size(13))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .center_x(Length::Fill)
                    .center_y(Length::Fill)
            )
            .width(Length::Fixed(22.0))
            .height(Length::Fixed(22.0))
            .padding(0)
            .style(move |theme, _state| style::input::style(theme, track.input_monitor))
            .on_press(Message::Request(Action::TrackToggleInputMonitor(
                track.name.clone(),
            ))),
            button(
                container(disc().size(13))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .center_x(Length::Fill)
                    .center_y(Length::Fill)
            )
            .width(Length::Fixed(22.0))
            .height(Length::Fixed(22.0))
            .padding(0)
            .style(move |theme, _state| style::disk::style(theme, track.disk_monitor))
            .on_press(Message::Request(Action::TrackToggleDiskMonitor(
                track.name.clone(),
            ))),
        ]
        .spacing(4)
        .align_y(Alignment::Center);

        let inline_midi_channel: Element<'static, Message> = if inline_midi_lane_selector {
            let track_name = track.name.clone();
            let selected_channel = MidiLaneChannelSelection::from_engine(
                track.midi_lane_channels.first().copied().flatten(),
            );
            container(
                pick_list(
                    MidiLaneChannelSelection::ALL,
                    Some(selected_channel),
                    move |channel| Message::TrackMidiLaneChannelSelected {
                        track_name: track_name.clone(),
                        lane: 0,
                        channel,
                    },
                )
                .placeholder("Channel"),
            )
            .padding([0, 2])
            .into()
        } else {
            Space::new().into()
        };

        let mut lane_rows: Column<'static, Message> = column![];
        for lane_index in 0..track.midi_ins {
            if inline_midi_lane_selector && lane_index == 0 {
                continue;
            }
            let track_name = track.name.clone();
            let selected_channel = MidiLaneChannelSelection::from_engine(
                track.midi_lane_channels.get(lane_index).copied().flatten(),
            );
            lane_rows = lane_rows.push(
                container(
                    row![
                        Self::info_badge(format!("MIDI {}", lane_index + 1), true),
                        Space::new().width(Length::Fill),
                        pick_list(
                            MidiLaneChannelSelection::ALL,
                            Some(selected_channel),
                            move |channel| Message::TrackMidiLaneChannelSelected {
                                track_name: track_name.clone(),
                                lane: lane_index,
                                channel,
                            },
                        )
                        .placeholder("Channel"),
                    ]
                    .align_y(Alignment::Center)
                    .spacing(6),
                )
                .width(Length::Fill)
                .height(Length::Fixed(lane_h))
                .padding([4, 6])
                .style(move |_theme| container::Style {
                    background: Some(Background::Color(Color::from_rgba(0.08, 0.18, 0.14, 0.9))),
                    border: Border {
                        color: Color::from_rgba(0.34, 0.63, 0.48, 0.26),
                        width: 1.0,
                        radius: 6.0.into(),
                    },
                    text_color: Some(Color::from_rgb(0.83, 0.93, 0.88)),
                    ..container::Style::default()
                }),
            );
        }
        for lane in &track.visible_automation_lanes {
            lane_rows = lane_rows.push(
                container(
                    row![
                        Self::info_badge("AUTO".to_string(), false),
                        text(format!("{}", lane.target)).size(11),
                        Space::new().width(Length::Fill),
                        text(format!("{} pts", lane.points_len)).size(10),
                    ]
                    .align_y(Alignment::Center)
                    .spacing(6),
                )
                .width(Length::Fill)
                .height(Length::Fixed(lane_h))
                .padding([4, 6])
                .style(move |_theme| container::Style {
                    background: Some(Background::Color(Color::from_rgba(0.19, 0.16, 0.11, 0.88))),
                    border: Border {
                        color: Color::from_rgba(0.62, 0.49, 0.28, 0.22),
                        width: 1.0,
                        radius: 6.0.into(),
                    },
                    text_color: Some(Color::from_rgb(0.84, 0.79, 0.69)),
                    ..container::Style::default()
                }),
            );
        }

        let audio_io = format!("A {}/{}", track.audio_ins, track.audio_outs);
        let return_io = format!(
            "RET {}",
            track.audio_ins.saturating_sub(track.primary_audio_ins)
        );
        let midi_io = format!("M {}/{}", track.midi_ins, track.midi_outs);
        let send_io = format!(
            "SND {}",
            track.audio_outs.saturating_sub(track.primary_audio_outs)
        );
        let mode_label = track.automation_mode.clone();
        let balance_label = Self::format_balance(track.balance);
        let automation_hint = if has_visible_automation { "AUTO" } else { "" };

        let body = row![
            controls,
            column![
                row![
                    inline_midi_channel,
                    Self::info_badge(audio_io, false),
                    Self::info_badge(return_io, false),
                    Self::info_badge(midi_io, false),
                    Self::info_badge(send_io, false),
                    Self::info_badge(mode_label, false),
                    Self::info_badge(balance_label, false),
                    container(text(automation_hint).size(9))
                        .padding([1, 0])
                        .style(|_theme: &Theme| container::Style {
                            text_color: Some(Color::from_rgba(0.77, 0.67, 0.52, 0.96)),
                            ..container::Style::default()
                        }),
                ]
                .spacing(4)
                .align_y(Alignment::Center),
            ]
            .spacing(3)
            .width(Length::Fill)
            .align_x(Horizontal::Left),
        ]
        .spacing(8)
        .align_y(Alignment::Start);

        let track_ui: Column<'static, Message> = column![
            header,
            container(body)
                .height(Length::Fixed(body_height))
                .padding([3, 6])
                .style(move |_theme| container::Style {
                    background: Some(Background::Color(if selected {
                        Color::from_rgba(0.13, 0.18, 0.27, 0.98)
                    } else {
                        Color::from_rgba(0.11, 0.14, 0.20, 0.96)
                    })),
                    border: Border {
                        color: Color::from_rgba(
                            0.71,
                            0.82,
                            0.97,
                            if selected { 0.22 } else { 0.1 }
                        ),
                        width: 1.0,
                        radius: 8.0.into(),
                    },
                    text_color: Some(Color::from_rgb(0.90, 0.93, 0.98)),
                    ..container::Style::default()
                }),
            lane_rows.spacing(TRACK_SUBTRACK_GAP),
            mouse_area(
                container("")
                    .width(Length::Fill)
                    .height(Length::Fixed(resize_handle_height))
                    .style(move |_theme| container::Style {
                        background: Some(Background::Color(if is_resize_hovered {
                            Color::from_rgba(0.51, 0.68, 0.92, 0.95)
                        } else {
                            Color::from_rgba(0.33, 0.40, 0.52, 0.65)
                        })),
                        border: Border {
                            color: Color::TRANSPARENT,
                            width: 0.0,
                            radius: 2.0.into(),
                        },
                        ..container::Style::default()
                    }),
            )
            .on_enter(Message::TrackResizeHover(track.name.clone(), true))
            .on_exit(Message::TrackResizeHover(track.name.clone(), false))
            .on_press(Message::TrackResizeStart(track.name.clone())),
        ]
        .spacing(TRACK_SUBTRACK_GAP);

        let vca_strip_width = 12.0;
        let vca_strip = if let Some(vca) = vca {
            container(
                container(text(vca.label).size(9).align_x(Horizontal::Center))
                    .width(Length::Fill)
                    .align_x(Alignment::Center),
            )
            .width(Length::Fixed(vca_strip_width))
            .height(Length::Fill)
            .padding([6, 1])
            .style(move |_theme| container::Style {
                background: Some(Background::Color(Color::from_rgba(0.25, 0.34, 0.49, 0.92))),
                border: Border {
                    color: Color::from_rgba(0.82, 0.9, 1.0, 0.18),
                    width: 1.0,
                    radius: iced::border::Radius {
                        top_left: if vca.prev_same { 0.0 } else { 4.0 },
                        top_right: 0.0,
                        bottom_right: 0.0,
                        bottom_left: if vca.next_same { 0.0 } else { 4.0 },
                    },
                },
                text_color: Some(Color::from_rgb(0.89, 0.93, 0.99)),
                ..container::Style::default()
            })
        } else {
            container("")
                .width(Length::Fixed(vca_strip_width))
                .height(Length::Fill)
        };
        let vca_strip = container(row![Space::new().width(Length::Fixed(4.0)), vca_strip])
            .width(Length::Fixed(vca_strip_width + 4.0))
            .height(Length::Fill);
        let track_body = container(track_ui)
            .id(track.name.clone())
            .width(Length::Fill)
            .height(Length::Fixed(height))
            .padding([4, 6])
            .style(move |_theme| container::Style {
                background: Some(Background::Color(if selected {
                    Color::from_rgba(0.10, 0.14, 0.22, 0.98)
                } else {
                    Color::from_rgba(0.08, 0.10, 0.16, 0.96)
                })),
                border: Border {
                    color: Color::from_rgba(0.74, 0.84, 0.98, if selected { 0.32 } else { 0.08 }),
                    width: 1.0,
                    radius: 8.0.into(),
                },
                ..container::Style::default()
            });
        let track_body = container(row![vca_strip, track_body])
            .width(Length::Fill)
            .height(Length::Fixed(height));

        let track_with_drop = droppable(track_body)
            .on_drag(move |position, _| Message::TrackDrag { index, position })
            .on_drop(Message::TrackDropped);

        let track_name_for_menu = track.name.clone();
        let track_name_for_hover = track_name_for_menu.clone();
        let track_name_for_toggle = track_name_for_menu.clone();
        mouse_area(track_with_drop)
            .on_move(move |position| Message::TrackContextMenuHover {
                track_name: track_name_for_hover.clone(),
                position,
            })
            .on_right_press(Message::TrackContextMenuToggle(track_name_for_toggle))
            .into()
    }

    pub fn view(&self, visible_window: VisibleTrackWindow) -> Element<'_, Message> {
        let (tracks, width) = {
            let state = self.state.blocking_read();
            let hovered_resize_track = state.hovered_track_resize_handle.as_deref();
            let tracks = state
                .tracks
                .iter()
                .filter(|track| track.name != METRONOME_TRACK_ID)
                .enumerate()
                .filter(|(index, _)| {
                    *index >= visible_window.start_index && *index < visible_window.end_index
                })
                .map(|(index, track)| {
                    (
                        index,
                        TrackViewData {
                            name: track.name.clone(),
                            height: track.height,
                            layout: track.lane_layout(),
                            selected: state.selected.contains(track.name.as_str()),
                            resize_hovered: hovered_resize_track == Some(track.name.as_str()),
                            armed: track.armed,
                            frozen: track.frozen,
                            muted: track.muted,
                            soloed: track.soloed,
                            input_monitor: track.input_monitor,
                            disk_monitor: track.disk_monitor,
                            audio_ins: track.audio.ins,
                            audio_outs: track.audio.outs,
                            primary_audio_ins: track.primary_audio_ins(),
                            primary_audio_outs: track.primary_audio_outs(),
                            midi_ins: track.midi.ins,
                            midi_outs: track.midi.outs,
                            midi_lane_channels: track.midi_lane_channels.clone(),
                            balance: track.balance,
                            automation_mode: track.automation_mode.to_string(),
                            visible_automation_lanes: track
                                .automation_lanes
                                .iter()
                                .filter(|lane| lane.visible)
                                .map(|lane| VisibleAutomationLane {
                                    target: lane.target,
                                    points_len: lane.points.len(),
                                })
                                .collect(),
                            midi_learn_vol: track.midi_learn_volume.is_some(),
                            midi_learn_bal: track.midi_learn_balance.is_some(),
                            midi_learn_mute: track.midi_learn_mute.is_some(),
                            midi_learn_solo: track.midi_learn_solo.is_some(),
                            midi_learn_arm: track.midi_learn_arm.is_some(),
                            midi_learn_input_monitor: track.midi_learn_input_monitor.is_some(),
                            midi_learn_disk_monitor: track.midi_learn_disk_monitor.is_some(),
                            vca_master: track.vca_master.clone(),
                        },
                    )
                })
                .collect::<Vec<_>>();
            (tracks, state.tracks_width)
        };
        let track_width_px = match width {
            Length::Fixed(v) => v,
            _ => 200.0,
        };
        let track_heights: Vec<f32> = tracks.iter().map(|(_, t)| t.height).collect();
        let total_height = (visible_window.top_padding
            + track_heights.iter().sum::<f32>()
            + visible_window.bottom_padding)
            .max(1.0);
        let mut vca_has_followers = std::collections::HashSet::new();
        for (_, track) in &tracks {
            if let Some(master) = track.vca_master.as_ref() {
                vca_has_followers.insert(master.clone());
            }
        }
        let vca_display_labels: Vec<Option<String>> = tracks
            .iter()
            .map(|(_, track)| {
                track.vca_master.clone().or_else(|| {
                    vca_has_followers
                        .contains(&track.name)
                        .then(|| track.name.clone())
                })
            })
            .collect();
        let vca_view_data: Vec<Option<VcaStripViewData>> = tracks
            .iter()
            .enumerate()
            .map(|(index, _track)| {
                let master = vca_display_labels[index].as_ref()?;
                let prev_same =
                    index > 0 && vca_display_labels[index - 1].as_deref() == Some(master.as_str());
                let next_same = index + 1 < vca_display_labels.len()
                    && vca_display_labels[index + 1].as_deref() == Some(master.as_str());
                let mut group_start = index;
                while group_start > 0
                    && vca_display_labels[group_start - 1].as_deref() == Some(master.as_str())
                {
                    group_start -= 1;
                }
                let mut group_end = index;
                while group_end + 1 < vca_display_labels.len()
                    && vca_display_labels[group_end + 1].as_deref() == Some(master.as_str())
                {
                    group_end += 1;
                }
                let line_h = 10.0;
                let cap_for = |h: f32| -> usize { ((h - 8.0).max(0.0) / line_h).floor() as usize };
                let mut capacity_before = 0usize;
                for h in &track_heights[group_start..index] {
                    capacity_before += cap_for(*h);
                }
                let segment_capacity = cap_for(track_heights[index]);
                let label = if segment_capacity == 0 {
                    String::new()
                } else {
                    let mut group_capacity = 0usize;
                    for h in &track_heights[group_start..=group_end] {
                        group_capacity += cap_for(*h);
                    }
                    let full = Self::trim_with_ellipsis(master, group_capacity.max(1));
                    let segment = Self::char_slice(&full, capacity_before, segment_capacity);
                    Self::rotate_text_cw(&segment, segment_capacity)
                };
                Some(VcaStripViewData {
                    label,
                    prev_same,
                    next_same,
                })
            })
            .collect();
        let children: Vec<Element<'_, Message>> = tracks
            .into_iter()
            .enumerate()
            .map(|(visible_index, (index, track))| {
                let vca = vca_view_data[visible_index].clone();
                let hash = Self::track_row_render_hash(&track, track_width_px, index, vca.as_ref());
                lazy(hash, move |_| {
                    Self::render_track_row(track.clone(), track_width_px, index, vca.clone())
                })
                .into()
            })
            .collect();
        let mut result = column![];
        if visible_window.top_padding > 0.0 {
            result = result.push(Space::new().height(Length::Fixed(visible_window.top_padding)));
        }
        for child in children {
            result = result.push(child);
        }
        if visible_window.bottom_padding > 0.0 {
            result = result.push(Space::new().height(Length::Fixed(visible_window.bottom_padding)));
        }
        container(result.width(width))
            .style(|_theme| crate::style::app_background())
            .width(width)
            .height(Length::Fixed(total_height))
            .into()
    }
}
