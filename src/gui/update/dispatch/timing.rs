use super::*;

impl Maolan {
    fn ensure_tempo_anchor_at_zero(state: &mut crate::state::StateData) {
        if state.tempo_points.iter().all(|p| p.sample != 0) {
            state.tempo_points.push(TempoPoint {
                sample: 0,
                bpm: state.tempo.clamp(20.0, 300.0),
            });
            state.tempo_points.sort_unstable_by_key(|p| p.sample);
        }
    }

    fn ensure_time_signature_anchor_at_zero(state: &mut crate::state::StateData) {
        if state.time_signature_points.iter().all(|p| p.sample != 0) {
            state.time_signature_points.push(TimeSignaturePoint {
                sample: 0,
                numerator: state.time_signature_num.max(1),
                denominator: state.time_signature_denom.max(1),
            });
            state
                .time_signature_points
                .sort_unstable_by_key(|p| p.sample);
        }
    }

    fn send_current_tempo_map(&self) -> Task<Message> {
        let state = self.state.read().expect("state lock poisoned");
        let tempo_points = state
            .tempo_points
            .iter()
            .map(|p| maolan_engine::message::TempoPoint {
                sample: p.sample,
                bpm: p.bpm as f64,
            })
            .collect();
        let time_signature_points = state
            .time_signature_points
            .iter()
            .map(|p| maolan_engine::message::TimeSignaturePoint {
                sample: p.sample,
                numerator: p.numerator as u16,
                denominator: p.denominator as u16,
            })
            .collect();
        drop(state);
        self.send(Action::SetTempoMap {
            tempo_points,
            time_signature_points,
        })
    }

