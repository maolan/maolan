use crate::{
    menu::{menu_dropdown, menu_item, submenu},
    message::{Message, PianoControllerLane, PianoNrpnKind, PianoRpnKind, PianoVelocityKind},
    state::State,
};
use iced::{
    Background, Color, Element, Event, Length, Point, Rectangle, Renderer, Size, Theme, mouse,
    widget::{
        Id, Stack,
        canvas::{self, Action as CanvasAction, Frame, Geometry, Path, Program},
        column, container, pin, row, scrollable, slider, text, vertical_slider,
    },
};
use iced_aw::{
    menu::{DrawPath, Item, Menu as IcedMenu},
    menu_bar, menu_items,
};
use std::collections::{HashMap, HashSet};

const MIDI_CHANNELS: usize = 16;

#[derive(Debug)]
pub struct Piano {
    state: State,
}

pub const KEYS_SCROLL_ID: &str = "piano.keys.scroll";
pub const NOTES_SCROLL_ID: &str = "piano.notes.scroll";
pub const CTRL_SCROLL_ID: &str = "piano.ctrl.scroll";
pub const H_SCROLL_ID: &str = "piano.h.scroll";
pub const V_SCROLL_ID: &str = "piano.v.scroll";

impl Piano {
    pub const KEYBOARD_WIDTH: f32 = 84.0;
    const H_ZOOM_MIN: f32 = 1.0;
    const H_ZOOM_MAX: f32 = 127.0;
    const OCTAVES: usize = 10;
    const WHITE_KEYS_PER_OCTAVE: usize = 7;
    const NOTES_PER_OCTAVE: usize = 12;
    const PITCH_MAX: u8 = (Self::OCTAVES as u8 * Self::NOTES_PER_OCTAVE as u8) - 1;
    const WHITE_KEY_HEIGHT: f32 = 14.0;

    pub fn new(state: State) -> Self {
        Self { state }
    }

    fn is_black_key(pitch: u8) -> bool {
        matches!(pitch % 12, 1 | 3 | 6 | 8 | 10)
    }

    fn note_color(velocity: u8, channel: u8) -> Color {
        let t = (velocity as f32 / 127.0).clamp(0.0, 1.0);
        let c = (channel as f32 / 15.0).clamp(0.0, 1.0);
        Color {
            r: 0.25 + 0.45 * t,
            g: 0.35 + 0.4 * (1.0 - c),
            b: 0.65 + 0.3 * c,
            a: 0.9,
        }
    }

    fn controller_color(controller: u8, channel: u8) -> Color {
        let h = (controller as f32 / 127.0).clamp(0.0, 1.0);
        let c = (channel as f32 / 15.0).clamp(0.0, 1.0);
        Color {
            r: 0.3 + 0.5 * h,
            g: 0.85 - 0.45 * h,
            b: 0.25 + 0.45 * (1.0 - c),
            a: 0.85,
        }
    }

    fn zoom_x_to_slider(zoom_x: f32) -> f32 {
        (Self::H_ZOOM_MIN + Self::H_ZOOM_MAX - zoom_x).clamp(Self::H_ZOOM_MIN, Self::H_ZOOM_MAX)
    }

    fn slider_to_zoom_x(slider_value: f32) -> f32 {
        (Self::H_ZOOM_MIN + Self::H_ZOOM_MAX - slider_value)
            .clamp(Self::H_ZOOM_MIN, Self::H_ZOOM_MAX)
    }

