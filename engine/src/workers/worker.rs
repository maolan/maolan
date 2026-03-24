use crate::message::{
    Action, Message, OfflineAutomationLane, OfflineAutomationPoint, OfflineAutomationTarget,
    OfflineBounceWork,
};
#[cfg(unix)]
use nix::libc;
use tokio::sync::mpsc::{Receiver, Sender};
use tracing::error;
use wavers::write as write_wav;

#[derive(Debug)]
pub struct Worker {
    id: usize,
    rx: Receiver<Message>,
    tx: Sender<Message>,
}

impl Worker {
    fn automation_lane_value_at(points: &[OfflineAutomationPoint], sample: usize) -> Option<f32> {
        if points.is_empty() {
            return None;
        }
        if sample <= points[0].sample {
            return Some(points[0].value.clamp(0.0, 1.0));
        }
        if sample >= points[points.len().saturating_sub(1)].sample {
            return Some(points[points.len().saturating_sub(1)].value.clamp(0.0, 1.0));
        }
        for segment in points.windows(2) {
            let left = &segment[0];
            let right = &segment[1];
            if sample < left.sample || sample > right.sample {
                continue;
            }
            let span = right.sample.saturating_sub(left.sample).max(1) as f32;
            let t = (sample.saturating_sub(left.sample) as f32 / span).clamp(0.0, 1.0);
            return Some((left.value + (right.value - left.value) * t).clamp(0.0, 1.0));
        }
        None
    }

    fn apply_freeze_automation_at_sample(
        track: &mut crate::track::Track,
        sample: usize,
        lanes: &[OfflineAutomationLane],
    ) {
        for lane in lanes {
            if matches!(
                lane.target,
                OfflineAutomationTarget::Volume | OfflineAutomationTarget::Balance
            ) {
                continue;
            }
            let Some(value) = Self::automation_lane_value_at(&lane.points, sample) else {
                continue;
            };
            match lane.target {
                OfflineAutomationTarget::Volume | OfflineAutomationTarget::Balance => {}
                OfflineAutomationTarget::Mute => {
                    track.set_muted(value >= 0.5);
                }
                #[cfg(all(unix, not(target_os = "macos")))]
                OfflineAutomationTarget::Lv2Parameter {
                    instance_id,
                    index,
                    min,
                    max,
                } => {
                    let lo = min.min(max);
                    let hi = max.max(min);
                    let param_value = (lo + value * (hi - lo)).clamp(lo, hi);
                    let _ = track.set_lv2_control_value(instance_id, index, param_value);
                }
                OfflineAutomationTarget::Vst3Parameter {
                    instance_id,
                    param_id,
                } => {
                    let _ = track.set_vst3_parameter(instance_id, param_id, value.clamp(0.0, 1.0));
                }
                OfflineAutomationTarget::ClapParameter {
                    instance_id,
                    param_id,
                    min,
                    max,
                } => {
                    let lo = min.min(max);
                    let hi = max.max(min);
                    let param_value = (lo + value as f64 * (hi - lo)).clamp(lo, hi);
                    let _ = track.set_clap_parameter_at(instance_id, param_id, param_value, 0);
                }
            }
        }
    }

    fn prepare_track_for_freeze_render(track: &mut crate::track::Track) -> (f32, f32) {
        let original_level = track.level();
        let original_balance = track.balance;
        track.set_level(0.0);
        track.set_balance(0.0);
        (original_level, original_balance)
    }

    fn restore_track_after_freeze_render(
        track: &mut crate::track::Track,
        original_level: f32,
        original_balance: f32,
    ) {
        track.set_level(original_level);
        track.set_balance(original_balance);
    }

