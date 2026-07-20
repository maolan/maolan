use crate::{
    consts::{state_ids::METRONOME_TRACK_ID, workspace_mixer::*},
    message::{Message, TrackAutomationTarget},
    state::{Modulator, State, Track},
    style,
};
use iced::{
    Alignment, Element, Length, Padding,
    widget::{
        Row, Space, Stack, button, column, container, lazy, mouse_area, row, scrollable, text,
        text_input,
    },
};
use maolan_engine::message::{Action, TrackMidiLearnTarget};
use maolan_widgets::{horizontal_slider::horizontal_slider, meters, slider::slider, ticks};
use std::collections::{HashMap, HashSet};

const STRIP_SPACING: f32 = 2.0;
const STRIP_ROW_PADDING_X: f32 = 8.0;
const MIXER_OVERSCAN_PX: f32 = 160.0;

#[derive(Debug, Default)]
pub struct Mixer {
    state: State,
}

struct StripReadout<'a> {
    track_name: String,
    editing: bool,
    edit_input: &'a str,
    level_label: &'static str,
}

#[derive(Clone, Copy, Hash)]
struct ModulatorAssignment {
    assignable: bool,
    assigned: bool,
    selected_id: Option<usize>,
}

#[derive(Clone, Copy)]
struct TrackStripSpec<'a> {
    track: &'a Track,
    width: f32,
    total_width: f32,
}

struct RenderContext<'a> {
    editing_track: Option<&'a str>,
    editing_input: &'a str,
    fader_height: f32,
    modulators_pane_visible: bool,
    selected_modulator: Option<&'a Modulator>,
}

impl Mixer {
    pub fn new(state: State) -> Self {
        Self { state }
    }

    fn fader_height_from_panel(height: Length) -> f32 {
        match height {
            Length::Fixed(panel_h) => (panel_h - 146.0).max(92.0),
            _ => 160.0,
        }
    }

    fn trim_strip_name(name: &str, width: f32) -> String {
        const STRIP_SHELL_H_PAD: f32 = 6.0;
        const STRIP_NAME_H_PAD: f32 = 2.0;
        let total_pad = (STRIP_SHELL_H_PAD + STRIP_NAME_H_PAD) * 2.0;
        let usable_width = (width - total_pad).max(0.0);
        let max_chars = (usable_width / STRIP_NAME_CHAR_PX).floor() as usize;
        if max_chars == 0 {
            String::new()
        } else {
            name.chars().take(max_chars).collect()
        }
    }

