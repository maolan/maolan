use crate::state::{ConnectionViewSelection, HW_IN_ID, HW_OUT_ID, StateData};
use maolan_engine::message::{Action, ConnectableConnection, PluginGraphConnection};
use std::collections::HashSet;

pub fn is_bezier_hit(
    start: iced::Point,
    end: iced::Point,
    cursor: iced::Point,
    samples: usize,
    threshold: f32,
) -> bool {
    let dist_x = (end.x - start.x).abs() / 2.0;
    let p1 = iced::Point::new(start.x + dist_x, start.y);
    let p2 = iced::Point::new(end.x - dist_x, end.y);

    is_cubic_bezier_hit(start, p1, p2, end, cursor, samples, threshold)
}

pub fn is_cubic_bezier_hit(
    start: iced::Point,
    control1: iced::Point,
    control2: iced::Point,
    end: iced::Point,
    cursor: iced::Point,
    samples: usize,
    threshold: f32,
) -> bool {
    let samples = samples.max(1);
    let mut min_dist = f32::MAX;
    let mut previous = start;

    for i in 1..=samples {
        let t = i as f32 / samples as f32;
        let mt = 1.0 - t;
        let x = mt.powi(3) * start.x
            + 3.0 * mt.powi(2) * t * control1.x
            + 3.0 * mt * t.powi(2) * control2.x
            + t.powi(3) * end.x;
        let y = mt.powi(3) * start.y
            + 3.0 * mt.powi(2) * t * control1.y
            + 3.0 * mt * t.powi(2) * control2.y
            + t.powi(3) * end.y;
        let current = iced::Point::new(x, y);
        min_dist = min_dist.min(distance_to_segment(cursor, previous, current));
        previous = current;
    }

    min_dist < threshold
}

fn distance_to_segment(point: iced::Point, start: iced::Point, end: iced::Point) -> f32 {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let len_sq = dx * dx + dy * dy;
    if len_sq <= f32::EPSILON {
        return point.distance(start);
    }

    let t = (((point.x - start.x) * dx + (point.y - start.y) * dy) / len_sq).clamp(0.0, 1.0);
    point.distance(iced::Point::new(start.x + t * dx, start.y + t * dy))
}

pub fn select_connection_indices(selected: &mut HashSet<usize>, idx: usize, ctrl: bool) {
    if ctrl {
        if selected.contains(&idx) {
            selected.remove(&idx);
        } else {
            selected.insert(idx);
        }
    } else {
        selected.clear();
        selected.insert(idx);
    }
}

pub fn apply_track_connection_selection(state: &mut StateData, idx: usize, ctrl: bool) {
    match &mut state.connection_view_selection {
        ConnectionViewSelection::Connections(set) => {
            select_connection_indices(set, idx, ctrl);
        }
        _ => {
            let mut set = HashSet::new();
            set.insert(idx);
            state.connection_view_selection = ConnectionViewSelection::Connections(set);
        }
    }
}

pub fn track_disconnect_actions(state: &StateData, selected: &HashSet<usize>) -> Vec<Action> {
    selected
        .iter()
        .filter_map(|idx| state.connections.get(*idx))
        .map(|conn| Action::Disconnect {
            from_track: if conn.from_track == HW_IN_ID {
                HW_IN_ID.to_string()
            } else if conn.from_track == HW_OUT_ID {
                HW_OUT_ID.to_string()
            } else {
                conn.from_track.clone()
            },
            from_port: conn.from_port,
            to_track: if conn.to_track == HW_OUT_ID {
                HW_OUT_ID.to_string()
            } else if conn.to_track == HW_IN_ID {
                HW_IN_ID.to_string()
            } else {
                conn.to_track.clone()
            },
            to_port: conn.to_port,
            kind: conn.kind,
        })
        .collect()
}

#[allow(dead_code)]
pub fn plugin_disconnect_actions(
    track_name: &str,
    connections: &[PluginGraphConnection],
    selected: &HashSet<usize>,
) -> Vec<Action> {
    selected
        .iter()
        .filter_map(|idx| connections.get(*idx))
        .map(|conn| match conn.kind {
            maolan_engine::kind::Kind::Audio => Action::TrackDisconnectPluginAudio {
                track_name: track_name.to_string(),
                from_node: conn.from_node.clone(),
                from_port: conn.from_port,
                to_node: conn.to_node.clone(),
                to_port: conn.to_port,
            },
            maolan_engine::kind::Kind::MIDI => Action::TrackDisconnectPluginMidi {
                track_name: track_name.to_string(),
                from_node: conn.from_node.clone(),
                from_port: conn.from_port,
                to_node: conn.to_node.clone(),
                to_port: conn.to_port,
            },
        })
        .collect()
}

