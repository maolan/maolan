use super::*;

impl Maolan {
    pub(super) fn handle_modulators_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ModulatorAdd => {
                let id = self.modulators.iter().map(|m| m.id).max().unwrap_or(0) + 1;
                self.modulators.push(crate::state::Modulator::new(id));
                self.selected_modulator_id = Some(id);
                self.state
                    .write()
                    .expect("state lock poisoned")
                    .selected_modulator_id = Some(id);
                self.session_ops.has_unsaved_changes = true;
                return self.send_modulators_to_engine();
            }
            Message::ModulatorRemove(id) => {
                self.modulators.retain(|m| m.id != id);
                if self.selected_modulator_id == Some(id) {
                    self.selected_modulator_id = None;
                    self.state
                        .write()
                        .expect("state lock poisoned")
                        .selected_modulator_id = None;
                }
                self.session_ops.has_unsaved_changes = true;
                return self.send_modulators_to_engine();
            }
            Message::ModulatorSelect(id) => {
                self.selected_modulator_id = id;
                self.state
                    .write()
                    .expect("state lock poisoned")
                    .selected_modulator_id = id;
            }
            Message::ModulatorToggleTarget { id, ref target } => {
                if let Some(m) = self.modulators.iter_mut().find(|m| m.id == id) {
                    let pos = m
                        .targets
                        .iter()
                        .position(|t| t.matches_target(&target.track_name, &target.target));
                    if let Some(pos) = pos {
                        m.targets.remove(pos);
                    } else {
                        m.targets.push(target.clone());
                    }
                }
                self.session_ops.has_unsaved_changes = true;
                return self.send_modulators_to_engine();
            }
            Message::ModulatorToggleSelectedTarget { ref target } => {
                if let Some(id) = self.selected_modulator_id {
                    if let Some(m) = self.modulators.iter_mut().find(|m| m.id == id) {
                        let pos = m
                            .targets
                            .iter()
                            .position(|t| t.matches_target(&target.track_name, &target.target));
                        if let Some(pos) = pos {
                            m.targets.remove(pos);
                        } else {
                            m.targets.push(target.clone());
                        }
                    }
                    self.session_ops.has_unsaved_changes = true;
                    return self.send_modulators_to_engine();
                }
            }
            Message::ModulatorUpdate { id, ref change } => {
                if let Some(m) = self.modulators.iter_mut().find(|m| m.id == id) {
                    match change {
                        ModulatorChange::Name(v) => m.name = v.clone(),
                        ModulatorChange::Shape(v) => m.shape = *v,
                        ModulatorChange::Rate(v) => m.rate = *v,
                        ModulatorChange::Phase(v) => m.phase = *v,
                        ModulatorChange::Enabled(v) => m.enabled = *v,
                        ModulatorChange::Targets(v) => m.targets = v.clone(),
                    }
                }
                self.session_ops.has_unsaved_changes = true;
                return self.send_modulators_to_engine();
            }
            Message::ModulatorTargetShow {
                modulator_id,
                ref track_name,
                ref target,
            } => {
                let (default_min, default_max) = target.default_range();
                let existing = self
                    .modulators
                    .iter()
                    .find(|m| m.id == modulator_id)
                    .and_then(|m| {
                        m.targets
                            .iter()
                            .find(|t| t.matches_target(track_name, target))
                    });
                let existing_bool = existing.is_some();
                let (min_input, max_input) = existing
                    .map_or((default_min.to_string(), default_max.to_string()), |t| {
                        (t.min.to_string(), t.max.to_string())
                    });
                self.modulator_target_dialog
                    .open(crate::state::ModulatorTargetDialog {
                        modulator_id,
                        track_name: track_name.clone(),
                        target: target.clone(),
                        min_input,
                        max_input,
                        existing: existing_bool,
                    });
            }
            Message::ModulatorTargetMinInput(_) | Message::ModulatorTargetMaxInput(_) => {
                self.modulator_target_dialog.update(&message);
            }
            Message::ModulatorTargetConfirm => {
                let dialog = self.modulator_target_dialog.dialog.clone();
                let Some(dialog) = dialog else {
                    return Task::none();
                };

                let Ok(min) = dialog.min_input.trim().parse::<f32>() else {
                    self.state.write().expect("state lock poisoned").message =
                        "Invalid min value".to_string();
                    return Task::none();
                };
                let Ok(max) = dialog.max_input.trim().parse::<f32>() else {
                    self.state.write().expect("state lock poisoned").message =
                        "Invalid max value".to_string();
                    return Task::none();
                };

                if let Some(m) = self
                    .modulators
                    .iter_mut()
                    .find(|m| m.id == dialog.modulator_id)
                {
                    if let Some(target) = m
                        .targets
                        .iter_mut()
                        .find(|t| t.matches_target(&dialog.track_name, &dialog.target))
                    {
                        target.min = min;
                        target.max = max;
                    } else {
                        m.targets.push(crate::state::ModulatorTarget {
                            track_name: dialog.track_name,
                            target: dialog.target,
                            min,
                            max,
                        });
                    }
                }
                self.modulator_target_dialog.close();
                self.session_ops.has_unsaved_changes = true;
                return self.send_modulators_to_engine();
            }
            Message::ModulatorTargetCancel => {
                self.modulator_target_dialog.close();
            }
            Message::ModulatorTargetRemove {
                modulator_id,
                ref track_name,
                target,
            } => {
                if let Some(m) = self.modulators.iter_mut().find(|m| m.id == modulator_id) {
                    m.targets.retain(|t| !t.matches_target(track_name, &target));
                }
                self.modulator_target_dialog.close();
                self.session_ops.has_unsaved_changes = true;
                return self.send_modulators_to_engine();
            }
            _ => {}
        }
        self.update_children(&message);
        Task::none()
    }
}
