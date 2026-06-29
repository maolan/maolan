use super::VisibleTrackWindow;
use crate::{
    consts::state_ids::METRONOME_TRACK_ID,
    consts::state_track::{TRACK_FOLDER_HEADER_HEIGHT, TRACK_SUBTRACK_GAP},
    menu,
    message::{Message, MidiLaneChannelSelection, Show, TrackAutomationTarget},
    state::{State, StateData, TrackLaneLayout},
    style,
};
use iced::{
    Alignment, Background, Border, Color, Element, Length, Point, Theme,
    widget::{
        Column, Row, Space, Stack, button, column, container, lazy, mouse_area, pick_list, pin,
        row, scrollable, text,
    },
};
use iced_drop::droppable;
use iced_fonts::lucide::{audio_waveform, disc, settings};
use maolan_engine::message::{Action, TrackMidiLearnTarget};
use maolan_widgets::horizontal_slider::horizontal_slider;
use std::{
    hash::{Hash, Hasher},
    sync::Arc,
};

#[derive(Debug, Default)]
pub struct Tracks {
    state: State,
}

#[derive(Clone)]
struct VisibleAutomationLane {
    target: TrackAutomationTarget,
    points_len: usize,
}

fn automation_target_current_value(target: TrackAutomationTarget, level: f32, balance: f32) -> f32 {
    match target {
        TrackAutomationTarget::Volume => level,
        TrackAutomationTarget::Balance => balance,
        _ => {
            let (min, max) = target.default_range();
            (min + max) / 2.0
        }
    }
}

fn automation_target_set_message(
    target: TrackAutomationTarget,
    track_name: String,
    value: f32,
) -> Option<Message> {
    match target {
        TrackAutomationTarget::Volume => {
            Some(Message::Request(Action::TrackLevel(track_name, value)))
        }
        TrackAutomationTarget::Balance => {
            Some(Message::Request(Action::TrackBalance(track_name, value)))
        }
        TrackAutomationTarget::MidiCc { channel, cc } => {
            Some(Message::Request(Action::TrackMidiCc {
                track_name,
                channel,
                cc,
                value: value.round().clamp(0.0, 127.0) as u8,
            }))
        }
        TrackAutomationTarget::ClapParameter {
            instance_id,
            param_id,
            ..
        } => Some(Message::Request(Action::TrackSetClapParameter {
            track_name,
            instance_id,
            param_id,
            value: value as f64,
        })),
        TrackAutomationTarget::Vst3Parameter {
            instance_id,
            param_id,
        } => Some(Message::Request(Action::TrackSetVst3Parameter {
            track_name,
            instance_id,
            param_id,
            value,
        })),
        #[cfg(all(unix, not(target_os = "macos")))]
        TrackAutomationTarget::Lv2Parameter {
            instance_id, index, ..
        } => Some(Message::Request(Action::TrackSetLv2ControlValue {
            track_name,
            instance_id,
            index,
            value,
        })),
        #[cfg(not(all(unix, not(target_os = "macos"))))]
        TrackAutomationTarget::Lv2Parameter { .. } => None,
    }
}

fn automation_lane_header_control(
    track_name: String,
    level: f32,
    balance: f32,
    lane: &VisibleAutomationLane,
    selected_modulator: Option<&crate::state::Modulator>,
) -> Element<'static, Message> {
    let target = lane.target;
    let (min, max) = target.default_range();
    let value = automation_target_current_value(target, level, balance);
    let track_name_for_slider = track_name.clone();
    let slider = horizontal_slider(min..=max, value, move |v| {
        automation_target_set_message(target, track_name_for_slider.clone(), v)
            .unwrap_or(Message::DeselectClips)
    })
    .width(Length::Fill)
    .height(Length::Fixed(12.0));
    let label = text(format!("{}", target))
        .size(9)
        .width(Length::Fixed(70.0));
    let base: Element<'static, Message> = row![label, slider]
        .spacing(4)
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .into();

    let Some(m) = selected_modulator else {
        return base;
    };
    let assigned = m
        .targets
        .iter()
        .any(|t| t.track_name == track_name && t.target == target);
    let show_message = Message::ModulatorTargetShow {
        modulator_id: m.id,
        track_name,
        target,
    };
    Stack::new()
        .push(base)
        .push(
            mouse_area(
                container(Space::new().width(Length::Fill).height(Length::Fill))
                    .style(move |_theme| crate::style::mixer::modulator_target(assigned)),
            )
            .on_press(show_message),
        )
        .into()
}

