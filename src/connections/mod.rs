use crate::{
    message::Message,
    state::{Resizing, State},
};
use iced::{
    Color, Element, Length, Point, Rectangle, Renderer, Theme,
    alignment::{Horizontal, Vertical},
    event::Event,
    mouse,
    widget::{
        canvas,
        canvas::{Action, Frame, Geometry, Path, Text},
        container,
    },
};

pub struct Graph {
    state: State,
}

impl Graph {
    pub fn new(state: State) -> Self {
        Self { state }
    }

    pub fn update(&mut self, _message: Message) {}

    pub fn view(&self) -> Element<'_, Message> {
        canvas(self).width(Length::Fill).height(Length::Fill).into()
    }
}

pub struct Connections {
    state: State,
    graph: Graph,
}

impl Connections {
    pub fn new(state: State) -> Self {
        Self {
            state: state.clone(),
            graph: Graph::new(state.clone()),
        }
    }
    pub fn update(&mut self, message: Message) {
        self.graph.update(message.clone());
    }

    pub fn view(&self) -> iced::Element<'_, Message> {
        container(self.graph.view())
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

impl<Message> canvas::Program<Message> for Graph {
    type State = ();

    fn update(
        &self,
        _state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<Message>> {
        let cursor_position = cursor.position_in(bounds)?;

        if let Ok(mut data) = self.state.try_write() {
            match event {
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                    for (i, track) in data.tracks.iter().enumerate().rev() {
                        let rect = Rectangle::new(track.position, iced::Size::new(140.0, 80.0));
                        if rect.contains(cursor_position) {
                            let offset_x = cursor_position.x - track.position.x;
                            let offset_y = cursor_position.y - track.position.y;
                            data.resizing = Some(Resizing::Track(i, offset_x, offset_y));
                            return Some(Action::capture());
                        }
                    }
                }
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                    if data.resizing.is_some() {
                        data.resizing = None;
                        return Some(Action::request_redraw());
                    }
                }
                Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                    if let Some(Resizing::Track(idx, offset_x, offset_y)) = data.resizing
                        && let Some(track) = data.tracks.get_mut(idx)
                    {
                        track.position.x = cursor_position.x - offset_x;
                        track.position.y = cursor_position.y - offset_y;
                        return Some(Action::request_redraw());
                    }
                }
                _ => {}
            }
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
        let mut frame = Frame::new(renderer, bounds.size());

        if let Ok(data) = self.state.try_read() {
            for track in data.tracks.iter() {
                let pos = track.position;
                let size = iced::Size::new(140.0, 80.0);

                let path = Path::rectangle(pos, size);
                frame.fill(&path, Color::from_rgb8(45, 45, 45));

                frame.stroke(
                    &path,
                    canvas::Stroke::default()
                        .with_color(Color::from_rgb8(80, 80, 80))
                        .with_width(1.0),
                );

                let total_ins = track.audio.ins.len() + track.midi.ins.len();
                if total_ins > 0 {
                    for j in 0..total_ins {
                        // py se računa relativno u odnosu na pos.y
                        let py = pos.y + (size.height / (total_ins + 1) as f32) * (j + 1) as f32;
                        let color = if j < track.audio.ins.len() {
                            Color::from_rgb(0.2, 0.5, 1.0)
                        } else {
                            Color::from_rgb(1.0, 0.6, 0.0)
                        };
                        frame.fill(&Path::circle(Point::new(pos.x, py), 4.0), color);
                    }
                }

                let total_outs = track.audio.outs.len() + track.midi.outs.len();
                if total_outs > 0 {
                    for j in 0..total_outs {
                        let py = pos.y + (size.height / (total_outs + 1) as f32) * (j + 1) as f32;
                        let color = if j < track.audio.outs.len() {
                            Color::from_rgb(0.2, 0.5, 1.0)
                        } else {
                            Color::from_rgb(1.0, 0.6, 0.0)
                        };
                        frame.fill(
                            &Path::circle(Point::new(pos.x + size.width, py), 4.0),
                            color,
                        );
                    }
                }

                frame.fill_text(Text {
                    content: track.name.clone(),
                    position: Point::new(pos.x + size.width / 2.0, pos.y + size.height / 2.0),
                    color: Color::WHITE,
                    size: 14.0.into(),
                    align_x: Horizontal::Center.into(),
                    align_y: Vertical::Center,
                    ..Default::default()
                });
            }
        }

        vec![frame.into_geometry()]
    }
}