    fn cc_name(cc: u8) -> &'static str {
        match cc {
            0 => "Bank Select",
            1 => "Modulation Wheel",
            2 => "Breath Controller",
            4 => "Foot Controller",
            5 => "Portamento Time",
            6 => "Data Entry MSB",
            7 => "Channel Volume",
            8 => "Balance",
            10 => "Pan",
            11 => "Expression Controller",
            12 => "Effect Control 1",
            13 => "Effect Control 2",
            16 => "General Purpose Controller 1",
            17 => "General Purpose Controller 2",
            18 => "General Purpose Controller 3",
            19 => "General Purpose Controller 4",
            32 => "Bank Select LSB",
            33 => "Modulation Wheel LSB",
            34 => "Breath Controller LSB",
            36 => "Foot Controller LSB",
            37 => "Portamento Time LSB",
            38 => "Data Entry LSB",
            39 => "Channel Volume LSB",
            40 => "Balance LSB",
            42 => "Pan LSB",
            43 => "Expression Controller LSB",
            44 => "Effect Control 1 LSB",
            45 => "Effect Control 2 LSB",
            48 => "General Purpose Controller 1 LSB",
            49 => "General Purpose Controller 2 LSB",
            50 => "General Purpose Controller 3 LSB",
            51 => "General Purpose Controller 4 LSB",
            64 => "Sustain Pedal",
            65 => "Portamento",
            66 => "Sostenuto",
            67 => "Soft Pedal",
            68 => "Legato Footswitch",
            69 => "Hold 2",
            70 => "Sound Controller 1",
            71 => "Sound Controller 2",
            72 => "Sound Controller 3",
            73 => "Sound Controller 4",
            74 => "Sound Controller 5",
            75 => "Sound Controller 6",
            76 => "Sound Controller 7",
            77 => "Sound Controller 8",
            78 => "Sound Controller 9",
            79 => "Sound Controller 10",
            80 => "General Purpose Controller 5",
            81 => "General Purpose Controller 6",
            82 => "General Purpose Controller 7",
            83 => "General Purpose Controller 8",
            84 => "Portamento Control",
            91 => "Effects 1 Depth",
            92 => "Effects 2 Depth",
            93 => "Effects 3 Depth",
            94 => "Effects 4 Depth",
            95 => "Effects 5 Depth",
            96 => "Data Increment",
            97 => "Data Decrement",
            98 => "NRPN LSB",
            99 => "NRPN MSB",
            100 => "RPN LSB",
            101 => "RPN MSB",
            120 => "All Sound Off",
            121 => "Reset All Controllers",
            122 => "Local Control",
            123 => "All Notes Off",
            124 => "Omni Mode Off",
            125 => "Omni Mode On",
            126 => "Mono Mode On",
            127 => "Poly Mode On",
            _ => "Undefined",
        }
    }

    fn cc_label(cc: u8) -> String {
        format!("CC{cc:03} {}", Self::cc_name(cc))
    }

    fn controller_lane_line_count(lane: PianoControllerLane) -> usize {
        match lane {
            PianoControllerLane::Controller => 128,
            PianoControllerLane::Velocity => 128,
            PianoControllerLane::Rpn => PianoRpnKind::ALL.len(),
            PianoControllerLane::Nrpn => PianoNrpnKind::ALL.len(),
        }
    }

    fn controller_row_for_lane(lane: PianoControllerLane, controller: u8) -> Option<usize> {
        match lane {
            PianoControllerLane::Controller => Some(usize::from(127_u8.saturating_sub(controller))),
            PianoControllerLane::Velocity => None,
            PianoControllerLane::Rpn => match controller {
                101 => Some(0),
                100 => Some(1),
                6 | 38 | 96 | 97 => Some(2),
                _ => None,
            },
            PianoControllerLane::Nrpn => match controller {
                99 => Some(0),
                98 => Some(1),
                6 | 38 | 96 | 97 => Some(2),
                _ => None,
            },
        }
    }

    fn rpn_param(kind: PianoRpnKind) -> (u8, u8) {
        match kind {
            PianoRpnKind::PitchBendSensitivity => (0, 0),
            PianoRpnKind::FineTuning => (0, 1),
            PianoRpnKind::CoarseTuning => (0, 2),
        }
    }

    fn nrpn_param(kind: PianoNrpnKind) -> (u8, u8) {
        match kind {
            PianoNrpnKind::Brightness => (1, 8),
            PianoNrpnKind::VibratoRate => (1, 9),
            PianoNrpnKind::VibratoDepth => (1, 10),
        }
    }

    fn rpn_row_for_param(msb: u8, lsb: u8) -> Option<usize> {
        PianoRpnKind::ALL
            .iter()
            .position(|kind| Self::rpn_param(*kind) == (msb, lsb))
    }

    fn nrpn_row_for_param(msb: u8, lsb: u8) -> Option<usize> {
        PianoNrpnKind::ALL
            .iter()
            .position(|kind| Self::nrpn_param(*kind) == (msb, lsb))
    }

    fn lane_controller_events(
        lane: PianoControllerLane,
        controllers: &[crate::state::PianoControllerPoint],
    ) -> Vec<(usize, usize)> {
        match lane {
            PianoControllerLane::Controller => controllers
                .iter()
                .enumerate()
                .filter_map(|(idx, ctrl)| {
                    Self::controller_row_for_lane(lane, ctrl.controller).map(|row| (idx, row))
                })
                .collect(),
            PianoControllerLane::Velocity => vec![],
            PianoControllerLane::Rpn => {
                let mut ordered: Vec<usize> = (0..controllers.len()).collect();
                ordered.sort_unstable_by_key(|idx| (controllers[*idx].sample, *idx));
                let mut current_msb: [Option<u8>; MIDI_CHANNELS] = [None; MIDI_CHANNELS];
                let mut current_lsb: [Option<u8>; MIDI_CHANNELS] = [None; MIDI_CHANNELS];
                let mut out = Vec::new();
                for idx in ordered {
                    let ctrl = &controllers[idx];
                    let channel = usize::from(ctrl.channel.min((MIDI_CHANNELS - 1) as u8));
                    match ctrl.controller {
                        101 => current_msb[channel] = Some(ctrl.value),
                        100 => current_lsb[channel] = Some(ctrl.value),
                        6 | 38 | 96 | 97 => {
                            if let (Some(msb), Some(lsb)) =
                                (current_msb[channel], current_lsb[channel])
                                && let Some(row) = Self::rpn_row_for_param(msb, lsb)
                            {
                                out.push((idx, row));
                            }
                        }
                        _ => {}
                    }
                }
                out
            }
            PianoControllerLane::Nrpn => {
                let mut ordered: Vec<usize> = (0..controllers.len()).collect();
                ordered.sort_unstable_by_key(|idx| (controllers[*idx].sample, *idx));
                let mut current_msb: [Option<u8>; MIDI_CHANNELS] = [None; MIDI_CHANNELS];
                let mut current_lsb: [Option<u8>; MIDI_CHANNELS] = [None; MIDI_CHANNELS];
                let mut out = Vec::new();
                for idx in ordered {
                    let ctrl = &controllers[idx];
                    let channel = usize::from(ctrl.channel.min((MIDI_CHANNELS - 1) as u8));
                    match ctrl.controller {
                        99 => current_msb[channel] = Some(ctrl.value),
                        98 => current_lsb[channel] = Some(ctrl.value),
                        6 | 38 | 96 | 97 => {
                            if let (Some(msb), Some(lsb)) =
                                (current_msb[channel], current_lsb[channel])
                                && let Some(row) = Self::nrpn_row_for_param(msb, lsb)
                            {
                                out.push((idx, row));
                            }
                        }
                        _ => {}
                    }
                }
                out
            }
        }
    }

    pub fn view(&self, pixels_per_sample: f32, samples_per_bar: f32) -> Element<'_, Message> {
        let state = self.state.blocking_read();
        let zoom_x = state.piano_zoom_x;
        let zoom_y = state.piano_zoom_y;
        let controller_lane = state.piano_controller_lane;

        let Some(roll) = state.piano.as_ref() else {
            return container(text("No MIDI clip selected."))
                .width(Length::Fill)
                .height(Length::Fill)
                .into();
        };

        let pitch_count = Self::OCTAVES * Self::NOTES_PER_OCTAVE;
        let row_h = ((Self::WHITE_KEY_HEIGHT * Self::WHITE_KEYS_PER_OCTAVE as f32
            / Self::NOTES_PER_OCTAVE as f32)
            * zoom_y)
            .max(1.0);
        let notes_h = pitch_count as f32 * row_h;
        let ctrl_line_count = Self::controller_lane_line_count(controller_lane).max(1);
        let ctrl_h = (ctrl_line_count as f32).max(140.0);
        let ctrl_row_h = (ctrl_h / ctrl_line_count as f32).max(1.0);
        let pps_notes = (pixels_per_sample * zoom_x).max(0.0001);
        let pps_ctrl = (pixels_per_sample * zoom_x).max(0.0001);
        let notes_w = (roll.clip_length_samples as f32 * pps_notes).max(1.0);
        let ctrl_w = (roll.clip_length_samples as f32 * pps_ctrl).max(1.0);

        let mut note_layers: Vec<Element<'_, Message>> = vec![];
        for i in 0..pitch_count {
            let pitch = Self::PITCH_MAX.saturating_sub(i as u8);
            let is_black = Self::is_black_key(pitch);
            note_layers.push(
                pin(container("")
                    .width(Length::Fixed(notes_w))
                    .height(Length::Fixed(row_h))
                    .style(move |_theme| container::Style {
                        background: Some(Background::Color(if is_black {
                            Color {
                                r: 0.08,
                                g: 0.08,
                                b: 0.1,
                                a: 0.85,
                            }
                        } else {
                            Color {
                                r: 0.12,
                                g: 0.12,
                                b: 0.14,
                                a: 0.85,
                            }
                        })),
                        ..container::Style::default()
                    }))
                .position(Point::new(0.0, i as f32 * row_h))
                .into(),
            );
        }

        let mut ctrl_layers: Vec<Element<'_, Message>> = vec![
            pin(container("")
                .width(Length::Fixed(ctrl_w))
                .height(Length::Fixed(ctrl_h))
                .style(|_theme| container::Style {
                    background: Some(Background::Color(Color {
                        r: 0.16,
                        g: 0.16,
                        b: 0.18,
                        a: 0.9,
                    })),
                    ..container::Style::default()
                }))
            .position(Point::new(0.0, 0.0))
            .into(),
        ];

        for row in 0..ctrl_line_count {
            let y = row as f32 * ctrl_row_h;
            let divider = if row.is_multiple_of(8) { 0.28 } else { 0.2 };
            ctrl_layers.push(
                pin(container("")
                    .width(Length::Fixed(ctrl_w))
                    .height(Length::Fixed(1.0))
                    .style(move |_theme| container::Style {
                        background: Some(Background::Color(Color {
                            r: divider,
                            g: divider,
                            b: divider + 0.02,
                            a: 0.5,
                        })),
                        ..container::Style::default()
                    }))
                .position(Point::new(0.0, y))
                .into(),
            );
        }

        let beat_samples = (samples_per_bar / 4.0).max(1.0);
        let mut beat = 0usize;
        loop {
            let x_notes = beat as f32 * beat_samples * pps_notes;
            let x_ctrl = beat as f32 * beat_samples * pps_ctrl;
            if x_notes > notes_w && x_ctrl > ctrl_w {
                break;
            }
            let bar_line = beat.is_multiple_of(4);
            if x_notes <= notes_w {
                note_layers.push(
                    pin(container("")
                        .width(Length::Fixed(if bar_line { 2.0 } else { 1.0 }))
                        .height(Length::Fixed(notes_h))
                        .style(move |_theme| container::Style {
                            background: Some(Background::Color(Color {
                                r: if bar_line { 0.5 } else { 0.35 },
                                g: if bar_line { 0.5 } else { 0.35 },
                                b: if bar_line { 0.55 } else { 0.35 },
                                a: 0.45,
                            })),
                            ..container::Style::default()
                        }))
                    .position(Point::new(x_notes, 0.0))
                    .into(),
                );
            }
            if x_ctrl <= ctrl_w {
                ctrl_layers.push(
                    pin(container("")
                        .width(Length::Fixed(if bar_line { 2.0 } else { 1.0 }))
                        .height(Length::Fixed(ctrl_h))
                        .style(move |_theme| container::Style {
                            background: Some(Background::Color(Color {
                                r: if bar_line { 0.5 } else { 0.35 },
                                g: if bar_line { 0.5 } else { 0.35 },
                                b: if bar_line { 0.55 } else { 0.35 },
                                a: 0.45,
                            })),
                            ..container::Style::default()
                        }))
                    .position(Point::new(x_ctrl, 0.0))
                    .into(),
                );
            }
            beat += 1;
        }

        for note in &roll.notes {
            if note.pitch > Self::PITCH_MAX {
                continue;
            }
            let y_idx = usize::from(Self::PITCH_MAX.saturating_sub(note.pitch));
            let y = y_idx as f32 * row_h + 1.0;
            let x = note.start_sample as f32 * pps_notes;
            let w = (note.length_samples as f32 * pps_notes).max(2.0);
            let color = Self::note_color(note.velocity, note.channel);
            note_layers.push(
                pin(container("")
                    .width(Length::Fixed(w))
                    .height(Length::Fixed((row_h - 2.0).max(2.0)))
                    .style(move |_theme| container::Style {
                        background: Some(Background::Color(color)),
                        ..container::Style::default()
                    }))
                .position(Point::new(x, y))
                .into(),
            );
        }

        match controller_lane {
            PianoControllerLane::Controller => {
                for (idx, row) in Self::lane_controller_events(controller_lane, &roll.controllers) {
                    let ctrl = &roll.controllers[idx];
                    let x = ctrl.sample as f32 * pps_ctrl;
                    let mut color = Self::controller_color(ctrl.controller, ctrl.channel);
                    color.a = (0.2 + (ctrl.value as f32 / 127.0) * 0.8).clamp(0.2, 1.0);
                    let stem_h = (ctrl_h * (ctrl.value as f32 / 127.0)).max(1.0);
                    let stem_y = ctrl_h - stem_h;
                    ctrl_layers.push(
                        pin(container("")
                            .width(Length::Fixed(2.0))
                            .height(Length::Fixed(stem_h))
                            .style(move |_theme| container::Style {
                                background: Some(Background::Color(color)),
                                ..container::Style::default()
                            }))
                        .position(Point::new(x, stem_y))
                        .into(),
                    );
                    let y = row as f32 * ctrl_row_h;
                    ctrl_layers.push(
                        pin(container("")
                            .width(Length::Fixed(2.0))
                            .height(Length::Fixed(1.0))
                            .style(move |_theme| container::Style {
                                background: Some(Background::Color(Color::from_rgba(
                                    1.0, 1.0, 1.0, 0.35,
                                ))),
                                ..container::Style::default()
                            }))
                        .position(Point::new(x, y))
                        .into(),
                    );
                }
            }
            PianoControllerLane::Velocity => {
                for note in &roll.notes {
                    let x = note.start_sample as f32 * pps_ctrl;
                    let row = usize::from(127_u8.saturating_sub(note.velocity));
                    let y = row as f32 * ctrl_row_h;
                    let mut color = Self::note_color(note.velocity, note.channel);
                    color.a = 0.9;
                    let stem_h = (ctrl_h - y).max(ctrl_row_h);
                    ctrl_layers.push(
                        pin(container("")
                            .width(Length::Fixed(2.0))
                            .height(Length::Fixed(stem_h))
                            .style(move |_theme| container::Style {
                                background: Some(Background::Color(color)),
                                ..container::Style::default()
                            }))
                        .position(Point::new(x, y))
                        .into(),
                    );
                }
            }
            PianoControllerLane::Rpn => {
                for (idx, row) in Self::lane_controller_events(controller_lane, &roll.controllers) {
                    let ctrl = &roll.controllers[idx];
                    let x = ctrl.sample as f32 * pps_ctrl;
                    let mut color = Self::controller_color(ctrl.controller, ctrl.channel);
                    color.a = (0.2 + (ctrl.value as f32 / 127.0) * 0.8).clamp(0.2, 1.0);
                    let stem_h = (ctrl_h * (ctrl.value as f32 / 127.0)).max(1.0);
                    let stem_y = ctrl_h - stem_h;
                    ctrl_layers.push(
                        pin(container("")
                            .width(Length::Fixed(2.0))
                            .height(Length::Fixed(stem_h))
                            .style(move |_theme| container::Style {
                                background: Some(Background::Color(color)),
                                ..container::Style::default()
                            }))
                        .position(Point::new(x, stem_y))
                        .into(),
                    );
                    let y = row as f32 * ctrl_row_h;
                    ctrl_layers.push(
                        pin(container("")
                            .width(Length::Fixed(2.0))
                            .height(Length::Fixed(1.0))
                            .style(move |_theme| container::Style {
                                background: Some(Background::Color(Color::from_rgba(
                                    1.0, 1.0, 1.0, 0.35,
                                ))),
                                ..container::Style::default()
                            }))
                        .position(Point::new(x, y))
                        .into(),
                    );
                }
            }
            PianoControllerLane::Nrpn => {
                for (idx, row) in Self::lane_controller_events(controller_lane, &roll.controllers) {
                    let ctrl = &roll.controllers[idx];
                    let x = ctrl.sample as f32 * pps_ctrl;
                    let mut color = Self::controller_color(ctrl.controller, ctrl.channel);
                    color.a = (0.2 + (ctrl.value as f32 / 127.0) * 0.8).clamp(0.2, 1.0);
                    let stem_h = (ctrl_h * (ctrl.value as f32 / 127.0)).max(1.0);
                    let stem_y = ctrl_h - stem_h;
                    ctrl_layers.push(
                        pin(container("")
                            .width(Length::Fixed(2.0))
                            .height(Length::Fixed(stem_h))
                            .style(move |_theme| container::Style {
                                background: Some(Background::Color(color)),
                                ..container::Style::default()
                            }))
                        .position(Point::new(x, stem_y))
                        .into(),
                    );
                    let y = row as f32 * ctrl_row_h;
                    ctrl_layers.push(
                        pin(container("")
                            .width(Length::Fixed(2.0))
                            .height(Length::Fixed(1.0))
                            .style(move |_theme| container::Style {
                                background: Some(Background::Color(Color::from_rgba(
                                    1.0, 1.0, 1.0, 0.35,
                                ))),
                                ..container::Style::default()
                            }))
                        .position(Point::new(x, y))
                        .into(),
                    );
                }
            }
        }

        ctrl_layers.push(
            pin(iced::widget::canvas(ControllerRollInteraction::new(
                self.state.clone(),
                pixels_per_sample,
            ))
            .width(Length::Fixed(ctrl_w))
            .height(Length::Fixed(ctrl_h)))
            .position(Point::new(0.0, 0.0))
            .into(),
        );

        // Add interactive canvas overlay for note selection and dragging
        note_layers.push(
            pin(iced::widget::canvas(PianoRollInteraction::new(
                self.state.clone(),
                pixels_per_sample,
            ))
            .width(Length::Fixed(notes_w))
            .height(Length::Fixed(notes_h)))
            .position(Point::new(0.0, 0.0))
            .into(),
        );

        let notes_content = Stack::from_vec(note_layers)
            .width(Length::Fixed(notes_w))
            .height(Length::Fixed(notes_h));
        let ctrl_content = Stack::from_vec(ctrl_layers)
            .width(Length::Fixed(ctrl_w))
            .height(Length::Fixed(ctrl_h));

        let octave_h = (notes_h / Self::OCTAVES as f32).max(1.0);
        let midnam_note_names = roll.midnam_note_names.clone();
        let keyboard = (0..Self::OCTAVES).fold(column![], |col, octave_idx| {
            let octave = (Self::OCTAVES - 1 - octave_idx) as u8;
            col.push(
                iced::widget::canvas(OctaveKeyboard::new(octave, midnam_note_names.clone()))
                    .width(Length::Fixed(Self::KEYBOARD_WIDTH))
                    .height(Length::Fixed(octave_h)),
            )
        });
        let piano_note_keys = keyboard
            .width(Length::Fixed(Self::KEYBOARD_WIDTH))
            .height(Length::Fill);
        let lane_label = state.piano_controller_lane.to_string();
        let menu_tpl = |items| IcedMenu::new(items).width(220.0).offset(15.0).spacing(5.0);
        let controller_submenu = IcedMenu::new(
            (0u8..=127)
                .into_iter()
                .map(|cc| {
                    Item::new(menu_item(
                        Self::cc_label(cc),
                        Message::PianoControllerKindSelected(cc),
                    ))
                })
                .collect::<Vec<_>>(),
        )
        .width(240.0)
        .offset(15.0)
        .spacing(5.0);
        let velocity_submenu = IcedMenu::new(
            PianoVelocityKind::ALL
                .into_iter()
                .map(|kind| {
                    Item::new(menu_item(
                        kind.to_string(),
                        Message::PianoVelocityKindSelected(kind),
                    ))
                })
                .collect::<Vec<_>>(),
        )
        .width(240.0)
        .offset(15.0)
        .spacing(5.0);
        let rpn_submenu = IcedMenu::new(
            PianoRpnKind::ALL
                .into_iter()
                .map(|kind| {
                    Item::new(menu_item(
                        kind.to_string(),
                        Message::PianoRpnKindSelected(kind),
                    ))
                })
                .collect::<Vec<_>>(),
        )
        .width(240.0)
        .offset(15.0)
        .spacing(5.0);
        let nrpn_submenu = IcedMenu::new(
            PianoNrpnKind::ALL
                .into_iter()
                .map(|kind| {
                    Item::new(menu_item(
                        kind.to_string(),
                        Message::PianoNrpnKindSelected(kind),
                    ))
                })
                .collect::<Vec<_>>(),
        )
        .width(240.0)
        .offset(15.0)
        .spacing(5.0);
        #[rustfmt::skip]
        let controller_picker = menu_bar!(
            (menu_dropdown(lane_label, Message::None), {
                menu_tpl(menu_items!(
                    (submenu("Controller", Message::PianoControllerLaneSelected(PianoControllerLane::Controller)), controller_submenu),
                    (submenu("Velocity", Message::PianoControllerLaneSelected(PianoControllerLane::Velocity)), velocity_submenu),
                    (submenu("RPN", Message::PianoControllerLaneSelected(PianoControllerLane::Rpn)), rpn_submenu),
                    (submenu("NRPN", Message::PianoControllerLaneSelected(PianoControllerLane::Nrpn)), nrpn_submenu),
                ))
            })
        )
        .draw_path(DrawPath::Backdrop)
        .close_on_item_click_global(true)
        .width(Length::Fill);
        let controller_key = container(controller_picker)
            .width(Length::Fixed(Self::KEYBOARD_WIDTH))
            .height(Length::Fixed(ctrl_h))
            .padding([4, 3])
            .style(|_theme| container::Style {
                background: Some(Background::Color(Color {
                    r: 0.15,
                    g: 0.15,
                    b: 0.16,
                    a: 1.0,
                })),
                ..container::Style::default()
            });

        let keyboard_scroll = scrollable(
            container(piano_note_keys)
                .width(Length::Fixed(Self::KEYBOARD_WIDTH))
                .height(Length::Fixed(notes_h))
                .style(|_theme| container::Style {
                    background: Some(Background::Color(Color {
                        r: 0.12,
                        g: 0.12,
                        b: 0.12,
                        a: 1.0,
                    })),
                    ..container::Style::default()
                }),
        )
        .id(Id::new(KEYS_SCROLL_ID))
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::hidden(),
        ))
        .on_scroll(|viewport| Message::PianoScrollYChanged(viewport.relative_offset().y))
        .width(Length::Fixed(Self::KEYBOARD_WIDTH))
        .height(Length::Fill);

        let note_scroll = scrollable(
            container(notes_content)
                .width(Length::Shrink)
                .height(Length::Fixed(notes_h))
                .style(|_theme| container::Style {
                    background: Some(Background::Color(Color {
                        r: 0.07,
                        g: 0.07,
                        b: 0.09,
                        a: 1.0,
                    })),
                    ..container::Style::default()
                }),
        )
        .id(Id::new(NOTES_SCROLL_ID))
        .direction(scrollable::Direction::Both {
            vertical: scrollable::Scrollbar::hidden(),
            horizontal: scrollable::Scrollbar::hidden(),
        })
        .on_scroll(|viewport| {
            let offset = viewport.relative_offset();
            Message::PianoScrollChanged {
                x: offset.x,
                y: offset.y,
            }
        })
        .width(Length::Fill)
        .height(Length::Fill);

        let ctrl_scroll = scrollable(
            container(ctrl_content)
                .width(Length::Shrink)
                .height(Length::Fixed(ctrl_h))
                .style(|_theme| container::Style {
                    background: Some(Background::Color(Color {
                        r: 0.12,
                        g: 0.12,
                        b: 0.13,
                        a: 1.0,
                    })),
                    ..container::Style::default()
                }),
        )
        .id(Id::new(CTRL_SCROLL_ID))
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::hidden(),
        ))
        .on_scroll(|viewport| Message::PianoScrollXChanged(viewport.relative_offset().x))
        .width(Length::Fill)
        .height(Length::Fixed(ctrl_h));

        let h_scroll = scrollable(
            container("")
                .width(Length::Fixed(notes_w.max(ctrl_w)))
                .height(Length::Fixed(1.0)),
        )
        .id(Id::new(H_SCROLL_ID))
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::new(),
        ))
        .on_scroll(|viewport| Message::PianoScrollXChanged(viewport.relative_offset().x))
        .width(Length::Fill)
        .height(Length::Fixed(16.0));

        let v_scroll = scrollable(
            container("")
                .width(Length::Fixed(1.0))
                .height(Length::Fixed(notes_h)),
        )
        .id(Id::new(V_SCROLL_ID))
        .direction(scrollable::Direction::Vertical(scrollable::Scrollbar::new()))
        .on_scroll(|viewport| Message::PianoScrollYChanged(viewport.relative_offset().y))
        .width(Length::Fixed(16.0))
        .height(Length::Fill);

        container(row![
            column![
                row![keyboard_scroll, note_scroll]
                    .height(Length::Fill)
                    .width(Length::Fill),
                row![controller_key, ctrl_scroll],
                row![
                    container("")
                        .width(Length::Fixed(Self::KEYBOARD_WIDTH))
                        .height(Length::Fixed(16.0)),
                    row![
                        h_scroll,
                        slider(
                            Self::H_ZOOM_MIN..=Self::H_ZOOM_MAX,
                            Self::zoom_x_to_slider(zoom_x),
                            |value| Message::PianoZoomXChanged(Self::slider_to_zoom_x(value)),
                        )
                        .step(0.1)
                        .width(Length::Fixed(100.0)),
                    ]
                    .spacing(8)
                    .width(Length::Fill),
                ]
            ]
            .spacing(3),
            column![
                v_scroll,
                vertical_slider(1.0..=8.0, zoom_y, Message::PianoZoomYChanged)
                    .step(0.1)
                    .height(Length::Fixed(100.0)),
            ]
            .spacing(8)
            .height(Length::Fill),
        ])
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }
}

