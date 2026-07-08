pub mod rename;

use crate::{
    menu,
    message::Message,
    state::{SessionMatrix, SlotPlayState, SlotRuntimes, Track},
    style,
};
use iced::widget::canvas::{self, Canvas, Frame, Geometry, Path, Program};
use iced::{
    Alignment, Background, Border, Color, Length, Point, Radians, Rectangle, Renderer, Theme,
    mouse,
    widget::{
        Column, Id, Row, Space, Stack, button, column, container, lazy, mouse_area, pin, row,
        scrollable, text,
    },
};
use iced_fonts::lucide::{play, square};
use maolan_engine::message::MidiLearnBinding;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

pub struct SessionView;

#[derive(Clone, Default)]
pub struct SessionViewInput {
    pub tracks: Vec<crate::state::Track>,
    pub session: SessionMatrix,
    pub slot_runtimes: SlotRuntimes,
    pub selected_slots: HashSet<(String, usize)>,
    pub selected: HashSet<String>,
    pub selected_scene: Option<usize>,
    pub midi_learn: SessionMidiLearnBindings,
    pub master_track: Option<crate::state::Track>,
    pub session_scene_context_menu: Option<crate::state::SessionSceneContextMenuState>,
}

impl SessionViewInput {
    fn render_hash(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();

        for track in &self.tracks {
            track.name.hash(&mut hasher);
            track.height.to_bits().hash(&mut hasher);
            track.is_folder.hash(&mut hasher);
            track.folder_open.hash(&mut hasher);
            track.parent_track.hash(&mut hasher);
        }

        self.session.scenes.len().hash(&mut hasher);
        for scene in &self.session.scenes {
            scene.name.hash(&mut hasher);
        }

        for track in &self.tracks {
            if let Some(slots) = self.session.slots.get(&track.name) {
                for (scene_index, slot) in slots.iter().enumerate() {
                    track.name.hash(&mut hasher);
                    scene_index.hash(&mut hasher);
                    slot.clip
                        .as_ref()
                        .map(|clip_ref| clip_ref.clip_id.as_str())
                        .hash(&mut hasher);
                    slot.play_stop_icon.hash(&mut hasher);
                    slot.clip_name.hash(&mut hasher);
                    let selected = self
                        .selected_slots
                        .contains(&(track.name.clone(), scene_index));
                    selected.hash(&mut hasher);
                    if let Some(runtime) =
                        self.slot_runtimes.get(&(track.name.clone(), scene_index))
                    {
                        runtime.state.hash(&mut hasher);
                        runtime.play_position_samples.hash(&mut hasher);
                        runtime.elapsed_samples.hash(&mut hasher);
                    }
                    let has_midi_learn = self
                        .midi_learn
                        .slots
                        .contains_key(&(track.name.clone(), scene_index));
                    has_midi_learn.hash(&mut hasher);
                }
            }
        }

        self.midi_learn.scenes.len().hash(&mut hasher);
        for scene_index in self.midi_learn.scenes.keys() {
            scene_index.hash(&mut hasher);
        }

        self.selected.len().hash(&mut hasher);
        let mut selected_names: Vec<&String> = self.selected.iter().collect();
        selected_names.sort();
        for name in selected_names {
            name.hash(&mut hasher);
        }

        self.selected_scene.hash(&mut hasher);

        if let Some(master) = &self.master_track {
            master.name.hash(&mut hasher);
            master.audio.outs.hash(&mut hasher);
            master.level.to_bits().hash(&mut hasher);
            master.balance.to_bits().hash(&mut hasher);
            for value in &master.meter_out_db {
                value.to_bits().hash(&mut hasher);
            }
        }

        if let Some(menu) = &self.session_scene_context_menu {
            menu.scene_index.hash(&mut hasher);
            menu.anchor.x.to_bits().hash(&mut hasher);
            menu.anchor.y.to_bits().hash(&mut hasher);
        }

        hasher.finish()
    }
}

impl SessionView {
    pub fn new() -> Self {
        Self
    }

