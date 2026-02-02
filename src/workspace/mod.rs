mod add_track;
mod editor;
mod mixer;
mod open;
mod save;
mod tracks;

use crate::{
    message::{Message, Show},
    state::State,
};
use iced::{
    Element,
    widget::{container, pane_grid, pane_grid::Axis},
};
use maolan_engine::message::Action;
use serde_json::{Value, json};

#[derive(Clone)]
enum Pane {
    Tracks,
    Mixer,
    Editor,
}

pub struct Workspace {
    add_track: add_track::AddTrackView,
    editor: editor::Editor,
    mixer: mixer::Mixer,
    modal: Option<Show>,
    open: open::OpenView,
    panes: pane_grid::State<Pane>,
    save: save::SaveView,
    state: State,
    tracks: tracks::Tracks,
}

impl Workspace {
    pub fn new(state: State) -> Self {
        let (mut panes, pane) = pane_grid::State::new(Pane::Tracks);
        panes.split(Axis::Horizontal, pane, Pane::Mixer);
        panes.split(Axis::Vertical, pane, Pane::Editor);
        {
            let p = panes.clone();
            for (i, s) in p.layout().splits().enumerate() {
                let split = *s;
                if i == 0 {
                    panes.resize(split, 0.75);
                } else if i == 1 {
                    panes.resize(split, 0.1);
                }
            }
        }
        Self {
            add_track: add_track::AddTrackView::default(),
            editor: editor::Editor::new(state.clone()),
            mixer: mixer::Mixer::new(state.clone()),
            modal: None,
            open: open::OpenView::default(),
            panes,
            save: save::SaveView::default(),
            state: state.clone(),
            tracks: tracks::Tracks::new(state.clone()),
        }
    }

    pub fn json(&self) -> Value {
        json!({
            "tracks": &self.state.blocking_read().tracks,
        })
    }

    fn update_children(&mut self, message: Message) {
        self.editor.update(message.clone());
        self.mixer.update(message.clone());
        self.tracks.update(message.clone());
        self.add_track.update(message.clone());
        self.save.update(message.clone());
        self.open.update(message.clone());
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::PaneResized(pane_grid::ResizeEvent { split, ratio }) => {
                self.panes.resize(split, ratio)
            }
            Message::Show(modal) => self.modal = Some(modal),
            Message::Cancel => self.modal = None,
            Message::Save(_) => self.modal = None,
            Message::Response(Ok(Action::AddTrack { .. })) => self.modal = None,
            _ => {}
        }
        self.update_children(message);
    }

    pub fn view(&self) -> Element<'_, Message> {
        match &self.modal {
            Some(show) => match show {
                Show::AddTrack => self.add_track.view(),
                Show::Save => self.save.view(),
                Show::Open => self.open.view(),
            },
            None => pane_grid(&self.panes, |_pane, state, _is_maximized| {
                pane_grid::Content::new(match state {
                    Pane::Tracks => container(self.tracks.view()),
                    Pane::Mixer => container(self.mixer.view()),
                    Pane::Editor => container(self.editor.view()),
                })
            })
            .on_resize(10, Message::PaneResized)
            .into(),
        }
    }
}