fn draw_octave_with_midnam(
    renderer: &Renderer,
    bounds: Rectangle,
    pressed_notes: &HashSet<u8>,
    octave: u8,
    midnam_note_names: &HashMap<u8, String>,
) -> Vec<canvas::Geometry> {
    let mut frame = Frame::new(renderer, bounds.size());
    let note_height = bounds.height / 12.0;

    // Draw rectangles for each note in the octave (12 notes)
    for i in 0..12 {
        let note_in_octave = 11 - i; // Draw from top to bottom (high to low)
        let midi_note = octave * 12 + note_in_octave;
        let is_pressed = pressed_notes.contains(&note_in_octave);
        let y_pos = i as f32 * note_height;

        // Draw the rectangle
        let rect = Path::rectangle(
            Point::new(0.0, y_pos),
            Size::new(bounds.width, note_height - 1.0),
        );

        frame.fill(
            &rect,
            if is_pressed {
                Color::from_rgb(0.2, 0.6, 0.9)
            } else {
                Color::from_rgb(0.18, 0.18, 0.2)
            },
        );
        frame.stroke(
            &rect,
            canvas::Stroke::default()
                .with_width(1.0)
                .with_color(Color::from_rgb(0.25, 0.25, 0.28)),
        );

        // Draw the note name if available
        if let Some(note_name) = midnam_note_names.get(&midi_note) {
            use iced::widget::canvas::Text;
            let text_pos = Point::new(4.0, y_pos + note_height * 0.5 - 6.0);
            frame.fill_text(Text {
                content: note_name.clone(),
                position: text_pos,
                color: Color::from_rgb(0.85, 0.85, 0.9),
                size: 11.0.into(),
                ..Text::default()
            });
        }
    }

    vec![frame.into_geometry()]
}