#[derive(Clone, Default)]
struct TrackViewData {
    name: String,
    height: f32,
    layout: TrackLaneLayout,
    selected: bool,
    resize_hovered: bool,
    armed: bool,
    frozen: bool,
    muted: bool,
    phase_inverted: bool,
    soloed: bool,
    solo_upstream: bool,
    is_master: bool,
    input_monitor: Vec<bool>,
    disk_monitor: Vec<bool>,
    midi_input_monitor: Vec<bool>,
    midi_disk_monitor: Vec<bool>,
    audio_ins: usize,
    audio_outs: usize,
    primary_audio_ins: usize,
    primary_audio_outs: usize,
    midi_ins: usize,
    midi_outs: usize,
    setup_open: bool,
    midi_lane_channels: Vec<Option<u8>>,
    level: f32,
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
    color: Option<iced::Color>,
    is_folder: bool,
    folder_open: bool,
    folder_depth: usize,
    parent_track: Option<String>,
    visible_height: f32,
    hidden: bool,
    effective_muted: bool,
    effective_soloed: bool,
}

fn context_menu_panel_style(theme: &Theme) -> container::Style {
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
}

fn context_menu_panel(
    items: Vec<Element<'static, Message>>,
    width: f32,
) -> Element<'static, Message> {
    container(Column::with_children(items).spacing(2))
        .width(Length::Fixed(width))
        .padding(6)
        .style(context_menu_panel_style)
        .into()
}

fn context_menu_scrollable_panel(
    items: Vec<Element<'static, Message>>,
    width: f32,
    max_height: f32,
) -> Element<'static, Message> {
    container(scrollable(Column::with_children(items).spacing(2)).height(Length::Fixed(max_height)))
        .width(Length::Fixed(width))
        .padding(6)
        .style(context_menu_panel_style)
        .into()
}

fn context_submenu_item(
    label: String,
    submenu: crate::state::TrackContextSubmenu,
    active: bool,
) -> Element<'static, Message> {
    let item = menu::submenu(label, Message::None);
    mouse_area(
        container(item)
            .style(move |_theme| style::menu_submenu_background(active))
            .width(Length::Fill)
            .height(Length::Shrink),
    )
    .on_enter(Message::TrackContextMenuSubmenuOpen(submenu.clone()))
    .into()
}

