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

    pub(super) fn list_autosave_snapshots_for(
        path: &std::path::Path,
        branch: &str,
    ) -> Vec<std::path::PathBuf> {
        let snapshots_dir = Self::autosave_snapshots_dir_for(path);
        let branch_file = format!("{}.json", branch);
        let mut snapshots = fs::read_dir(snapshots_dir)
            .ok()
            .into_iter()
            .flat_map(|entries| entries.filter_map(|entry| entry.ok()))
            .map(|entry| entry.path())
            .filter(|snapshot_dir| {
                snapshot_dir.is_dir() && snapshot_dir.join(&branch_file).exists()
            })
            .collect::<Vec<_>>();
        snapshots.sort_by(|a, b| b.cmp(a));
        snapshots
    }

    pub(super) fn has_newer_autosave_snapshot(path: &std::path::Path, branch: &str) -> bool {
        let Some(autosave_session) = Self::list_autosave_snapshots_for(path, branch)
            .first()
            .cloned()
        else {
            return false;
        };
        let branch_file = format!("{}.json", branch);
        let autosave_mtime = fs::metadata(autosave_session.join(&branch_file))
            .and_then(|m| m.modified())
            .ok();
        let session_mtime = fs::metadata(path.join(&branch_file))
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
        branch: &str,
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

        let branch_file = format!("{}.json", branch);
        let live = read_counts(&session_dir.join(&branch_file));
        let snap = read_counts(&snapshot_dir.join(&branch_file));
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

    pub(super) fn prepare_pending_autosave_recovery(&mut self) -> Result<(), String> {
        if let Some(pending) = self.pending_autosave_recovery.as_mut() {
            pending.confirm_armed = false;
            return Ok(());
        }

        let base_session_dir = self
            .pending_recovery_session_dir
            .clone()
            .or_else(|| self.session_dir.clone())
            .ok_or_else(|| "No session available for autosave recovery".to_string())?;
        let branch = self.session_branch.clone();
        let snapshots = Self::list_autosave_snapshots_for(&base_session_dir, &branch);
        if snapshots.is_empty() {
            return Err("No autosave snapshot found for this session".to_string());
        }
        self.pending_autosave_recovery = Some(super::super::PendingAutosaveRecovery {
            session_dir: base_session_dir,
            snapshots,
            selected_index: 0,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_session_dir(label: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("maolan-{label}-{unique}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn list_autosave_snapshots_only_returns_valid_snapshot_dirs_in_reverse_order() {
        let session_dir = temp_session_dir("autosave-list");
        let snapshots_dir = Maolan::autosave_snapshots_dir_for(&session_dir);
        fs::create_dir_all(&snapshots_dir).unwrap();

        let older = snapshots_dir.join("20260101-010101");
        let newer = snapshots_dir.join("20260102-010101");
        let invalid = snapshots_dir.join("not-a-snapshot");
        fs::create_dir_all(&older).unwrap();
        fs::create_dir_all(&newer).unwrap();
        fs::create_dir_all(&invalid).unwrap();
        fs::write(older.join("main.json"), "{}").unwrap();
        fs::write(newer.join("main.json"), "{}").unwrap();

        let snapshots = Maolan::list_autosave_snapshots_for(&session_dir, "main");
        assert_eq!(snapshots, vec![newer, older]);
    }

    #[test]
    fn has_newer_autosave_snapshot_is_true_when_live_session_file_is_missing() {
        let session_dir = temp_session_dir("autosave-newer");
        let snapshot_dir = Maolan::autosave_snapshots_dir_for(&session_dir).join("20260102-010101");
        fs::create_dir_all(&snapshot_dir).unwrap();
        fs::write(snapshot_dir.join("main.json"), "{\"tracks\":[]}").unwrap();

        assert!(Maolan::has_newer_autosave_snapshot(&session_dir, "main"));
    }

    #[test]
    fn autosave_preview_summary_reports_track_and_clip_deltas() {
        let session_dir = temp_session_dir("autosave-preview-live");
        let snapshot_dir = temp_session_dir("autosave-preview-snap");
        fs::write(
            session_dir.join("main.json"),
            r#"{
                "tracks": [
                    { "audio": { "clips": [1] }, "midi": { "clips": [1, 2] } }
                ]
            }"#,
        )
        .unwrap();
        fs::write(
            snapshot_dir.join("main.json"),
            r#"{
                "tracks": [
                    { "audio": { "clips": [1, 2] }, "midi": { "clips": [] } },
                    { "audio": { "clips": [] }, "midi": { "clips": [1] } }
                ]
            }"#,
        )
        .unwrap();

        let summary =
            Maolan::autosave_recovery_preview_summary(&session_dir, &snapshot_dir, "main");
        assert!(summary.contains("tracks 1->2"));
        assert!(summary.contains("audio clips 1->2"));
        assert!(summary.contains("midi clips 2->1"));
    }
}