    pub fn view(input: SessionViewInput) -> iced::Element<'static, Message> {
        let mut render_input = input;
        render_input
            .tracks
            .retain(|t| t.name != crate::consts::state_ids::METRONOME_TRACK_ID);
        let hash = render_input.render_hash();
        lazy(hash, move |_| Self::build_body(render_input.clone())).into()
    }

    fn build_body(args: SessionViewInput) -> iced::Element<'static, Message> {
        let mut strips = Row::new()
            .spacing(2)
            .padding([8, 6])
            .align_y(Alignment::Start)
            .height(Length::Fill);

        let children_by_parent = build_children_map(&args.tracks);
        for track in args.tracks.iter().filter(|t| t.parent_track.is_none()) {
            let strip_width = crate::workspace::Workspace::mixer_strip_width(track.audio.outs);
            strips = strips.push(track_strip(track, &args, strip_width, &children_by_parent));
        }

        let track_strips = scrollable(strips)
            .width(Length::Fill)
            .height(Length::Fill)
            .direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::new(),
            ));

        let body: iced::Element<'static, Message> = if let Some(master) = &args.master_track {
            let master_width =
                crate::workspace::Workspace::mixer_strip_width(master.audio.outs.max(1));
            let empty_children = HashMap::new();
            let master_strip = track_strip(master, &args, master_width, &empty_children);
            row![track_strips, master_strip]
                .spacing(2)
                .align_y(Alignment::Start)
                .height(Length::Fill)
                .into()
        } else {
            track_strips.into()
        };

        container(body)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(Background::Color(Color::from_rgb(0.12, 0.12, 0.14))),
                ..Default::default()
            })
            .into()
    }
}

#[derive(Clone, Default)]
pub struct SessionMidiLearnBindings {
    pub slots: HashMap<(String, usize), MidiLearnBinding>,
    pub scenes: HashMap<usize, MidiLearnBinding>,
}

const SCENE_CONTEXT_MENU_WIDTH: f32 = 140.0;

fn scene_context_menu_overlay(
    menu_state: &crate::state::SessionSceneContextMenuState,
) -> iced::Element<'static, Message> {
    let scene_index = menu_state.scene_index;
    let items: Vec<iced::Element<'static, Message>> = vec![
        menu::menu_item("Rename", Message::SessionSceneRenameShow(scene_index)),
        menu::menu_item("Remove", Message::SessionSceneRemove(scene_index)),
    ];
    container(Column::with_children(items).spacing(2))
        .width(Length::Fixed(SCENE_CONTEXT_MENU_WIDTH))
        .padding(6)
        .style(|theme: &Theme| container::Style {
            background: Some(Background::Color(
                theme.extended_palette().background.weak.color,
            )),
            border: Border {
                color: theme.extended_palette().background.strong.color,
                width: 1.0,
                radius: 6.0.into(),
            },
            ..Default::default()
        })
        .into()
}

#[derive(Clone)]
struct SlotState {
    play_stop_icon: Option<bool>,
    clip_name: Option<String>,
}

fn display_clip_name(name: &str) -> String {
    let mut display = name
        .strip_prefix("audio/")
        .or_else(|| name.strip_prefix("midi/"))
        .unwrap_or(name)
        .to_string();
    for suffix in [".wav", ".midi", ".mid"] {
        if let Some(stripped) = display.strip_suffix(suffix) {
            display = stripped.to_string();
            break;
        }
    }
    display
}

fn slot_button(
    track_name: String,
    scene_index: usize,
    _track_color: Option<Color>,
    slot: SlotState,
    is_master: bool,
    label: String,
) -> iced::Element<'static, Message> {
    let icon_content: iced::Element<'static, Message> = match slot.play_stop_icon {
        Some(true) => play().size(14).color(Color::WHITE).into(),
        Some(false) => square().size(14).color(Color::WHITE).into(),
        None => square().size(14).color(Color::TRANSPARENT).into(),
    };
    let next_icon = Some(!slot.play_stop_icon.unwrap_or(true));
    let label_element: iced::Element<'static, Message> =
        container(text(label).size(12).color(Color::WHITE).width(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .clip(true)
            .into();
    let slot_row: iced::Element<'static, Message> = if is_master {
        row![label_element]
            .align_y(Alignment::Center)
            .width(Length::Fill)
            .height(Length::Fixed(30.0))
            .into()
    } else {
        row![icon_content, label_element]
            .spacing(4)
            .align_y(Alignment::Center)
            .width(Length::Fill)
            .height(Length::Fixed(30.0))
            .into()
    };
    let slot_container = container(slot_row)
        .width(Length::Fill)
        .height(Length::Fixed(30.0))
        .style(|_| style::mixer::slot())
        .padding([0, 4]);
    let slot_container = if is_master {
        slot_container
    } else {
        slot_container.id(Id::from(slot_zone_id(&track_name, scene_index)))
    };
    let slot_content: iced::Element<'static, Message> = slot_container.into();
    let area = mouse_area(slot_content).on_press(Message::SessionSlotSetPlayStopIcon {
        track_name: track_name.clone(),
        scene_index,
        icon: next_icon,
    });
    if is_master {
        area.on_move(move |position| Message::SessionSceneContextMenuHover {
            scene_index,
            position,
        })
        .on_right_press(Message::SessionSceneRightClick {
            scene_index,
            point: Point::new(0.0, 0.0),
        })
        .into()
    } else {
        area.on_right_press(Message::SessionSlotSetPlayStopIcon {
            track_name: track_name.clone(),
            scene_index,
            icon: None,
        })
        .into()
    }
}