pub(super) fn track_context_menu_overlay(
    state: &StateData,
    max_y: f32,
    selected_modulator: Option<&crate::state::Modulator>,
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
    let freeze_supported = track.midi.ins == 0;
    let mut items = vec![context_submenu_item(
        "Automation".to_string(),
        crate::state::TrackContextSubmenu::Automation,
        menu_state.submenu == Some(crate::state::TrackContextSubmenu::Automation),
    )];

    items.extend(vec![
        menu::menu_item("Rename", Message::TrackRenameShow(track_name.clone())),
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
            "Apply template",
            Message::Show(Show::ApplyTemplate {
                track_name: track_name.clone(),
            }),
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
    ]);

    if let Some(modulator) = selected_modulator {
        let mut modulator_items = Vec::new();
        for target in [
            TrackAutomationTarget::Volume,
            TrackAutomationTarget::Balance,
        ] {
            if !target.is_modulatable() {
                continue;
            }
            let is_assigned = modulator
                .targets
                .iter()
                .any(|t| t.track_name == track_name && t.target == target);
            modulator_items.push(menu::menu_item(
                format!(
                    "Modulator {} {} {}",
                    modulator.name,
                    if is_assigned { "✓" } else { " " },
                    target
                ),
                Message::ModulatorTargetShow {
                    modulator_id: modulator.id,
                    track_name: track_name.clone(),
                    target,
                },
            ));
        }
        for lane in track.automation_lanes.iter().filter(|lane| lane.visible) {
            if !lane.target.is_modulatable() {
                continue;
            }
            if matches!(
                lane.target,
                TrackAutomationTarget::Volume | TrackAutomationTarget::Balance
            ) {
                continue;
            }
            let is_assigned = modulator
                .targets
                .iter()
                .any(|t| t.track_name == track_name && t.target == lane.target);
            modulator_items.push(menu::menu_item(
                format!(
                    "Modulator {} {} {}",
                    modulator.name,
                    if is_assigned { "✓" } else { " " },
                    lane.target
                ),
                Message::ModulatorTargetShow {
                    modulator_id: modulator.id,
                    track_name: track_name.clone(),
                    target: lane.target,
                },
            ));
        }
        items.extend(modulator_items);
    }

    if track.primary_audio_outs() == 2 {
        items.push(menu::menu_item(
            if track.is_master {
                "Unmaster"
            } else {
                "Master"
            },
            Message::Request(Action::TrackToggleMaster(track_name.clone())),
        ));
    }

    items.push(menu::menu_item(
        "Color",
        Message::Show(Show::TrackColor {
            track_name: track_name.clone(),
        }),
    ));

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

    if track.is_folder {
        items.push(menu::menu_item(
            "Folder Connections",
            Message::OpenFolderConnections(track_name.clone()),
        ));
    }

    if !track.is_folder
        && !track.is_master
        && let Some(ref current_parent) = track.parent_track
    {
        items.push(menu::menu_item(
            format!("Remove from Folder ({})", current_parent),
            Message::TrackSetParent {
                track_name: track_name.clone(),
                parent_name: None,
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

    let is_automation_visible = |target: &TrackAutomationTarget| {
        track
            .automation_lanes
            .iter()
            .any(|lane| lane.target == *target && lane.visible)
    };
    let automation_menu_item =
        |label: String, target: TrackAutomationTarget| -> Element<'static, Message> {
            let visible = is_automation_visible(&target);
            menu::menu_item(
                format!("{} {}", if visible { "✓" } else { " " }, label),
                Message::TrackAutomationToggleLane {
                    track_name: track_name.clone(),
                    target,
                },
            )
        };

    let mut automation_items: Vec<Element<'static, Message>> = vec![
        automation_menu_item("Volume".to_string(), TrackAutomationTarget::Volume),
        automation_menu_item("Balance".to_string(), TrackAutomationTarget::Balance),
    ];

    let mut plugin_submenu_items: Vec<Element<'static, Message>> = Vec::new();
    if let Some((plugins, _)) = state.plugin_graphs_by_track.get(&track_name) {
        for plugin in plugins {
            if plugin.format.eq_ignore_ascii_case("LV2")
                && !cfg!(all(unix, not(target_os = "macos")))
            {
                continue;
            }
            let plugin_label = format!("{} ({})", plugin.name, plugin.format);
            let plugin_submenu_key = crate::state::TrackContextSubmenu::Plugin {
                instance_id: plugin.instance_id,
                format: plugin.format.clone(),
            };
            let is_active = menu_state.submenu == Some(plugin_submenu_key.clone());
            automation_items.push(context_submenu_item(
                plugin_label.clone(),
                plugin_submenu_key.clone(),
                is_active,
            ));
            if is_active {
                if let Some(cached) = state
                    .plugin_parameters_by_track
                    .get(&track_name)
                    .and_then(|cache| cache.get(&plugin.instance_id))
                {
                    for param in cached {
                        let target = if plugin.format.eq_ignore_ascii_case("CLAP") {
                            TrackAutomationTarget::ClapParameter {
                                instance_id: plugin.instance_id,
                                param_id: param.param_id,
                                min: param.min,
                                max: param.max,
                            }
                        } else {
                            TrackAutomationTarget::Vst3Parameter {
                                instance_id: plugin.instance_id,
                                param_id: param.param_id,
                            }
                        };
                        plugin_submenu_items.push(automation_menu_item(param.name.clone(), target));
                    }
                } else {
                    plugin_submenu_items
                        .push(menu::menu_item("Loading parameters...", Message::None));
                }
            }
        }
    }

    let has_midi = track.midi.ins > 0 || track.midi.outs > 0;
    let mut midi_submenu_items: Vec<Element<'static, Message>> = Vec::new();
    if has_midi {
        automation_items.push(context_submenu_item(
            "MIDI".to_string(),
            crate::state::TrackContextSubmenu::Midi,
            menu_state.submenu == Some(crate::state::TrackContextSubmenu::Midi),
        ));
        if menu_state.submenu == Some(crate::state::TrackContextSubmenu::Midi) {
            for cc in 0u8..=127 {
                let name = crate::midi::standard_cc_name(cc);
                let label = if name.is_empty() {
                    format!("CC{}", cc)
                } else {
                    format!("CC{} - {}", cc, name)
                };
                midi_submenu_items.push(automation_menu_item(
                    label,
                    TrackAutomationTarget::MidiCc { channel: 0, cc },
                ));
            }
        }
    }

    let menu_height = items.len() as f32 * 32.0 + 10.0;

    let main_panel = context_menu_panel(items, 220.0);
    let mut panels = vec![main_panel];
    if menu_state.submenu.is_some() {
        panels.push(context_menu_panel(automation_items, 220.0));
    }
    if menu_state
        .submenu
        .as_ref()
        .is_some_and(|s| matches!(s, crate::state::TrackContextSubmenu::Plugin { .. }))
    {
        panels.push(context_menu_panel(plugin_submenu_items, 260.0));
    }
    if menu_state
        .submenu
        .as_ref()
        .is_some_and(|s| matches!(s, crate::state::TrackContextSubmenu::Midi))
    {
        let midi_max_height = (max_y - 20.0).clamp(200.0, 500.0);
        panels.push(context_menu_scrollable_panel(
            midi_submenu_items,
            280.0,
            midi_max_height,
        ));
    }
    let mut y = (top + menu_state.anchor.y).max(0.0);
    if y + menu_height > max_y {
        y = (max_y - menu_height).max(0.0);
    }

    let combined = Row::with_children(panels).spacing(4).into();
    Some((Point::new(menu_state.anchor.x.max(0.0), y), combined))
}

struct TrackNode {
    index: usize,
    data: TrackViewData,
    children: Vec<TrackNode>,
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

    fn track_row_render_hash(
        track: &TrackViewData,
        track_width_px: f32,
        row_index: usize,
        selected_modulator: Option<&crate::state::Modulator>,
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
        track.phase_inverted.hash(&mut hasher);
        track.soloed.hash(&mut hasher);
        track.solo_upstream.hash(&mut hasher);
        track.input_monitor.hash(&mut hasher);
        track.disk_monitor.hash(&mut hasher);
        track.midi_input_monitor.hash(&mut hasher);
        track.midi_disk_monitor.hash(&mut hasher);
        track.audio_ins.hash(&mut hasher);
        track.audio_outs.hash(&mut hasher);
        track.primary_audio_ins.hash(&mut hasher);
        track.primary_audio_outs.hash(&mut hasher);
        track.midi_ins.hash(&mut hasher);
        track.midi_outs.hash(&mut hasher);
        track.setup_open.hash(&mut hasher);
        track.midi_lane_channels.hash(&mut hasher);
        track.level.to_bits().hash(&mut hasher);
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
                TrackAutomationTarget::Volume | TrackAutomationTarget::Balance => {}
                TrackAutomationTarget::MidiCc { channel, cc } => {
                    channel.hash(&mut hasher);
                    cc.hash(&mut hasher);
                }
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
        if let Some(m) = selected_modulator {
            m.id.hash(&mut hasher);
            for lane in &track.visible_automation_lanes {
                let assigned = m
                    .targets
                    .iter()
                    .any(|t| t.track_name == track.name && t.target == lane.target);
                assigned.hash(&mut hasher);
            }
        }
        if let Some(color) = track.color {
            color.r.to_bits().hash(&mut hasher);
            color.g.to_bits().hash(&mut hasher);
            color.b.to_bits().hash(&mut hasher);
            color.a.to_bits().hash(&mut hasher);
        } else {
            0u8.hash(&mut hasher);
        }
        track.is_folder.hash(&mut hasher);
        track.folder_open.hash(&mut hasher);
        track.folder_depth.hash(&mut hasher);
        if let Some(parent) = &track.parent_track {
            parent.hash(&mut hasher);
        }
        track.visible_height.to_bits().hash(&mut hasher);
        track.hidden.hash(&mut hasher);
        track.effective_muted.hash(&mut hasher);
        track.effective_soloed.hash(&mut hasher);
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
        selected_modulator: Option<&crate::state::Modulator>,
        children: Vec<Element<'static, Message>>,
    ) -> Element<'static, Message> {
        let selected = track.selected;
        let height = track.height;
        let visible_height = track.visible_height;
        let (outer_body_bg, header_bg, _inner_body_bg) = if let Some(color) = track.color {
            let with_alpha = |c: Color, a: f32| Color::from_rgba(c.r, c.g, c.b, a);
            if selected {
                (
                    with_alpha(style::mixer::brighten(color, 0.02), 0.98),
                    with_alpha(style::mixer::brighten(color, 0.12), 0.98),
                    with_alpha(style::mixer::brighten(color, 0.05), 0.98),
                )
            } else {
                (
                    with_alpha(style::mixer::darken(color, 0.06), 0.96),
                    with_alpha(style::mixer::brighten(color, 0.04), 0.96),
                    with_alpha(color, 0.96),
                )
            }
        } else {
            if selected {
                (
                    Color::from_rgba(0.10, 0.14, 0.22, 0.98),
                    Color::from_rgba(0.28, 0.39, 0.56, 0.98),
                    Color::from_rgba(0.13, 0.18, 0.27, 0.98),
                )
            } else {
                (
                    Color::from_rgba(0.08, 0.10, 0.16, 0.96),
                    Color::from_rgba(0.18, 0.22, 0.30, 0.96),
                    Color::from_rgba(0.11, 0.14, 0.20, 0.96),
                )
            }
        };

        let midi_learn_vol = track.midi_learn_vol;
        let midi_learn_bal = track.midi_learn_bal;
        let midi_learn_mute = track.midi_learn_mute;
        let midi_learn_solo = track.midi_learn_solo;
        let midi_learn_arm = track.midi_learn_arm;
        let midi_learn_input_monitor = track.midi_learn_input_monitor;
        let midi_learn_disk_monitor = track.midi_learn_disk_monitor;
        let layout = track.layout;
        let lane_h = layout.lane_height.max(12.0);

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
        let mut title_badges = row![].spacing(4).align_y(Alignment::Center);
        if track.frozen {
            title_badges = title_badges.push(Self::info_badge("FRZ".to_string(), false));
        }
        if learn_count > 0 {
            title_badges =
                title_badges.push(Self::info_badge(format!("CC {}", learn_count), false));
        }

        let folder_toggle: Element<'static, Message> = if track.is_folder {
            let icon = if track.folder_open { "▼" } else { "▶" };
            button(
                container(text(icon).size(11))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .center_x(Length::Fill)
                    .center_y(Length::Fill),
            )
            .width(Length::Fixed(18.0))
            .height(Length::Fixed(18.0))
            .padding(0)
            .style(button::secondary)
            .on_press(Message::TrackToggleFolder {
                track_name: track.name.clone(),
            })
            .into()
        } else {
            Space::new().width(Length::Fixed(0.0)).into()
        };

        let indent = 0.0;

        let header_body = mouse_area(
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
                .spacing(4),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .padding([2, 6])
            .style(move |_theme| container::Style {
                background: None,
                text_color: Some(Color::from_rgb(0.92, 0.95, 1.0)),
                ..container::Style::default()
            }),
        )
        .on_press(Message::SelectTrack(track.name.clone()))
        .on_enter(Message::ShortcutsHint(Some("Select track".to_string())))
        .on_exit(Message::ShortcutsHint(None));

        let header = container(
            row![folder_toggle, header_body]
                .align_y(Alignment::Center)
                .spacing(4),
        )
        .height(Length::Fixed(TRACK_FOLDER_HEADER_HEIGHT))
        .padding([0, 6])
        .style(move |_theme| container::Style {
            background: Some(Background::Color(header_bg)),
            border: Border {
                color: Color::from_rgba(0.78, 0.87, 0.99, if selected { 0.5 } else { 0.16 }),
                width: 1.0,
                radius: 7.0.into(),
            },
            text_color: Some(Color::from_rgb(0.92, 0.95, 1.0)),
            ..container::Style::default()
        });

        let track_name = track.name.clone();
        let mut controls: Vec<Element<'static, Message>> = vec![];
        if !track.is_master {
            controls.push(
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
                .on_press(Message::Request(Action::TrackToggleArm(track_name.clone())))
                .into(),
            );
        }
        controls.push(
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
            .style(move |theme, _state| style::mute::style(theme, track.effective_muted))
            .on_press(Message::Request(Action::TrackToggleMute(
                track.name.clone(),
            )))
            .into(),
        );
        controls.push(
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
            .style(move |theme, _state| {
                style::solo::style(theme, track.effective_soloed, track.solo_upstream)
            })
            .on_press(Message::Request(Action::TrackToggleSolo(
                track.name.clone(),
            )))
            .into(),
        );
        if !track.is_master {
            controls.push(
                button(
                    container(text("Ø").size(13))
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .center_x(Length::Fill)
                        .center_y(Length::Fill),
                )
                .width(Length::Fixed(22.0))
                .height(Length::Fixed(22.0))
                .padding(0)
                .style(move |theme, _state| style::phase_invert::style(theme, track.phase_inverted))
                .on_press(Message::Request(Action::TrackTogglePhase(
                    track.name.clone(),
                )))
                .into(),
            );
        }
        if !track.is_master {
            controls.push(
                button(
                    container(settings().size(13))
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .center_x(Length::Fill)
                        .center_y(Length::Fill),
                )
                .width(Length::Fixed(22.0))
                .height(Length::Fixed(22.0))
                .padding(0)
                .style(move |theme, _status| style::setup::style(theme, track.setup_open))
                .on_press(Message::TrackSetupToggle(track.name.clone()))
                .into(),
            );
        }
        let controls = Row::with_children(controls)
            .spacing(4)
            .align_y(Alignment::Center);

        let mut lane_rows: Column<'static, Message> = column![];
        for lane in &track.visible_automation_lanes {
            lane_rows = lane_rows.push(
                container(automation_lane_header_control(
                    track.name.clone(),
                    track.level,
                    track.balance,
                    lane,
                    selected_modulator,
                ))
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

        let body = row![controls].spacing(8).align_y(Alignment::Start);

        let body_container_height = 22.0 + 6.0;
        let buttons_bottom = TRACK_FOLDER_HEADER_HEIGHT + body_container_height;
        let resize_top = height - resize_handle_height;
        let show_buttons = track.is_folder || resize_top >= buttons_bottom;

        let is_resize_hovered = track.resize_hovered;

        let body_element: Element<'static, Message> = if show_buttons {
            container(body)
                .height(Length::Shrink)
                .padding([3, 6])
                .style(move |_theme| container::Style {
                    background: None,
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
                })
                .into()
        } else {
            Space::new().height(Length::Fixed(0.0)).into()
        };

        let setup_panel: Element<'static, Message> = if track.setup_open {
            let mut setup_lanes: Column<'static, Message> = column![];
            let track_name = track.name.clone();

            for lane in 0..track.audio_ins.min(if track.is_master { 0 } else { 1 }) {
                let input_monitor = track.input_monitor.get(lane).copied().unwrap_or(false);
                let disk_monitor = track.disk_monitor.get(lane).copied().unwrap_or(false);
                setup_lanes = setup_lanes.push(
                    container(
                        row![
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
                            .style(move |theme, _state| style::input::style(theme, input_monitor))
                            .on_press(Message::Request(
                                Action::TrackToggleInputMonitor {
                                    track_name: track.name.clone(),
                                    lane,
                                }
                            )),
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
                            .style(move |theme, _state| style::disk::style(theme, disk_monitor))
                            .on_press(Message::Request(
                                Action::TrackToggleDiskMonitor {
                                    track_name: track.name.clone(),
                                    lane,
                                }
                            )),
                        ]
                        .spacing(4)
                        .align_y(Alignment::Center),
                    )
                    .width(Length::Fill)
                    .height(Length::Fixed(lane_h))
                    .padding([4, 6])
                    .style(move |_theme| container::Style {
                        background: None,
                        ..container::Style::default()
                    }),
                );
            }

            for lane_index in 0..if track.is_master { 0 } else { track.midi_ins } {
                let selected_channel = MidiLaneChannelSelection::from_engine(
                    track.midi_lane_channels.get(lane_index).copied().flatten(),
                );
                let midi_input_monitor = track
                    .midi_input_monitor
                    .get(lane_index)
                    .copied()
                    .unwrap_or(false);
                let midi_disk_monitor = track
                    .midi_disk_monitor
                    .get(lane_index)
                    .copied()
                    .unwrap_or(false);
                setup_lanes = setup_lanes.push(
                    container(
                        row![
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
                            .style(move |theme, _state| style::input::style(
                                theme,
                                midi_input_monitor
                            ))
                            .on_press(Message::Request(
                                Action::TrackToggleMidiInputMonitor {
                                    track_name: track.name.clone(),
                                    lane: lane_index,
                                }
                            )),
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
                            .style(move |theme, _state| style::disk::style(
                                theme,
                                midi_disk_monitor
                            ))
                            .on_press(Message::Request(
                                Action::TrackToggleMidiDiskMonitor {
                                    track_name: track.name.clone(),
                                    lane: lane_index,
                                }
                            )),
                            Space::new().width(Length::Fill),
                            pick_list(MidiLaneChannelSelection::ALL, Some(selected_channel), {
                                let track_name = track_name.clone();
                                move |channel| Message::TrackMidiSetupChannelSelected {
                                    track_name: track_name.clone(),
                                    channel,
                                }
                            })
                            .width(Length::Shrink)
                            .placeholder("Channel"),
                        ]
                        .align_y(Alignment::Center)
                        .spacing(6),
                    )
                    .width(Length::Fill)
                    .height(Length::Fixed(lane_h))
                    .padding([4, 6])
                    .style(move |_theme| container::Style {
                        background: Some(Background::Color(Color::from_rgba(
                            0.12, 0.16, 0.22, 0.9,
                        ))),
                        border: Border {
                            color: Color::from_rgba(0.40, 0.45, 0.55, 0.25),
                            width: 1.0,
                            radius: 6.0.into(),
                        },
                        text_color: Some(Color::from_rgb(0.90, 0.93, 0.98)),
                        ..container::Style::default()
                    }),
                );
            }

            for _ in 0..track.visible_automation_lanes.len() {
                setup_lanes = setup_lanes.push(
                    container(Space::new())
                        .width(Length::Fill)
                        .height(Length::Fixed(lane_h))
                        .style(move |_theme| container::Style {
                            background: None,
                            ..container::Style::default()
                        }),
                );
            }

            container(setup_lanes.spacing(TRACK_SUBTRACK_GAP))
                .width(Length::Fixed(160.0))
                .height(Length::Fill)
                .padding([0, 6])
                .style(move |_theme| container::Style {
                    background: Some(Background::Color(Color::from_rgba(0.08, 0.10, 0.14, 0.6))),
                    border: Border {
                        color: Color::from_rgba(0.40, 0.45, 0.55, 0.25),
                        width: 1.0,
                        radius: 6.0.into(),
                    },
                    ..container::Style::default()
                })
                .into()
        } else {
            Space::new().width(Length::Fixed(0.0)).into()
        };

        let (track_container_element, track_fill_element): (
            Element<'static, Message>,
            Element<'static, Message>,
        ) = if track.is_folder {
            (
                container(Column::with_children(children).spacing(TRACK_SUBTRACK_GAP))
                    .width(Length::Fill)
                    .padding(iced::Padding {
                        left: 12.0,
                        ..Default::default()
                    })
                    .style(move |_theme| container::Style {
                        background: Some(Background::Color(Color::from_rgba(0.5, 0.5, 0.5, 0.25))),
                        ..container::Style::default()
                    })
                    .into(),
                Space::new().height(Length::Fill).into(),
            )
        } else {
            (
                Space::new().height(Length::Fixed(0.0)).into(),
                Space::new().height(Length::Fill).into(),
            )
        };

        let track_content: Element<'static, Message> = column![
            header,
            body_element,
            track_container_element,
            track_fill_element,
            lane_rows.spacing(TRACK_SUBTRACK_GAP),
        ]
        .spacing(0)
        .height(Length::Fill)
        .into();

        let resize_handle = mouse_area(
            mouse_area(
                container("")
                    .width(Length::Fill)
                    .height(Length::Fixed(resize_handle_height))
                    .style(move |_theme| container::Style {
                        background: if is_resize_hovered {
                            Some(Background::Color(Color::from_rgba(0.51, 0.68, 0.92, 0.95)))
                        } else {
                            None
                        },
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
        )
        .on_enter(Message::ShortcutsHint(Some("Resize height".to_string())))
        .on_exit(Message::ShortcutsHint(None));

        let track_ui = Stack::new()
            .push(track_content)
            .push(pin(resize_handle).position(Point::new(
                0.0,
                (visible_height - 8.0 - resize_handle_height).max(0.0),
            )))
            .height(Length::Fill);

        let track_body_content = container(row![track_ui.width(Length::Fill), setup_panel])
            .id(track.name.clone())
            .width(Length::Fill)
            .height(Length::Fill)
            .padding([0, 6])
            .style(move |_theme| container::Style {
                background: Some(Background::Color(outer_body_bg)),
                ..container::Style::default()
            });

        let track_body_border = container(Space::new())
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_theme| container::Style {
                background: None,
                border: Border {
                    color: Color::from_rgb(0.0, 0.0, 0.0),
                    width: 1.0,
                    radius: 8.0.into(),
                },
                ..container::Style::default()
            });

        let track_body: Element<'static, Message> = row![
            Space::new().width(Length::Fixed(indent)),
            Stack::new()
                .push(track_body_border)
                .push(track_body_content)
                .width(Length::Fill)
                .height(Length::Fixed(visible_height))
        ]
        .width(Length::Fill)
        .height(Length::Fixed(visible_height))
        .into();

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

    fn render_node(
        node: &TrackNode,
        track_width_px: f32,
        selected_modulator: Option<&crate::state::Modulator>,
        force_direct: bool,
    ) -> Element<'static, Message> {
        let child_elements: Vec<Element<'static, Message>> = node
            .children
            .iter()
            .map(|child| Self::render_node(child, track_width_px, selected_modulator, true))
            .collect();

        if node.children.is_empty() && !force_direct {
            let hash = Self::track_row_render_hash(
                &node.data,
                track_width_px,
                node.index,
                selected_modulator,
            );
            let track = node.data.clone();
            let index = node.index;
            let selected_modulator_arc = Arc::new(selected_modulator.cloned());
            lazy(hash, move |_| {
                Self::render_track_row(
                    track.clone(),
                    track_width_px,
                    index,
                    selected_modulator_arc.as_ref().as_ref(),
                    Vec::new(),
                )
            })
            .into()
        } else {
            Self::render_track_row(
                node.data.clone(),
                track_width_px,
                node.index,
                selected_modulator,
                child_elements,
            )
        }
    }

    fn build_track_tree(entries: &[(usize, TrackViewData)]) -> Vec<TrackNode> {
        let mut children_by_parent: std::collections::HashMap<String, Vec<usize>> =
            std::collections::HashMap::new();
        for (i, (_, data)) in entries.iter().enumerate() {
            if let Some(parent) = data.parent_track.as_deref() {
                children_by_parent
                    .entry(parent.to_string())
                    .or_default()
                    .push(i);
            }
        }

        let mut roots = Vec::new();
        for (i, (_, data)) in entries.iter().enumerate() {
            if data.parent_track.is_none() {
                roots.push(Self::build_subtree(entries, i, &children_by_parent));
            }
        }
        roots
    }

    fn build_subtree(
        entries: &[(usize, TrackViewData)],
        index: usize,
        children_by_parent: &std::collections::HashMap<String, Vec<usize>>,
    ) -> TrackNode {
        let (orig_index, data) = entries[index].clone();
        let mut children = Vec::new();
        if let Some(child_indices) = children_by_parent.get(&data.name) {
            for &child_index in child_indices {
                children.push(Self::build_subtree(
                    entries,
                    child_index,
                    children_by_parent,
                ));
            }
        }
        TrackNode {
            index: orig_index,
            data,
            children,
        }
    }

    pub fn view(
        &self,
        visible_window: VisibleTrackWindow,
        selected_modulator: Option<&crate::state::Modulator>,
    ) -> Element<'_, Message> {
        let (entries, width) = {
            let state = self.state.blocking_read();
            let hovered_resize_track = state.hovered_track_resize_handle.as_deref();
            let soloed_track_names: std::collections::HashSet<String> = state
                .tracks
                .iter()
                .filter(|t| t.soloed)
                .map(|t| t.name.clone())
                .collect();
            let mut upstream_track_names = std::collections::HashSet::new();
            if !soloed_track_names.is_empty() {
                let mut to_process: Vec<String> = soloed_track_names.iter().cloned().collect();
                let mut processed = std::collections::HashSet::new();
                while let Some(target_name) = to_process.pop() {
                    if !processed.insert(target_name.clone()) {
                        continue;
                    }

                    for conn in &state.connections {
                        if conn.kind == maolan_engine::kind::Kind::Audio
                            && conn.to_track == target_name
                            && !soloed_track_names.contains(&conn.from_track)
                        {
                            upstream_track_names.insert(conn.from_track.clone());
                            to_process.push(conn.from_track.clone());
                        }
                    }

                    for track in &state.tracks {
                        if track.parent_track.as_deref() == Some(target_name.as_str())
                            && !soloed_track_names.contains(&track.name)
                        {
                            upstream_track_names.insert(track.name.clone());
                            to_process.push(track.name.clone());
                        }
                    }
                }
            }
            let entries = state
                .tracks
                .iter()
                .enumerate()
                .filter(|(_, track)| {
                    track.name != METRONOME_TRACK_ID
                        && !track.is_inside_closed_folder(&state.tracks)
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
                            phase_inverted: track.phase_inverted,
                            soloed: track.soloed,
                            solo_upstream: upstream_track_names.contains(&track.name),
                            is_master: track.is_master,
                            input_monitor: track.input_monitor.clone(),
                            disk_monitor: track.disk_monitor.clone(),
                            midi_input_monitor: track.midi_input_monitor.clone(),
                            midi_disk_monitor: track.midi_disk_monitor.clone(),
                            audio_ins: track.audio.ins,
                            audio_outs: track.audio.outs,
                            primary_audio_ins: track.primary_audio_ins(),
                            primary_audio_outs: track.primary_audio_outs(),
                            midi_ins: track.midi.ins,
                            midi_outs: track.midi.outs,
                            setup_open: track.setup_open,
                            midi_lane_channels: track.midi_lane_channels.clone(),
                            level: track.level,
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
                            color: track.color,
                            is_folder: track.is_folder,
                            folder_open: track.folder_open,
                            folder_depth: track.folder_depth(&state.tracks),
                            parent_track: track.parent_track.clone(),
                            visible_height: track.visible_height(&state.tracks),
                            hidden: false,
                            effective_muted: track.effective_muted(&state.tracks),
                            effective_soloed: track.effective_soloed(&state.tracks),
                        },
                    )
                })
                .collect::<Vec<_>>();
            (entries, state.tracks_width)
        };
        let track_width_px = match width {
            Length::Fixed(v) => v,
            _ => 200.0,
        };

        let roots = Self::build_track_tree(&entries);

        let root_heights: Vec<f32> = roots.iter().map(|r| r.data.visible_height).collect();
        let start = visible_window.start_index.min(roots.len());
        let end = visible_window.end_index.min(roots.len());
        let top_padding = root_heights[..start].iter().sum::<f32>();
        let bottom_padding = root_heights[end..].iter().sum::<f32>();
        let visible_height = root_heights[start..end].iter().sum::<f32>();
        let total_height = (top_padding + visible_height + bottom_padding).max(1.0);

        let children_elements: Vec<Element<'_, Message>> = roots[start..end]
            .iter()
            .map(|node| Self::render_node(node, track_width_px, selected_modulator, false))
            .collect();

        let mut result = column![];
        if top_padding > 0.0 {
            result = result.push(Space::new().height(Length::Fixed(top_padding)));
        }
        for child in children_elements {
            result = result.push(child);
        }
        if bottom_padding > 0.0 {
            result = result.push(Space::new().height(Length::Fixed(bottom_padding)));
        }
        container(result.width(width))
            .style(|_theme| crate::style::app_background())
            .width(width)
            .height(Length::Fixed(total_height))
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_track_tree_groups_folder_children() {
        let entries: Vec<(usize, TrackViewData)> = vec![
            (
                0,
                TrackViewData {
                    name: "sdfv".to_string(),
                    folder_depth: 0,
                    is_folder: false,
                    ..TrackViewData::default()
                },
            ),
            (
                1,
                TrackViewData {
                    name: "folder".to_string(),
                    folder_depth: 0,
                    is_folder: true,
                    ..TrackViewData::default()
                },
            ),
            (
                2,
                TrackViewData {
                    name: "child".to_string(),
                    folder_depth: 1,
                    parent_track: Some("folder".to_string()),
                    is_folder: false,
                    ..TrackViewData::default()
                },
            ),
        ];
        let roots = Tracks::build_track_tree(&entries);
        assert_eq!(roots.len(), 2);
        assert!(roots[0].children.is_empty());
        assert_eq!(roots[1].data.name, "folder");
        assert_eq!(roots[1].children.len(), 1);
        assert_eq!(roots[1].children[0].data.name, "child");
    }

    #[test]
    fn build_track_tree_handles_child_before_parent() {
        let entries: Vec<(usize, TrackViewData)> = vec![
            (
                0,
                TrackViewData {
                    name: "child".to_string(),
                    folder_depth: 1,
                    parent_track: Some("folder".to_string()),
                    is_folder: false,
                    ..TrackViewData::default()
                },
            ),
            (
                1,
                TrackViewData {
                    name: "folder".to_string(),
                    folder_depth: 0,
                    is_folder: true,
                    ..TrackViewData::default()
                },
            ),
        ];
        let roots = Tracks::build_track_tree(&entries);
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].data.name, "folder");
        assert_eq!(roots[0].children.len(), 1);
        assert_eq!(roots[0].children[0].data.name, "child");
    }
}