fn draw_octave(
    renderer: &Renderer,
    bounds: Rectangle,
    pressed_notes: &HashSet<u8>,
) -> Vec<canvas::Geometry> {
    let mut frame = Frame::new(renderer, bounds.size());
    let white_key_height = bounds.height / 7.0;

    for i in 0..7 {
        let note_id = match i {
            0 => 0,
            1 => 2,
            2 => 4,
            3 => 5,
            4 => 7,
            5 => 9,
            6 => 11,
            _ => 0,
        };
        let is_pressed = pressed_notes.contains(&note_id);
        let y_pos = bounds.height - ((i + 1) as f32 * white_key_height);
        let rect = Path::rectangle(
            Point::new(0.0, y_pos),
            Size::new(bounds.width, white_key_height - 1.0),
        );

        frame.fill(
            &rect,
            if is_pressed {
                Color::from_rgb(0.0, 0.5, 1.0)
            } else {
                Color::WHITE
            },
        );
        frame.stroke(&rect, canvas::Stroke::default().with_width(1.0));
    }

    let black_key_offsets = [1, 2, 4, 5, 6];
    let black_note_ids = [1, 3, 6, 8, 10];
    let black_key_width = bounds.width * 0.6;
    let black_key_height = white_key_height * 0.6;

    for (idx, offset) in black_key_offsets.iter().enumerate() {
        let is_pressed = pressed_notes.contains(&black_note_ids[idx]);
        let y_pos_black =
            bounds.height - (*offset as f32 * white_key_height) - (black_key_height * 0.5);
        let rect = Path::rectangle(
            Point::new(0.0, y_pos_black),
            Size::new(black_key_width, black_key_height),
        );

        frame.fill(
            &rect,
            if is_pressed {
                Color::from_rgb(0.0, 0.4, 0.8)
            } else {
                Color::BLACK
            },
        );
    }

    vec![frame.into_geometry()]
}

