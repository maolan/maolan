use crate::{
    consts::{
        widget_piano::{
            H_ZOOM_MAX, H_ZOOM_MIN, KEYBOARD_WIDTH, MAIN_SPLIT_SPACING, MIDI_NOTE_COUNT,
            NOTES_PER_OCTAVE, OCTAVES, TOOLS_STRIP_WIDTH, WHITE_KEY_HEIGHT, WHITE_KEYS_PER_OCTAVE,
        },
        workspace::PLAYHEAD_WIDTH_PX,
    },
    menu::{menu_dropdown, menu_item},
    message::{Message, PianoControllerLane},
    state::State,
    widget::{
        controller::ControllerRollInteraction,
        piano::{self, PianoRollInteraction},
    },
};
use iced::{
    Background, Border, Color, Element, Length, Point,
    widget::{
        Id, Stack, button, checkbox, column, container, pick_list, pin, row, scrollable, slider,
        text, text_input, vertical_slider,
    },
};
use iced_aw::{
    menu::{DrawPath, Item, Menu as IcedMenu},
    menu_bar, menu_items,
};
use maolan_widgets::{
    controller::{self, ControllerKindOption},
    note_area::{NoteArea, PianoGridScrolls, piano_grid_scrollers},
    piano::{OctaveKeyboard, row_height},
    piano_roll::PianoRoll,
    vertical_scrollbar::VerticalScrollbar,
};

#[derive(Debug)]
pub struct MIDIEdit {
    state: State,
}

pub use crate::consts::widget_piano::{
    CTRL_SCROLL_ID, KEYS_SCROLL_ID, NOTES_SCROLL_ID, SYSEX_SCROLL_ID,
};

impl MIDIEdit {
    pub fn new(state: State) -> Self {
        Self { state }
    }

    fn zoom_x_to_slider(zoom_x: f32) -> f32 {
        (H_ZOOM_MIN + H_ZOOM_MAX - zoom_x).clamp(H_ZOOM_MIN, H_ZOOM_MAX)
    }

    fn slider_to_zoom_x(slider_value: f32) -> f32 {
        (H_ZOOM_MIN + H_ZOOM_MAX - slider_value).clamp(H_ZOOM_MIN, H_ZOOM_MAX)
    }

