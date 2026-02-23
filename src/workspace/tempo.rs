use iced::{
    Color, Element, Length, Point, Rectangle, Renderer, Theme,
    event::Event,
    mouse,
    widget::canvas,
    widget::canvas::{Action as CanvasAction, Frame, Geometry, Path, Stroke, Text},
};
use crate::message::Message;
use maolan_engine::message::Action as EngineAction;

const TEMPO_HEIGHT: f32 = 28.0;

#[derive(Debug, Default)]
pub struct Tempo;

#[derive(Debug, Default)]
struct TempoState {
    dragging: bool,
    drag_start_x: f32,
    last_x: f32,
}

#[derive(Debug, Clone, Copy)]
struct TempoCanvas {
    bpm: f32,
    time_signature: (u8, u8),
    pixels_per_sample: f32,
    playhead_x: Option<f32>,
}

impl Tempo {
    pub fn new() -> Self {
        Self
    }

    pub fn height(&self) -> f32 {
        TEMPO_HEIGHT
    }

    pub fn view(
        &self,
        bpm: f32,
        time_signature: (u8, u8),
        pixels_per_sample: f32,
        playhead_x: Option<f32>,
    ) -> Element<'_, Message> {
        canvas(TempoCanvas {
            bpm,
            time_signature,
            pixels_per_sample,
            playhead_x,
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }
}

impl canvas::Program<Message> for TempoCanvas {
    type State = TempoState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<CanvasAction<Message>> {
        let cursor_position = cursor.position_in(bounds);
        let cursor_x = cursor
            .position()
            .map(|pos| (pos.x - bounds.x).clamp(0.0, bounds.width.max(0.0)));

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(pos) = cursor_position {
                    state.dragging = true;
                    let x = cursor_x.unwrap_or(pos.x.clamp(0.0, bounds.width.max(0.0)));
                    state.drag_start_x = x;
                    state.last_x = x;
                    return Some(CanvasAction::capture());
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if state.dragging
                    && let Some(x) = cursor_x
                {
                    state.last_x = x;
                    return Some(CanvasAction::request_redraw());
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.dragging {
                    state.dragging = false;
                    if self.pixels_per_sample > 1.0e-9 {
                        let sample = (state.last_x / self.pixels_per_sample).max(0.0) as usize;
                        return Some(CanvasAction::publish(Message::Request(
                            EngineAction::TransportPosition(sample),
                        )));
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
        let mut frame = Frame::new(renderer, bounds.size());

        frame.fill(
            &Path::rectangle(Point::new(0.0, 0.0), bounds.size()),
            Color::from_rgba(0.12, 0.12, 0.12, 1.0),
        );

        // Display BPM
        frame.fill_text(Text {
            content: format!("{:.0} BPM", self.bpm),
            position: Point::new(10.0, 2.0),
            color: Color::WHITE,
            size: 14.0.into(),
            ..Default::default()
        });

        // Display Time Signature
        frame.fill_text(Text {
            content: format!("{}/{}", self.time_signature.0, self.time_signature.1),
            position: Point::new(10.0, 15.0),
            color: Color::WHITE,
            size: 10.0.into(),
            ..Default::default()
        });

        if let Some(x) = self.playhead_x {
            let path = Path::line(
                Point::new(x.max(0.0), 0.0),
                Point::new(x.max(0.0), bounds.height),
            );
            frame.stroke(
                &path,
                Stroke::default().with_width(2.0).with_color(Color {
                    r: 0.95,
                    g: 0.18,
                    b: 0.14,
                    a: 0.95,
                }),
            );
        }

        vec![frame.into_geometry()]
    }
}
