use super::*;

impl Maolan {
    pub(super) fn handle_timing_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::TempoAdjust(delta) => {
                let sample = self.transport_samples.max(0.0) as usize;
                let mut state = self.state.blocking_write();
                let selected_samples: Vec<usize> =
                    self.selected_tempo_points.iter().copied().collect();
                let current_bpm = if let Some(sel) = selected_samples.first().copied() {
                    state
                        .tempo_points
                        .iter()
                        .find(|p| p.sample == sel)
                        .map(|p| p.bpm)
                        .unwrap_or(state.tempo)
                } else {
                    state
                        .tempo_points
                        .iter()
                        .filter(|p| p.sample <= sample)
                        .max_by_key(|p| p.sample)
                        .map(|p| p.bpm)
                        .unwrap_or(state.tempo)
                };
                let tempo = (current_bpm + delta).clamp(20.0, 300.0);
                if !selected_samples.is_empty() {
                    for point in state.tempo_points.iter_mut() {
                        if selected_samples.contains(&point.sample) {
                            point.bpm = tempo;
                        }
                    }
                } else if let Some(point) =
                    state.tempo_points.iter_mut().find(|p| p.sample == sample)
                {
                    point.bpm = tempo;
                } else {
                    state.tempo_points.push(TempoPoint { sample, bpm: tempo });
                }
                state.tempo_points.sort_unstable_by_key(|p| p.sample);
                state.tempo = tempo;
                self.tempo_input = format!("{:.2}", tempo);
                drop(state);
                self.last_sent_tempo_bpm = Some(tempo as f64);
                return self.send(Action::SetTempo(tempo as f64));
            }
            Message::TempoPointAdd(sample) => {
                let mut state = self.state.blocking_write();
                let (bpm, numerator, denominator) = Self::timing_at_sample(&state, sample);
                if let Some(existing) = state.tempo_points.iter_mut().find(|p| p.sample == sample) {
                    existing.bpm = bpm;
                } else {
                    state.tempo_points.push(TempoPoint { sample, bpm });
                    state.tempo_points.sort_unstable_by_key(|p| p.sample);
                }
                if state
                    .time_signature_points
                    .iter()
                    .all(|p| p.sample != sample)
                {
                    state.time_signature_points.push(TimeSignaturePoint {
                        sample,
                        numerator,
                        denominator,
                    });
                    state
                        .time_signature_points
                        .sort_unstable_by_key(|p| p.sample);
                }
                self.selected_tempo_points.clear();
                self.selected_tempo_points.insert(sample);
                self.selected_time_signature_points.clear();
                self.timing_selection_lane = Some(super::super::super::TimingSelectionLane::Tempo);
                drop(state);
                self.sync_timing_inputs_from_selection();
                return self.update(Message::PlaybackTick);
            }
            Message::TempoPointSelect { sample, additive } => {
                if additive {
                    if !self.selected_tempo_points.insert(sample) {
                        self.selected_tempo_points.remove(&sample);
                    }
                } else {
                    self.selected_tempo_points.clear();
                    self.selected_tempo_points.insert(sample);
                }
                self.selected_time_signature_points.clear();
                self.timing_selection_lane = if self.selected_tempo_points.is_empty() {
                    None
                } else {
                    Some(super::super::super::TimingSelectionLane::Tempo)
                };
                self.sync_timing_inputs_from_selection();
            }
            Message::TempoPointsMove {
                from_samples,
                to_samples,
            } => {
                if from_samples.is_empty() || from_samples.len() != to_samples.len() {
                    return Task::none();
                }
                let mut state = self.state.blocking_write();
                let mut moved_values: Vec<f32> = Vec::new();
                for sample in from_samples.iter().copied() {
                    if sample == 0 {
                        continue;
                    }
                    if let Some(idx) = state.tempo_points.iter().position(|p| p.sample == sample) {
                        moved_values.push(state.tempo_points[idx].bpm);
                        state.tempo_points.remove(idx);
                    }
                }
                for (to, bpm) in to_samples.iter().copied().zip(moved_values.into_iter()) {
                    if let Some(existing) = state.tempo_points.iter_mut().find(|p| p.sample == to) {
                        existing.bpm = bpm;
                    } else {
                        state.tempo_points.push(TempoPoint { sample: to, bpm });
                    }
                }
                state.tempo_points.sort_unstable_by_key(|p| p.sample);
                drop(state);
                self.selected_tempo_points.clear();
                self.selected_tempo_points.extend(to_samples);
                self.selected_time_signature_points.clear();
                self.timing_selection_lane = Some(super::super::super::TimingSelectionLane::Tempo);
                self.sync_timing_inputs_from_selection();
                return self.update(Message::PlaybackTick);
            }
            Message::TempoSelectionDuplicate => {
                if self.selected_tempo_points.is_empty() {
                    return Task::none();
                }
                let beat_step = self.samples_per_beat().round() as usize;
                let mut state = self.state.blocking_write();
                let mut inserted = Vec::new();
                for sample in self.selected_tempo_points.iter().copied() {
                    let Some(point) = state
                        .tempo_points
                        .iter()
                        .find(|p| p.sample == sample)
                        .cloned()
                    else {
                        continue;
                    };
                    let new_sample = sample.saturating_add(beat_step).max(1);
                    if let Some(existing) = state
                        .tempo_points
                        .iter_mut()
                        .find(|p| p.sample == new_sample)
                    {
                        existing.bpm = point.bpm;
                    } else {
                        state.tempo_points.push(TempoPoint {
                            sample: new_sample,
                            bpm: point.bpm,
                        });
                    }
                    inserted.push(new_sample);
                }
                state.tempo_points.sort_unstable_by_key(|p| p.sample);
                drop(state);
                self.selected_tempo_points.clear();
                self.selected_tempo_points.extend(inserted);
                self.selected_time_signature_points.clear();
                self.timing_selection_lane = Some(super::super::super::TimingSelectionLane::Tempo);
                self.sync_timing_inputs_from_selection();
                return self.update(Message::PlaybackTick);
            }
            Message::TempoSelectionResetToPrevious => {
                if self.selected_tempo_points.is_empty() {
                    return Task::none();
                }
                let mut state = self.state.blocking_write();
                let samples: Vec<usize> = self.selected_tempo_points.iter().copied().collect();
                for sample in samples {
                    let previous_bpm = state
                        .tempo_points
                        .iter()
                        .filter(|p| p.sample < sample)
                        .max_by_key(|p| p.sample)
                        .map(|p| p.bpm)
                        .unwrap_or(state.tempo);
                    if let Some(point) = state.tempo_points.iter_mut().find(|p| p.sample == sample)
                    {
                        point.bpm = previous_bpm;
                    }
                }
                drop(state);
                self.sync_timing_inputs_from_selection();
                return self.update(Message::PlaybackTick);
            }
            Message::TempoSelectionDelete => {
                if self.selected_tempo_points.is_empty() {
                    return Task::none();
                }
                let selected: Vec<usize> = self.selected_tempo_points.iter().copied().collect();
                let mut state = self.state.blocking_write();
                state
                    .tempo_points
                    .retain(|p| p.sample == 0 || selected.binary_search(&p.sample).is_err());
                drop(state);
                self.selected_tempo_points.clear();
                self.timing_selection_lane = None;
                return self.update(Message::PlaybackTick);
            }
            Message::TimeSignaturePointAdd(sample) => {
                let mut state = self.state.blocking_write();
                let (bpm, numerator, denominator) = Self::timing_at_sample(&state, sample);
                if let Some(existing) = state
                    .time_signature_points
                    .iter_mut()
                    .find(|p| p.sample == sample)
                {
                    existing.numerator = numerator;
                    existing.denominator = denominator;
                } else {
                    state.time_signature_points.push(TimeSignaturePoint {
                        sample,
                        numerator,
                        denominator,
                    });
                    state
                        .time_signature_points
                        .sort_unstable_by_key(|p| p.sample);
                }
                if state.tempo_points.iter().all(|p| p.sample != sample) {
                    state.tempo_points.push(TempoPoint { sample, bpm });
                    state.tempo_points.sort_unstable_by_key(|p| p.sample);
                }
                self.selected_time_signature_points.clear();
                self.selected_time_signature_points.insert(sample);
                self.selected_tempo_points.clear();
                self.timing_selection_lane =
                    Some(super::super::super::TimingSelectionLane::TimeSignature);
                drop(state);
                self.sync_timing_inputs_from_selection();
                return self.update(Message::PlaybackTick);
            }
            Message::TimeSignaturePointSelect { sample, additive } => {
                if additive {
                    if !self.selected_time_signature_points.insert(sample) {
                        self.selected_time_signature_points.remove(&sample);
                    }
                } else {
                    self.selected_time_signature_points.clear();
                    self.selected_time_signature_points.insert(sample);
                }
                self.selected_tempo_points.clear();
                self.timing_selection_lane = if self.selected_time_signature_points.is_empty() {
                    None
                } else {
                    Some(super::super::super::TimingSelectionLane::TimeSignature)
                };
                self.sync_timing_inputs_from_selection();
            }
            Message::TimeSignaturePointsMove {
                from_samples,
                to_samples,
            } => {
                if from_samples.is_empty() || from_samples.len() != to_samples.len() {
                    return Task::none();
                }
                let mut state = self.state.blocking_write();
                let mut moved_values: Vec<(u8, u8)> = Vec::new();
                for sample in from_samples.iter().copied() {
                    if sample == 0 {
                        continue;
                    }
                    if let Some(idx) = state
                        .time_signature_points
                        .iter()
                        .position(|p| p.sample == sample)
                    {
                        moved_values.push((
                            state.time_signature_points[idx].numerator,
                            state.time_signature_points[idx].denominator,
                        ));
                        state.time_signature_points.remove(idx);
                    }
                }
                for (to, (numerator, denominator)) in
                    to_samples.iter().copied().zip(moved_values.into_iter())
                {
                    if let Some(existing) = state
                        .time_signature_points
                        .iter_mut()
                        .find(|p| p.sample == to)
                    {
                        existing.numerator = numerator;
                        existing.denominator = denominator;
                    } else {
                        state.time_signature_points.push(TimeSignaturePoint {
                            sample: to,
                            numerator,
                            denominator,
                        });
                    }
                }
                state
                    .time_signature_points
                    .sort_unstable_by_key(|p| p.sample);
                drop(state);
                self.selected_time_signature_points.clear();
                self.selected_time_signature_points.extend(to_samples);
                self.selected_tempo_points.clear();
                self.timing_selection_lane =
                    Some(super::super::super::TimingSelectionLane::TimeSignature);
                self.sync_timing_inputs_from_selection();
                return self.update(Message::PlaybackTick);
            }
            Message::TimeSignatureSelectionDuplicate => {
                if self.selected_time_signature_points.is_empty() {
                    return Task::none();
                }
                let beat_step = self.samples_per_beat().round() as usize;
                let mut state = self.state.blocking_write();
                let mut inserted = Vec::new();
                for sample in self.selected_time_signature_points.iter().copied() {
                    let Some(point) = state
                        .time_signature_points
                        .iter()
                        .find(|p| p.sample == sample)
                        .cloned()
                    else {
                        continue;
                    };
                    let new_sample = sample.saturating_add(beat_step).max(1);
                    if let Some(existing) = state
                        .time_signature_points
                        .iter_mut()
                        .find(|p| p.sample == new_sample)
                    {
                        existing.numerator = point.numerator;
                        existing.denominator = point.denominator;
                    } else {
                        state.time_signature_points.push(TimeSignaturePoint {
                            sample: new_sample,
                            numerator: point.numerator,
                            denominator: point.denominator,
                        });
                    }
                    inserted.push(new_sample);
                }
                state
                    .time_signature_points
                    .sort_unstable_by_key(|p| p.sample);
                drop(state);
                self.selected_time_signature_points.clear();
                self.selected_time_signature_points.extend(inserted);
                self.selected_tempo_points.clear();
                self.timing_selection_lane =
                    Some(super::super::super::TimingSelectionLane::TimeSignature);
                self.sync_timing_inputs_from_selection();
                return self.update(Message::PlaybackTick);
            }
            Message::TimeSignatureSelectionResetToPrevious => {
                if self.selected_time_signature_points.is_empty() {
                    return Task::none();
                }
                let mut state = self.state.blocking_write();
                let samples: Vec<usize> = self
                    .selected_time_signature_points
                    .iter()
                    .copied()
                    .collect();
                for sample in samples {
                    let (num, den) = state
                        .time_signature_points
                        .iter()
                        .filter(|p| p.sample < sample)
                        .max_by_key(|p| p.sample)
                        .map(|p| (p.numerator, p.denominator))
                        .unwrap_or((state.time_signature_num, state.time_signature_denom));
                    if let Some(point) = state
                        .time_signature_points
                        .iter_mut()
                        .find(|p| p.sample == sample)
                    {
                        point.numerator = num.max(1);
                        point.denominator = den.max(1);
                    }
                }
                drop(state);
                self.sync_timing_inputs_from_selection();
                return self.update(Message::PlaybackTick);
            }
            Message::TimeSignatureSelectionDelete => {
                if self.selected_time_signature_points.is_empty() {
                    return Task::none();
                }
                let selected: Vec<usize> = self
                    .selected_time_signature_points
                    .iter()
                    .copied()
                    .collect();
                let mut state = self.state.blocking_write();
                state
                    .time_signature_points
                    .retain(|p| p.sample == 0 || selected.binary_search(&p.sample).is_err());
                drop(state);
                self.selected_time_signature_points.clear();
                self.timing_selection_lane = None;
                return self.update(Message::PlaybackTick);
            }
            Message::ClearTimingPointSelection => {
                self.selected_tempo_points.clear();
                self.selected_time_signature_points.clear();
                self.timing_selection_lane = None;
            }
            Message::TimeSignatureNumeratorAdjust(delta) => {
                let sample = self.transport_samples.max(0.0) as usize;
                let mut state = self.state.blocking_write();
                let selected_samples: Vec<usize> = self
                    .selected_time_signature_points
                    .iter()
                    .copied()
                    .collect();
                let current = if let Some(sel) = selected_samples.first().copied() {
                    state
                        .time_signature_points
                        .iter()
                        .find(|p| p.sample == sel)
                        .map(|p| i16::from(p.numerator))
                        .unwrap_or(i16::from(state.time_signature_num))
                } else {
                    state
                        .time_signature_points
                        .iter()
                        .filter(|p| p.sample <= sample)
                        .max_by_key(|p| p.sample)
                        .map(|p| i16::from(p.numerator))
                        .unwrap_or(i16::from(state.time_signature_num))
                };
                let next = (current + i16::from(delta)).clamp(1, 16) as u8;
                if !selected_samples.is_empty() {
                    for point in state.time_signature_points.iter_mut() {
                        if selected_samples.contains(&point.sample) {
                            point.numerator = next;
                        }
                    }
                } else if let Some(point) = state
                    .time_signature_points
                    .iter_mut()
                    .find(|p| p.sample == sample)
                {
                    point.numerator = next;
                } else {
                    let (_, _, denominator) = Self::timing_at_sample(&state, sample);
                    state.time_signature_points.push(TimeSignaturePoint {
                        sample,
                        numerator: next,
                        denominator,
                    });
                }
                state
                    .time_signature_points
                    .sort_unstable_by_key(|p| p.sample);
                state.time_signature_num = next;
                let numerator = state.time_signature_num as u16;
                let denominator = state.time_signature_denom as u16;
                self.time_signature_num_input = numerator.to_string();
                drop(state);
                self.last_sent_time_signature = Some((numerator, denominator));
                return self.send(Action::SetTimeSignature {
                    numerator,
                    denominator,
                });
            }
            Message::TimeSignatureDenominatorAdjust(delta) => {
                let sample = self.transport_samples.max(0.0) as usize;
                let mut state = self.state.blocking_write();
                let values = [2_u8, 4, 8, 16];
                let selected_samples: Vec<usize> = self
                    .selected_time_signature_points
                    .iter()
                    .copied()
                    .collect();
                let current = if let Some(sel) = selected_samples.first().copied() {
                    state
                        .time_signature_points
                        .iter()
                        .find(|p| p.sample == sel)
                        .map(|p| p.denominator)
                        .unwrap_or(state.time_signature_denom)
                } else {
                    state
                        .time_signature_points
                        .iter()
                        .filter(|p| p.sample <= sample)
                        .max_by_key(|p| p.sample)
                        .map(|p| p.denominator)
                        .unwrap_or(state.time_signature_denom)
                };
                let current_idx = values.iter().position(|v| *v == current).unwrap_or(1) as i16;
                let next_idx = (current_idx + i16::from(delta)).clamp(0, 3) as usize;
                let next = values[next_idx];
                if !selected_samples.is_empty() {
                    for point in state.time_signature_points.iter_mut() {
                        if selected_samples.contains(&point.sample) {
                            point.denominator = next;
                        }
                    }
                } else if let Some(point) = state
                    .time_signature_points
                    .iter_mut()
                    .find(|p| p.sample == sample)
                {
                    point.denominator = next;
                } else {
                    let (_, numerator, _) = Self::timing_at_sample(&state, sample);
                    state.time_signature_points.push(TimeSignaturePoint {
                        sample,
                        numerator,
                        denominator: next,
                    });
                }
                state
                    .time_signature_points
                    .sort_unstable_by_key(|p| p.sample);
                state.time_signature_denom = next;
                let numerator = state.time_signature_num as u16;
                let denominator = state.time_signature_denom as u16;
                self.time_signature_denom_input = denominator.to_string();
                drop(state);
                self.last_sent_time_signature = Some((numerator, denominator));
                return self.send(Action::SetTimeSignature {
                    numerator,
                    denominator,
                });
            }
            Message::TempoInputChanged(ref value) => {
                self.tempo_input = value.clone();
            }
            Message::TempoInputCommit => {
                let Ok(parsed) = self.tempo_input.trim().parse::<f32>() else {
                    self.state.blocking_write().message = "Invalid BPM value".to_string();
                    return Task::none();
                };
                let bpm = parsed.clamp(20.0, 300.0);
                let sample = self.transport_samples.max(0.0) as usize;
                let mut state = self.state.blocking_write();
                let (_, numerator, denominator) = Self::timing_at_sample(&state, sample);
                if let Some(point) = state.tempo_points.iter_mut().find(|p| p.sample == sample) {
                    point.bpm = bpm;
                } else {
                    state.tempo_points.push(TempoPoint { sample, bpm });
                }
                if state
                    .time_signature_points
                    .iter()
                    .all(|p| p.sample != sample)
                {
                    state.time_signature_points.push(TimeSignaturePoint {
                        sample,
                        numerator,
                        denominator,
                    });
                }
                state.tempo_points.sort_unstable_by_key(|p| p.sample);
                state
                    .time_signature_points
                    .sort_unstable_by_key(|p| p.sample);
                state.tempo = bpm;
                self.tempo_input = format!("{:.2}", bpm);
                drop(state);
                self.selected_tempo_points.clear();
                self.selected_time_signature_points.clear();
                self.timing_selection_lane = None;
                self.last_sent_tempo_bpm = Some(bpm as f64);
                return self.send(Action::SetTempo(bpm as f64));
            }
            Message::TimeSignatureNumeratorInputChanged(ref value) => {
                self.time_signature_num_input = value.clone();
            }
            Message::TimeSignatureDenominatorInputChanged(ref value) => {
                self.time_signature_denom_input = value.clone();
            }
            Message::TimeSignatureInputCommit => {
                let Ok(num) = self.time_signature_num_input.trim().parse::<u16>() else {
                    self.state.blocking_write().message =
                        "Invalid time signature numerator".to_string();
                    return Task::none();
                };
                let Ok(den) = self.time_signature_denom_input.trim().parse::<u16>() else {
                    self.state.blocking_write().message =
                        "Invalid time signature denominator".to_string();
                    return Task::none();
                };
                let numerator = num.clamp(1, 16) as u8;
                let denominator = match den {
                    2 | 4 | 8 | 16 => den as u8,
                    _ => {
                        self.state.blocking_write().message =
                            "Time signature denominator must be 2, 4, 8, or 16".to_string();
                        return Task::none();
                    }
                };
                let sample = self.transport_samples.max(0.0) as usize;
                let mut state = self.state.blocking_write();
                let (bpm, _, _) = Self::timing_at_sample(&state, sample);
                if let Some(point) = state
                    .time_signature_points
                    .iter_mut()
                    .find(|p| p.sample == sample)
                {
                    point.numerator = numerator;
                    point.denominator = denominator;
                } else {
                    state.time_signature_points.push(TimeSignaturePoint {
                        sample,
                        numerator,
                        denominator,
                    });
                }
                if state.tempo_points.iter().all(|p| p.sample != sample) {
                    state.tempo_points.push(TempoPoint { sample, bpm });
                }
                state
                    .time_signature_points
                    .sort_unstable_by_key(|p| p.sample);
                state.tempo_points.sort_unstable_by_key(|p| p.sample);
                state.time_signature_num = numerator;
                state.time_signature_denom = denominator;
                self.time_signature_num_input = numerator.to_string();
                self.time_signature_denom_input = denominator.to_string();
                drop(state);
                self.selected_time_signature_points.clear();
                self.selected_tempo_points.clear();
                self.timing_selection_lane = None;
                self.last_sent_time_signature = Some((numerator as u16, denominator as u16));
                return self.send(Action::SetTimeSignature {
                    numerator: numerator as u16,
                    denominator: denominator as u16,
                });
            }
            _ => {}
        }
        Task::none()
    }
}