#[derive(Debug, Clone)]
struct OctaveKeyboard {
    octave: u8,
    midnam_note_names: HashMap<u8, String>,
}

impl OctaveKeyboard {
    fn new(octave: u8, midnam_note_names: HashMap<u8, String>) -> Self {
        Self {
            octave,
            midnam_note_names,
        }
    }

    fn note_class_at(&self, cursor: Point, bounds: Rectangle) -> Option<u8> {
        let white_key_height = bounds.height / 7.0;
        let black_key_offsets = [1, 2, 4, 5, 6];
        let black_note_ids = [1, 3, 6, 8, 10];
        let black_key_width = bounds.width * 0.6;
        let black_key_height = white_key_height * 0.6;

        if cursor.x <= black_key_width {
            for (idx, offset) in black_key_offsets.iter().enumerate() {
                let y_pos_black =
                    bounds.height - (*offset as f32 * white_key_height) - (black_key_height * 0.5);
                if cursor.y >= y_pos_black && cursor.y <= y_pos_black + black_key_height {
                    return Some(black_note_ids[idx]);
                }
            }
        }

        for i in 0..7 {
            let note_id = match i {
                0 => 0,
                1 => 2,
                2 => 4,
                3 => 5,
                4 => 7,
                5 => 9,
                6 => 11,
                _ => 0,
            };
            let y_pos = bounds.height - ((i + 1) as f32 * white_key_height);
            if cursor.y >= y_pos && cursor.y <= y_pos + white_key_height {
                return Some(note_id);
            }
        }
        None
    }

    fn midi_note(&self, note_class: u8) -> u8 {
        (usize::from(self.octave) * 12 + usize::from(note_class)) as u8
    }
}

#[derive(Default, Debug)]
struct OctaveKeyboardState {
    pressed_notes: HashSet<u8>,
    active_note_class: Option<u8>,
}

impl Program<Message> for OctaveKeyboard {
    type State = OctaveKeyboardState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<CanvasAction<Message>> {
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(position) = cursor.position_in(bounds)
                    && let Some(note_class) = self.note_class_at(position, bounds)
                {
                    state.active_note_class = Some(note_class);
                    state.pressed_notes.clear();
                    state.pressed_notes.insert(note_class);
                    return Some(
                        CanvasAction::publish(Message::PianoKeyPressed(self.midi_note(note_class)))
                            .and_capture(),
                    );
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if let Some(note_class) = state.active_note_class.take() {
                    state.pressed_notes.clear();
                    return Some(CanvasAction::publish(Message::PianoKeyReleased(
                        self.midi_note(note_class),
                    )));
                }
            }
            _ => {}
        }
        None
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        if self.midnam_note_names.is_empty() {
            draw_octave(renderer, bounds, &state.pressed_notes)
        } else {
            draw_octave_with_midnam(
                renderer,
                bounds,
                &state.pressed_notes,
                self.octave,
                &self.midnam_note_names,
            )
        }
    }
}

#[derive(Debug)]
pub struct ControllerRollInteraction {
    pub state: State,
    pub pixels_per_sample: f32,
}

#[derive(Debug, Clone, Copy)]
enum ControllerAdjustTarget {
    Controller(usize),
    Velocity(usize),
}

#[derive(Default, Debug)]
pub struct ControllerRollInteractionState {
    mode: ControllerDragMode,
}

#[derive(Default, Debug, Clone, Copy)]
enum ControllerDragMode {
    #[default]
    None,
    Adjusting {
        target: ControllerAdjustTarget,
        start_y: f32,
        start_value: u8,
    },
    Drawing {
        start: Point,
        current: Point,
    },
}

impl ControllerRollInteraction {
    pub fn new(state: State, pixels_per_sample: f32) -> Self {
        Self {
            state,
            pixels_per_sample,
        }
    }

    fn controller_at_position(
        &self,
        position: Point,
        lane: PianoControllerLane,
        row_h: f32,
        pps: f32,
        selected_row: Option<usize>,
        controllers: &[crate::state::PianoControllerPoint],
    ) -> Option<usize> {
        let mut best: Option<(usize, f32)> = None;
        for (idx, row) in Piano::lane_controller_events(lane, controllers) {
            if let Some(selected_row) = selected_row
                && row != selected_row
            {
                continue;
            }
            let y0 = row as f32 * row_h;
            let y1 = y0 + row_h;
            if position.y < y0 || position.y > y1 {
                continue;
            }
            let ctrl = &controllers[idx];
            let x = ctrl.sample as f32 * pps;
            let dx = (position.x - x).abs();
            if dx > 5.0 {
                continue;
            }
            match best {
                Some((_, best_dx)) if dx >= best_dx => {}
                _ => best = Some((idx, dx)),
            }
        }
        best.map(|(idx, _)| idx)
    }

    fn velocity_note_at_position(
        &self,
        position: Point,
        row_h: f32,
        pps: f32,
        notes: &[crate::state::PianoNote],
    ) -> Option<usize> {
        let mut best: Option<(usize, f32)> = None;
        for (idx, note) in notes.iter().enumerate() {
            let row = usize::from(127_u8.saturating_sub(note.velocity));
            let y0 = row as f32 * row_h;
            let y1 = y0 + row_h;
            if position.y < y0 || position.y > y1 {
                continue;
            }
            let x = note.start_sample as f32 * pps;
            let dx = (position.x - x).abs();
            if dx > 5.0 {
                continue;
            }
            match best {
                Some((_, best_dx)) if dx >= best_dx => {}
                _ => best = Some((idx, dx)),
            }
        }
        best.map(|(idx, _)| idx)
    }
}

