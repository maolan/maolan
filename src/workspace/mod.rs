use crate::message::Message;
use iced::{
    Element,
    widget::{pane_grid, pane_grid::Axis, text},
};

#[derive(Clone)]
enum Pane {
    Tracks,
    Mixer,
    Clips,
}

pub struct MaolanWorkspace {
    panes: pane_grid::State<Pane>,
}

impl MaolanWorkspace {
    pub fn update(&mut self, message: Message) {
        match message {
            Message::PaneResized(pane_grid::ResizeEvent { split, ratio }) => {
                self.panes.resize(split, ratio);
            }
            _ => {}
        }
    }
    pub fn view(&self) -> Element<'_, Message> {
        pane_grid(&self.panes, |_pane, state, _is_maximized| {
            pane_grid::Content::new(match state {
                Pane::Tracks => text("Tracks"),
                Pane::Mixer => text("Mixer"),
                Pane::Clips => text("Clips"),
            })
        })
        .on_resize(10, Message::PaneResized)
        .into()
    }
}

impl Default for MaolanWorkspace {
    fn default() -> Self {
        let (mut panes, pane) = pane_grid::State::new(Pane::Tracks);
        panes.split(Axis::Horizontal, pane, Pane::Mixer);
        panes.split(Axis::Vertical, pane, Pane::Clips);
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
        Self { panes }
    }
}
