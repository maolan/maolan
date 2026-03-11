use crate::{
    consts::message_lists::SNAP_MODE_ALL,
    consts::workspace::MIDI_CLIP_BORDER,
    message::{Message, SnapMode},
};
use iced::{
    Alignment, Background, Border, Color, Length, Theme,
    widget::{button, container, pick_list, row, text, text_input},
};
use iced_fonts::lucide::{
    audio_lines, brackets, cable, circle, fast_forward, pause, play, repeat, rewind, square,
    triangle,
};
#[derive(Debug, Default)]
pub struct Toolbar;

#[derive(Debug, Clone)]
pub struct ToolbarViewState {
    pub playing: bool,
    pub paused: bool,
    pub recording: bool,
    pub metronome_enabled: bool,
    pub has_session_end: bool,
    pub has_loop_range: bool,
    pub loop_enabled: bool,
    pub has_punch_range: bool,
    pub punch_enabled: bool,
    pub snap_mode: SnapMode,
    pub tempo_input: String,
    pub tsig_num_input: String,
    pub tsig_denom_input: String,
    pub playhead_time_label: String,
    pub playhead_bar: u64,
    pub playhead_beat: u64,
}

impl Toolbar {
    pub fn new() -> Self {
        Self
    }
    pub fn update(&mut self, _message: &Message) {}

    fn button_style(
        enabled: bool,
        active: bool,
        active_color: Color,
    ) -> impl Fn(&Theme, button::Status) -> button::Style + Copy {
        move |theme: &Theme, _status: button::Status| {
            let status = if enabled {
                button::Status::Active
            } else {
                button::Status::Disabled
            };
            let mut style = button::secondary(theme, status);
            style.border.radius = 3.0.into();
            style.border.width = 1.0;
            style.border.color = Color::BLACK;
            style.text_color = if enabled {
                Color::from_rgb(0.92, 0.92, 0.92)
            } else {
                Color::from_rgba(0.92, 0.92, 0.92, 0.45)
            };
            style.background = Some(Background::Color(if active {
                active_color
            } else {
                Color::TRANSPARENT
            }));
            style
        }
    }

    pub fn view(&self, view_state: ToolbarViewState) -> iced::Element<'_, Message> {
        let play_active = view_state.playing && !view_state.paused;
        let pause_active = view_state.playing && view_state.paused;
        let stop_active = !view_state.playing && !view_state.paused;
        let rec_active = view_state.recording;
        let metronome_active = view_state.metronome_enabled;
        let loop_active = view_state.has_loop_range && view_state.loop_enabled;
        let punch_active = view_state.has_punch_range && view_state.punch_enabled;
        let loop_button = if view_state.has_loop_range {
            button(repeat())
                .style(Self::button_style(
                    view_state.has_loop_range,
                    loop_active,
                    Color::from_rgba(0.2, 0.55, 0.9, 0.35),
                ))
                .on_press(Message::ToggleLoop)
        } else {
            button(repeat()).style(Self::button_style(
                view_state.has_loop_range,
                loop_active,
                Color::from_rgba(0.2, 0.55, 0.9, 0.35),
            ))
        };
        let punch_button = if view_state.has_punch_range {
            button(brackets())
                .style(Self::button_style(
                    view_state.has_punch_range,
                    punch_active,
                    Color::from_rgba(0.85, 0.25, 0.25, 0.4),
                ))
                .on_press(Message::TogglePunch)
        } else {
            button(brackets()).style(Self::button_style(
                view_state.has_punch_range,
                punch_active,
                Color::from_rgba(0.85, 0.25, 0.25, 0.4),
            ))
        };
        let readout_style = |_theme: &Theme| container::Style {
            text_color: Some(Color::from_rgb(0.92, 0.92, 0.92)),
            background: Some(Background::Color(Color::from_rgba(0.10, 0.10, 0.10, 1.0))),
            border: Border {
                color: Color::from_rgba(0.28, 0.28, 0.28, 1.0),
                width: 1.0,
                radius: 2.0.into(),
            },
            ..container::Style::default()
        };
        row![
            row![
                button(triangle())
                    .style(Self::button_style(
                        true,
                        metronome_active,
                        Color::from_rgba(0.15, 0.65, 0.9, 0.35)
                    ))
                    .on_press(Message::ToggleMetronome),
                button(rewind())
                    .style(Self::button_style(true, false, Color::TRANSPARENT))
                    .on_press(Message::JumpToStart),
                button(play())
                    .style(Self::button_style(
                        true,
                        play_active,
                        Color::from_rgba(0.2, 0.7, 0.35, 0.35)
                    ))
                    .on_press(Message::TransportPlay),
                button(pause())
                    .style(Self::button_style(
                        true,
                        pause_active,
                        Color::from_rgba(0.85, 0.7, 0.1, 0.35)
                    ))
                    .on_press(Message::TransportPause),
                button(square())
                    .style(Self::button_style(
                        true,
                        stop_active,
                        Color::from_rgba(0.45, 0.45, 0.45, 0.35)
                    ))
                    .on_press(Message::TransportStop),
                button(circle())
                    .style(Self::button_style(
                        true,
                        rec_active,
                        Color::from_rgba(0.9, 0.15, 0.15, 0.45)
                    ))
                    .on_press(Message::TransportRecordToggle),
                loop_button,
                punch_button,
                if view_state.has_session_end {
                    button(fast_forward())
                        .style(Self::button_style(true, false, Color::TRANSPARENT))
                        .on_press(Message::JumpToEnd)
                } else {
                    button(fast_forward()).style(Self::button_style(
                        false,
                        false,
                        Color::TRANSPARENT,
                    ))
                },
                pick_list(
                    &SNAP_MODE_ALL[..],
                    Some(view_state.snap_mode),
                    Message::SetSnapMode
                ),
                text_input("BPM", &view_state.tempo_input)
                    .on_input(Message::TempoInputChanged)
                    .on_submit(Message::TempoInputCommit)
                    .width(Length::Fixed(72.0)),
                text_input("Num", &view_state.tsig_num_input)
                    .on_input(Message::TimeSignatureNumeratorInputChanged)
                    .on_submit(Message::TimeSignatureInputCommit)
                    .width(Length::Fixed(44.0)),
                text_input("Den", &view_state.tsig_denom_input)
                    .on_input(Message::TimeSignatureDenominatorInputChanged)
                    .on_submit(Message::TimeSignatureInputCommit)
                    .width(Length::Fixed(44.0)),
                container(
                    row![
                        text(view_state.playhead_time_label)
                            .size(16)
                            .color(MIDI_CLIP_BORDER),
                    ]
                    .spacing(4),
                )
                .padding([5, 8])
                .style(readout_style),
                container(
                    row![
                        text(view_state.playhead_bar.to_string())
                            .size(16)
                            .color(MIDI_CLIP_BORDER),
                        text(" / ").size(16),
                        text(view_state.playhead_beat.to_string())
                            .size(16)
                            .color(MIDI_CLIP_BORDER),
                    ]
                    .spacing(0),
                )
                .padding([5, 8])
                .style(readout_style)
            ]
            .spacing(3)
            .align_y(Alignment::Center)
            .width(Length::Fill),
            button(audio_lines())
                .style(Self::button_style(true, false, Color::TRANSPARENT))
                .on_press(Message::Workspace),
            button(cable())
                .style(Self::button_style(true, false, Color::TRANSPARENT))
                .on_press(Message::Connections),
        ]
        .align_y(Alignment::Center)
        .into()
    }
}