impl Program<Message> for ControllerRollInteraction {
    type State = ControllerRollInteractionState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<CanvasAction<Message>> {
        let app_state = self.state.blocking_read();
        let Some(roll) = app_state.piano.as_ref() else {
            return None;
        };
        let lane = app_state.piano_controller_lane;
        let selected_row = match lane {
            PianoControllerLane::Controller => Some(usize::from(
                127_u8.saturating_sub(app_state.piano_controller_kind),
            )),
            PianoControllerLane::Rpn => PianoRpnKind::ALL
                .iter()
                .position(|kind| *kind == app_state.piano_rpn_kind),
            PianoControllerLane::Nrpn => PianoNrpnKind::ALL
                .iter()
                .position(|kind| *kind == app_state.piano_nrpn_kind),
            PianoControllerLane::Velocity => None,
        };
        let controllers = roll.controllers.clone();
        let notes = roll.notes.clone();
        let clip_len = roll.clip_length_samples;
        let zoom_x = app_state.piano_zoom_x;
        let pps = (self.pixels_per_sample * zoom_x).max(0.0001);
        let row_h =
            (bounds.height / Piano::controller_lane_line_count(lane).max(1) as f32).max(1.0);
        drop(app_state);

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(position) = cursor.position_in(bounds) else {
                    return None;
                };
                let target = if matches!(lane, PianoControllerLane::Velocity) {
                    self.velocity_note_at_position(position, row_h, pps, &notes)
                        .and_then(|idx| notes.get(idx).map(|n| (idx, n.velocity)))
                        .map(|(idx, velocity)| (ControllerAdjustTarget::Velocity(idx), velocity))
                } else {
                    self.controller_at_position(
                        position,
                        lane,
                        row_h,
                        pps,
                        selected_row,
                        &controllers,
                    )
                    .and_then(|idx| controllers.get(idx).map(|c| (idx, c.value)))
                    .map(|(idx, value)| (ControllerAdjustTarget::Controller(idx), value))
                };
                if let Some((target, start_value)) = target {
                    state.mode = ControllerDragMode::Adjusting {
                        target,
                        start_y: position.y,
                        start_value,
                    };
                    return Some(CanvasAction::capture());
                }
                None
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                if matches!(lane, PianoControllerLane::Velocity) {
                    return None;
                }
                let Some(position) = cursor.position_in(bounds) else {
                    return None;
                };
                let Some(controller_index) = self.controller_at_position(
                    position,
                    lane,
                    row_h,
                    pps,
                    selected_row,
                    &controllers,
                ) else {
                    return None;
                };
                let value_delta = PianoRollInteraction::velocity_delta_from_scroll(delta);
                if value_delta == 0 {
                    return None;
                }
                Some(
                    CanvasAction::publish(Message::PianoAdjustController {
                        controller_index,
                        delta: value_delta,
                    })
                    .and_capture(),
                )
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let Some(position) = cursor.position_in(bounds)
                    && let ControllerDragMode::Drawing { start, .. } = state.mode
                {
                    state.mode = ControllerDragMode::Drawing {
                        start,
                        current: position,
                    };
                    return Some(CanvasAction::capture());
                }
                None
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if let ControllerDragMode::Adjusting {
                    target,
                    start_y,
                    start_value,
                } = state.mode
                {
                    state.mode = ControllerDragMode::None;
                    let Some(position) = cursor.position_in(bounds) else {
                        return Some(CanvasAction::capture());
                    };
                    let delta = ((start_y - position.y) / 4.0).round() as i16;
                    let value = (i16::from(start_value) + delta).clamp(0, 127) as u8;
                    let msg = match target {
                        ControllerAdjustTarget::Controller(controller_index) => {
                            Message::PianoSetControllerValue {
                                controller_index,
                                value,
                            }
                        }
                        ControllerAdjustTarget::Velocity(note_index) => Message::PianoSetVelocity {
                            note_index,
                            velocity: value,
                        },
                    };
                    return Some(CanvasAction::publish(msg).and_capture());
                }
                None
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                if matches!(lane, PianoControllerLane::Velocity) {
                    return None;
                }
                if let Some(position) = cursor.position_in(bounds) {
                    state.mode = ControllerDragMode::Drawing {
                        start: position,
                        current: position,
                    };
                    return Some(CanvasAction::capture());
                }
                None
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right)) => {
                if let ControllerDragMode::Drawing { start, current } = state.mode {
                    state.mode = ControllerDragMode::None;
                    let lane_cfg = match lane {
                        PianoControllerLane::Controller => {
                            controllers_lane::LaneConfig::Controller(
                                127_u8.saturating_sub(selected_row.unwrap_or(126) as u8),
                            )
                        }
                        PianoControllerLane::Rpn => {
                            controllers_lane::LaneConfig::Rpn(selected_row.unwrap_or(0))
                        }
                        PianoControllerLane::Nrpn => {
                            controllers_lane::LaneConfig::Nrpn(selected_row.unwrap_or(0))
                        }
                        PianoControllerLane::Velocity => return Some(CanvasAction::capture()),
                    };
                    let new_controllers = controllers_lane::build_drawn_controllers(
                        start, current, bounds, pps, clip_len, lane_cfg,
                    );
                    if new_controllers.is_empty() {
                        return Some(CanvasAction::capture());
                    }
                    return Some(
                        CanvasAction::publish(Message::PianoInsertControllers {
                            controllers: new_controllers,
                        })
                        .and_capture(),
                    );
                }
                None
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
        _renderer: &Renderer,
        _theme: &Theme,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        vec![]
    }
}

mod controllers_lane {
    use iced::{Point, Rectangle};

    use crate::message::{PianoControllerEditData, PianoNrpnKind, PianoRpnKind};

    pub enum LaneConfig {
        Controller(u8),
        Rpn(usize),
        Nrpn(usize),
    }

    fn y_to_value(y: f32, bounds: Rectangle) -> u8 {
        if bounds.height <= f32::EPSILON {
            return 64;
        }
        let t = (1.0 - (y / bounds.height)).clamp(0.0, 1.0);
        (t * 127.0).round().clamp(0.0, 127.0) as u8
    }

    fn rpn_param(row: usize) -> (u8, u8) {
        let kind = PianoRpnKind::ALL
            .get(row)
            .copied()
            .unwrap_or(PianoRpnKind::PitchBendSensitivity);
        match kind {
            PianoRpnKind::PitchBendSensitivity => (0, 0),
            PianoRpnKind::FineTuning => (0, 1),
            PianoRpnKind::CoarseTuning => (0, 2),
        }
    }

    fn nrpn_param(row: usize) -> (u8, u8) {
        let kind = PianoNrpnKind::ALL
            .get(row)
            .copied()
            .unwrap_or(PianoNrpnKind::Brightness);
        match kind {
            PianoNrpnKind::Brightness => (1, 8),
            PianoNrpnKind::VibratoRate => (1, 9),
            PianoNrpnKind::VibratoDepth => (1, 10),
        }
    }

