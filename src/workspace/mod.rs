mod add_track;
mod editor;
mod mixer;
mod open;
mod save;
mod tracks;

use crate::message::{Message, Show};
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
    panes: pane_grid::State<Pane>,
    editor: editor::Editor,
    mixer: mixer::Mixer,
    tracks: tracks::Tracks,
    modal: Option<Show>,
    add_track: add_track::AddTrackView,
    save: save::SaveView,
    open: open::OpenView,
}

impl Workspace {
    pub fn json(&self) -> Value {
        json!({
            "tracks": self.tracks.json(),
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
            Message::Response(Ok(ref a)) => match a {
                Action::AddTrack { .. } => self.modal = None,
                _ => {}
            },
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

impl Default for Workspace {
    fn default() -> Self {
        let (mut panes, pane) = pane_grid::State::new(Pane::Tracks);
        panes.split(Axis::Horizontal, pane, Pane::Mixer);
        panes.split(Axis::Vertical, pane, Pane::Editor);
        {
            let p = panes.clone();
            let mut i = 0;
            for s in p.layout().splits() {
                let split = s.clone();
                if i == 0 {
                    panes.resize(split, 0.75);
                } else if i == 1 {
                    panes.resize(split, 0.1);
                }
                i += 1;
            }
        }
        Self {
            panes,
            editor: editor::Editor::default(),
            mixer: mixer::Mixer::default(),
            tracks: tracks::Tracks::default(),
            modal: None,
            add_track: add_track::AddTrackView::default(),
            save: save::SaveView::default(),
            open: open::OpenView::default(),
        }
    }
}