fn slot_zone_id(track_name: &str, scene_index: usize) -> String {
    format!("session-slot:{}:{}", track_name, scene_index)
}

fn build_children_map(tracks: &[Track]) -> HashMap<String, Vec<&Track>> {
    let mut map: HashMap<String, Vec<&Track>> = HashMap::new();
    for track in tracks {
        if let Some(parent) = track.parent_track.as_deref() {
            map.entry(parent.to_string()).or_default().push(track);
        }
    }
    map
}

fn clip_length_for_slot(
    track: &Track,
    session: &SessionMatrix,
    scene_index: usize,
) -> Option<usize> {
    let slots = session.slots.get(&track.name)?;
    let slot = slots.get(scene_index)?;
    let clip_ref = slot.clip.as_ref()?;
    track
        .audio
        .clips
        .iter()
        .find(|c| c.id == clip_ref.clip_id)
        .map(|c| c.length)
        .or_else(|| {
            track
                .midi
                .clips
                .iter()
                .find(|c| c.id == clip_ref.clip_id)
                .map(|c| c.length)
        })
}

fn scene_cycle_length(tracks: &[Track], session: &SessionMatrix, scene_index: usize) -> usize {
    tracks
        .iter()
        .filter(|t| !t.is_master && !t.is_folder)
        .filter_map(|t| clip_length_for_slot(t, session, scene_index))
        .max()
        .unwrap_or(0)
}

fn clip_repeat_count(
    track: &Track,
    session: &SessionMatrix,
    scene_index: usize,
    tracks: &[Track],
) -> usize {
    let clip_length = match clip_length_for_slot(track, session, scene_index) {
        Some(len) if len > 0 => len,
        _ => return 1,
    };
    let cycle_length = scene_cycle_length(tracks, session, scene_index);
    if cycle_length == 0 {
        return 1;
    }
    let ratio = cycle_length as f64 / clip_length as f64;
    ratio.round().max(1.0) as usize
}

fn clip_loop_count(
    track: &Track,
    session: &SessionMatrix,
    scene_index: usize,
    tracks: &[Track],
    slot_runtimes: &SlotRuntimes,
) -> (usize, usize) {
    let clip_length = match clip_length_for_slot(track, session, scene_index) {
        Some(len) if len > 0 => len,
        _ => return (1, 1),
    };
    let total = clip_repeat_count(track, session, scene_index, tracks);
    let elapsed = slot_runtimes
        .get(&(track.name.clone(), scene_index))
        .map(|r| r.elapsed_samples)
        .unwrap_or(0);
    let current = (elapsed / clip_length) % total + 1;
    (current, total)
}

fn clip_play_fill(
    track: &Track,
    session: &SessionMatrix,
    scene_index: usize,
    slot_runtimes: &SlotRuntimes,
) -> f32 {
    let clip_length = match clip_length_for_slot(track, session, scene_index) {
        Some(len) if len > 0 => len,
        _ => return 0.0,
    };
    let position = slot_runtimes
        .get(&(track.name.clone(), scene_index))
        .map(|r| r.play_position_samples)
        .unwrap_or(0);
    (position as f32 / clip_length as f32).clamp(0.0, 1.0)
}

