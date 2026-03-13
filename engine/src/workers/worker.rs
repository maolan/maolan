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

    fn apply_offline_automation_at_sample(
        track: &mut crate::track::Track,
        sample: usize,
        lanes: &[OfflineAutomationLane],
    ) {
        for lane in lanes {
            let Some(value) = Self::automation_lane_value_at(&lane.points, sample) else {
                continue;
            };
            match lane.target {
                OfflineAutomationTarget::Volume => {
                    track.set_level((-90.0 + value * 110.0).clamp(-90.0, 20.0));
                }
                OfflineAutomationTarget::Balance => {
                    track.set_balance((value * 2.0 - 1.0).clamp(-1.0, 1.0));
                }
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

        let mut rendered = vec![0.0_f32; job.length_samples.saturating_mul(channels)];
        let mut cursor = 0usize;
        while cursor < job.length_samples {
            if job.cancel.load(std::sync::atomic::Ordering::Relaxed) {
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
                            Self::apply_offline_automation_at_sample(
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
                            Self::apply_offline_automation_at_sample(
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