    pub fn build_drawn_controllers(
        start: Point,
        end: Point,
        bounds: Rectangle,
        pps: f32,
        clip_len: usize,
        lane: LaneConfig,
    ) -> Vec<PianoControllerEditData> {
        if pps <= f32::EPSILON || clip_len == 0 {
            return vec![];
        }
        let x0 = start.x.min(end.x).max(0.0);
        let x1 = start.x.max(end.x).max(0.0);
        let s0 = (x0 / pps).round().max(0.0) as usize;
        let s1 = (x1 / pps).round().max(0.0) as usize;
        let start_sample = s0.min(clip_len.saturating_sub(1));
        let end_sample = s1.min(clip_len.saturating_sub(1)).max(start_sample);
        let start_value = i16::from(y_to_value(start.y, bounds));
        let end_value = i16::from(y_to_value(end.y, bounds));
        let delta = end_value - start_value;
        let value_steps = delta.unsigned_abs() as usize;
        let points = value_steps + 1;

        let mut out = Vec::new();
        match lane {
            LaneConfig::Controller(cc) => {
                for i in 0..points {
                    let t = if points <= 1 {
                        0.0
                    } else {
                        i as f32 / (points - 1) as f32
                    };
                    let sample = start_sample + ((end_sample - start_sample) as f32 * t) as usize;
                    let value = if delta >= 0 {
                        (start_value + i as i16).clamp(0, 127) as u8
                    } else {
                        (start_value - i as i16).clamp(0, 127) as u8
                    };
                    out.push(PianoControllerEditData {
                        sample,
                        controller: cc,
                        value,
                        channel: 0,
                    });
                }
            }
            LaneConfig::Rpn(row) => {
                let (msb, lsb) = rpn_param(row);
                out.push(PianoControllerEditData {
                    sample: start_sample,
                    controller: 101,
                    value: msb,
                    channel: 0,
                });
                out.push(PianoControllerEditData {
                    sample: start_sample,
                    controller: 100,
                    value: lsb,
                    channel: 0,
                });
                for i in 0..points {
                    let t = if points <= 1 {
                        0.0
                    } else {
                        i as f32 / (points - 1) as f32
                    };
                    let sample = start_sample + ((end_sample - start_sample) as f32 * t) as usize;
                    let value = if delta >= 0 {
                        (start_value + i as i16).clamp(0, 127) as u8
                    } else {
                        (start_value - i as i16).clamp(0, 127) as u8
                    };
                    out.push(PianoControllerEditData {
                        sample,
                        controller: 6,
                        value,
                        channel: 0,
                    });
                }
            }
            LaneConfig::Nrpn(row) => {
                let (msb, lsb) = nrpn_param(row);
                out.push(PianoControllerEditData {
                    sample: start_sample,
                    controller: 99,
                    value: msb,
                    channel: 0,
                });
                out.push(PianoControllerEditData {
                    sample: start_sample,
                    controller: 98,
                    value: lsb,
                    channel: 0,
                });
                for i in 0..points {
                    let t = if points <= 1 {
                        0.0
                    } else {
                        i as f32 / (points - 1) as f32
                    };
                    let sample = start_sample + ((end_sample - start_sample) as f32 * t) as usize;
                    let value = if delta >= 0 {
                        (start_value + i as i16).clamp(0, 127) as u8
                    } else {
                        (start_value - i as i16).clamp(0, 127) as u8
                    };
                    out.push(PianoControllerEditData {
                        sample,
                        controller: 6,
                        value,
                        channel: 0,
                    });
                }
            }
        }
        out
    }
}

#[derive(Debug)]
pub struct PianoRollInteraction {
    pub state: State,
    pub pixels_per_sample: f32,
}

impl PianoRollInteraction {
    pub fn new(state: State, pixels_per_sample: f32) -> Self {
        Self {
            state,
            pixels_per_sample,
        }
    }

    fn note_at_position(
        &self,
        position: Point,
        row_h: f32,
        pps: f32,
        notes: &[crate::state::PianoNote],
    ) -> Option<usize> {
        for (idx, note) in notes.iter().enumerate() {
            if note.pitch > Piano::PITCH_MAX {
                continue;
            }
            let y_idx = usize::from(Piano::PITCH_MAX.saturating_sub(note.pitch));
            let y = y_idx as f32 * row_h + 1.0;
            let x = note.start_sample as f32 * pps;
            let w = (note.length_samples as f32 * pps).max(2.0);
            let h = (row_h - 2.0).max(2.0);

            if position.x >= x && position.x <= x + w && position.y >= y && position.y <= y + h {
                return Some(idx);
            }
        }
        None
    }

    fn velocity_delta_from_scroll(delta: &mouse::ScrollDelta) -> i8 {
        let raw = match delta {
            mouse::ScrollDelta::Lines { y, .. } => *y,
            mouse::ScrollDelta::Pixels { y, .. } => *y / 16.0,
        };
        let mut steps = raw.round() as i32;
        if steps == 0 && raw.abs() > f32::EPSILON {
            steps = raw.signum() as i32;
        }
        steps.clamp(-24, 24) as i8
    }
}

#[derive(Default, Debug)]
pub struct PianoRollInteractionState {
    dragging_mode: DraggingMode,
    drag_start: Option<Point>,
}

#[derive(Default, Debug, Clone, Copy, PartialEq)]
enum DraggingMode {
    #[default]
    None,
    SelectingRect,
    DraggingNotes,
    ResizingNote,
    CreatingNote,
}