fn active_scene_for_track(
    track: &Track,
    selected_scene: Option<usize>,
    slot_runtimes: &SlotRuntimes,
) -> usize {
    if let Some(scene_index) = selected_scene {
        return scene_index;
    }
    slot_runtimes
        .iter()
        .filter(|((t, _), r)| t == &track.name && r.state == SlotPlayState::Playing)
        .map(|((_, scene_index), _)| *scene_index)
        .min()
        .unwrap_or(0)
}

fn track_strip(
    track: &Track,
    args: &SessionViewInput,
    width: f32,
    children_by_parent: &HashMap<String, Vec<&Track>>,
) -> iced::Element<'static, Message> {
    let track_color = track.color;
    let selected = if track.is_master {
        args.selected.contains("hw:out")
    } else {
        args.selected.contains(&track.name)
    };
    let header = strip_header(track);
    let mut body = column![header]
        .spacing(4)
        .padding(iced::Padding::new(4.0).bottom(0.0))
        .height(Length::Fill);
    if !track.is_folder {
        let mut slots = Column::new().spacing(2).height(Length::Fill);
        for scene_index in 0..args.session.scenes.len() {
            let slot = slot_state(track, args, scene_index);
            let label = if track.is_master {
                args.session
                    .scenes
                    .get(scene_index)
                    .map(|scene| scene.name.clone())
                    .unwrap_or_else(|| "scene".to_string())
            } else {
                slot.clip_name.clone().unwrap_or_default()
            };
            slots = slots.push(slot_button(
                track.name.clone(),
                scene_index,
                track_color,
                slot,
                track.is_master,
                label,
            ));
        }
        if track.is_master && args.session_scene_context_menu.is_some() {
            slots = slots.push(
                mouse_area(Space::new().width(Length::Fill).height(Length::Fill))
                    .on_press(Message::SessionSceneContextMenuHide),
            );
        }
        body = body.push(slots);

        let scene_index = active_scene_for_track(track, args.selected_scene, &args.slot_runtimes);
        let (current_loop, total_loops) = clip_loop_count(
            track,
            &args.session,
            scene_index,
            &args.tracks,
            &args.slot_runtimes,
        );
        let fill = clip_play_fill(track, &args.session, scene_index, &args.slot_runtimes);
        let bottom_bar: iced::Element<'static, Message> = container(
            row![
                Canvas::new(PieCircle { fill })
                    .width(Length::Fixed(14.0))
                    .height(Length::Fixed(14.0)),
                text(format!("{} / {}", current_loop, total_loops))
                    .size(12)
                    .color(Color::WHITE),
            ]
            .spacing(4)
            .align_y(Alignment::Center)
            .width(Length::Fill)
            .height(Length::Fixed(30.0)),
        )
        .width(Length::Fill)
        .height(Length::Fixed(30.0))
        .style(|_| style::mixer::slot())
        .padding([0, 4])
        .into();
        body = body.push(bottom_bar);
    }

    let left_content = container(body)
        .width(Length::Fixed(width))
        .height(Length::Fill);

    let content: iced::Element<'static, Message> = if track.is_folder && track.folder_open {
        if let Some(children) = children_by_parent.get(&track.name) {
            let mut child_strips = Row::new()
                .spacing(2)
                .align_y(Alignment::Start)
                .height(Length::Fill);
            for child in children {
                let child_width = crate::workspace::Workspace::mixer_strip_width(child.audio.outs);
                child_strips =
                    child_strips.push(track_strip(child, args, child_width, children_by_parent));
            }
            let right = container(child_strips)
                .height(Length::Fill)
                .width(Length::Shrink);
            row![left_content, right]
                .spacing(2)
                .align_y(Alignment::Start)
                .height(Length::Fill)
                .into()
        } else {
            left_content.into()
        }
    } else {
        left_content.into()
    };

    let select_target = if track.is_master {
        "hw:out".to_string()
    } else {
        track.name.clone()
    };
    let strip: iced::Element<'static, Message> = container(content)
        .height(Length::Fill)
        .style(move |_theme| style::mixer::strip(selected, track_color))
        .into();
    let mut strip_area = mouse_area(strip).on_press(Message::SelectTrackFromMixer(select_target));
    if !track.is_master {
        let track_name = track.name.clone();
        strip_area = strip_area.on_double_click(Message::SessionViewConnectionsOpen(track_name));
    }
    let strip = strip_area.into();

    if let (true, Some(menu_state)) = (track.is_master, &args.session_scene_context_menu) {
        let menu = scene_context_menu_overlay(menu_state);
        let slot_top = 26.0 + menu_state.scene_index as f32 * 32.0;
        let menu_x = menu_state
            .anchor
            .x
            .min((width - SCENE_CONTEXT_MENU_WIDTH).max(0.0))
            .max(0.0);
        return Stack::new()
            .push(strip)
            .push(pin(menu).position(Point::new(
                menu_x,
                (slot_top + menu_state.anchor.y).max(0.0),
            )))
            .into();
    }
    strip
}