    pub(super) fn handle_timing_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::TempoAdjust(delta) => {
                let sample = self.transport.transport_samples.max(0.0) as usize;
                let mut state = self.state.write().expect("state lock poisoned");
                let selected_samples: Vec<usize> =
                    self.timing.selected_tempo_points.iter().copied().collect();
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
                let updates_global = if selected_samples.is_empty() {
                    sample == 0
                } else {
                    selected_samples.contains(&0)
                };
                if updates_global {
                    state.tempo = tempo;
                } else {
                    Self::ensure_tempo_anchor_at_zero(&mut state);
                }
                self.timing.tempo_input = format!("{:.2}", tempo);
                drop(state);
                self.timing.last_sent_tempo_bpm = Some(tempo as f64);
                return self.send_current_tempo_map();
            }
            Message::TempoPointAdd(sample) => {
                let mut state = self.state.write().expect("state lock poisoned");
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
                self.timing.selected_tempo_points.clear();
                self.timing.selected_tempo_points.insert(sample);
                self.timing.selected_time_signature_points.clear();
                self.timing.selected_time_signature_points.insert(sample);
                self.timing.timing_selection_lane =
                    Some(super::super::super::TimingSelectionLane::Tempo);
                drop(state);
                self.timing.sync_timing_inputs_from_selection(&self.state);
                return Task::batch(vec![
                    self.send_current_tempo_map(),
                    self.update(Message::PlaybackTick),
                ]);
            }
            Message::TempoPointSelect { sample, additive } => {
                if additive {
                    let removed = !self.timing.selected_tempo_points.insert(sample);
                    self.timing.selected_time_signature_points.insert(sample);
                    if removed {
                        self.timing.selected_tempo_points.remove(&sample);
                        self.timing.selected_time_signature_points.remove(&sample);
                    }
                } else {
                    self.timing.set_selection(
                        [sample],
                        Some(super::super::super::TimingSelectionLane::Tempo),
                    );
                    self.timing.sync_timing_inputs_from_selection(&self.state);
                    return Task::none();
                }
                self.timing.timing_selection_lane = if self.timing.selected_tempo_points.is_empty()
                {
                    None
                } else {
                    Some(super::super::super::TimingSelectionLane::Tempo)
                };
                self.timing.sync_timing_inputs_from_selection(&self.state);
            }
            Message::TempoPointsMove {
                from_samples,
                to_samples,
            } => {
                if from_samples.is_empty() || from_samples.len() != to_samples.len() {
                    return Task::none();
                }
                let mut state = self.state.write().expect("state lock poisoned");
                let moves = from_samples
                    .iter()
                    .copied()
                    .zip(to_samples.iter().copied())
                    .filter(|(from, _)| *from != 0)
                    .collect::<Vec<_>>();
                let mut moved_tempos: Vec<(usize, f32)> = Vec::new();
                let mut moved_signatures: Vec<(usize, u8, u8)> = Vec::new();
                for (from, to) in moves.iter().copied() {
                    if let Some(idx) = state.tempo_points.iter().position(|p| p.sample == from) {
                        let bpm = state.tempo_points[idx].bpm;
                        state.tempo_points.remove(idx);
                        moved_tempos.push((to, bpm));
                    }
                    if let Some(idx) = state
                        .time_signature_points
                        .iter()
                        .position(|p| p.sample == from)
                    {
                        let point = state.time_signature_points.remove(idx);
                        moved_signatures.push((to, point.numerator, point.denominator));
                    }
                }
                for (to, bpm) in moved_tempos {
                    if let Some(existing) = state.tempo_points.iter_mut().find(|p| p.sample == to) {
                        existing.bpm = bpm;
                    } else {
                        state.tempo_points.push(TempoPoint { sample: to, bpm });
                    }
                }
                for (to, numerator, denominator) in moved_signatures {
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
                state.tempo_points.sort_unstable_by_key(|p| p.sample);
                state
                    .time_signature_points
                    .sort_unstable_by_key(|p| p.sample);
                drop(state);
                self.timing.set_selection(
                    to_samples,
                    Some(super::super::super::TimingSelectionLane::Tempo),
                );
                self.timing.sync_timing_inputs_from_selection(&self.state);
                return Task::batch(vec![
                    self.send_current_tempo_map(),
                    self.update(Message::PlaybackTick),
                ]);
            }
            Message::TempoSelectionDuplicate => {
                let selected_samples = self.timing.selected_samples();
                if selected_samples.is_empty() {
                    return Task::none();
                }
                let beat_step = self.transport.samples_per_beat(&self.state).round() as usize;
                let mut state = self.state.write().expect("state lock poisoned");
                let mut inserted = Vec::new();
                for sample in selected_samples {
                    let new_sample = sample.saturating_add(beat_step).max(1);
                    if let Some(point) = state
                        .tempo_points
                        .iter()
                        .find(|p| p.sample == sample)
                        .cloned()
                    {
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
                    }
                    if let Some(point) = state
                        .time_signature_points
                        .iter()
                        .find(|p| p.sample == sample)
                        .cloned()
                    {
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
                    }
                    inserted.push(new_sample);
                }
                state.tempo_points.sort_unstable_by_key(|p| p.sample);
                state
                    .time_signature_points
                    .sort_unstable_by_key(|p| p.sample);
                drop(state);
                self.timing.set_selection(
                    inserted,
                    Some(super::super::super::TimingSelectionLane::Tempo),
                );
                self.timing.sync_timing_inputs_from_selection(&self.state);
                return Task::batch(vec![
                    self.send_current_tempo_map(),
                    self.update(Message::PlaybackTick),
                ]);
            }
            Message::TempoSelectionResetToPrevious => {
                let samples = self.timing.selected_samples();
                if samples.is_empty() {
                    return Task::none();
                }
                let mut state = self.state.write().expect("state lock poisoned");
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
                self.timing.sync_timing_inputs_from_selection(&self.state);
                return Task::batch(vec![
                    self.send_current_tempo_map(),
                    self.update(Message::PlaybackTick),
                ]);
            }
            Message::TempoSelectionDelete => {
                let selected = self.timing.selected_samples();
                if selected.is_empty() {
                    return Task::none();
                }
                let mut state = self.state.write().expect("state lock poisoned");
                state
                    .tempo_points
                    .retain(|p| p.sample == 0 || selected.binary_search(&p.sample).is_err());
                state
                    .time_signature_points
                    .retain(|p| p.sample == 0 || selected.binary_search(&p.sample).is_err());
                drop(state);
                self.timing.set_selection([], None);
                return Task::batch(vec![
                    self.send_current_tempo_map(),
                    self.update(Message::PlaybackTick),
                ]);
            }
            Message::TimeSignaturePointAdd(sample) => {
                let mut state = self.state.write().expect("state lock poisoned");
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
                self.timing.selected_time_signature_points.clear();
                self.timing.selected_time_signature_points.insert(sample);
                self.timing.selected_tempo_points.clear();
                self.timing.selected_tempo_points.insert(sample);
                self.timing.timing_selection_lane =
                    Some(super::super::super::TimingSelectionLane::TimeSignature);
                drop(state);
                self.timing.sync_timing_inputs_from_selection(&self.state);
                return Task::batch(vec![
                    self.send_current_tempo_map(),
                    self.update(Message::PlaybackTick),
                ]);
            }
            Message::TimeSignaturePointSelect { sample, additive } => {
                if additive {
                    let removed = !self.timing.selected_time_signature_points.insert(sample);
                    self.timing.selected_tempo_points.insert(sample);
                    if removed {
                        self.timing.selected_time_signature_points.remove(&sample);
                        self.timing.selected_tempo_points.remove(&sample);
                    }
                } else {
                    self.timing.set_selection(
                        [sample],
                        Some(super::super::super::TimingSelectionLane::TimeSignature),
                    );
                    self.timing.sync_timing_inputs_from_selection(&self.state);
                    return Task::none();
                }
                self.timing.timing_selection_lane =
                    if self.timing.selected_time_signature_points.is_empty() {
                        None
                    } else {
                        Some(super::super::super::TimingSelectionLane::TimeSignature)
                    };
                self.timing.sync_timing_inputs_from_selection(&self.state);
            }
            Message::TimeSignaturePointsMove {
                from_samples,
                to_samples,
            } => {
                if from_samples.is_empty() || from_samples.len() != to_samples.len() {
                    return Task::none();
                }
                let mut state = self.state.write().expect("state lock poisoned");
                let moves = from_samples
                    .iter()
                    .copied()
                    .zip(to_samples.iter().copied())
                    .filter(|(from, _)| *from != 0)
                    .collect::<Vec<_>>();
                let mut moved_signatures: Vec<(usize, u8, u8)> = Vec::new();
                let mut moved_tempos: Vec<(usize, f32)> = Vec::new();
                for (from, to) in moves.iter().copied() {
                    if let Some(idx) = state
                        .time_signature_points
                        .iter()
                        .position(|p| p.sample == from)
                    {
                        let point = state.time_signature_points.remove(idx);
                        moved_signatures.push((to, point.numerator, point.denominator));
                    }
                    if let Some(idx) = state.tempo_points.iter().position(|p| p.sample == from) {
                        let bpm = state.tempo_points[idx].bpm;
                        state.tempo_points.remove(idx);
                        moved_tempos.push((to, bpm));
                    }
                }
                for (to, (numerator, denominator)) in moved_signatures
                    .into_iter()
                    .map(|(to, num, den)| (to, (num, den)))
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
                for (to, bpm) in moved_tempos {
                    if let Some(existing) = state.tempo_points.iter_mut().find(|p| p.sample == to) {
                        existing.bpm = bpm;
                    } else {
                        state.tempo_points.push(TempoPoint { sample: to, bpm });
                    }
                }
                state
                    .time_signature_points
                    .sort_unstable_by_key(|p| p.sample);
                state.tempo_points.sort_unstable_by_key(|p| p.sample);
                drop(state);
                self.timing.set_selection(
                    to_samples,
                    Some(super::super::super::TimingSelectionLane::TimeSignature),
                );
                self.timing.sync_timing_inputs_from_selection(&self.state);
                return Task::batch(vec![
                    self.send_current_tempo_map(),
                    self.update(Message::PlaybackTick),
                ]);
            }
            Message::TimeSignatureSelectionDuplicate => {
                let selected_samples = self.timing.selected_samples();
                if selected_samples.is_empty() {
                    return Task::none();
                }
                let beat_step = self.transport.samples_per_beat(&self.state).round() as usize;
                let mut state = self.state.write().expect("state lock poisoned");
                let mut inserted = Vec::new();
                for sample in selected_samples {
                    let new_sample = sample.saturating_add(beat_step).max(1);
                    if let Some(point) = state
                        .time_signature_points
                        .iter()
                        .find(|p| p.sample == sample)
                        .cloned()
                    {
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
                    }
                    if let Some(point) = state
                        .tempo_points
                        .iter()
                        .find(|p| p.sample == sample)
                        .cloned()
                    {
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
                    }
                    inserted.push(new_sample);
                }
                state
                    .time_signature_points
                    .sort_unstable_by_key(|p| p.sample);
                state.tempo_points.sort_unstable_by_key(|p| p.sample);
                drop(state);
                self.timing.set_selection(
                    inserted,
                    Some(super::super::super::TimingSelectionLane::TimeSignature),
                );
                self.timing.sync_timing_inputs_from_selection(&self.state);
                return Task::batch(vec![
                    self.send_current_tempo_map(),
                    self.update(Message::PlaybackTick),
                ]);
            }
            Message::TimeSignatureSelectionResetToPrevious => {
                let samples = self.timing.selected_samples();
                if samples.is_empty() {
                    return Task::none();
                }
                let mut state = self.state.write().expect("state lock poisoned");
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
                self.timing.sync_timing_inputs_from_selection(&self.state);
                return Task::batch(vec![
                    self.send_current_tempo_map(),
                    self.update(Message::PlaybackTick),
                ]);
            }
            Message::TimeSignatureSelectionDelete => {
                let selected = self.timing.selected_samples();
                if selected.is_empty() {
                    return Task::none();
                }
                let mut state = self.state.write().expect("state lock poisoned");
                state
                    .time_signature_points
                    .retain(|p| p.sample == 0 || selected.binary_search(&p.sample).is_err());
                state
                    .tempo_points
                    .retain(|p| p.sample == 0 || selected.binary_search(&p.sample).is_err());
                drop(state);
                self.timing.set_selection([], None);
                return Task::batch(vec![
                    self.send_current_tempo_map(),
                    self.update(Message::PlaybackTick),
                ]);
            }
            Message::ClearTimingPointSelection => {
                self.timing.selected_tempo_points.clear();
                self.timing.selected_time_signature_points.clear();
                self.timing.timing_selection_lane = None;
            }
            Message::TimeSignatureNumeratorAdjust(delta) => {
                let sample = self.transport.transport_samples.max(0.0) as usize;
                let mut state = self.state.write().expect("state lock poisoned");
                let selected_samples: Vec<usize> = self
                    .timing
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
                let updates_global = if selected_samples.is_empty() {
                    sample == 0
                } else {
                    selected_samples.contains(&0)
                };
                if updates_global {
                    state.time_signature_num = next;
                } else {
                    Self::ensure_time_signature_anchor_at_zero(&mut state);
                }
                let numerator = next as u16;
                let denominator = state.time_signature_denom as u16;
                self.timing.time_signature_num_input = numerator.to_string();
                drop(state);
                self.timing.last_sent_time_signature = Some((numerator, denominator));
                return self.send_current_tempo_map();
            }
            Message::TimeSignatureDenominatorAdjust(delta) => {
                let sample = self.transport.transport_samples.max(0.0) as usize;
                let mut state = self.state.write().expect("state lock poisoned");
                let values = [2_u8, 4, 8, 16];
                let selected_samples: Vec<usize> = self
                    .timing
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
                let updates_global = if selected_samples.is_empty() {
                    sample == 0
                } else {
                    selected_samples.contains(&0)
                };
                if updates_global {
                    state.time_signature_denom = next;
                } else {
                    Self::ensure_time_signature_anchor_at_zero(&mut state);
                }
                let numerator = state.time_signature_num as u16;
                let denominator = next as u16;
                self.timing.time_signature_denom_input = denominator.to_string();
                drop(state);
                self.timing.last_sent_time_signature = Some((numerator, denominator));
                return self.send_current_tempo_map();
            }
            Message::TempoInputChanged(ref value) => {
                self.timing.tempo_input = value.clone();
            }
            Message::TempoInputCommit => {
                let Ok(parsed) = self.timing.tempo_input.trim().parse::<f32>() else {
                    self.state.write().expect("state lock poisoned").message =
                        "Invalid BPM value".to_string();
                    return Task::none();
                };
                let bpm = parsed.clamp(20.0, 300.0);
                let sample = self.transport.transport_samples.max(0.0) as usize;
                let mut state = self.state.write().expect("state lock poisoned");
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
                if sample == 0 {
                    state.tempo = bpm;
                } else {
                    Self::ensure_tempo_anchor_at_zero(&mut state);
                }
                self.timing.tempo_input = format!("{:.2}", bpm);
                drop(state);
                self.timing.selected_tempo_points.clear();
                self.timing.selected_time_signature_points.clear();
                self.timing.timing_selection_lane = None;
                self.timing.last_sent_tempo_bpm = Some(bpm as f64);
                return self.send_current_tempo_map();
            }
            Message::TapTempo => {
                let now = Instant::now();
                const TAP_TIMEOUT: Duration = Duration::from_secs(2);
                const MAX_TAPS: usize = 8;

                if let Some(last) = self.timing.tap_tempo_times.last()
                    && now.duration_since(*last) > TAP_TIMEOUT
                {
                    self.timing.tap_tempo_times.clear();
                }

                self.timing.tap_tempo_times.push(now);

                if self.timing.tap_tempo_times.len() > MAX_TAPS {
                    self.timing.tap_tempo_times.remove(0);
                }

                if self.timing.tap_tempo_times.len() >= 2 {
                    let intervals: Vec<f32> = self
                        .timing
                        .tap_tempo_times
                        .windows(2)
                        .map(|w| w[1].duration_since(w[0]).as_secs_f32())
                        .collect();

                    let avg_interval = intervals.iter().sum::<f32>() / intervals.len() as f32;

                    if avg_interval > 0.0 {
                        let bpm = (60.0 / avg_interval).clamp(20.0, 300.0);
                        let sample = self.transport.transport_samples.max(0.0) as usize;
                        let mut state = self.state.write().expect("state lock poisoned");
                        let (_, numerator, denominator) = Self::timing_at_sample(&state, sample);
                        if let Some(point) =
                            state.tempo_points.iter_mut().find(|p| p.sample == sample)
                        {
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
                        if sample == 0 {
                            state.tempo = bpm;
                        } else {
                            Self::ensure_tempo_anchor_at_zero(&mut state);
                        }
                        self.timing.tempo_input = format!("{:.2}", bpm);
                        drop(state);
                        self.timing.selected_tempo_points.clear();
                        self.timing.selected_time_signature_points.clear();
                        self.timing.timing_selection_lane = None;
                        self.timing.last_sent_tempo_bpm = Some(bpm as f64);
                        return self.send_current_tempo_map();
                    }
                }
                return Task::none();
            }
            Message::TimeSignatureNumeratorInputChanged(ref value) => {
                self.timing.time_signature_num_input = value.clone();
            }
            Message::TimeSignatureDenominatorInputChanged(ref value) => {
                self.timing.time_signature_denom_input = value.clone();
            }
            Message::TimeSignatureInputCommit => {
                let Ok(num) = self.timing.time_signature_num_input.trim().parse::<u16>() else {
                    self.state.write().expect("state lock poisoned").message =
                        "Invalid time signature numerator".to_string();
                    return Task::none();
                };
                let Ok(den) = self.timing.time_signature_denom_input.trim().parse::<u16>() else {
                    self.state.write().expect("state lock poisoned").message =
                        "Invalid time signature denominator".to_string();
                    return Task::none();
                };
                let numerator = num.clamp(1, 16) as u8;
                let denominator = match den {
                    2 | 4 | 8 | 16 => den as u8,
                    _ => {
                        self.state.write().expect("state lock poisoned").message =
                            "Time signature denominator must be 2, 4, 8, or 16".to_string();
                        return Task::none();
                    }
                };
                let sample = self.transport.transport_samples.max(0.0) as usize;
                let mut state = self.state.write().expect("state lock poisoned");
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
                if sample == 0 {
                    state.time_signature_num = numerator;
                    state.time_signature_denom = denominator;
                } else {
                    Self::ensure_time_signature_anchor_at_zero(&mut state);
                }
                self.timing.time_signature_num_input = numerator.to_string();
                self.timing.time_signature_denom_input = denominator.to_string();
                drop(state);
                self.timing.selected_time_signature_points.clear();
                self.timing.selected_tempo_points.clear();
                self.timing.timing_selection_lane = None;
                self.timing.last_sent_time_signature = Some((numerator as u16, denominator as u16));
                return self.send_current_tempo_map();
            }
            _ => {}
        }
        Task::none()
    }
}