    fn populated_menu_item(
        label: impl Into<String>,
        message: Message,
        populated: bool,
    ) -> Element<'static, Message> {
        let label = label.into();
        button(
            text(label)
                .width(Length::Fill)
                .style(move |_theme| iced::widget::text::Style {
                    color: Some(if populated {
                        Color::from_rgb(0.60, 0.88, 0.45)
                    } else {
                        Color::from_rgb(0.86, 0.86, 0.9)
                    }),
                }),
        )
        .padding([4, 8])
        .width(Length::Fill)
        .style(move |_theme, status| {
            use iced::widget::button::{Status, Style};

            let background = match status {
                Status::Hovered => {
                    if populated {
                        Color::from_rgba(0.16, 0.34, 0.16, 1.0)
                    } else {
                        Color::from_rgba(0.22, 0.23, 0.26, 1.0)
                    }
                }
                Status::Pressed => {
                    if populated {
                        Color::from_rgba(0.20, 0.42, 0.18, 1.0)
                    } else {
                        Color::from_rgba(0.30, 0.31, 0.35, 1.0)
                    }
                }
                Status::Disabled => Color::from_rgba(0.18, 0.18, 0.20, 0.7),
                Status::Active => {
                    if populated {
                        Color::from_rgba(0.12, 0.26, 0.12, 0.95)
                    } else {
                        Color::TRANSPARENT
                    }
                }
            };

            Style {
                background: Some(Background::Color(background)),
                text_color: if populated {
                    Color::from_rgb(0.60, 0.88, 0.45)
                } else {
                    Color::from_rgb(0.86, 0.86, 0.9)
                },
                border: Border::default().rounded(6.0),
                ..Style::default()
            }
        })
        .on_press(message)
        .into()
    }

    pub fn view(
        &self,
        pixels_per_sample: f32,
        samples_per_bar: f32,
        playhead_x: Option<f32>,
    ) -> Element<'_, Message> {
        let state = self.state.blocking_read();
        let zoom_x = state.piano_zoom_x;
        let zoom_y = state.piano_zoom_y;
        let humanize_time_amount = state.piano_humanize_time_amount.clamp(0.0, 1.0);
        let humanize_velocity_amount = state.piano_humanize_velocity_amount.clamp(0.0, 1.0);
        let groove_amount = state.piano_groove_amount.clamp(0.0, 1.0);
        let scale_root = state.piano_scale_root;
        let scale_minor = state.piano_scale_minor;
        let chord_kind = state.piano_chord_kind;
        let velocity_shape_amount = state.piano_velocity_shape_amount.clamp(0.0, 1.0);
        let controller_lane = state.piano_controller_lane;

        let Some(roll) = state.piano.as_ref() else {
            return container(text("No MIDI clip selected."))
                .width(Length::Fill)
                .height(Length::Fill)
                .into();
        };
        let roll = roll.clone();

        let notes_content = NoteArea {
            zoom_x,
            zoom_y,
            pixels_per_sample,
            samples_per_bar: Some(samples_per_bar),
            playhead_x,
            playhead_width: PLAYHEAD_WIDTH_PX,
            clip_length_samples: roll.clip_length_samples,
        }
        .view(vec![
            PianoRoll::new(
                roll.notes.clone(),
                roll.clip_length_samples,
                zoom_y,
                pixels_per_sample,
                zoom_x,
                iced::widget::canvas(PianoRollInteraction::new(
                    self.state.clone(),
                    pixels_per_sample,
                ))
                .width(Length::Fixed(
                    (roll.clip_length_samples as f32 * (pixels_per_sample * zoom_x).max(0.0001))
                        .max(1.0),
                ))
                .height(Length::Fixed(MIDI_NOTE_COUNT as f32 * row_height(zoom_y)))
                .into(),
            )
            .into_element(),
        ]);

        let ctrl_line_count = controller::controller_lane_line_count(controller_lane).max(1);
        let ctrl_h = (ctrl_line_count as f32).max(140.0);
        let ctrl_row_h = (ctrl_h / ctrl_line_count as f32).max(1.0);
        let pps_ctrl = (pixels_per_sample * zoom_x).max(0.0001);
        let ctrl_w = (roll.clip_length_samples as f32 * pps_ctrl).max(1.0);

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
            let divider = if (row % 8) == 0 { 0.28 } else { 0.2 };
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
            let x_ctrl = beat as f32 * beat_samples * pps_ctrl;
            if x_ctrl > ctrl_w {
                break;
            }
            let bar_line = beat.is_multiple_of(4);
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

        match controller_lane {
            PianoControllerLane::Controller => {
                for (idx, row) in
                    controller::lane_controller_events(controller_lane, &roll.controllers)
                {
                    let ctrl = &roll.controllers[idx];
                    let x = ctrl.sample as f32 * pps_ctrl;
                    let mut color = controller::controller_color(ctrl.controller, ctrl.channel);
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
                    let mut color = piano::note_color(note.velocity, note.channel);
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
                for (idx, row) in
                    controller::lane_controller_events(controller_lane, &roll.controllers)
                {
                    let ctrl = &roll.controllers[idx];
                    let x = ctrl.sample as f32 * pps_ctrl;
                    let mut color = controller::controller_color(ctrl.controller, ctrl.channel);
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
                for (idx, row) in
                    controller::lane_controller_events(controller_lane, &roll.controllers)
                {
                    let ctrl = &roll.controllers[idx];
                    let x = ctrl.sample as f32 * pps_ctrl;
                    let mut color = controller::controller_color(ctrl.controller, ctrl.channel);
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
            PianoControllerLane::SysEx => {
                for (idx, sysex) in roll.sysexes.iter().enumerate() {
                    let x = sysex.sample as f32 * pps_ctrl;
                    let selected = state.piano_selected_sysex == Some(idx);
                    let color = if selected {
                        Color::from_rgba(1.0, 0.55, 0.2, 0.95)
                    } else {
                        Color::from_rgba(0.95, 0.35, 0.2, 0.75)
                    };
                    ctrl_layers.push(
                        pin(container("")
                            .width(Length::Fixed(2.0))
                            .height(Length::Fixed(ctrl_h))
                            .style(move |_theme| container::Style {
                                background: Some(Background::Color(color)),
                                ..container::Style::default()
                            }))
                        .position(Point::new(x, 0.0))
                        .into(),
                    );
                    ctrl_layers.push(
                        pin(container("")
                            .width(Length::Fixed(6.0))
                            .height(Length::Fixed(6.0))
                            .style(move |_theme| container::Style {
                                background: Some(Background::Color(color)),
                                ..container::Style::default()
                            }))
                        .position(Point::new((x - 2.0).max(0.0), 0.0))
                        .into(),
                    );
                }
            }
        }

        if let Some(x) = playhead_x {
            let x = x.max(0.0);
            ctrl_layers.push(
                pin(container("")
                    .width(Length::Fixed(PLAYHEAD_WIDTH_PX))
                    .height(Length::Fixed(ctrl_h))
                    .style(|_theme| container::Style {
                        background: Some(Background::Color(Color {
                            r: 0.95,
                            g: 0.18,
                            b: 0.14,
                            a: 0.95,
                        })),
                        ..container::Style::default()
                    }))
                .position(Point::new(x, 0.0))
                .into(),
            );
        }

        ctrl_layers.push(
            pin(iced::widget::canvas(ControllerRollInteraction::new(
                self.state.clone(),
                pixels_per_sample,
                (samples_per_bar as f64 * state.tempo as f64 / 240.0).max(1.0),
                samples_per_bar,
            ))
            .width(Length::Fixed(ctrl_w))
            .height(Length::Fixed(ctrl_h)))
            .position(Point::new(0.0, 0.0))
            .into(),
        );

        let ctrl_content = Stack::from_vec(ctrl_layers)
            .width(Length::Fixed(ctrl_w))
            .height(Length::Fixed(ctrl_h));

        let notes_h = MIDI_NOTE_COUNT as f32
            * ((WHITE_KEY_HEIGHT * WHITE_KEYS_PER_OCTAVE as f32 / NOTES_PER_OCTAVE as f32)
                * zoom_y)
                .max(1.0);
        let midnam_note_names = roll.midnam_note_names.clone();
        let keyboard = (0..OCTAVES).fold(column![], |col, octave_idx| {
            let octave = (OCTAVES - 1 - octave_idx) as u8;
            let octave_h = piano::octave_note_count(octave) as f32
                * ((WHITE_KEY_HEIGHT * WHITE_KEYS_PER_OCTAVE as f32 / NOTES_PER_OCTAVE as f32)
                    * zoom_y)
                    .max(1.0);
            col.push(
                iced::widget::canvas(OctaveKeyboard::new(
                    octave,
                    midnam_note_names.clone(),
                    Message::PianoKeyPressed,
                    Message::PianoKeyReleased,
                ))
                .width(Length::Fixed(KEYBOARD_WIDTH))
                .height(Length::Fixed(octave_h)),
            )
        });
        let piano_note_keys = keyboard
            .width(Length::Fixed(KEYBOARD_WIDTH))
            .height(Length::Fill);
        let populated_ccs = controller::populated_controller_ccs(&roll.controllers);
        let populated_rpn_rows =
            controller::populated_controller_rows(PianoControllerLane::Rpn, &roll.controllers);
        let populated_nrpn_rows =
            controller::populated_controller_rows(PianoControllerLane::Nrpn, &roll.controllers);
        let controller_picker = pick_list(
            vec![
                PianoControllerLane::Controller,
                PianoControllerLane::Velocity,
                PianoControllerLane::Rpn,
                PianoControllerLane::Nrpn,
                PianoControllerLane::SysEx,
            ],
            Some(state.piano_controller_lane),
            Message::PianoControllerLaneSelected,
        )
        .width(Length::Fill);
        let controller_number_picker: Element<'_, Message> = match state.piano_controller_lane {
            PianoControllerLane::Controller => {
                let selected = format!("CC{:03} \u{25BE}", state.piano_controller_kind);
                let cc_menu = IcedMenu::new(
                    (0u8..=127)
                        .map(|cc| {
                            Item::new(Self::populated_menu_item(
                                ControllerKindOption(cc).to_string(),
                                Message::PianoControllerKindSelected(cc),
                                populated_ccs.contains(&cc),
                            ))
                        })
                        .collect::<Vec<_>>(),
                )
                .width(320.0)
                .offset(10.0)
                .spacing(4.0);
                #[rustfmt::skip]
                let picker = menu_bar!(
                    (menu_dropdown(selected, Message::None), {
                        cc_menu
                    })
                )
                .draw_path(DrawPath::Backdrop)
                .close_on_item_click_global(true)
                .width(Length::Fill);
                picker.into()
            }
            PianoControllerLane::Velocity => {
                let selected = format!("{} \u{25BE}", state.piano_velocity_kind);
                let velocity_menu = IcedMenu::new(
                    crate::consts::message_lists::PIANO_VELOCITY_KIND_ALL
                        .iter()
                        .copied()
                        .map(|kind| {
                            Item::new(menu_item(
                                kind.to_string(),
                                Message::PianoVelocityKindSelected(kind),
                            ))
                        })
                        .collect::<Vec<_>>(),
                )
                .width(280.0)
                .offset(10.0)
                .spacing(4.0);
                #[rustfmt::skip]
                let picker = menu_bar!(
                    (menu_dropdown(selected, Message::None), {
                        velocity_menu
                    })
                )
                .draw_path(DrawPath::Backdrop)
                .close_on_item_click_global(true)
                .width(Length::Fill);
                picker.into()
            }
            PianoControllerLane::Rpn => {
                let selected = format!("{} \u{25BE}", state.piano_rpn_kind);
                let rpn_menu = IcedMenu::new(
                    crate::consts::message_lists::PIANO_RPN_KIND_ALL
                        .iter()
                        .copied()
                        .enumerate()
                        .map(|(row, kind)| {
                            Item::new(Self::populated_menu_item(
                                kind.to_string(),
                                Message::PianoRpnKindSelected(kind),
                                populated_rpn_rows.contains(&row),
                            ))
                        })
                        .collect::<Vec<_>>(),
                )
                .width(300.0)
                .offset(10.0)
                .spacing(4.0);
                #[rustfmt::skip]
                let picker = menu_bar!(
                    (menu_dropdown(selected, Message::None), {
                        rpn_menu
                    })
                )
                .draw_path(DrawPath::Backdrop)
                .close_on_item_click_global(true)
                .width(Length::Fill);
                picker.into()
            }
            PianoControllerLane::Nrpn => {
                let selected = format!("{} \u{25BE}", state.piano_nrpn_kind);
                let nrpn_menu = IcedMenu::new(
                    crate::consts::message_lists::PIANO_NRPN_KIND_ALL
                        .iter()
                        .copied()
                        .enumerate()
                        .map(|(row, kind)| {
                            Item::new(Self::populated_menu_item(
                                kind.to_string(),
                                Message::PianoNrpnKindSelected(kind),
                                populated_nrpn_rows.contains(&row),
                            ))
                        })
                        .collect::<Vec<_>>(),
                )
                .width(300.0)
                .offset(10.0)
                .spacing(4.0);
                #[rustfmt::skip]
                let picker = menu_bar!(
                    (menu_dropdown(selected, Message::None), {
                        nrpn_menu
                    })
                )
                .draw_path(DrawPath::Backdrop)
                .close_on_item_click_global(true)
                .width(Length::Fill);
                picker.into()
            }
            PianoControllerLane::SysEx => text("SysEx events")
                .size(10)
                .style(|_theme| iced::widget::text::Style {
                    color: Some(Color::from_rgb(0.86, 0.86, 0.9)),
                })
                .into(),
        };
        let controller_header = column![controller_picker, controller_number_picker].spacing(2);
        let controller_key = container(controller_header)
            .width(Length::Fixed(KEYBOARD_WIDTH))
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

        let keyboard_scroll = container(piano_note_keys)
            .width(Length::Fixed(KEYBOARD_WIDTH))
            .height(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(Background::Color(Color {
                    r: 0.12,
                    g: 0.12,
                    b: 0.12,
                    a: 1.0,
                })),
                ..container::Style::default()
            });

        let PianoGridScrolls {
            keyboard_scroll,
            note_scroll,
            h_scroll,
            v_scroll,
        } = piano_grid_scrollers(
            keyboard_scroll.into(),
            notes_content,
            notes_h,
            ctrl_w,
            state.piano_scroll_x,
            state.piano_scroll_y,
            Message::PianoScrollYChanged,
            |x, y| Message::PianoScrollChanged { x, y },
        );

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

        let selected_sysex = state.piano_selected_sysex;
        let sysex_rows = roll
            .sysexes
            .iter()
            .enumerate()
            .fold(column![], |col, (idx, ev)| {
                let is_selected = selected_sysex == Some(idx);
                let label = format!("{:>8}  {}", ev.sample, controller::sysex_preview(&ev.data));
                col.push(
                    button(text(label).size(11))
                        .on_press(Message::PianoSysExSelect(Some(idx)))
                        .style(move |_theme, _status| iced::widget::button::Style {
                            background: Some(Background::Color(if is_selected {
                                Color::from_rgba(0.38, 0.28, 0.18, 1.0)
                            } else {
                                Color::from_rgba(0.17, 0.17, 0.19, 1.0)
                            })),
                            text_color: if is_selected {
                                Color::from_rgb(1.0, 0.86, 0.6)
                            } else {
                                Color::from_rgb(0.82, 0.82, 0.86)
                            },
                            ..Default::default()
                        })
                        .width(Length::Fill),
                )
            });
        let sysex_content_height = ((roll.sysexes.len() as f32) * 24.0).max(1.0);
        let sysex_panel = container(
            column![
                text("SysEx").size(12),
                text_input("F0 ... F7", &state.piano_sysex_hex_input)
                    .on_input(Message::PianoSysExHexInput)
                    .size(12)
                    .padding(6),
                row![
                    button(text("Add").size(11)).on_press(Message::PianoSysExAdd),
                    button(text("Update").size(11)).on_press(Message::PianoSysExUpdate),
                    button(text("Delete").size(11)).on_press(Message::PianoSysExDelete),
                    button(text("Cancel").size(11)).on_press(Message::PianoSysExCloseEditor),
                ]
                .spacing(4),
                row![
                    scrollable(sysex_rows.spacing(2).width(Length::Fill))
                        .id(Id::new(SYSEX_SCROLL_ID))
                        .height(Length::Fill)
                        .direction(scrollable::Direction::Vertical(
                            scrollable::Scrollbar::hidden(),
                        ))
                        .on_scroll(|viewport| {
                            Message::PianoSysExScrollYChanged(viewport.relative_offset().y)
                        }),
                    VerticalScrollbar::new(
                        sysex_content_height,
                        state.piano_sysex_scroll_y,
                        Message::PianoSysExScrollYChanged,
                    )
                    .width(Length::Fixed(16.0))
                    .height(Length::Fill),
                ]
                .spacing(0)
                .height(Length::Fill),
            ]
            .spacing(6)
            .height(Length::Fill),
        )
        .width(Length::Fixed(280.0))
        .height(Length::Fill)
        .padding([6, 6])
        .style(|_theme| container::Style {
            background: Some(Background::Color(Color::from_rgba(0.11, 0.11, 0.13, 1.0))),
            ..container::Style::default()
        });
        let edit_tools_strip = container(
            column![
                text("MIDI Tools").size(12),
                row![
                    button(text("Scale").size(11)).on_press(Message::PianoScaleSelectedNotes),
                    pick_list(
                        crate::consts::message_lists::PIANO_SCALE_ROOT_ALL.to_vec(),
                        Some(scale_root),
                        Message::PianoScaleRootSelected
                    )
                    .width(Length::Fixed(62.0)),
                    checkbox(scale_minor)
                        .label("Min")
                        .on_toggle(Message::PianoScaleMinorToggled),
                ]
                .spacing(6),
                row![
                    button(text("Chord").size(11)).on_press(Message::PianoChordSelectedNotes),
                    pick_list(
                        crate::consts::message_lists::PIANO_CHORD_KIND_ALL.to_vec(),
                        Some(chord_kind),
                        Message::PianoChordKindSelected
                    )
                    .width(Length::Fixed(86.0)),
                ]
                .spacing(6),
                button(text("Legato").size(11)).on_press(Message::PianoLegatoSelectedNotes),
                row![
                    button(text("VelShape").size(11))
                        .on_press(Message::PianoVelocityShapeSelectedNotes),
                    slider(
                        0.0..=1.0,
                        velocity_shape_amount,
                        Message::PianoVelocityShapeAmountChanged
                    )
                    .step(0.01)
                    .width(Length::Fill),
                ]
                .spacing(6),
                button(text("Quantize").size(11)).on_press(Message::PianoQuantizeSelectedNotes),
                row![
                    button(text("Humanize").size(11)).on_press(Message::PianoHumanizeSelectedNotes),
                    text("T").size(10),
                    slider(
                        0.0..=1.0,
                        humanize_time_amount,
                        Message::PianoHumanizeTimeAmountChanged,
                    )
                    .step(0.01)
                    .width(Length::Fill),
                    text("V").size(10),
                    slider(
                        0.0..=1.0,
                        humanize_velocity_amount,
                        Message::PianoHumanizeVelocityAmountChanged,
                    )
                    .step(0.01)
                    .width(Length::Fill),
                ]
                .spacing(6),
                row![
                    button(text("Groove").size(11)).on_press(Message::PianoGrooveSelectedNotes),
                    slider(0.0..=1.0, groove_amount, Message::PianoGrooveAmountChanged)
                        .step(0.01)
                        .width(Length::Fill),
                ]
                .spacing(6),
            ]
            .spacing(8)
            .width(Length::Fill),
        )
        .width(Length::Fixed(TOOLS_STRIP_WIDTH))
        .height(Length::Fill)
        .padding([8, 8])
        .style(|_theme| container::Style {
            background: Some(Background::Color(Color::from_rgba(0.10, 0.10, 0.12, 1.0))),
            ..container::Style::default()
        });

        let mut layout = row![
            row![
                edit_tools_strip,
                column![
                    row![keyboard_scroll, note_scroll]
                        .height(Length::Fill)
                        .width(Length::Fill),
                    row![controller_key, ctrl_scroll],
                    row![
                        container("")
                            .width(Length::Fixed(KEYBOARD_WIDTH))
                            .height(Length::Fixed(16.0)),
                        row![
                            h_scroll,
                            slider(
                                H_ZOOM_MIN..=H_ZOOM_MAX,
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
                .spacing(3)
                .width(Length::Fill)
                .height(Length::Fill),
            ]
            .spacing(MAIN_SPLIT_SPACING)
            .width(Length::Fill)
            .height(Length::Fill),
            column![
                v_scroll,
                vertical_slider(1.0..=8.0, zoom_y, Message::PianoZoomYChanged)
                    .step(0.1)
                    .height(Length::Fixed(100.0)),
            ]
            .spacing(8)
            .height(Length::Fill),
        ];
        if state.piano_sysex_panel_open
            && matches!(state.piano_controller_lane, PianoControllerLane::SysEx)
        {
            layout = layout.push(sysex_panel);
        }
        container(layout)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}
