use super::*;

impl Maolan {
    pub(super) fn autosave_snapshot_root(&self) -> Option<std::path::PathBuf> {
        self.session_dir
            .as_ref()
            .map(|session_dir| session_dir.join(".maolan_autosave"))
    }

    pub(super) fn autosave_snapshots_dir_for(path: &std::path::Path) -> std::path::PathBuf {
        path.join(".maolan_autosave/snapshots")
    }

    pub(super) fn list_autosave_snapshots_for(path: &std::path::Path) -> Vec<std::path::PathBuf> {
        let snapshots_dir = Self::autosave_snapshots_dir_for(path);
        let mut snapshots = fs::read_dir(snapshots_dir)
            .ok()
            .into_iter()
            .flat_map(|entries| entries.filter_map(|entry| entry.ok()))
            .map(|entry| entry.path())
            .filter(|snapshot_dir| {
                snapshot_dir.is_dir() && snapshot_dir.join("session.json").exists()
            })
            .collect::<Vec<_>>();
        snapshots.sort_by(|a, b| b.cmp(a));
        snapshots
    }

    pub(super) fn has_newer_autosave_snapshot(path: &std::path::Path) -> bool {
        let Some(autosave_session) = Self::list_autosave_snapshots_for(path).first().cloned()
        else {
            return false;
        };
        let autosave_mtime = fs::metadata(autosave_session.join("session.json"))
            .and_then(|m| m.modified())
            .ok();
        let session_mtime = fs::metadata(path.join("session.json"))
            .and_then(|m| m.modified())
            .ok();
        match (autosave_mtime, session_mtime) {
            (Some(a), Some(s)) => a > s,
            (Some(_), None) => true,
            _ => false,
        }
    }

    pub(super) fn autosave_recovery_preview_summary(
        session_dir: &std::path::Path,
        snapshot_dir: &std::path::Path,
    ) -> String {
        fn read_counts(path: &std::path::Path) -> Option<(usize, usize, usize)> {
            let f = fs::File::open(path).ok()?;
            let json: serde_json::Value = serde_json::from_reader(f).ok()?;
            let tracks = json.get("tracks")?.as_array()?;
            let track_count = tracks.len();
            let mut audio_count = 0usize;
            let mut midi_count = 0usize;
            for track in tracks {
                audio_count = audio_count.saturating_add(
                    track
                        .get("audio")
                        .and_then(|a| a.get("clips"))
                        .and_then(serde_json::Value::as_array)
                        .map(|clips| clips.len())
                        .unwrap_or(0),
                );
                midi_count = midi_count.saturating_add(
                    track
                        .get("midi")
                        .and_then(|m| m.get("clips"))
                        .and_then(serde_json::Value::as_array)
                        .map(|clips| clips.len())
                        .unwrap_or(0),
                );
            }
            Some((track_count, audio_count, midi_count))
        }

        let live = read_counts(&session_dir.join("session.json"));
        let snap = read_counts(&snapshot_dir.join("session.json"));
        let label = snapshot_dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("snapshot");
        match (live, snap) {
            (Some((lt, la, lm)), Some((st, sa, sm))) => format!(
                "Autosave preview [{label}]: tracks {lt}->{st}, audio clips {la}->{sa}, midi clips {lm}->{sm}"
            ),
            _ => format!("Autosave preview [{label}]: unable to compute diff summary"),
        }
    }

    pub(super) fn write_last_session_hint(path: &str) {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let config_dir = std::path::PathBuf::from(home).join(".config/maolan");
        let _ = fs::create_dir_all(&config_dir);
        let _ = fs::write(config_dir.join("last_session_path"), path);
    }

    pub(super) fn export_diagnostics_bundle(&self) -> Result<std::path::PathBuf, String> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let root = self
            .session_dir
            .clone()
            .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
            .join(format!("maolan_diagnostics_{stamp}"));
        fs::create_dir_all(&root).map_err(|e| e.to_string())?;

        let state = self.state.blocking_read();
        let diagnostics = state
            .diagnostics_report
            .clone()
            .unwrap_or_else(|| "No diagnostics report captured yet".to_string());
        fs::write(root.join("session_diagnostics.txt"), diagnostics).map_err(|e| e.to_string())?;
        fs::write(
            root.join("midi_mappings.txt"),
            if self.midi_mappings_report_lines.is_empty() {
                "No MIDI mappings captured yet".to_string()
            } else {
                self.midi_mappings_report_lines.join("\n")
            },
        )
        .map_err(|e| e.to_string())?;

        let summary = serde_json::json!({
            "transport": {
                "playing": self.playing,
                "paused": self.paused,
                "sample": self.transport_samples.max(0.0) as usize,
            },
            "session_dir": self.session_dir.as_ref().map(|p| p.to_string_lossy().to_string()),
            "track_count": state.tracks.len(),
            "selected_tracks": state.selected.iter().cloned().collect::<Vec<String>>(),
            "export_in_progress": self.export_in_progress,
            "freeze_in_progress": self.freeze_in_progress,
            "record_armed": self.record_armed,
            "timestamp_unix": stamp,
        });
        let f = fs::File::create(root.join("ui_summary.json")).map_err(|e| e.to_string())?;
        serde_json::to_writer_pretty(f, &summary).map_err(|e| e.to_string())?;
        Ok(root)
    }

    pub(super) fn prepare_pending_autosave_recovery(&mut self, select_older: bool) -> Result<(), String> {
        if let Some(pending) = self.pending_autosave_recovery.as_mut() {
            if select_older {
                if pending.selected_index + 1 < pending.snapshots.len() {
                    pending.selected_index += 1;
                } else {
                    return Err("No older autosave snapshot available".to_string());
                }
            }
            pending.confirm_armed = false;
            return Ok(());
        }

        let base_session_dir = self
            .pending_recovery_session_dir
            .clone()
            .or_else(|| self.session_dir.clone())
            .ok_or_else(|| "No session available for autosave recovery".to_string())?;
        let snapshots = Self::list_autosave_snapshots_for(&base_session_dir);
        if snapshots.is_empty() {
            return Err("No autosave snapshot found for this session".to_string());
        }
        let selected_index = if select_older {
            if snapshots.len() >= 2 {
                1
            } else {
                return Err("No older autosave snapshot available".to_string());
            }
        } else {
            0
        };
        self.pending_autosave_recovery = Some(super::super::PendingAutosaveRecovery {
            session_dir: base_session_dir,
            snapshots,
            selected_index,
            confirm_armed: false,
        });
        Ok(())
    }

    pub(super) fn apply_pending_autosave_recovery(&mut self) -> Task<Message> {
        let Some(pending) = self.pending_autosave_recovery.clone() else {
            self.state.blocking_write().message = "No autosave recovery pending".to_string();
            return Task::none();
        };
        if let Some(snapshot) = pending.snapshots.get(pending.selected_index) {
            self.session_dir = Some(pending.session_dir.clone());
            self.stop_recording_preview();
            self.pending_recovery_session_dir = None;
            self.pending_autosave_recovery = None;
            self.pending_open_session_dir = None;
            self.has_unsaved_changes = true;
            self.state.blocking_write().message =
                format!("Recovering autosave snapshot '{}'...", snapshot.display());
            let snapshot = snapshot.clone();
            return Task::perform(async move { snapshot }, Message::LoadSessionPath);
        }
        self.pending_autosave_recovery = None;
        self.pending_open_session_dir = None;
        self.state.blocking_write().message =
            "Autosave recovery failed: no snapshot available".to_string();
        Task::none()
    }
}