pub fn connectable_disconnect_actions(
    track_name: &str,
    connections: &[ConnectableConnection],
    selected: &HashSet<usize>,
) -> Vec<Action> {
    selected
        .iter()
        .filter_map(|idx| connections.get(*idx))
        .map(|conn| match conn.kind {
            maolan_engine::kind::Kind::Audio => Action::TrackDisconnectAudio {
                track_name: track_name.to_string(),
                from: conn.from.clone(),
                from_port: conn.from_port,
                to: conn.to.clone(),
                to_port: conn.to_port,
            },
            maolan_engine::kind::Kind::MIDI => Action::TrackDisconnectMidi {
                track_name: track_name.to_string(),
                from: conn.from.clone(),
                from_port: conn.from_port,
                to: conn.to.clone(),
                to_port: conn.to_port,
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_bezier_hit_detects_nearby_points() {
        let start = iced::Point::new(0.0, 0.0);
        let end = iced::Point::new(100.0, 0.0);
        let cursor = iced::Point::new(50.0, 0.0);

        assert!(is_bezier_hit(start, end, cursor, 10, 10.0));
    }

    #[test]
    fn is_bezier_hit_rejects_far_points() {
        let start = iced::Point::new(0.0, 0.0);
        let end = iced::Point::new(100.0, 0.0);
        let cursor = iced::Point::new(50.0, 100.0);

        assert!(!is_bezier_hit(start, end, cursor, 10, 5.0));
    }

    #[test]
    fn is_cubic_bezier_hit_uses_actual_control_points() {
        let start = iced::Point::new(0.0, 0.0);
        let control1 = iced::Point::new(0.0, 100.0);
        let control2 = iced::Point::new(100.0, 100.0);
        let end = iced::Point::new(100.0, 0.0);
        let cursor = iced::Point::new(50.0, 75.0);

        assert!(is_cubic_bezier_hit(
            start, control1, control2, end, cursor, 12, 6.0
        ));
        assert!(!is_bezier_hit(start, end, cursor, 12, 6.0));
    }

    #[test]
    fn is_cubic_bezier_hit_checks_between_samples() {
        let start = iced::Point::new(0.0, 0.0);
        let end = iced::Point::new(100.0, 0.0);
        let cursor = iced::Point::new(25.0, 0.0);

        assert!(is_cubic_bezier_hit(start, start, end, end, cursor, 2, 2.0));
    }

    #[test]
    fn select_connection_indices_adds_index() {
        let mut selected = HashSet::new();
        select_connection_indices(&mut selected, 5, false);
        assert!(selected.contains(&5));
        assert_eq!(selected.len(), 1);
    }

    #[test]
    fn select_connection_indices_clears_others_without_ctrl() {
        let mut selected = HashSet::new();
        selected.insert(1);
        selected.insert(2);
        select_connection_indices(&mut selected, 5, false);
        assert!(selected.contains(&5));
        assert!(!selected.contains(&1));
        assert!(!selected.contains(&2));
    }

    #[test]
    fn select_connection_indices_toggles_with_ctrl() {
        let mut selected = HashSet::new();
        selected.insert(1);

        select_connection_indices(&mut selected, 5, true);
        assert!(selected.contains(&5));
        assert!(selected.contains(&1));

        select_connection_indices(&mut selected, 5, true);
        assert!(!selected.contains(&5));
        assert!(selected.contains(&1));
    }

    #[test]
    fn connectable_disconnect_actions_maps_audio_and_midi_edges() {
        let audio_conn = ConnectableConnection {
            from: maolan_engine::message::ConnectableRef::ChildTrack("Child".to_string()),
            from_port: 0,
            to: maolan_engine::message::ConnectableRef::ClapPlugin(7),
            to_port: 0,
            kind: maolan_engine::kind::Kind::Audio,
        };
        let midi_conn = ConnectableConnection {
            from: maolan_engine::message::ConnectableRef::ClapPlugin(7),
            from_port: 1,
            to: maolan_engine::message::ConnectableRef::ChildTrack("Child".to_string()),
            to_port: 0,
            kind: maolan_engine::kind::Kind::MIDI,
        };
        let folder_input_conn = ConnectableConnection {
            from: maolan_engine::message::ConnectableRef::TrackInput,
            from_port: 1,
            to: maolan_engine::message::ConnectableRef::ChildTrack("Child".to_string()),
            to_port: 1,
            kind: maolan_engine::kind::Kind::Audio,
        };
        let connections = vec![
            audio_conn.clone(),
            midi_conn.clone(),
            folder_input_conn.clone(),
        ];
        let selected: HashSet<usize> = [0, 1, 2].into_iter().collect();

        let actions = connectable_disconnect_actions("Folder", &connections, &selected);

        assert_eq!(actions.len(), 3);
        let mut saw_audio = false;
        let mut saw_folder_input = false;
        let mut saw_midi = false;
        for action in &actions {
            match action {
                Action::TrackDisconnectAudio {
                    track_name,
                    from,
                    from_port,
                    to,
                    to_port,
                } if track_name == "Folder"
                    && from == &audio_conn.from
                    && *from_port == 0
                    && to == &audio_conn.to
                    && *to_port == 0 =>
                {
                    saw_audio = true;
                }
                Action::TrackDisconnectAudio {
                    track_name,
                    from,
                    from_port,
                    to,
                    to_port,
                } if track_name == "Folder"
                    && from == &folder_input_conn.from
                    && *from_port == 1
                    && to == &folder_input_conn.to
                    && *to_port == 1 =>
                {
                    saw_folder_input = true;
                }
                Action::TrackDisconnectMidi {
                    track_name,
                    from,
                    from_port,
                    to,
                    to_port,
                } if track_name == "Folder"
                    && from == &midi_conn.from
                    && *from_port == 1
                    && to == &midi_conn.to
                    && *to_port == 0 =>
                {
                    saw_midi = true;
                }
                _ => {}
            }
        }
        assert!(saw_audio);
        assert!(saw_folder_input);
        assert!(saw_midi);
    }
}