fn strip_header(track: &Track) -> iced::Element<'static, Message> {
    let folder_toggle: iced::Element<'static, Message> = if track.is_folder {
        let icon = if track.folder_open { "▶" } else { "▼" };
        button(
            container(text(icon).size(10))
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill),
        )
        .width(Length::Fixed(18.0))
        .height(Length::Fixed(18.0))
        .padding(0)
        .style(|theme: &Theme, _status| button::Style {
            background: None,
            text_color: theme.palette().text,
            ..button::Style::default()
        })
        .on_press(Message::TrackToggleFolder {
            track_name: track.name.clone(),
        })
        .into()
    } else {
        Space::new().width(Length::Fixed(0.0)).into()
    };

    let add_scene_button: iced::Element<'static, Message> = if track.is_master {
        button(
            container(text("+").size(10))
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill),
        )
        .width(Length::Fixed(18.0))
        .height(Length::Fixed(18.0))
        .padding(0)
        .style(|theme: &Theme, _status| button::Style {
            background: None,
            text_color: theme.palette().text,
            ..button::Style::default()
        })
        .on_press(Message::SessionSceneAdd)
        .into()
    } else {
        Space::new().width(Length::Fixed(0.0)).into()
    };

    column![
        row![
            folder_toggle,
            text(track.name.clone()).size(10),
            add_scene_button
        ]
        .spacing(2)
        .align_y(Alignment::Center),
    ]
    .spacing(2)
    .align_x(Alignment::Center)
    .into()
}

fn slot_state(track: &Track, args: &SessionViewInput, scene_index: usize) -> SlotState {
    let slots = args.session.slots.get(&track.name);
    let slot = slots.and_then(|s| s.get(scene_index));
    SlotState {
        play_stop_icon: slot.and_then(|s| s.play_stop_icon),
        clip_name: slot
            .and_then(|s| s.clip_name.as_deref())
            .map(display_clip_name),
    }
}

struct PieCircle {
    fill: f32,
}

impl<Message> Program<Message> for PieCircle {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        _renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(_renderer, bounds.size());
        let center = Point::new(bounds.width / 2.0, bounds.height / 2.0);
        let radius = (bounds.width.min(bounds.height) / 2.0).max(1.0);

        let outline = Path::circle(center, radius);
        frame.stroke(
            &outline,
            canvas::Stroke::default()
                .with_color(Color::WHITE)
                .with_width(1.0),
        );

        let fill = self.fill.clamp(0.0, 1.0);
        if fill > 0.0 {
            let start_angle = -std::f32::consts::FRAC_PI_2;
            let end_angle = start_angle + fill * 2.0 * std::f32::consts::PI;
            let start = Point::new(
                center.x + radius * f32::cos(start_angle),
                center.y + radius * f32::sin(start_angle),
            );
            let wedge = Path::new(|builder| {
                builder.move_to(center);
                builder.line_to(start);
                builder.arc(canvas::path::Arc {
                    center,
                    radius,
                    start_angle: Radians(start_angle),
                    end_angle: Radians(end_angle),
                });
                builder.line_to(center);
                builder.close();
            });
            frame.fill(&wedge, Color::WHITE);
        }

        vec![frame.into_geometry()]
    }
}

impl Default for SessionView {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{SessionMatrix, Track};

    #[test]
    fn session_view_renders_with_empty_session() {
        let tracks = vec![Track::new("Track 1".to_string(), 0.0, 2, 2, 0, 0)];
        let session = SessionMatrix::default();
        let slot_runtimes = SlotRuntimes::new();
        let selected_slots = HashSet::new();
        let midi_learn = SessionMidiLearnBindings::default();

        let input = SessionViewInput {
            tracks,
            session,
            slot_runtimes,
            selected_slots,
            selected: HashSet::new(),
            selected_scene: None,
            midi_learn,
            master_track: None,
            session_scene_context_menu: None,
        };
        let _element = SessionView::view(input);
    }

