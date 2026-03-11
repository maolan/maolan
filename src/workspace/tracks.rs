use crate::{
    consts::state_ids::METRONOME_TRACK_ID,
    menu,
    message::{Message, TrackAutomationTarget},
    state::{State, StateData, TrackLaneLayout},
    style,
};
use iced::{
    Alignment, Background, Border, Color, Element, Length, Point, Theme,
    alignment::Horizontal,
    widget::{Column, Space, Stack, button, column, container, mouse_area, pin, row, text},
};
use iced_drop::droppable;
use iced_fonts::lucide::{audio_waveform, disc};
use maolan_engine::message::{Action, TrackMidiLearnTarget};

#[derive(Debug, Default)]
pub struct Tracks {
    state: State,
}

fn track_context_menu_overlay(state: &StateData) -> Option<(Point, Element<'static, Message>)> {
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

    if let Some(master) = track.vca_master.as_ref() {
        items.push(menu::menu_item(
            format!("VCA: Unassign ({master})"),
            Message::TrackSetVcaMaster {
                track_name: track_name.clone(),
                master_track: None,
            },
        ));
    }
    for send in &track.aux_sends {
        items.push(menu::menu_item(
            format!("Send {} Level -1dB ({:.1})", send.aux_track, send.level_db),
            Message::TrackAuxSendLevelAdjust {
                track_name: track_name.clone(),
                aux_track: send.aux_track.clone(),
                delta_db: -1.0,
            },
        ));
        items.push(menu::menu_item(
            format!("Send {} Level +1dB ({:.1})", send.aux_track, send.level_db),
            Message::TrackAuxSendLevelAdjust {
                track_name: track_name.clone(),
                aux_track: send.aux_track.clone(),
                delta_db: 1.0,
            },
        ));
        items.push(menu::menu_item(
            format!("Send {} Pan L ({:.2})", send.aux_track, send.pan),
            Message::TrackAuxSendPanAdjust {
                track_name: track_name.clone(),
                aux_track: send.aux_track.clone(),
                delta: -0.1,
            },
        ));
        items.push(menu::menu_item(
            format!("Send {} Pan R ({:.2})", send.aux_track, send.pan),
            Message::TrackAuxSendPanAdjust {
                track_name: track_name.clone(),
                aux_track: send.aux_track.clone(),
                delta: 0.1,
            },
        ));
        items.push(menu::menu_item(
            format!(
                "Send {} Mode: {}",
                send.aux_track,
                if send.pre_fader {
                    "Pre-Fader"
                } else {
                    "Post-Fader"
                }
            ),
            Message::TrackAuxSendTogglePrePost {
                track_name: track_name.clone(),
                aux_track: send.aux_track.clone(),
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

    for candidate in state
        .tracks
        .iter()
        .filter(|candidate| candidate.name != track.name)
    {
        items.push(menu::menu_item(
            format!("VCA -> {}", candidate.name),
            Message::TrackSetVcaMaster {
                track_name: track_name.clone(),
                master_track: Some(candidate.name.clone()),
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

    Some((Point::new(0.0, (top + menu_state.anchor.y).max(0.0)), panel))
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

    pub fn view(&self) -> Element<'_, Message> {
        struct VisibleAutomationLane {
            target: TrackAutomationTarget,
            points_len: usize,
        }

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
            midi_ins: usize,
            midi_outs: usize,
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

        let (tracks, width, context_menu_overlay) = {
            let state = self.state.blocking_read();
            let hovered_resize_track = state.hovered_track_resize_handle.as_deref();
            let tracks = state
                .tracks
                .iter()
                .filter(|track| track.name != METRONOME_TRACK_ID)
                .map(|track| TrackViewData {
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
                    midi_ins: track.midi.ins,
                    midi_outs: track.midi.outs,
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
                })
                .collect::<Vec<_>>();
            (
                tracks,
                state.tracks_width,
                track_context_menu_overlay(&state),
            )
        };
        let track_width_px = match width {
            Length::Fixed(v) => v,
            _ => 200.0,
        };
        let track_heights: Vec<f32> = tracks.iter().map(|t| t.height).collect();
        let mut vca_has_followers = std::collections::HashSet::new();
        for track in &tracks {
            if let Some(master) = track.vca_master.as_ref() {
                vca_has_followers.insert(master.clone());
            }
        }
        let vca_display_labels: Vec<Option<String>> = tracks
            .iter()
            .map(|track| {
                track.vca_master.clone().or_else(|| {
                    vca_has_followers
                        .contains(&track.name)
                        .then(|| track.name.clone())
                })
            })
            .collect();
        let result = Column::with_children(tracks.into_iter().enumerate().map(|(index, track)| {
            let selected = track.selected;
            let height = track.height;
            let is_resize_hovered = track.resize_hovered;
            let vca_master_label = vca_display_labels[index].clone();
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
            let automation_height = if has_visible_automation {
                track.visible_automation_lanes.len() as f32 * (lane_h + 4.0)
            } else {
                0.0
            };
            let inner_available_height = (height - inner_vertical_padding).max(16.0);
            let body_height = (inner_available_height
                - layout.header_height
                - resize_handle_height
                - outer_spacing
                - automation_height)
                .max(12.0);
            let mut title_badges = row![].spacing(4).align_y(Alignment::Center);
            if track.armed {
                title_badges = title_badges.push(Self::info_badge("REC".to_string(), true));
            }
            if track.frozen {
                title_badges = title_badges.push(Self::info_badge("FRZ".to_string(), false));
            }
            if learn_count > 0 {
                title_badges =
                    title_badges.push(Self::info_badge(format!("CC {}", learn_count), false));
            }
            if let Some(master) = vca_master_label.as_ref() {
                title_badges = title_badges.push(Self::info_badge(
                    format!("VCA {}", Self::trim_with_ellipsis(master, 8)),
                    false,
                ));
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
                        color: Color::from_rgba(
                            0.78,
                            0.87,
                            0.99,
                            if selected { 0.5 } else { 0.16 },
                        ),
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
                        .center_y(Length::Fill),
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
                        .center_y(Length::Fill),
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
                        .center_y(Length::Fill),
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
                        .center_y(Length::Fill),
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
                        .center_y(Length::Fill),
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

            let mut lane_rows: Column<'_, Message> = column![];
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
                        background: Some(Background::Color(Color::from_rgba(
                            0.19, 0.16, 0.11, 0.88,
                        ))),
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
            let midi_io = format!("M {}/{}", track.midi_ins, track.midi_outs);
            let mode_label = track.automation_mode.clone();
            let balance_label = Self::format_balance(track.balance);
            let automation_hint = if has_visible_automation { "AUTO" } else { "" };

            let body = row![
                controls,
                column![
                    row![
                        Self::info_badge(audio_io, false),
                        Self::info_badge(midi_io, false),
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

            let track_ui: Column<'_, Message> = column![
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
                                if selected { 0.22 } else { 0.1 },
                            ),
                            width: 1.0,
                            radius: 8.0.into(),
                        },
                        text_color: Some(Color::from_rgb(0.90, 0.93, 0.98)),
                        ..container::Style::default()
                    }),
                lane_rows.spacing(4),
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
            .spacing(2.0);

            {
                let vca_strip_width = 12.0;
                let vca_strip = if let Some(master) = vca_master_label.as_ref() {
                    let prev_same = index > 0
                        && vca_display_labels[index - 1].as_deref() == Some(master.as_str());
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
                    let cap_for =
                        |h: f32| -> usize { ((h - 8.0).max(0.0) / line_h).floor() as usize };
                    let mut capacity_before = 0usize;
                    for h in &track_heights[group_start..index] {
                        capacity_before += cap_for(*h);
                    }
                    let segment_capacity = cap_for(track_heights[index]);
                    let strip_name = if segment_capacity == 0 {
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
                    container(text(strip_name).size(9).align_x(Horizontal::Center))
                        .width(Length::Fixed(vca_strip_width))
                        .height(Length::Fill)
                        .padding([6, 1])
                        .style(move |_theme| container::Style {
                            background: Some(Background::Color(Color::from_rgba(
                                0.25, 0.34, 0.49, 0.92,
                            ))),
                            border: Border {
                                color: Color::from_rgba(0.82, 0.9, 1.0, 0.18),
                                width: 1.0,
                                radius: if prev_same || next_same { 0.0 } else { 4.0 }.into(),
                            },
                            text_color: Some(Color::from_rgb(0.89, 0.93, 0.99)),
                            ..container::Style::default()
                        })
                } else {
                    container("")
                        .width(Length::Fixed(vca_strip_width))
                        .height(Length::Fill)
                };
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
                            color: Color::from_rgba(
                                0.74,
                                0.84,
                                0.98,
                                if selected { 0.32 } else { 0.08 },
                            ),
                            width: 1.0,
                            radius: 8.0.into(),
                        },
                        ..container::Style::default()
                    });
                let track_body = container(row![vca_strip, track_body].spacing(4.0))
                    .width(Length::Fill)
                    .height(Length::Fixed(height));

                let track_with_drop = droppable(track_body)
                    .on_drag(move |_, _| Message::TrackDrag(index))
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
        }));
        let track_list: Element<'_, Message> = result.width(width).into();
        if let Some((anchor, menu)) = context_menu_overlay {
            Stack::new()
                .push(track_list)
                .push(pin(menu).position(Point::new(anchor.x.max(0.0), anchor.y.max(0.0))))
                .width(width)
                .into()
        } else {
            track_list
        }
    }
}
