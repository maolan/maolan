use crate::{message::Message, state::State};
use maolan_widgets::iced::{Rectangle, event::Event, mouse, widget::canvas::Action, window};
use std::time::Instant;

#[derive(Debug, Default)]
pub(super) struct Animation {
    transition: Option<Transition>,
    pan_pointer: Option<maolan_widgets::iced::Point>,
}

#[derive(Debug)]
struct Transition {
    started: Instant,
    from: f32,
    target: f32,
    displayed: f32,
}

impl Animation {
    pub(super) fn begin_pan(&mut self, pointer: maolan_widgets::iced::Point) -> Action<Message> {
        self.transition = None;
        self.pan_pointer = Some(pointer);
        Action::request_redraw().and_capture()
    }

    pub(super) fn update(
        &mut self,
        shared: &State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<Message>> {
        if let Some(previous) = self.pan_pointer {
            match event {
                Event::Mouse(mouse::Event::CursorMoved { position }) => {
                    if let Ok(mut data) = shared.try_write() {
                        data.connections_pan += *position - previous;
                        self.pan_pointer = Some(*position);
                    }
                    return Some(Action::request_redraw().and_capture());
                }
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                | Event::Window(window::Event::Unfocused) => {
                    self.pan_pointer = None;
                    return Some(Action::request_redraw().and_capture());
                }
                Event::Mouse(_) => return Some(Action::capture()),
                _ => {}
            }
        }
        let scroll = match event {
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let position = cursor.position_in(bounds)?;
                let amount = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => *y,
                    mouse::ScrollDelta::Pixels { y, .. } => *y / 40.0,
                };
                Some((position, amount))
            }
            Event::Window(window::Event::RedrawRequested(_)) => None,
            // Finish smoothing before a drag starts so its coordinates stay stable.
            Event::Mouse(mouse::Event::ButtonPressed(_)) => {
                self.transition = None;
                return None;
            }
            _ => return None,
        };
        if scroll.is_none() && self.transition.is_none() {
            return None;
        }
        let mut data = shared.try_write().ok()?;
        // A toolbar command takes precedence over an in-flight wheel animation.
        if self
            .transition
            .as_ref()
            .is_some_and(|t| t.displayed.to_bits() != data.connections_zoom.to_bits())
        {
            self.transition = None;
        }
        if let Some((position, amount)) = scroll {
            if !amount.is_finite() || amount == 0.0 {
                return None;
            }
            let target = self
                .transition
                .as_ref()
                .map_or(data.connections_zoom, |t| t.target);
            // Keep the existing pivot while zoomed, avoiding jumps as the mouse moves.
            if data.connections_zoom == 1.0 || data.connections_zoom_anchor.is_none() {
                data.connections_zoom_anchor = Some(position);
            }
            self.transition = Some(Transition {
                started: Instant::now(),
                from: data.connections_zoom,
                target: scaled(target, (amount * 0.12).exp()),
                displayed: data.connections_zoom,
            });
            return Some(Action::request_redraw().and_capture());
        }
        let transition = self.transition.as_mut()?;
        let progress = (transition.started.elapsed().as_secs_f32() / 0.14).min(1.0);
        let eased = 1.0 - (1.0 - progress).powi(3);
        data.connections_zoom = if progress == 1.0 {
            transition.target
        } else {
            transition.from + (transition.target - transition.from) * eased
        };
        transition.displayed = data.connections_zoom;
        if progress == 1.0 {
            self.transition = None;
        }
        Some(Action::request_redraw())
    }
}

