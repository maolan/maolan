use crate::state::{ConnectionViewSelection, HW_IN_ID, HW_OUT_ID, StateData};
use maolan_engine::message::Action;
#[cfg(all(unix, not(target_os = "macos")))]
use maolan_engine::message::Lv2GraphConnection;
use std::collections::HashSet;

pub fn is_bezier_hit(
    start: iced::Point,
    end: iced::Point,
    cursor: iced::Point,
    samples: usize,
    threshold: f32,
) -> bool {
    let dist_x = (end.x - start.x).abs() / 2.0;
    let mut min_dist = f32::MAX;

    for i in 0..=samples {
        let t = i as f32 / samples as f32;
        let mt = 1.0 - t;
        let p1 = iced::Point::new(start.x + dist_x, start.y);
        let p2 = iced::Point::new(end.x - dist_x, end.y);
        let x = mt.powi(3) * start.x
            + 3.0 * mt.powi(2) * t * p1.x
            + 3.0 * mt * t.powi(2) * p2.x
            + t.powi(3) * end.x;
        let y = mt.powi(3) * start.y
            + 3.0 * mt.powi(2) * t * p1.y
            + 3.0 * mt * t.powi(2) * p2.y
            + t.powi(3) * end.y;
        min_dist = min_dist.min(cursor.distance(iced::Point::new(x, y)));
    }

    min_dist < threshold
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

#[cfg(all(unix, not(target_os = "macos")))]
pub fn plugin_disconnect_actions(
    track_name: &str,
    connections: &[Lv2GraphConnection],
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