impl Program<Message> for PianoRollInteraction {
    type State = PianoRollInteractionState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<CanvasAction<Message>> {
        let app_state = self.state.blocking_read();
        let Some(roll) = app_state.piano.as_ref() else {
            return None;
        };

        let zoom_x = app_state.piano_zoom_x;
        let zoom_y = app_state.piano_zoom_y;
        let row_h = ((Piano::WHITE_KEY_HEIGHT * Piano::WHITE_KEYS_PER_OCTAVE as f32
            / Piano::NOTES_PER_OCTAVE as f32)
            * zoom_y)
            .max(1.0);
        let pps = (self.pixels_per_sample * zoom_x).max(0.0001);
        let notes = roll.notes.clone();
        drop(app_state);

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(position) = cursor.position_in(bounds) {
                    if let Some(note_idx) = self.note_at_position(position, row_h, pps, &notes) {
                        let Some(note) = notes.get(note_idx) else {
                            return None;
                        };
                        let note_x = note.start_sample as f32 * pps;
                        let note_w = (note.length_samples as f32 * pps).max(2.0);
                        let resize_handle_w = 6.0;
                        if position.x <= note_x + resize_handle_w {
                            state.drag_start = Some(position);
                            state.dragging_mode = DraggingMode::ResizingNote;
                            return Some(
                                CanvasAction::publish(Message::PianoNoteResizeStart {
                                    note_index: note_idx,
                                    position,
                                    resize_start: true,
                                })
                                .and_capture(),
                            );
                        }
                        if position.x >= note_x + note_w - resize_handle_w {
                            state.drag_start = Some(position);
                            state.dragging_mode = DraggingMode::ResizingNote;
                            return Some(
                                CanvasAction::publish(Message::PianoNoteResizeStart {
                                    note_index: note_idx,
                                    position,
                                    resize_start: false,
                                })
                                .and_capture(),
                            );
                        }
                        state.drag_start = Some(position);
                        state.dragging_mode = DraggingMode::DraggingNotes;
                        return Some(
                            CanvasAction::publish(Message::PianoNoteClick {
                                note_index: note_idx,
                                position,
                            })
                            .and_capture(),
                        );
                    } else {
                        // Clicking on empty space starts rectangle selection
                        state.drag_start = Some(position);
                        state.dragging_mode = DraggingMode::SelectingRect;
                        return Some(
                            CanvasAction::publish(Message::PianoSelectRectStart { position })
                                .and_capture(),
                        );
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                if let Some(position) = cursor.position_in(bounds) {
                    state.drag_start = Some(position);
                    state.dragging_mode = DraggingMode::CreatingNote;
                    return Some(
                        CanvasAction::publish(Message::PianoCreateNoteStart { position })
                            .and_capture(),
                    );
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let Some(position) = cursor.position_in(bounds) {
                    if state.drag_start.is_some() {
                        match state.dragging_mode {
                            DraggingMode::SelectingRect => {
                                return Some(CanvasAction::publish(Message::PianoSelectRectDrag {
                                    position,
                                }));
                            }
                            DraggingMode::DraggingNotes => {
                                return Some(CanvasAction::publish(Message::PianoNotesDrag {
                                    position,
                                }));
                            }
                            DraggingMode::ResizingNote => {
                                return Some(CanvasAction::publish(Message::PianoNoteResizeDrag {
                                    position,
                                }));
                            }
                            DraggingMode::CreatingNote => {
                                return Some(CanvasAction::publish(Message::PianoCreateNoteDrag {
                                    position,
                                }));
                            }
                            DraggingMode::None => {}
                        }
                    }
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                if let Some(position) = cursor.position_in(bounds)
                    && let Some(note_idx) = self.note_at_position(position, row_h, pps, &notes)
                {
                    let velocity_delta = Self::velocity_delta_from_scroll(delta);
                    if velocity_delta != 0 {
                        return Some(
                            CanvasAction::publish(Message::PianoAdjustVelocity {
                                note_index: note_idx,
                                delta: velocity_delta,
                            })
                            .and_capture(),
                        );
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.drag_start.is_some() {
                    let mode = state.dragging_mode;
                    state.drag_start = None;
                    state.dragging_mode = DraggingMode::None;

                    match mode {
                        DraggingMode::SelectingRect => {
                            return Some(CanvasAction::publish(Message::PianoSelectRectEnd));
                        }
                        DraggingMode::DraggingNotes => {
                            return Some(CanvasAction::publish(Message::PianoNotesEndDrag));
                        }
                        DraggingMode::ResizingNote => {
                            return Some(CanvasAction::publish(Message::PianoNoteResizeEnd));
                        }
                        DraggingMode::CreatingNote => {
                            return Some(CanvasAction::publish(Message::PianoCreateNoteEnd));
                        }
                        DraggingMode::None => {}
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right)) => {
                if state.drag_start.is_some() {
                    let mode = state.dragging_mode;
                    state.drag_start = None;
                    state.dragging_mode = DraggingMode::None;

                    match mode {
                        DraggingMode::CreatingNote => {
                            return Some(CanvasAction::publish(Message::PianoCreateNoteEnd));
                        }
                        DraggingMode::None => {}
                        DraggingMode::SelectingRect
                        | DraggingMode::DraggingNotes
                        | DraggingMode::ResizingNote => {}
                    }
                }
            }
            _ => {}
        }
        None
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let app_state = self.state.blocking_read();
        let Some(roll) = app_state.piano.as_ref() else {
            return vec![];
        };

        let zoom_x = app_state.piano_zoom_x;
        let zoom_y = app_state.piano_zoom_y;
        let selected_notes = &app_state.piano_selected_notes;
        let selecting_rect = app_state.piano_selecting_rect;
        let dragging_notes = app_state.piano_dragging_notes.as_ref();
        let resizing_note = app_state.piano_resizing_note.as_ref();
        let creating_note = app_state.piano_creating_note;

        let row_h = ((Piano::WHITE_KEY_HEIGHT * Piano::WHITE_KEYS_PER_OCTAVE as f32
            / Piano::NOTES_PER_OCTAVE as f32)
            * zoom_y)
            .max(1.0);
        let pps = (self.pixels_per_sample * zoom_x).max(0.0001);

        let mut frame = Frame::new(renderer, bounds.size());

        // Draw selection highlights for selected notes
        for &note_idx in selected_notes {
            if let Some(note) = roll.notes.get(note_idx) {
                if note.pitch > Piano::PITCH_MAX {
                    continue;
                }
                let y_idx = usize::from(Piano::PITCH_MAX.saturating_sub(note.pitch));
                let y = y_idx as f32 * row_h + 1.0;
                let x = note.start_sample as f32 * pps;
                let w = (note.length_samples as f32 * pps).max(2.0);
                let h = (row_h - 2.0).max(2.0);

                let selection_rect = Rectangle {
                    x: x - 1.0,
                    y: y - 1.0,
                    width: w + 2.0,
                    height: h + 2.0,
                };

                let path = Path::rectangle(
                    Point::new(selection_rect.x, selection_rect.y),
                    Size::new(selection_rect.width, selection_rect.height),
                );
                frame.stroke(
                    &path,
                    canvas::Stroke::default()
                        .with_color(Color::from_rgb(0.9, 0.7, 0.3))
                        .with_width(2.0),
                );
            }
        }

        // Draw dragging notes preview
        if let Some(dragging) = dragging_notes {
            let delta_x = dragging.current_point.x - dragging.start_point.x;
            let delta_y = dragging.current_point.y - dragging.start_point.y;

            for note in &dragging.original_notes {
                if note.pitch > Piano::PITCH_MAX {
                    continue;
                }
                let y_idx = usize::from(Piano::PITCH_MAX.saturating_sub(note.pitch));
                let y = y_idx as f32 * row_h + 1.0 + delta_y;
                let x = note.start_sample as f32 * pps + delta_x;
                let w = (note.length_samples as f32 * pps).max(2.0);
                let h = (row_h - 2.0).max(2.0);

                let note_rect = Rectangle {
                    x,
                    y,
                    width: w,
                    height: h,
                };

                let path = Path::rectangle(
                    Point::new(note_rect.x, note_rect.y),
                    Size::new(note_rect.width, note_rect.height),
                );
                frame.fill(
                    &path,
                    Color {
                        r: 0.5,
                        g: 0.5,
                        b: 0.7,
                        a: 0.5,
                    },
                );
            }
        }

        // Draw rectangle selection box
        if let Some((start, end)) = selecting_rect {
            let min_x = start.x.min(end.x);
            let min_y = start.y.min(end.y);
            let max_x = start.x.max(end.x);
            let max_y = start.y.max(end.y);

            let selection_rect = Rectangle {
                x: min_x,
                y: min_y,
                width: max_x - min_x,
                height: max_y - min_y,
            };

            let path = Path::rectangle(
                Point::new(selection_rect.x, selection_rect.y),
                Size::new(selection_rect.width, selection_rect.height),
            );
            frame.fill(
                &path,
                Color {
                    r: 0.3,
                    g: 0.5,
                    b: 0.8,
                    a: 0.2,
                },
            );
            frame.stroke(
                &path,
                canvas::Stroke::default()
                    .with_color(Color::from_rgb(0.4, 0.6, 0.9))
                    .with_width(1.5),
            );
        }

        // Draw note-resize preview
        if let Some(resizing) = resizing_note {
            let delta_x = resizing.current_point.x - resizing.start_point.x;
            let delta_samples = (delta_x / pps) as i64;
            let original = &resizing.original_note;
            let original_end = original
                .start_sample
                .saturating_add(original.length_samples)
                .max(1);
            let (preview_start, preview_len) = if resizing.resize_start {
                let max_start = original_end.saturating_sub(1) as i64;
                let new_start = (original.start_sample as i64 + delta_samples).clamp(0, max_start);
                let new_start = new_start as usize;
                (new_start, original_end.saturating_sub(new_start).max(1))
            } else {
                let min_end = original.start_sample.saturating_add(1) as i64;
                let new_end = (original_end as i64 + delta_samples).max(min_end) as usize;
                (
                    original.start_sample,
                    new_end.saturating_sub(original.start_sample).max(1),
                )
            };

            if original.pitch <= Piano::PITCH_MAX {
                let y_idx = usize::from(Piano::PITCH_MAX.saturating_sub(original.pitch));
                let y = y_idx as f32 * row_h + 1.0;
                let x = preview_start as f32 * pps;
                let w = (preview_len as f32 * pps).max(2.0);
                let h = (row_h - 2.0).max(2.0);
                let path = Path::rectangle(Point::new(x, y), Size::new(w, h));
                frame.fill(
                    &path,
                    Color {
                        r: 0.95,
                        g: 0.8,
                        b: 0.4,
                        a: 0.35,
                    },
                );
                frame.stroke(
                    &path,
                    canvas::Stroke::default()
                        .with_color(Color::from_rgb(0.95, 0.8, 0.4))
                        .with_width(1.5),
                );
            }
        }

        // Draw note-creation preview from right-click drag
        if let Some((start, end)) = creating_note {
            let start_x = start.x.min(end.x).max(0.0);
            let end_x = start.x.max(end.x).max(0.0);
            let y_row = (start.y / row_h).floor().max(0.0);
            let y = y_row * row_h + 1.0;
            let h = (row_h - 2.0).max(2.0);
            let w = (end_x - start_x).max(2.0);

            let path = Path::rectangle(Point::new(start_x, y), Size::new(w, h));
            frame.fill(
                &path,
                Color {
                    r: 0.6,
                    g: 0.75,
                    b: 0.95,
                    a: 0.35,
                },
            );
            frame.stroke(
                &path,
                canvas::Stroke::default()
                    .with_color(Color::from_rgb(0.7, 0.85, 1.0))
                    .with_width(1.5),
            );
        }

        drop(app_state);
        vec![frame.into_geometry()]
    }
}