    async fn process_offline_bounce(&self, job: OfflineBounceWork) {
        let track_handle = job.state.lock().tracks.get(&job.track_name).cloned();
        let Some(target_track) = track_handle else {
            let _ = self
                .tx
                .send(Message::OfflineBounceFinished {
                    result: Err(format!("Track not found: {}", job.track_name)),
                })
                .await;
            return;
        };
        let (channels, block_size, sample_rate) = {
            let t = target_track.lock();
            let block_size = t
                .audio
                .outs
                .first()
                .map(|io| io.buffer.lock().len())
                .or_else(|| t.audio.ins.first().map(|io| io.buffer.lock().len()))
                .unwrap_or(0)
                .max(1);
            (
                t.audio.outs.len().max(1),
                block_size,
                t.sample_rate.round().max(1.0) as i32,
            )
        };
        let (original_level, original_balance) = {
            let t = target_track.lock();
            Self::prepare_track_for_freeze_render(t)
        };

        let mut rendered = vec![0.0_f32; job.length_samples.saturating_mul(channels)];
        let mut cursor = 0usize;
        while cursor < job.length_samples {
            if job.cancel.load(std::sync::atomic::Ordering::Relaxed) {
                {
                    let t = target_track.lock();
                    Self::restore_track_after_freeze_render(t, original_level, original_balance);
                }
                let _ = self
                    .tx
                    .send(Message::OfflineBounceFinished {
                        result: Ok(Action::TrackOfflineBounceCanceled {
                            track_name: job.track_name.clone(),
                        }),
                    })
                    .await;
                let _ = self.tx.send(Message::Ready(self.id)).await;
                return;
            }

            let step = (job.length_samples - cursor).min(block_size);
            let tracks: Vec<_> = job.state.lock().tracks.values().cloned().collect();
            for handle in &tracks {
                let t = handle.lock();
                t.audio.finished = false;
                t.audio.processing = false;
                t.set_transport_sample(job.start_sample.saturating_add(cursor));
                t.set_loop_config(false, None);
                t.set_transport_timing(job.tempo_bpm, job.tsig_num, job.tsig_denom);
                t.set_clip_playback_enabled(true);
                t.set_record_tap_enabled(false);
            }

            let mut remaining = tracks.len();
            while remaining > 0 {
                let mut progressed = false;
                for handle in &tracks {
                    let t = handle.lock();
                    if t.audio.finished || t.audio.processing {
                        continue;
                    }
                    if t.audio.ready() {
                        if t.name == job.track_name {
                            Self::apply_freeze_automation_at_sample(
                                t,
                                job.start_sample.saturating_add(cursor),
                                &job.automation_lanes,
                            );
                        }
                        t.audio.processing = true;
                        t.process();
                        t.audio.processing = false;
                        progressed = true;
                        remaining = remaining.saturating_sub(1);
                    }
                }
                if !progressed {
                    for handle in &tracks {
                        let t = handle.lock();
                        if t.audio.finished {
                            continue;
                        }
                        if t.name == job.track_name {
                            Self::apply_freeze_automation_at_sample(
                                t,
                                job.start_sample.saturating_add(cursor),
                                &job.automation_lanes,
                            );
                        }
                        t.audio.processing = true;
                        t.process();
                        t.audio.processing = false;
                        remaining = remaining.saturating_sub(1);
                    }
                }
            }

            {
                let t = target_track.lock();
                for ch in 0..channels {
                    let out = t.audio.outs[ch].buffer.lock();
                    let copy_len = step.min(out.len());
                    for i in 0..copy_len {
                        let dst = (cursor + i) * channels + ch;
                        rendered[dst] = out[i];
                    }
                }
            }

            cursor = cursor.saturating_add(step);
            let _ = self
                .tx
                .send(Message::OfflineBounceFinished {
                    result: Ok(Action::TrackOfflineBounceProgress {
                        track_name: job.track_name.clone(),
                        progress: (cursor as f32 / job.length_samples as f32).clamp(0.0, 1.0),
                        operation: Some("Rendering freeze".to_string()),
                    }),
                })
                .await;
        }

        if let Err(e) =
            write_wav::<f32, _>(&job.output_path, &rendered, sample_rate, channels as u16)
        {
            {
                let t = target_track.lock();
                Self::restore_track_after_freeze_render(t, original_level, original_balance);
            }
            let _ = self
                .tx
                .send(Message::OfflineBounceFinished {
                    result: Err(format!(
                        "Failed to write offline bounce '{}': {e}",
                        job.output_path
                    )),
                })
                .await;
            let _ = self.tx.send(Message::Ready(self.id)).await;
            return;
        }

        {
            let t = target_track.lock();
            Self::restore_track_after_freeze_render(t, original_level, original_balance);
        }

        let _ = self
            .tx
            .send(Message::OfflineBounceFinished {
                result: Ok(Action::TrackOfflineBounce {
                    track_name: job.track_name,
                    output_path: job.output_path,
                    start_sample: job.start_sample,
                    length_samples: job.length_samples,
                    automation_lanes: vec![],
                }),
            })
            .await;
        let _ = self.tx.send(Message::Ready(self.id)).await;
    }