    fn strip_name(name: String, width: f32) -> Element<'static, Message> {
        container(text(Self::trim_strip_name(&name, width)).size(12))
            .width(Length::Fill)
            .align_x(Alignment::Center)
            .padding([0, 2])
            .into()
    }

    fn strip_name_cached(name: String, width: f32) -> Element<'static, Message> {
        let dep = (
            name,
            (width.max(0.0) * 10.0).round().clamp(0.0, u16::MAX as f32) as u16,
        );
        lazy(dep, |(name, width_tenths)| -> Element<'static, Message> {
            Self::strip_name(name.clone(), *width_tenths as f32 / 10.0)
        })
        .into()
    }

    fn format_level_db(level: f32) -> &'static str {
        if level <= FADER_MIN_DB {
            "-inf dB"
        } else {
            let clamped = level.clamp(FADER_MIN_DB, FADER_MAX_DB);
            let idx = ((clamped - FADER_MIN_DB) * 10.0).round() as usize;
            LEVEL_LABELS[idx.min(LEVEL_LABELS.len() - 1)]
        }
    }

    fn format_balance(balance: f32) -> &'static str {
        let b = balance.clamp(-1.0, 1.0);
        let idx = ((b * 100.0).round() as i32 + 100).clamp(0, 200) as usize;
        BALANCE_LABELS[idx]
    }

    fn format_lufs(value: f32) -> String {
        if value <= maolan_engine::LoudnessValues::SILENCE + 0.1 {
            "-inf".to_string()
        } else {
            format!("{:.1}", value)
        }
    }

    fn lufs_readout(lufs: Option<maolan_engine::LoudnessValues>) -> Element<'static, Message> {
        let (momentary, short_term, integrated) = lufs.map_or(
            (
                maolan_engine::LoudnessValues::SILENCE,
                maolan_engine::LoudnessValues::SILENCE,
                maolan_engine::LoudnessValues::SILENCE,
            ),
            |l| (l.momentary, l.short_term, l.integrated),
        );
        container(
            row![
                text(Self::format_lufs(momentary).to_string()).size(10),
                text(Self::format_lufs(short_term).to_string()).size(10),
                text(Self::format_lufs(integrated).to_string()).size(10),
            ]
            .spacing(6),
        )
        .width(Length::Fill)
        .align_x(Alignment::Center)
        .into()
    }

    fn value_pill<'a>(
        track_name: String,
        content: &'static str,
        editing: bool,
        edit_input: &'a str,
    ) -> Element<'a, Message> {
        if editing {
            container(
                text_input("dB", edit_input)
                    .on_input(Message::MixerLevelEditInput)
                    .on_submit(Message::MixerLevelEditCommit)
                    .padding([2, 4])
                    .size(11),
            )
            .width(Length::Fixed(READOUT_WIDTH))
            .style(|_theme| style::mixer::readout())
            .into()
        } else {
            mouse_area(
                container(text(content).size(11))
                    .width(Length::Fixed(READOUT_WIDTH))
                    .padding([4, 6])
                    .align_x(Alignment::Center)
                    .style(|_theme| style::mixer::readout()),
            )
            .on_press(Message::MixerLevelEditStart(track_name))
            .into()
        }
    }

    fn value_pill_cached<'a>(
        track_name: String,
        content: &'static str,
        editing: bool,
        edit_input: &'a str,
    ) -> Element<'a, Message> {
        if editing {
            return Self::value_pill(track_name, content, true, edit_input);
        }
        let dep = (track_name, content);
        lazy(dep, |(track_name, content)| -> Element<'static, Message> {
            mouse_area(
                container(text(*content).size(11))
                    .width(Length::Fixed(READOUT_WIDTH))
                    .padding([4, 6])
                    .align_x(Alignment::Center)
                    .style(|_theme| style::mixer::readout()),
            )
            .on_press(Message::MixerLevelEditStart(track_name.clone()))
            .into()
        })
        .into()
    }

    fn pan_section(
        track_name: String,
        value: f32,
        assignable: bool,
        assigned: bool,
        selected_id: Option<usize>,
    ) -> Element<'static, Message> {
        let on_change_track = track_name.clone();
        let learn_track = track_name.clone();
        let show_message = selected_id.map(|id| Message::ModulatorTargetShow {
            modulator_id: id,
            track_name: track_name.clone(),
            target: TrackAutomationTarget::Balance,
        });
        let slider = mouse_area(
            horizontal_slider(-1.0..=1.0, value, move |value| {
                Message::Request(Action::TrackBalance(on_change_track.clone(), value))
            })
            .width(Length::Fixed(PAN_SLIDER_WIDTH))
            .height(Length::Fixed(PAN_ROW_HEIGHT))
            .double_click_reset(0.0),
        )
        .on_right_press(Message::TrackMidiLearnArm {
            track_name: learn_track,
            target: TrackMidiLearnTarget::Balance,
        });
        let control: Element<'static, Message> = if assignable {
            Stack::new()
                .push(slider)
                .push(
                    mouse_area(
                        container(Space::new().width(Length::Fill).height(Length::Fill))
                            .style(move |_theme| style::mixer::modulator_target(assigned)),
                    )
                    .on_press(show_message.unwrap()),
                )
                .into()
        } else {
            slider.into()
        };
        row![
            container(text(Self::format_balance(value)).size(9))
                .width(Length::Fixed(24.0))
                .align_x(Alignment::Center),
            control,
        ]
        .spacing(4)
        .align_y(Alignment::Center)
        .into()
    }

    fn quantized_balance_hundredths(balance: f32) -> i16 {
        (balance.clamp(-1.0, 1.0) * 100.0).round() as i16
    }

    fn pan_section_cached(
        track_name: String,
        value: f32,
        modulators_pane_visible: bool,
        selected_modulator: Option<&crate::state::Modulator>,
    ) -> Element<'static, Message> {
        let assignment = Self::modulator_assignment_state(
            modulators_pane_visible,
            selected_modulator,
            &track_name,
            TrackAutomationTarget::Balance,
        );
        let dep = (
            track_name,
            Self::quantized_balance_hundredths(value),
            assignment,
        );
        lazy(
            dep,
            move |(track_name, value_hundredths, assignment)| -> Element<'static, Message> {
                let value = (*value_hundredths as f32) / 100.0;
                let ModulatorAssignment {
                    assignable,
                    assigned,
                    selected_id,
                } = *assignment;
                Self::pan_section(track_name.clone(), value, assignable, assigned, selected_id)
            },
        )
        .into()
    }

    fn pan_placeholder() -> Element<'static, Message> {
        Space::new()
            .width(Length::Fill)
            .height(Length::Fixed(PAN_ROW_HEIGHT))
            .into()
    }

    fn level_to_qdb(level_db: f32) -> u8 {
        (level_db
            .clamp(FADER_MIN_DB, FADER_MAX_DB)
            .round()
            .max(FADER_MIN_DB) as i16)
            .saturating_add(90)
            .clamp(0, 110) as u8
    }

    fn qdb_to_level(q: u8) -> f32 {
        q as f32 - 90.0
    }

    fn modulator_assignment_state(
        modulators_pane_visible: bool,
        selected_modulator: Option<&crate::state::Modulator>,
        track_name: &str,
        target: TrackAutomationTarget,
    ) -> ModulatorAssignment {
        let assignable = modulators_pane_visible && selected_modulator.is_some();
        let id = selected_modulator.map(|m| m.id);
        let assigned = selected_modulator.is_some_and(|m| {
            m.targets
                .iter()
                .any(|t| t.matches_target(track_name, &target))
        });
        ModulatorAssignment {
            assignable,
            assigned,
            selected_id: id,
        }
    }

    fn fader_bay(
        track_name: String,
        channels: usize,
        levels_db: &[f32],
        value: f32,
        fader_height: f32,
        show_ticks: bool,
        assignment: ModulatorAssignment,
    ) -> Element<'static, Message> {
        let channels = channels.max(1);
        container(
            row![
                lazy(
                    (
                        track_name.clone(),
                        channels,
                        Self::level_to_qdb(value),
                        (fader_height.max(0.0) * 10.0).round() as u16,
                        show_ticks,
                        assignment,
                    ),
                    move |(
                        track_name,
                        _channels,
                        value_qdb,
                        fader_height_tenths,
                        show_ticks,
                        assignment,
                    )|
                          -> Element<'static, Message> {
                        let value = Self::qdb_to_level(*value_qdb);
                        let fader_height = *fader_height_tenths as f32 / 10.0;
                        let ModulatorAssignment {
                            assignable,
                            assigned,
                            selected_id,
                        } = *assignment;
                        let on_change_track = track_name.clone();
                        let learn_track = track_name.clone();
                        let slider = mouse_area(
                            container(
                                slider(FADER_MIN_DB..=FADER_MAX_DB, value, move |value| {
                                    Message::Request(Action::TrackLevel(
                                        on_change_track.clone(),
                                        value,
                                    ))
                                })
                                .width(Length::Fixed(FADER_WIDTH))
                                .height(Length::Fixed(fader_height))
                                .double_click_reset(0.0),
                            )
                            .padding([7.0, 8.0]),
                        )
                        .on_right_press(Message::TrackMidiLearnArm {
                            track_name: learn_track,
                            target: TrackMidiLearnTarget::Volume,
                        });
                        let control = if assignable {
                            Stack::new()
                                .push(slider)
                                .push(
                                    mouse_area(
                                        container(
                                            Space::new().width(Length::Fill).height(Length::Fill),
                                        )
                                        .style(
                                            move |_theme| style::mixer::modulator_target(assigned),
                                        ),
                                    )
                                    .on_press(
                                        Message::ModulatorTargetShow {
                                            modulator_id: selected_id.unwrap(),
                                            track_name: track_name.clone(),
                                            target: TrackAutomationTarget::Volume,
                                        },
                                    ),
                                )
                                .into()
                        } else {
                            slider.into()
                        };

                        if *show_ticks {
                            row![
                                control,
                                ticks::ticks(FADER_MIN_DB..=FADER_MAX_DB, fader_height)
                            ]
                            .into()
                        } else {
                            control
                        }
                    },
                ),
                meters::meters(channels, levels_db, fader_height),
            ]
            .spacing(8.0),
        )
        .width(Length::Fill)
        .padding(BAY_INSET)
        .style(|_theme| style::mixer::bay())
        .into()
    }

    pub fn strip_width_for_channels(channels: usize) -> f32 {
        (FADER_WIDTH
            + SCALE_WIDTH
            + 3.0
            + 8.0
            + meters::total_width(channels.max(1))
            + 16.0
            + (BAY_INSET * 2.0))
            .max(STRIP_WIDTH)
    }

    fn output_strips_width(
        metronome_width: Option<f32>,
        master_channels: usize,
        metronome_enabled: bool,
    ) -> f32 {
        let strip_width = Self::strip_width_for_channels(master_channels.max(1));
        let mut widths = vec![strip_width];
        if metronome_enabled && let Some(width) = metronome_width {
            widths.insert(0, width);
        }
        let spacing_count = widths.len().saturating_sub(1) as f32;
        widths.iter().copied().sum::<f32>() + (STRIP_SPACING * spacing_count)
    }

    fn visible_track_window(
        track_specs: &[TrackStripSpec<'_>],
        viewport_width: f32,
        scroll_x: f32,
    ) -> (usize, usize, f32, f32) {
        if track_specs.is_empty() {
            return (0, 0, 0.0, 0.0);
        }

        let content_width = track_specs.iter().map(|spec| spec.total_width).sum::<f32>()
            + (STRIP_SPACING * track_specs.len().saturating_sub(1) as f32)
            + (STRIP_ROW_PADDING_X * 2.0);
        if viewport_width <= 0.0 || content_width <= viewport_width {
            return (0, track_specs.len(), 0.0, 0.0);
        }

        let max_scroll = (content_width - viewport_width).max(0.0);
        let left_edge = (scroll_x.clamp(0.0, 1.0) * max_scroll - MIXER_OVERSCAN_PX).max(0.0);
        let right_edge =
            (left_edge + viewport_width + (MIXER_OVERSCAN_PX * 2.0)).min(content_width);

        let mut current_x = STRIP_ROW_PADDING_X;
        let mut first_visible = track_specs.len();
        let mut last_visible = 0usize;
        for (idx, spec) in track_specs.iter().enumerate() {
            let strip_start = current_x;
            let strip_end = strip_start + spec.total_width;
            if strip_end >= left_edge && strip_start <= right_edge {
                first_visible = first_visible.min(idx);
                last_visible = idx + 1;
            }
            current_x = strip_end + STRIP_SPACING;
        }

        if first_visible == track_specs.len() {
            return (0, track_specs.len(), 0.0, 0.0);
        }

        let left_spacer = if first_visible == 0 {
            0.0
        } else {
            STRIP_ROW_PADDING_X
                + track_specs[..first_visible]
                    .iter()
                    .map(|spec| spec.total_width)
                    .sum::<f32>()
                + (STRIP_SPACING * first_visible as f32)
        };
        let right_spacer = if last_visible >= track_specs.len() {
            0.0
        } else {
            STRIP_ROW_PADDING_X
                + track_specs[last_visible..]
                    .iter()
                    .map(|spec| spec.total_width)
                    .sum::<f32>()
                + (STRIP_SPACING * (track_specs.len() - last_visible) as f32)
        };

        (first_visible, last_visible, left_spacer, right_spacer)
    }

    fn total_width_with_children(
        specs: &[TrackStripSpec<'_>],
        children_by_parent: &HashMap<String, Vec<usize>>,
        index: usize,
    ) -> f32 {
        let spec = &specs[index];
        let mut total = spec.width;
        if spec.track.is_folder && !spec.track.folder_open {
            return total;
        }
        if let Some(children) = children_by_parent.get(&spec.track.name) {
            for &child_index in children {
                total += STRIP_SPACING
                    + Self::total_width_with_children(specs, children_by_parent, child_index);
            }
        }
        total
    }

    fn render_track_strip<'a>(
        spec: &TrackStripSpec<'_>,
        children_by_parent: &HashMap<String, Vec<usize>>,
        all_specs: &[TrackStripSpec<'_>],
        ctx: &RenderContext<'a>,
        selected: &HashSet<String>,
        upstream_track_names: &HashSet<String>,
        last_child_of_parent: bool,
    ) -> Element<'a, Message> {
        let track = spec.track;
        let pan = if track.audio.outs == 2 {
            Some(Self::pan_section_cached(
                track.name.clone(),
                track.balance,
                ctx.modulators_pane_visible,
                ctx.selected_modulator,
            ))
        } else {
            None
        };
        let assignment = Self::modulator_assignment_state(
            ctx.modulators_pane_visible,
            ctx.selected_modulator,
            &track.name,
            TrackAutomationTarget::Volume,
        );
        let bay = Self::fader_bay(
            track.name.clone(),
            track.audio.outs,
            &track.meter_out_db,
            track.level,
            ctx.fader_height,
            true,
            assignment,
        );
        let solo_upstream = upstream_track_names.contains(&track.name);
        let children = if track.is_folder && track.folder_open {
            children_by_parent.get(&track.name).map(|indices| {
                let child_elements: Vec<Element<'a, Message>> = indices
                    .iter()
                    .enumerate()
                    .map(|(pos, &i)| {
                        Self::render_track_strip(
                            &all_specs[i],
                            children_by_parent,
                            all_specs,
                            ctx,
                            selected,
                            upstream_track_names,
                            pos + 1 == indices.len(),
                        )
                    })
                    .collect();
                container(
                    Row::with_children(child_elements)
                        .spacing(STRIP_SPACING)
                        .align_y(Alignment::Start),
                )
                .padding(Padding {
                    top: 10.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 0.0,
                })
                .height(Length::Fill)
                .width(Length::Shrink)
                .into()
            })
        } else {
            None
        };
        let parent_selected = track
            .parent_track
            .as_deref()
            .is_some_and(|parent| selected.contains(parent));
        let shell: Element<'a, Message> = Self::strip_shell(
            track.name.clone(),
            selected.contains(track.name.as_str()),
            track.color,
            spec.width,
            spec.total_width,
            pan,
            bay,
            StripReadout {
                track_name: track.name.clone(),
                editing: ctx.editing_track == Some(track.name.as_str()),
                edit_input: ctx.editing_input,
                level_label: Self::format_level_db(track.level),
            },
            Self::strip_controls(track, solo_upstream),
            children,
            None,
        );
        // iced borders are uniform on all sides, so highlight only the top
        // and bottom edges of a strip whose immediate parent folder is
        // selected with flush bars above and below the strip. The far-right
        // child additionally gets a right-edge bar. It is overlaid with a
        // Stack because the folder's children row is shrink-wrapped
        // vertically and a sibling Fill bar would collapse to zero height;
        // it is inset by the corner radius so it clears the rounded corners.
        let shell: Element<'a, Message> = if parent_selected {
            let bar = || {
                container(Space::new().width(Length::Fill).height(Length::Fixed(2.0)))
                    .width(Length::Fill)
                    .style(|_| style::mixer::strip_parent_edge_highlight())
            };
            let with_hbars: Element<'a, Message> =
                column![bar(), shell, bar()].height(Length::Fill).into();
            if last_child_of_parent {
                let right_edge = container(
                    column![
                        Space::new().height(Length::Fixed(style::mixer::STRIP_CORNER_RADIUS)),
                        container(Space::new().width(Length::Fixed(2.0)).height(Length::Fill))
                            .height(Length::Fill)
                            .style(|_| style::mixer::strip_parent_edge_highlight()),
                        Space::new().height(Length::Fixed(style::mixer::STRIP_CORNER_RADIUS)),
                    ]
                    .height(Length::Fill),
                )
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::End);
                Stack::new().push(with_hbars).push(right_edge).into()
            } else {
                with_hbars
            }
        } else {
            shell
        };
        mouse_area(shell)
            .on_press(Message::SelectTrackFromMixer(track.name.clone()))
            .on_double_click(if track.is_folder {
                Message::OpenFolderConnections(track.name.clone())
            } else {
                Message::OpenTrackPlugins(track.name.clone())
            })
            .into()
    }

    fn strip_controls(track: &Track, solo_upstream: bool) -> Option<Element<'static, Message>> {
        if track.is_master || track.name == METRONOME_TRACK_ID {
            return None;
        }
        let track_name = track.name.clone();
        let muted = track.muted;
        let soloed = track.soloed;
        let armed = track.armed;
        let mut controls = row![].spacing(4).align_y(Alignment::Center);
        let mute_track = track_name.clone();
        controls = controls.push(
            mouse_area(
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
                .style(move |theme, _state| style::mute::style(theme, muted))
                .on_press(Message::Request(Action::TrackToggleMute(
                    track_name.clone(),
                ))),
            )
            .on_right_press(Message::TrackMidiLearnArm {
                track_name: mute_track,
                target: TrackMidiLearnTarget::Mute,
            }),
        );
        let track_name = track.name.clone();
        let solo_track = track_name.clone();
        controls = controls.push(
            mouse_area(
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
                .style(move |theme, _state| style::solo::style(theme, soloed, solo_upstream))
                .on_press(Message::Request(Action::TrackToggleSolo(
                    track_name.clone(),
                ))),
            )
            .on_right_press(Message::TrackMidiLearnArm {
                track_name: solo_track,
                target: TrackMidiLearnTarget::Solo,
            }),
        );
        if !track.is_folder {
            let track_name = track.name.clone();
            let arm_track = track_name.clone();
            controls = controls.push(
                mouse_area(
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
                    .style(move |theme, _state| style::arm::style(theme, armed))
                    .on_press(Message::Request(Action::TrackToggleArm(track_name.clone()))),
                )
                .on_right_press(Message::TrackMidiLearnArm {
                    track_name: arm_track,
                    target: TrackMidiLearnTarget::Arm,
                }),
            );
        }
        Some(controls.into())
    }

    #[allow(clippy::too_many_arguments)]
    fn strip_shell<'a>(
        name: String,
        selected: bool,
        color: Option<iced::Color>,
        left_width: f32,
        total_width: f32,
        pan_section: Option<Element<'static, Message>>,
        bay: Element<'static, Message>,
        readout: StripReadout<'a>,
        controls: Option<Element<'static, Message>>,
        children: Option<Element<'a, Message>>,
        loudness_section: Option<Element<'static, Message>>,
    ) -> Element<'a, Message> {
        let has_loudness = loudness_section.is_some();
        let mut left_content = column![].spacing(8).width(Length::Fill);
        if let Some(loudness_section) = loudness_section {
            left_content = left_content.push(container(loudness_section).padding(Padding {
                top: 8.0,
                right: 0.0,
                bottom: 8.0,
                left: 0.0,
            }));
        }
        if let Some(pan) = pan_section {
            if !has_loudness {
                left_content = left_content.push(container(pan).padding(Padding {
                    top: 8.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 0.0,
                }));
            } else {
                left_content = left_content.push(pan);
            }
        } else if !has_loudness {
            left_content = left_content.push(Self::pan_placeholder());
        }
        left_content = left_content.push(bay).push(Self::value_pill_cached(
            readout.track_name,
            readout.level_label,
            readout.editing,
            readout.edit_input,
        ));
        if let Some(controls) = controls {
            left_content = left_content.push(controls);
        }
        left_content = left_content.push(
            container(Self::strip_name_cached(name, left_width)).padding(Padding {
                top: 0.0,
                right: 0.0,
                bottom: 8.0,
                left: 0.0,
            }),
        );

        let left = container(left_content)
            .width(Length::Fixed(left_width))
            .height(Length::Fill)
            .padding(Padding {
                top: 0.0,
                right: 6.0,
                bottom: 0.0,
                left: 6.0,
            });

        let content: Element<'a, Message> = if let Some(children) = children {
            let right = container(children)
                .padding(Padding {
                    top: 10.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 0.0,
                })
                .height(Length::Fill)
                .width(Length::Shrink);
            row![left, right]
                .spacing(STRIP_SPACING)
                .align_y(Alignment::Start)
                .height(Length::Fill)
                .into()
        } else {
            left.into()
        };

        container(content)
            .width(Length::Fixed(total_width))
            .height(Length::Fill)
            .style(move |_theme| style::mixer::strip(selected, color))
            .into()
    }

    pub fn view<'a>(
        &'a self,
        editing_track: Option<&'a str>,
        editing_input: &'a str,
        viewport_width: f32,
        scroll_x: f32,
        modulators_pane_visible: bool,
        selected_modulator: Option<&'a crate::state::Modulator>,
    ) -> Element<'a, Message> {
        let mut strips = row![].spacing(2).align_y(Alignment::Start);
        let state = self.state.blocking_read();
        let height = state.mixer_height;
        let hw_out_channels = state.hw_out.as_ref().map(|hw| hw.channels).unwrap_or(0);
        let hw_out_level = state.hw_out_level;
        let hw_out_balance = state.hw_out_balance;
        let master_selected = state.selected.contains("hw:out");
        let fader_height = Self::fader_height_from_panel(height);
        let metronome_enabled = state.metronome_enabled;
        let mut metronome_strip: Option<Element<'a, Message>> = None;
        let track_specs: Vec<_> = state
            .tracks
            .iter()
            .map(|track| {
                let width = Self::strip_width_for_channels(track.audio.outs);
                TrackStripSpec {
                    track,
                    width,
                    total_width: width,
                }
            })
            .collect();
        let metronome_width = track_specs
            .iter()
            .find(|spec| spec.track.name == METRONOME_TRACK_ID)
            .map(|spec| spec.width);
        let output_strips_width =
            Self::output_strips_width(metronome_width, hw_out_channels, metronome_enabled);
        let track_viewport_width = (viewport_width - output_strips_width).max(0.0);
        let normal_track_specs: Vec<_> = track_specs
            .into_iter()
            .filter(|spec| spec.track.name != METRONOME_TRACK_ID)
            .collect();

        let mut children_by_parent: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, spec) in normal_track_specs.iter().enumerate() {
            if let Some(parent) = spec.track.parent_track.as_deref() {
                children_by_parent
                    .entry(parent.to_string())
                    .or_default()
                    .push(i);
            }
        }

        let total_widths: Vec<f32> = (0..normal_track_specs.len())
            .map(|i| Self::total_width_with_children(&normal_track_specs, &children_by_parent, i))
            .collect();
        let normal_track_specs: Vec<_> = normal_track_specs
            .into_iter()
            .enumerate()
            .map(|(i, mut spec)| {
                spec.total_width = total_widths[i];
                spec
            })
            .collect();

        let root_specs: Vec<TrackStripSpec> = normal_track_specs
            .iter()
            .filter(|spec| spec.track.parent_track.is_none())
            .copied()
            .collect();

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
            }
        }

        let ctx = RenderContext {
            editing_track,
            editing_input,
            fader_height,
            modulators_pane_visible,
            selected_modulator,
        };

        let (first_visible, last_visible, left_spacer, right_spacer) =
            Self::visible_track_window(&root_specs, track_viewport_width, scroll_x);

        if metronome_enabled
            && let Some(track) = state
                .tracks
                .iter()
                .find(|track| track.name == METRONOME_TRACK_ID)
        {
            let strip_width = Self::strip_width_for_channels(track.audio.outs);
            let pan = if track.audio.outs == 2 {
                Some(Self::pan_section_cached(
                    track.name.clone(),
                    track.balance,
                    modulators_pane_visible,
                    selected_modulator,
                ))
            } else {
                None
            };
            let assignment = Self::modulator_assignment_state(
                modulators_pane_visible,
                selected_modulator,
                &track.name,
                TrackAutomationTarget::Volume,
            );
            let bay = Self::fader_bay(
                track.name.clone(),
                track.audio.outs,
                &track.meter_out_db,
                track.level,
                fader_height,
                true,
                assignment,
            );
            metronome_strip = Some(
                mouse_area(Self::strip_shell(
                    track.name.clone(),
                    state.selected.contains(track.name.as_str()),
                    track.color,
                    strip_width,
                    strip_width,
                    pan,
                    bay,
                    StripReadout {
                        track_name: track.name.clone(),
                        editing: editing_track == Some(track.name.as_str()),
                        edit_input: editing_input,
                        level_label: Self::format_level_db(track.level),
                    },
                    None,
                    None,
                    None,
                ))
                .on_press(Message::SelectTrackFromMixer(track.name.clone()))
                .into(),
            );
        }

        for (index, spec) in root_specs.iter().enumerate() {
            if index < first_visible || index >= last_visible {
                continue;
            }
            strips = strips.push(Self::render_track_strip(
                spec,
                &children_by_parent,
                &normal_track_specs,
                &ctx,
                &state.selected,
                &upstream_track_names,
                false,
            ));
        }
        if left_spacer > 0.0 {
            strips = row![Space::new().width(Length::Fixed(left_spacer)), strips]
                .spacing(STRIP_SPACING)
                .align_y(Alignment::Start);
        }
        if right_spacer > 0.0 {
            strips = strips.push(Space::new().width(Length::Fixed(right_spacer)));
        }

        let master_strip_width = Self::strip_width_for_channels(hw_out_channels.max(1));
        let master_strip: Element<'a, Message> = mouse_area(Self::strip_shell(
            "Master".to_string(),
            master_selected,
            None,
            master_strip_width,
            master_strip_width,
            if hw_out_channels == 2 {
                Some(Self::pan_section_cached(
                    "hw:out".to_string(),
                    hw_out_balance,
                    modulators_pane_visible,
                    selected_modulator,
                ))
            } else {
                None
            },
            {
                let assignment = Self::modulator_assignment_state(
                    modulators_pane_visible,
                    selected_modulator,
                    "hw:out",
                    TrackAutomationTarget::Volume,
                );
                Self::fader_bay(
                    "hw:out".to_string(),
                    hw_out_channels.max(1),
                    &state.hw_out_meter_db,
                    hw_out_level,
                    fader_height,
                    true,
                    assignment,
                )
            },
            StripReadout {
                track_name: "hw:out".to_string(),
                editing: editing_track == Some("hw:out"),
                edit_input: editing_input,
                level_label: Self::format_level_db(hw_out_level),
            },
            None,
            None,
            Some(Self::lufs_readout(state.hw_out_lufs)),
        ))
        .on_press(Message::SelectTrackFromMixer("hw:out".to_string()))
        .into();
        let mut output_strips = row![].spacing(2).align_y(Alignment::Start);
        if let Some(strip) = metronome_strip {
            output_strips = output_strips.push(strip);
        }
        output_strips = output_strips.push(master_strip);

        let track_strips = scrollable(
            row![strips, Space::new().width(Length::Fill)]
                .height(height)
                .padding([8, 6])
                .align_y(Alignment::Start),
        )
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::new(),
        ))
        .on_scroll(|viewport| Message::MixerScrollXChanged(viewport.relative_offset().x))
        .width(Length::Fill)
        .height(height);

        container(
            mouse_area(
                row![track_strips, output_strips]
                    .height(height)
                    .align_y(Alignment::Start),
            )
            .on_press(Message::DeselectAll),
        )
        .style(|_theme| crate::style::app_background())
        .width(Length::Fill)
        .height(height)
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fader_height_from_panel_fixed() {
        let height = Mixer::fader_height_from_panel(Length::Fixed(400.0));
        assert!(height >= 92.0);
    }

    #[test]
    fn fader_height_from_panel_fill() {
        let height = Mixer::fader_height_from_panel(Length::Fill);
        assert_eq!(height, 160.0);
    }

    #[test]
    fn trim_strip_name_truncates_long_names() {
        let name = "VeryLongTrackNameThatExceedsWidth";
        let trimmed = Mixer::trim_strip_name(name, 50.0);
        assert!(trimmed.len() < name.len());
    }

    #[test]
    fn trim_strip_name_keeps_short_names() {
        let name = "Short";
        let trimmed = Mixer::trim_strip_name(name, 200.0);
        assert_eq!(trimmed, name);
    }

    #[test]
    fn trim_strip_name_returns_empty_for_zero_width() {
        let trimmed = Mixer::trim_strip_name("Name", 0.0);
        assert!(trimmed.is_empty());
    }

    #[test]
    fn format_level_db_inf() {
        let label = Mixer::format_level_db(-100.0);
        assert_eq!(label, "-inf dB");
    }

    #[test]
    fn format_level_db_clamps_min() {
        let label = Mixer::format_level_db(-95.0);
        assert_eq!(label, "-inf dB");
    }

    #[test]
    fn format_level_db_valid_range() {
        let label = Mixer::format_level_db(0.0);
        assert!(!label.is_empty());
    }

    #[test]
    fn format_balance_center() {
        let label = Mixer::format_balance(0.0);
        assert_eq!(label, "C");
    }

    #[test]
    fn format_balance_left() {
        let label = Mixer::format_balance(-0.5);
        assert!(label.starts_with('L'));
    }

    #[test]
    fn format_balance_right() {
        let label = Mixer::format_balance(0.5);
        assert!(label.starts_with('R'));
    }

    #[test]
    fn format_balance_clamps() {
        let label_min = Mixer::format_balance(-10.0);
        let label_max = Mixer::format_balance(10.0);
        assert!(!label_min.is_empty());
        assert!(!label_max.is_empty());
    }

    #[test]
    fn mixer_new_creates_instance() {
        let state = crate::state::State::default();
        let mixer = Mixer::new(state);
        let _ = &mixer;
    }

    #[test]
    fn mixer_default_creates_instance() {
        let mixer: Mixer = Default::default();
        let _ = &mixer;
    }
}