    #[test]
    fn session_view_renders_with_referenced_clip() {
        let mut track = Track::new("Track 1".to_string(), 0.0, 2, 2, 0, 0);
        let clip_id = crate::state::generate_clip_id();
        track.audio.clips.push(crate::state::AudioClip {
            id: clip_id.clone(),
            name: "sine.wav".to_string(),
            ..Default::default()
        });
        let mut session = SessionMatrix::default();
        session.ensure_track_slots("Track 1");
        if let Some(slot) = session.slot_mut("Track 1", 0) {
            slot.clip = Some(crate::state::SlotClipRef {
                clip_id,
                launch_mode: crate::state::LaunchMode::Toggle,
                launch_quantization: crate::state::LaunchQuantization::Bar,
                loop_enabled: true,
                loop_start_samples: 0,
                loop_end_samples: 0,
            });
        }
        let tracks = vec![track];
        let slot_runtimes = SlotRuntimes::new();
        let mut selected_slots = HashSet::new();
        selected_slots.insert(("Track 1".to_string(), 0));
        let midi_learn = SessionMidiLearnBindings::default();

        let input = SessionViewInput {
            tracks,
            session,
            slot_runtimes,
            selected_slots,
            selected: HashSet::new(),
            selected_scene: None,
            midi_learn,
            master_track: None,
            session_scene_context_menu: None,
        };
        let _element = SessionView::view(input);
    }

    #[test]
    fn session_view_renders_folder_with_open_children() {
        let mut folder = Track::new("Folder".to_string(), 0.0, 2, 2, 0, 0);
        folder.is_folder = true;
        folder.folder_open = true;
        let mut child = Track::new("Child".to_string(), 0.0, 2, 2, 0, 0);
        child.parent_track = Some("Folder".to_string());
        let tracks = vec![folder, child];
        let session = SessionMatrix::default();
        let slot_runtimes = SlotRuntimes::new();
        let selected_slots = HashSet::new();
        let midi_learn = SessionMidiLearnBindings::default();

        let input = SessionViewInput {
            tracks,
            session,
            slot_runtimes,
            selected_slots,
            selected: HashSet::new(),
            selected_scene: None,
            midi_learn,
            master_track: None,
            session_scene_context_menu: None,
        };
        let _element = SessionView::view(input);
    }

    #[test]
    fn session_view_renders_closed_folder_without_children() {
        let mut folder = Track::new("Folder".to_string(), 0.0, 2, 2, 0, 0);
        folder.is_folder = true;
        folder.folder_open = false;
        let mut child = Track::new("Child".to_string(), 0.0, 2, 2, 0, 0);
        child.parent_track = Some("Folder".to_string());
        let tracks = vec![folder, child];
        let session = SessionMatrix::default();
        let slot_runtimes = SlotRuntimes::new();
        let selected_slots = HashSet::new();
        let midi_learn = SessionMidiLearnBindings::default();

        let input = SessionViewInput {
            tracks,
            session,
            slot_runtimes,
            selected_slots,
            selected: HashSet::new(),
            selected_scene: None,
            midi_learn,
            master_track: None,
            session_scene_context_menu: None,
        };
        let _element = SessionView::view(input);
    }

    #[test]
    fn session_view_renders_with_master_track() {
        let tracks = vec![Track::new("Track 1".to_string(), 0.0, 2, 2, 0, 0)];
        let mut master = Track::new("Master".to_string(), 0.0, 0, 2, 0, 0);
        master.is_master = true;
        let session = SessionMatrix::default();
        let slot_runtimes = SlotRuntimes::new();
        let selected_slots = HashSet::new();
        let midi_learn = SessionMidiLearnBindings::default();

        let input = SessionViewInput {
            tracks,
            session,
            slot_runtimes,
            selected_slots,
            selected: HashSet::new(),
            selected_scene: None,
            midi_learn,
            master_track: Some(master),
            session_scene_context_menu: None,
        };
        let _element = SessionView::view(input);
    }
}