    #[cfg(unix)]
    fn try_enable_realtime() -> Result<(), String> {
        let thread = unsafe { libc::pthread_self() };
        let policy = libc::SCHED_FIFO;
        let param = unsafe {
            let mut p = std::mem::zeroed::<libc::sched_param>();
            p.sched_priority = 10;
            p
        };
        let rc = unsafe { libc::pthread_setschedparam(thread, policy, &param) };
        if rc == 0 {
            Ok(())
        } else {
            Err(format!("pthread_setschedparam failed with errno {}", rc))
        }
    }

    #[cfg(not(unix))]
    fn try_enable_realtime() -> Result<(), String> {
        Err("Realtime thread priority is not supported on this platform".to_string())
    }

    pub async fn new(id: usize, rx: Receiver<Message>, tx: Sender<Message>) -> Worker {
        let worker = Worker { id, rx, tx };
        worker.send(Message::Ready(id)).await;
        worker
    }

    pub async fn send(&self, message: Message) {
        self.tx
            .send(message)
            .await
            .expect("Failed to send message from worker");
    }

    pub async fn work(&mut self) {
        if let Err(e) = Self::try_enable_realtime() {
            error!("Worker {} realtime priority not enabled: {}", self.id, e);
        }
        while let Some(message) = self.rx.recv().await {
            match message {
                Message::Request(Action::Quit) => {
                    return;
                }
                Message::ProcessTrack(t) => {
                    let (track_name, output_linear, process_epoch) = {
                        let track = t.lock();
                        let process_epoch = track.process_epoch;
                        track.process();
                        track.audio.processing = false;
                        (
                            track.name.clone(),
                            track.output_meter_linear(),
                            process_epoch,
                        )
                    };
                    match self
                        .tx
                        .send(Message::Finished {
                            worker_id: self.id,
                            track_name,
                            output_linear,
                            process_epoch,
                        })
                        .await
                    {
                        Ok(_) => {}
                        Err(e) => {
                            error!("Error while sending Finished: {}", e);
                        }
                    }
                }
                Message::ProcessOfflineBounce(job) => {
                    self.process_offline_bounce(job).await;
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Worker;
    use crate::message::{
        Action, Message, OfflineAutomationLane, OfflineAutomationPoint, OfflineAutomationTarget,
        OfflineBounceWork,
    };
    use crate::mutex::UnsafeMutex;
    use crate::state::State;
    use crate::track::Track;
    use std::path::PathBuf;
    use std::sync::{Arc, atomic::AtomicBool};
    use std::time::{SystemTime, UNIX_EPOCH};
    use tokio::sync::mpsc::channel;

    fn make_state_with_track(track: Track) -> Arc<UnsafeMutex<State>> {
        let mut state = State::default();
        state.tracks.insert(
            track.name.clone(),
            Arc::new(UnsafeMutex::new(Box::new(track))),
        );
        Arc::new(UnsafeMutex::new(state))
    }

    fn unique_temp_wav(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("maolan_{name}_{nanos}.wav"))
    }

    #[test]
    fn prepare_track_for_freeze_render_neutralizes_level_and_balance() {
        let mut track = Track::new("track".to_string(), 1, 2, 0, 0, 64, 48_000.0);
        track.set_level(-6.0);
        track.set_balance(0.35);

        let (level, balance) = Worker::prepare_track_for_freeze_render(&mut track);

        assert_eq!(level, -6.0);
        assert_eq!(balance, 0.35);
        assert_eq!(track.level(), 0.0);
        assert_eq!(track.balance, 0.0);

        Worker::restore_track_after_freeze_render(&mut track, level, balance);
        assert_eq!(track.level(), -6.0);
        assert_eq!(track.balance, 0.35);
    }

    #[test]
    fn freeze_automation_ignores_volume_and_balance_lanes() {
        let mut track = Track::new("track".to_string(), 1, 2, 0, 0, 64, 48_000.0);
        let lanes = vec![
            OfflineAutomationLane {
                target: OfflineAutomationTarget::Volume,
                points: vec![OfflineAutomationPoint {
                    sample: 0,
                    value: 0.0,
                }],
            },
            OfflineAutomationLane {
                target: OfflineAutomationTarget::Balance,
                points: vec![OfflineAutomationPoint {
                    sample: 0,
                    value: 1.0,
                }],
            },
            OfflineAutomationLane {
                target: OfflineAutomationTarget::Mute,
                points: vec![OfflineAutomationPoint {
                    sample: 0,
                    value: 1.0,
                }],
            },
        ];

        Worker::apply_freeze_automation_at_sample(&mut track, 0, &lanes);

        assert_eq!(track.level(), 0.0);
        assert_eq!(track.balance, 0.0);
        assert!(track.muted);
    }

    #[test]
    fn automation_lane_value_at_interpolates_between_points() {
        let value = Worker::automation_lane_value_at(
            &[
                OfflineAutomationPoint {
                    sample: 10,
                    value: 0.25,
                },
                OfflineAutomationPoint {
                    sample: 20,
                    value: 0.75,
                },
            ],
            15,
        )
        .expect("value");

        assert!((value - 0.5).abs() < 1.0e-6);
    }

    #[test]
    fn freeze_automation_applies_interpolated_mute_lane() {
        let mut track = Track::new("track".to_string(), 1, 1, 0, 0, 64, 48_000.0);
        let lanes = vec![OfflineAutomationLane {
            target: OfflineAutomationTarget::Mute,
            points: vec![
                OfflineAutomationPoint {
                    sample: 0,
                    value: 0.0,
                },
                OfflineAutomationPoint {
                    sample: 10,
                    value: 1.0,
                },
            ],
        }];

        Worker::apply_freeze_automation_at_sample(&mut track, 5, &lanes);
        assert!(track.muted);

        track.set_muted(false);
        Worker::apply_freeze_automation_at_sample(&mut track, 2, &lanes);
        assert!(!track.muted);
    }

    #[tokio::test]
    async fn process_offline_bounce_errors_when_track_is_missing() {
        let (_rx_unused_tx, rx_unused) = channel(1);
        let (tx, mut out_rx) = channel(8);
        let worker = Worker {
            id: 7,
            rx: rx_unused,
            tx,
        };
        let job = OfflineBounceWork {
            state: Arc::new(UnsafeMutex::new(State::default())),
            track_name: "missing".to_string(),
            output_path: unique_temp_wav("missing").to_string_lossy().to_string(),
            start_sample: 0,
            length_samples: 8,
            tempo_bpm: 120.0,
            tsig_num: 4,
            tsig_denom: 4,
            automation_lanes: vec![],
            cancel: Arc::new(AtomicBool::new(false)),
        };

        worker.process_offline_bounce(job).await;

        match out_rx.recv().await.expect("message") {
            Message::OfflineBounceFinished { result: Err(err) } => {
                assert!(err.contains("Track not found: missing"));
            }
            other => panic!("unexpected message: {other:?}"),
        }
    }

    #[tokio::test]
    async fn process_offline_bounce_cancels_and_restores_track_state() {
        let (_rx_unused_tx, rx_unused) = channel(1);
        let (tx, mut out_rx) = channel(8);
        let worker = Worker {
            id: 5,
            rx: rx_unused,
            tx,
        };
        let mut track = Track::new("track".to_string(), 1, 2, 0, 0, 4, 48_000.0);
        track.set_level(-9.0);
        track.set_balance(-0.3);
        let state = make_state_with_track(track);
        let job = OfflineBounceWork {
            state: state.clone(),
            track_name: "track".to_string(),
            output_path: unique_temp_wav("cancel").to_string_lossy().to_string(),
            start_sample: 0,
            length_samples: 8,
            tempo_bpm: 120.0,
            tsig_num: 4,
            tsig_denom: 4,
            automation_lanes: vec![],
            cancel: Arc::new(AtomicBool::new(true)),
        };

        worker.process_offline_bounce(job).await;

        match out_rx.recv().await.expect("message") {
            Message::OfflineBounceFinished {
                result: Ok(Action::TrackOfflineBounceCanceled { track_name }),
            } => assert_eq!(track_name, "track"),
            other => panic!("unexpected message: {other:?}"),
        }
        assert!(matches!(out_rx.recv().await, Some(Message::Ready(5))));
        let track = state.lock().tracks.get("track").expect("track").lock();
        assert_eq!(track.level(), -9.0);
        assert_eq!(track.balance, -0.3);
    }

    #[tokio::test]
    async fn process_offline_bounce_restores_track_state_on_write_failure() {
        let (_rx_unused_tx, rx_unused) = channel(1);
        let (tx, mut out_rx) = channel(8);
        let worker = Worker {
            id: 3,
            rx: rx_unused,
            tx,
        };
        let mut track = Track::new("track".to_string(), 1, 2, 0, 0, 4, 48_000.0);
        track.set_level(-4.0);
        track.set_balance(0.25);
        let state = make_state_with_track(track);
        let output_path = std::env::temp_dir().to_string_lossy().to_string();
        let job = OfflineBounceWork {
            state: state.clone(),
            track_name: "track".to_string(),
            output_path,
            start_sample: 0,
            length_samples: 4,
            tempo_bpm: 120.0,
            tsig_num: 4,
            tsig_denom: 4,
            automation_lanes: vec![],
            cancel: Arc::new(AtomicBool::new(false)),
        };

        worker.process_offline_bounce(job).await;

        let mut saw_error = false;
        while let Some(message) = out_rx.recv().await {
            match message {
                Message::OfflineBounceFinished {
                    result: Ok(Action::TrackOfflineBounceProgress { .. }),
                } => {}
                Message::OfflineBounceFinished { result: Err(err) } => {
                    assert!(err.contains("Failed to write offline bounce"));
                    saw_error = true;
                }
                Message::Ready(3) => break,
                other => panic!("unexpected message: {other:?}"),
            }
        }
        assert!(saw_error);
        let track = state.lock().tracks.get("track").expect("track").lock();
        assert_eq!(track.level(), -4.0);
        assert_eq!(track.balance, 0.25);
    }

    #[tokio::test]
    async fn process_offline_bounce_emits_progress_and_completion() {
        let (_rx_unused_tx, rx_unused) = channel(1);
        let (tx, mut out_rx) = channel(16);
        let worker = Worker {
            id: 2,
            rx: rx_unused,
            tx,
        };
        let mut track = Track::new("track".to_string(), 1, 1, 0, 0, 4, 48_000.0);
        track.set_level(-3.0);
        track.set_balance(0.4);
        let state = make_state_with_track(track);
        let output = unique_temp_wav("success");
        let job = OfflineBounceWork {
            state: state.clone(),
            track_name: "track".to_string(),
            output_path: output.to_string_lossy().to_string(),
            start_sample: 0,
            length_samples: 8,
            tempo_bpm: 120.0,
            tsig_num: 4,
            tsig_denom: 4,
            automation_lanes: vec![],
            cancel: Arc::new(AtomicBool::new(false)),
        };

        worker.process_offline_bounce(job).await;

        let mut saw_progress = false;
        let mut saw_complete = false;
        let mut saw_ready = false;
        while let Some(message) = out_rx.recv().await {
            match message {
                Message::OfflineBounceFinished {
                    result:
                        Ok(Action::TrackOfflineBounceProgress {
                            track_name,
                            progress,
                            ..
                        }),
                } => {
                    assert_eq!(track_name, "track");
                    assert!(progress > 0.0);
                    saw_progress = true;
                }
                Message::OfflineBounceFinished {
                    result:
                        Ok(Action::TrackOfflineBounce {
                            track_name,
                            output_path,
                            ..
                        }),
                } => {
                    assert_eq!(track_name, "track");
                    assert_eq!(output_path, output.to_string_lossy());
                    saw_complete = true;
                }
                Message::Ready(2) => {
                    saw_ready = true;
                    break;
                }
                other => panic!("unexpected message: {other:?}"),
            }
        }

        assert!(saw_progress);
        assert!(saw_complete);
        assert!(saw_ready);
        assert!(output.exists());
        std::fs::remove_file(&output).expect("remove temp wav");
        let track = state.lock().tracks.get("track").expect("track").lock();
        assert_eq!(track.level(), -3.0);
        assert_eq!(track.balance, 0.4);
        assert!(!track.muted);
    }
}
