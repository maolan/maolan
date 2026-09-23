use super::*;

impl Maolan {
    pub(super) fn handle_automation_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::TrackAutomationToggleLane {
                ref track_name,
                target,
            } => {
                let mut state = self.state.write().expect("state lock poisoned");
                if let Some(track) = state
                    .tracks
                    .iter_mut()
                    .find(|track| track.name == track_name.as_str())
                {
                    let previous_lane_height = track
                        .lane_layout()
                        .representative_height()
                        .max(TRACK_SUBTRACK_MIN_HEIGHT);
                    let previously_visible = track.automation_lane_count();
                    if let Some(lane) = track
                        .automation_lanes
                        .iter_mut()
                        .find(|lane| lane.target == target)
                    {
                        lane.visible = !lane.visible;
                    } else {
                        track
                            .automation_lanes
                            .push(crate::state::TrackAutomationLane {
                                target,
                                visible: true,
                                points: vec![],
                            });
                    }
                    let lanes_delta =
                        track.automation_lane_count() as isize - previously_visible as isize;
                    track.adjust_height_for_automation_lanes(previous_lane_height, lanes_delta);
                }
                drop(state);
                return self.send_track_automation_lanes(track_name);
            }
            Message::TrackAutomationCycleMode { ref track_name } => {
                let mut state = self.state.write().expect("state lock poisoned");
                if let Some(track) = state
                    .tracks
                    .iter_mut()
                    .find(|track| track.name == track_name.as_str())
                {
                    let next_mode = match track.automation_mode {
                        TrackAutomationMode::Read => TrackAutomationMode::Touch,
                        TrackAutomationMode::Touch => TrackAutomationMode::Latch,
                        TrackAutomationMode::Latch => TrackAutomationMode::Write,
                        TrackAutomationMode::Write => TrackAutomationMode::Read,
                    };
                    track.automation_mode = next_mode;
                    state.message = format!(
                        "Track '{}' automation mode: {}",
                        track.name, track.automation_mode
                    );
                }
                drop(state);
                let key = track_name.clone();
                let mode = self
                    .state
                    .read()
                    .expect("state lock poisoned")
                    .tracks
                    .iter()
                    .find(|track| track.name == key)
                    .map(|track| track.automation_mode);
                let mode_task = self.send_track_automation_lanes(track_name);
                match mode {
                    Some(TrackAutomationMode::Read) => {
                        self.automation.touch_active_keys.remove(&key);
                        self.automation.touch_automation_overrides.remove(&key);
                        self.automation.latch_automation_overrides.remove(&key);
                    }
                    Some(TrackAutomationMode::Touch) => {
                        self.automation.latch_automation_overrides.remove(&key);
                    }
                    Some(TrackAutomationMode::Write) => {
                        self.automation.touch_active_keys.remove(&key);
                        self.automation.touch_automation_overrides.remove(&key);
                    }
                    _ => {}
                }
                return mode_task;
            }
            Message::TrackAutomationAddPluginLanes {
                ref track_name,
                ref plugin_id,
                ref format,
            } => {
                match format.as_str() {
                    "CLAP" => {
                        self.pending
                            .pending_add_clap_automation_paths
                            .insert((track_name.clone(), plugin_id.clone()));
                    }
                    "VST3" => {
                        self.pending
                            .pending_add_vst3_automation_paths
                            .insert((track_name.clone(), plugin_id.clone()));
                    }
                    #[cfg(unix)]
                    "LV2" => {
                        self.pending
                            .pending_add_lv2_automation_uris
                            .insert((track_name.clone(), plugin_id.clone()));
                    }
                    _ => {}
                }
                return self.send(Action::TrackGetPluginGraph {
                    track_name: track_name.clone(),
                    include_state: false,
                });
            }
            Message::TrackAutomationLaneInsertPoints {
                ref track_name,
                target,
                ref points,
            } => {
                if points.is_empty() {
                    return Task::none();
                }
                let mut state = self.state.write().expect("state lock poisoned");
                let Some(track) = state
                    .tracks
                    .iter_mut()
                    .find(|track| track.name == track_name.as_str())
                else {
                    return Task::none();
                };
                let previous_lane_height = track
                    .lane_layout()
                    .representative_height()
                    .max(TRACK_SUBTRACK_MIN_HEIGHT);
                let previously_visible = track.automation_lane_count();

                let lane_exists = track
                    .automation_lanes
                    .iter()
                    .any(|lane| lane.target == target);
                if !lane_exists {
                    track
                        .automation_lanes
                        .push(crate::state::TrackAutomationLane {
                            target: target.clone(),
                            visible: true,
                            points: vec![],
                        });
                }
                let lane = track
                    .automation_lanes
                    .iter_mut()
                    .find(|lane| lane.target == target)
                    .expect("lane was just created");
                lane.visible = true;

                let min_sample = points.iter().map(|p| p.sample).min().unwrap_or(0);
                let max_sample = points.iter().map(|p| p.sample).max().unwrap_or(min_sample);

                // Replace any existing points in the drawn range so the new line
                // overrides the previous curve, matching MIDI controller lane
                // behavior. Points outside the range are kept, creating an
                // interpolation from the old line to the new line at the edges.
                lane.points
                    .retain(|point| point.sample < min_sample || point.sample > max_sample);
                for point in points {
                    lane.points.push(crate::state::TrackAutomationPoint {
                        sample: point.sample,
                        value: point.value,
                    });
                }
                lane.points.sort_unstable_by_key(|p| p.sample);

                let lanes_delta =
                    track.automation_lane_count() as isize - previously_visible as isize;
                track.adjust_height_for_automation_lanes(previous_lane_height, lanes_delta);
                drop(state);
                return self.send_track_automation_lanes(track_name);
            }
            Message::TrackAutomationLaneDeletePoint {
                ref track_name,
                target,
                sample,
            } => {
                let mut state = self.state.write().expect("state lock poisoned");
                if let Some(track) = state
                    .tracks
                    .iter_mut()
                    .find(|track| track.name == track_name.as_str())
                    && let Some(lane) = track
                        .automation_lanes
                        .iter_mut()
                        .find(|lane| lane.target == target)
                {
                    lane.points.retain(|point| point.sample != sample);
                }
                drop(state);
                return self.send_track_automation_lanes(track_name);
            }
            _ => {}
        }
        self.update_children(&message);
        Task::none()
    }
}
