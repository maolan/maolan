use crate::{
    consts::{state_ids::METRONOME_TRACK_ID, workspace_mixer::*},
    message::Message,
    state::{State, Track},
    style,
};
use iced::{
    Alignment, Element, Length,
    widget::{Space, column, container, lazy, mouse_area, row, scrollable, text, text_input},
};
use maolan_engine::message::Action;
use maolan_widgets::{horizontal_slider::horizontal_slider, meters, slider::slider, ticks};

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

struct TrackStripSpec<'a> {
    track: &'a Track,
    width: f32,
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
        let usable_width = (width - (STRIP_NAME_SIDE_PADDING * 2.0)).max(0.0);
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

    fn pan_section(track_name: String, value: f32) -> Element<'static, Message> {
        let on_change_track = track_name.clone();
        row![
            container(text(Self::format_balance(value)).size(9))
                .width(Length::Fixed(24.0))
                .align_x(Alignment::Center),
            horizontal_slider(-1.0..=1.0, value, move |value| {
                Message::Request(Action::TrackBalance(on_change_track.clone(), value))
            })
            .width(Length::Fixed(PAN_SLIDER_WIDTH))
            .height(Length::Fixed(PAN_ROW_HEIGHT)),
        ]
        .spacing(4)
        .align_y(Alignment::Center)
        .into()
    }

    fn quantized_balance_hundredths(balance: f32) -> i16 {
        (balance.clamp(-1.0, 1.0) * 100.0).round() as i16
    }

    fn pan_section_cached(track_name: String, value: f32) -> Element<'static, Message> {
        let dep = (track_name, Self::quantized_balance_hundredths(value));
        lazy(
            dep,
            move |(track_name, value_hundredths)| -> Element<'static, Message> {
                let value = (*value_hundredths as f32) / 100.0;
                Self::pan_section(track_name.clone(), value)
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

    fn fader_bay(
        track_name: String,
        channels: usize,
        levels_db: &[f32],
        value: f32,
        fader_height: f32,
        show_ticks: bool,
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
                    ),
                    move |(track_name, _channels, value_qdb, fader_height_tenths, show_ticks)| -> Element<'static, Message> {
                        let value = Self::qdb_to_level(*value_qdb);
                        let fader_height = *fader_height_tenths as f32 / 10.0;
                        let on_change_track = track_name.clone();
                        let slider = container(
                            slider(FADER_MIN_DB..=FADER_MAX_DB, value, move |value| {
                                Message::Request(Action::TrackLevel(on_change_track.clone(), value))
                            })
                            .width(Length::Fixed(FADER_WIDTH))
                            .height(Length::Fixed(fader_height))
                        )
                        .padding([7.0, 8.0]);

                        if *show_ticks {
                            row![slider, ticks::ticks(FADER_MIN_DB..=FADER_MAX_DB, fader_height)].into()
                        } else {
                            slider.into()
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

    fn fader_bay_cached(
        track_name: String,
        channels: usize,
        levels_db: &[f32],
        value: f32,
        fader_height: f32,
        show_ticks: bool,
    ) -> Element<'static, Message> {
        Self::fader_bay(
            track_name,
            channels,
            levels_db,
            value,
            fader_height,
            show_ticks,
        )
    }

    fn strip_width_for_channels(channels: usize) -> f32 {
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

        let content_width = track_specs.iter().map(|spec| spec.width).sum::<f32>()
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
            let strip_end = strip_start + spec.width;
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
                    .map(|spec| spec.width)
                    .sum::<f32>()
                + (STRIP_SPACING * first_visible as f32)
        };
        let right_spacer = if last_visible >= track_specs.len() {
            0.0
        } else {
            STRIP_ROW_PADDING_X
                + track_specs[last_visible..]
                    .iter()
                    .map(|spec| spec.width)
                    .sum::<f32>()
                + (STRIP_SPACING * (track_specs.len() - last_visible) as f32)
        };

        (first_visible, last_visible, left_spacer, right_spacer)
    }

    fn strip_shell<'a>(
        name: String,
        selected: bool,
        width: f32,
        pan_section: Option<Element<'static, Message>>,
        bay: Element<'static, Message>,
        readout: StripReadout<'a>,
    ) -> Element<'a, Message> {
        let mut content = column![].spacing(8).width(Length::Fill);
        content = content.push(pan_section.unwrap_or_else(Self::pan_placeholder));
        content = content.push(bay).push(Self::value_pill_cached(
            readout.track_name,
            readout.level_label,
            readout.editing,
            readout.edit_input,
        ));
        content = content.push(Self::strip_name_cached(name, width));

        container(content)
            .width(Length::Fixed(width))
            .height(Length::Fill)
            .padding([8, 6])
            .style(move |_theme| style::mixer::strip(selected))
            .into()
    }

    pub fn view<'a>(
        &'a self,
        editing_track: Option<&'a str>,
        editing_input: &'a str,
        viewport_width: f32,
        scroll_x: f32,
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
            .map(|track| TrackStripSpec {
                track,
                width: Self::strip_width_for_channels(track.audio.outs),
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
        let (first_visible, last_visible, left_spacer, right_spacer) =
            Self::visible_track_window(&normal_track_specs, track_viewport_width, scroll_x);

        if metronome_enabled
            && let Some(track) = state
                .tracks
                .iter()
                .find(|track| track.name == METRONOME_TRACK_ID)
        {
            let strip_width = Self::strip_width_for_channels(track.audio.outs);
            let pan = if track.audio.outs == 2 {
                Some(Self::pan_section_cached(track.name.clone(), track.balance))
            } else {
                None
            };
            let bay = Self::fader_bay_cached(
                track.name.clone(),
                track.audio.outs,
                &track.meter_out_db,
                track.level,
                fader_height,
                true,
            );
            metronome_strip = Some(
                mouse_area(Self::strip_shell(
                    track.name.clone(),
                    state.selected.contains(track.name.as_str()),
                    strip_width,
                    pan,
                    bay,
                    StripReadout {
                        track_name: track.name.clone(),
                        editing: editing_track == Some(track.name.as_str()),
                        edit_input: editing_input,
                        level_label: Self::format_level_db(track.level),
                    },
                ))
                .on_press(Message::SelectTrackFromMixer(track.name.clone()))
                .into(),
            );
        }

        for (index, spec) in normal_track_specs.iter().enumerate() {
            let track = spec.track;
            if index < first_visible || index >= last_visible {
                continue;
            }
            let strip_name = track.name.clone();
            let strip_width = spec.width;
            let pan = if track.audio.outs == 2 {
                Some(Self::pan_section_cached(track.name.clone(), track.balance))
            } else {
                None
            };
            let bay = Self::fader_bay_cached(
                track.name.clone(),
                track.audio.outs,
                &track.meter_out_db,
                track.level,
                fader_height,
                true,
            );
            let strip: Element<'a, Message> = mouse_area(Self::strip_shell(
                strip_name,
                state.selected.contains(track.name.as_str()),
                strip_width,
                pan,
                bay,
                StripReadout {
                    track_name: track.name.clone(),
                    editing: editing_track == Some(track.name.as_str()),
                    edit_input: editing_input,
                    level_label: Self::format_level_db(track.level),
                },
            ))
            .on_press(Message::SelectTrackFromMixer(track.name.clone()))
            .into();

            strips = strips.push(strip);
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
            master_strip_width,
            if hw_out_channels == 2 {
                Some(Self::pan_section_cached(
                    "hw:out".to_string(),
                    hw_out_balance,
                ))
            } else {
                None
            },
            Self::fader_bay_cached(
                "hw:out".to_string(),
                hw_out_channels.max(1),
                &state.hw_out_meter_db,
                hw_out_level,
                fader_height,
                true,
            ),
            StripReadout {
                track_name: "hw:out".to_string(),
                editing: editing_track == Some("hw:out"),
                edit_input: editing_input,
                level_label: Self::format_level_db(hw_out_level),
            },
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