// No user-facing zoom limits; reject only floating-point overflow/underflow.
pub(crate) fn scaled(current: f32, factor: f32) -> f32 {
    let next = current * factor;
    if next.is_normal() && next > 0.0 {
        next
    } else {
        current
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Viewport {
    pub scale: f32,
    pub pan: maolan_widgets::iced::Vector,
    pub center: maolan_widgets::iced::Point,
}

impl Viewport {
    pub fn new(data: &crate::state::StateData, bounds: Rectangle) -> Self {
        Self {
            scale: data.connections_zoom,
            pan: data.connections_pan,
            center: data
                .connections_zoom_anchor
                .unwrap_or(maolan_widgets::iced::Point::new(
                    bounds.width / 2.0,
                    bounds.height / 2.0,
                )),
        }
    }

    pub fn world(self, point: maolan_widgets::iced::Point) -> maolan_widgets::iced::Point {
        maolan_widgets::iced::Point::new(
            self.center.x + (point.x - self.pan.x - self.center.x) / self.scale,
            self.center.y + (point.y - self.pan.y - self.center.y) / self.scale,
        )
    }

    pub fn world_rect(self, rect: Rectangle) -> Rectangle {
        Rectangle::new(
            self.world(rect.position()),
            maolan_widgets::iced::Size::new(rect.width / self.scale, rect.height / self.scale),
        )
    }
}

pub(super) fn draw_grid(
    frame: &mut maolan_widgets::iced::widget::canvas::Frame,
    bounds: Rectangle,
    viewport: Viewport,
) {
    use maolan_widgets::iced::{
        Color, Point,
        widget::canvas::{Path, Stroke},
    };
    // Keep a bounded number of grid lines even when zooming far out.
    let mut spacing = 24.0_f64 * f64::from(viewport.scale);
    if spacing < 8.0 {
        spacing *= 2.0_f64.powf((8.0 / spacing).log2().ceil());
    }
    let offset_x = f64::from(viewport.pan.x)
        + f64::from(viewport.center.x) * (1.0 - f64::from(viewport.scale));
    let offset_y = f64::from(viewport.pan.y)
        + f64::from(viewport.center.y) * (1.0 - f64::from(viewport.scale));
    let stroke = Stroke::default().with_color(Color::from_rgba(0.78, 0.86, 1.0, 0.07));
    let mut x = offset_x.rem_euclid(spacing);
    while x <= f64::from(bounds.width) {
        frame.stroke(
            &Path::line(
                Point::new(x as f32, 0.0),
                Point::new(x as f32, bounds.height),
            ),
            stroke,
        );
        x += spacing;
    }
    let mut y = offset_y.rem_euclid(spacing);
    while y <= f64::from(bounds.height) {
        frame.stroke(
            &Path::line(
                Point::new(0.0, y as f32),
                Point::new(bounds.width, y as f32),
            ),
            stroke,
        );
        y += spacing;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use maolan_widgets::iced::{Point, Size};

    fn exercise_pan<P: maolan_widgets::iced::widget::canvas::Program<Message>>(
        graph: P,
        shared: State,
    ) {
        let bounds = Rectangle::new(Point::new(20.0, 30.0), Size::new(800.0, 600.0));
        let mut local = P::State::default();
        let start = Point::new(500.0, 450.0);
        let end = Point::new(1200.0, 900.0);
        let before = shared.read().unwrap().tracks[0].position;
        graph
            .update(
                &mut local,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                bounds,
                mouse::Cursor::Available(start),
            )
            .unwrap();
        graph
            .update(
                &mut local,
                &Event::Mouse(mouse::Event::CursorMoved { position: end }),
                bounds,
                mouse::Cursor::Available(end),
            )
            .unwrap();
        assert_eq!(shared.read().unwrap().connections_pan, end - start);
        // Releasing outside the canvas must stop panning.
        graph
            .update(
                &mut local,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                mouse::Cursor::Available(end),
            )
            .unwrap();
        let _ = graph.update(
            &mut local,
            &Event::Mouse(mouse::Event::CursorMoved { position: start }),
            bounds,
            mouse::Cursor::Available(start),
        );
        let data = shared.read().unwrap();
        assert_eq!(data.connections_pan, end - start);
        assert_eq!(data.tracks[0].position, before);
    }

    #[test]
    fn shift_drag_pans_both_graphs_without_moving_nodes() {
        for zoom in [0.25, 1.0, 4.0] {
            for plugins in [false, true] {
                let mut data = crate::state::StateData {
                    shift: true,
                    connections_zoom: zoom,
                    plugin_graph_track: Some("Track".into()),
                    ..Default::default()
                };
                data.tracks
                    .push(crate::state::Track::new("Track".into(), 0.0, 2, 2, 0, 0));
                let shared = std::sync::Arc::new(std::sync::RwLock::new(data));
                if plugins {
                    exercise_pan(
                        crate::connections::plugins::Graph::new(shared.clone()),
                        shared,
                    );
                } else {
                    exercise_pan(
                        crate::connections::tracks::Graph::new_with_focus(
                            shared.clone(),
                            None,
                            None,
                        ),
                        shared,
                    );
                }
            }
        }
    }

    #[test]
    fn zoom_can_go_beyond_previous_limits() {
        assert!(scaled(2.5, 1.2) > 2.5);
        assert!(scaled(0.35, 1.0 / 1.2) < 0.35);
        assert_eq!(scaled(1.0, f32::INFINITY), 1.0);
        assert_eq!(scaled(1.0, 0.0), 1.0);
    }

    #[test]
    fn fixed_panel_cable_endpoints_remain_in_screen_space() {
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(800.0, 600.0));
        let panel = Rectangle::new(Point::new(20.0, 80.0), Size::new(70.0, 300.0));
        for zoom in [0.05, 0.25, 1.0, 4.0, 20.0] {
            let data = crate::state::StateData {
                connections_zoom: zoom,
                connections_zoom_anchor: Some(Point::new(234.0, 172.0)),
                connections_pan: maolan_widgets::iced::Vector::new(123.0, -76.0),
                ..Default::default()
            };
            let viewport = Viewport::new(&data, bounds);
            let world = viewport.world_rect(panel);
            let port = Point::new(world.x + world.width, world.y + world.height / 3.0);
            let screen = Point::new(
                viewport.pan.x + viewport.center.x + (port.x - viewport.center.x) * zoom,
                viewport.pan.y + viewport.center.y + (port.y - viewport.center.y) * zoom,
            );
            assert!(
                screen.distance(Point::new(
                    panel.x + panel.width,
                    panel.y + panel.height / 3.0
                )) < 0.001
            );
        }
    }
}
