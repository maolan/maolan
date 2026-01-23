mod editor;
mod mixer;
mod tracks;
mod meta;

use crate::message::Message;
use iced::{
    Element,
    widget::{container, pane_grid, pane_grid::Axis},
};

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
}

impl Workspace {
    fn update_children(&mut self, message: Message) {
        self.editor.update(message.clone());
        self.mixer.update(message.clone());
        self.tracks.update(message.clone());
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::PaneResized(pane_grid::ResizeEvent { split, ratio }) => {
                self.panes.resize(split, ratio);
            }
            _ => self.update_children(message),
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        pane_grid(&self.panes, |_pane, state, _is_maximized| {
            pane_grid::Content::new(match state {
                Pane::Tracks => container(self.tracks.view()),
                Pane::Mixer => container(self.mixer.view()),
                Pane::Editor => container(self.editor.view()),
            })
        })
        .on_resize(10, Message::PaneResized)
        .into()
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
        }
    }
}
