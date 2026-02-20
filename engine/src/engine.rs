use std::{
    collections::VecDeque,
    fs::File,
    fs::read_dir,
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use midly::{
    Arena, Format, Header, MetaMessage, Smf, Timing, TrackEvent, TrackEventKind,
    live::LiveEvent,
    num::{u15, u24, u28},
};
use tokio::sync::mpsc::{Receiver, Sender, channel};
use tokio::task::JoinHandle;
use wavers::write as write_wav;

use crate::{
    audio::clip::AudioClip,
    hw::oss::{self, MidiHub},
    kind::Kind,
    message::{Action, Message},
    midi::clip::MIDIClip,
    midi::io::MidiEvent,
    mutex::UnsafeMutex,
    oss_worker::OssWorker,
    routing,
    state::State,
    track::Track,
    worker::Worker,
};

const VU_METERS_ENABLED: bool = false;

#[derive(Debug)]
struct WorkerData {
    tx: Sender<Message>,
    handle: JoinHandle<()>,
}

impl WorkerData {
    pub fn new(tx: Sender<Message>, handle: JoinHandle<()>) -> Self {
        Self { tx, handle }
    }
}

#[derive(Debug, Clone)]
struct RecordingSession {
    start_sample: usize,
    samples: Vec<f32>,
    channels: usize,
    file_name: String,
}

#[derive(Debug, Clone)]
struct MidiRecordingSession {
    start_sample: usize,
    events: Vec<(u64, Vec<u8>)>,
    file_name: String,
}

pub struct Engine {
    clients: Vec<Sender<Message>>,
    rx: Receiver<Message>,
    state: Arc<UnsafeMutex<State>>,
    tx: Sender<Message>,
    workers: Vec<WorkerData>,
    oss_in: Option<Arc<UnsafeMutex<oss::Audio>>>,
    oss_out: Option<Arc<UnsafeMutex<oss::Audio>>>,
    midi_hub: Arc<UnsafeMutex<MidiHub>>,
    oss_worker: Option<WorkerData>,
    pending_hw_midi_events: Vec<MidiEvent>,
    pending_hw_midi_out_events: Vec<MidiEvent>,
    ready_workers: Vec<usize>,
    pending_requests: VecDeque<Action>,
    awaiting_hwfinished: bool,
    transport_sample: usize,
    audio_recordings: std::collections::HashMap<String, RecordingSession>,
    midi_recordings: std::collections::HashMap<String, MidiRecordingSession>,
    playing: bool,
    record_enabled: bool,
    session_dir: Option<PathBuf>,
    hw_out_level_db: f32,
    hw_out_muted: bool,
}

impl Engine {
    fn session_plugins_dir(&self) -> Option<PathBuf> {
        self.session_dir.as_ref().map(|d| d.join("plugins"))
    }

    fn session_audio_dir(&self) -> Option<PathBuf> {
        self.session_dir.as_ref().map(|d| d.join("audio"))
    }

    fn session_midi_dir(&self) -> Option<PathBuf> {
        self.session_dir.as_ref().map(|d| d.join("midi"))
    }

    fn ensure_session_subdirs(&self) {
        if let Some(root) = &self.session_dir {
            let _ = std::fs::create_dir_all(root.join("plugins"));
            let _ = std::fs::create_dir_all(root.join("audio"));
            let _ = std::fs::create_dir_all(root.join("midi"));
        }
    }

    fn discover_midi_hw_devices() -> Vec<String> {
        let mut devices: Vec<String> = read_dir("/dev")
            .map(|rd| {
                rd.filter_map(Result::ok)
                    .map(|e| e.path())
                    .filter_map(|path| {
                        let name = path.file_name()?.to_str()?;
                        if name.starts_with("umidi") || name.starts_with("midi") {
                            Some(path.to_string_lossy().into_owned())
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        devices.sort();
        devices.dedup();
        devices
    }

    pub fn new(rx: Receiver<Message>, tx: Sender<Message>) -> Self {
        Self {
            rx,
            tx,
            clients: vec![],
            state: Arc::new(UnsafeMutex::new(State::default())),
            workers: vec![],
            oss_in: None,
            oss_out: None,
            midi_hub: Arc::new(UnsafeMutex::new(MidiHub::default())),
            oss_worker: None,
            pending_hw_midi_events: vec![],
            pending_hw_midi_out_events: vec![],
            ready_workers: vec![],
            pending_requests: VecDeque::new(),
            awaiting_hwfinished: false,
            transport_sample: 0,
            audio_recordings: std::collections::HashMap::new(),
            midi_recordings: std::collections::HashMap::new(),
            playing: false,
            record_enabled: false,
            session_dir: None,
            hw_out_level_db: 0.0,
            hw_out_muted: false,
        }
    }

    fn current_cycle_samples(&self) -> usize {
        self.oss_out
            .as_ref()
            .map(|o| o.lock().chsamples)
            .or_else(|| self.oss_in.as_ref().map(|i| i.lock().chsamples))
            .unwrap_or(0)
    }

    fn apply_mute_solo_policy(&self) {
        let tracks = &self.state.lock().tracks;
        let any_soloed = tracks.values().any(|t| t.lock().soloed);
        for track in tracks.values() {
            let t = track.lock();
            let enabled = if any_soloed {
                t.soloed && !t.muted
            } else {
                !t.muted
            };
            t.set_output_enabled(enabled);
        }
    }

    fn sanitize_file_stem(name: &str) -> String {
        let mut out = String::with_capacity(name.len());
        for c in name.chars() {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                out.push(c);
            } else {
                out.push('_');
            }
        }
        if out.is_empty() {
            "track".to_string()
        } else {
            out
        }
    }

    fn next_recording_file_name(track_name: &str) -> String {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        format!("{}_{}.wav", Self::sanitize_file_stem(track_name), ts)
    }

    fn next_midi_recording_file_name(track_name: &str) -> String {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        format!("{}_{}.mid", Self::sanitize_file_stem(track_name), ts)
    }

    fn append_recorded_cycle(&mut self) {
        if !self.playing || !self.record_enabled {
            return;
        }
        for (name, track_handle) in &self.state.lock().tracks {
            let track = track_handle.lock();
            if !track.armed {
                continue;
            }
            if track.record_tap_outs.is_empty() {
                continue;
            }
            let channels = track.record_tap_outs.len();
            let frames = track.record_tap_outs[0].len();
            if frames == 0 {
                continue;
            }
            let entry = self.audio_recordings.entry(name.clone()).or_insert_with(|| RecordingSession {
                start_sample: self.transport_sample,
                samples: Vec::with_capacity(frames * channels * 4),
                channels,
                file_name: Self::next_recording_file_name(name),
            });
            if entry.channels != channels {
                continue;
            }
            for frame in 0..frames {
                for ch in 0..channels {
                    entry.samples.push(track.record_tap_outs[ch][frame]);
                }
            }

            let midi_entry = self
                .midi_recordings
                .entry(name.clone())
                .or_insert_with(|| MidiRecordingSession {
                    start_sample: self.transport_sample,
                    events: Vec::new(),
                    file_name: Self::next_midi_recording_file_name(name),
                });
            for event in &track.record_tap_midi_in {
                let abs_sample = self.transport_sample as u64 + event.frame as u64;
                midi_entry.events.push((abs_sample, event.data.clone()));
            }
        }
    }

    async fn flush_recordings(&mut self) {
        let Some(audio_dir) = self.session_audio_dir() else {
            if !self.audio_recordings.is_empty() || !self.midi_recordings.is_empty() {
                self.notify_clients(Err("Recording stopped: session path is not set".to_string()))
                    .await;
            }
            self.audio_recordings.clear();
            self.midi_recordings.clear();
            return;
        };
        if std::fs::create_dir_all(&audio_dir).is_err() {
            self.notify_clients(Err(format!(
                "Recording stopped: failed to create audio directory {}",
                audio_dir.display()
            )))
            .await;
            self.audio_recordings.clear();
            self.midi_recordings.clear();
            return;
        }
        let Some(midi_dir) = self.session_midi_dir() else {
            self.audio_recordings.clear();
            self.midi_recordings.clear();
            return;
        };
        if std::fs::create_dir_all(&midi_dir).is_err() {
            self.audio_recordings.clear();
            self.midi_recordings.clear();
            return;
        }
        let rate = self
            .oss_out
            .as_ref()
            .map(|o| o.lock().rate)
            .unwrap_or(48_000);
        let recordings = std::mem::take(&mut self.audio_recordings);
        for (track_name, rec) in recordings {
            self.flush_recording_entry(&audio_dir, rate, track_name, rec)
                .await;
        }
        let midi_recordings = std::mem::take(&mut self.midi_recordings);
        for (track_name, rec) in midi_recordings {
            self.flush_midi_recording_entry(&midi_dir, rate as u32, track_name, rec)
                .await;
        }
    }

    async fn flush_recording_entry(
        &mut self,
        audio_dir: &Path,
        rate: i32,
        track_name: String,
        rec: RecordingSession,
    ) {
        if rec.samples.is_empty() || rec.channels == 0 {
            return;
        }
        let file_path = audio_dir.join(&rec.file_name);
        if let Err(e) = write_wav::<f32, _>(&file_path, &rec.samples, rate, rec.channels as u16) {
            self.notify_clients(Err(format!(
                "Failed to write recording {}: {}",
                file_path.display(),
                e
            )))
            .await;
            return;
        }
        let length = rec.samples.len() / rec.channels;
        let clip_rel_name = format!("audio/{}", rec.file_name);
        let clip = AudioClip::new(clip_rel_name.clone(), rec.start_sample, length);
        if let Some(track) = self.state.lock().tracks.get(&track_name) {
            track.lock().audio.clips.push(clip);
        }
        self.notify_clients(Ok(Action::AddClip {
            name: clip_rel_name,
            track_name: track_name.clone(),
            start: rec.start_sample,
            length,
            offset: 0,
            kind: Kind::Audio,
        }))
        .await;
    }

    async fn flush_track_recording(&mut self, track_name: &str) {
        let Some(rec) = self.audio_recordings.remove(track_name) else {
            if let Some(mrec) = self.midi_recordings.remove(track_name)
                && let Some(midi_dir) = self.session_midi_dir()
            {
                let _ = std::fs::create_dir_all(&midi_dir);
                let rate = self
                    .oss_out
                    .as_ref()
                    .map(|o| o.lock().rate as u32)
                    .unwrap_or(48_000);
                self.flush_midi_recording_entry(&midi_dir, rate, track_name.to_string(), mrec)
                    .await;
            }
            return;
        };
        let Some(audio_dir) = self.session_audio_dir() else {
            return;
        };
        if std::fs::create_dir_all(&audio_dir).is_err() {
            return;
        }
        let rate = self
            .oss_out
            .as_ref()
            .map(|o| o.lock().rate)
            .unwrap_or(48_000);
        self.flush_recording_entry(&audio_dir, rate, track_name.to_string(), rec)
            .await;
        if let Some(mrec) = self.midi_recordings.remove(track_name)
            && let Some(midi_dir) = self.session_midi_dir()
        {
            let _ = std::fs::create_dir_all(&midi_dir);
            self.flush_midi_recording_entry(&midi_dir, rate as u32, track_name.to_string(), mrec)
                .await;
        }
    }

    async fn flush_midi_recording_entry(
        &mut self,
        midi_dir: &Path,
        sample_rate: u32,
        track_name: String,
        mut rec: MidiRecordingSession,
    ) {
        if rec.events.is_empty() {
            return;
        }
        rec.events.sort_by_key(|(sample, _)| *sample);
        let path = midi_dir.join(&rec.file_name);
        if let Err(e) = Self::write_midi_file(&path, sample_rate, &rec.events) {
            self.notify_clients(Err(format!(
                "Failed to write MIDI recording {}: {}",
                path.display(),
                e
            )))
            .await;
            return;
        }
        let clip_rel_name = format!("midi/{}", rec.file_name);
        let clip_len_samples = rec
            .events
            .last()
            .map(|(s, _)| s.saturating_sub(rec.start_sample as u64) as usize + 1)
            .unwrap_or(1);
        let mut clip = MIDIClip::new(clip_rel_name.clone(), rec.start_sample, clip_len_samples);
        clip.offset = 0;
        if let Some(track) = self.state.lock().tracks.get(&track_name) {
            track.lock().midi.clips.push(clip);
        }
        self.notify_clients(Ok(Action::AddClip {
            name: clip_rel_name,
            track_name,
            start: rec.start_sample,
            length: clip_len_samples,
            offset: 0,
            kind: Kind::MIDI,
        }))
        .await;
    }

    fn write_midi_file(
        path: &Path,
        sample_rate: u32,
        events: &[(u64, Vec<u8>)],
    ) -> Result<(), String> {
        let ppq: u16 = 480;
        let ticks_per_second: u64 = 960; // 120 BPM at 480 PPQ
        let arena = Arena::new();
        let mut track_events: Vec<TrackEvent<'_>> = vec![TrackEvent {
            delta: u28::new(0),
            kind: TrackEventKind::Meta(MetaMessage::Tempo(u24::new(500_000))),
        }];
        let mut prev_ticks = 0_u64;
        for (sample, data) in events {
            let ticks = sample.saturating_mul(ticks_per_second) / sample_rate.max(1) as u64;
            let delta = ticks.saturating_sub(prev_ticks).min(u32::MAX as u64) as u32;
            prev_ticks = ticks;
            let Ok(live) = LiveEvent::parse(data) else {
                continue;
            };
            let kind = live.as_track_event(&arena);
            track_events.push(TrackEvent {
                delta: u28::new(delta),
                kind,
            });
        }
        track_events.push(TrackEvent {
            delta: u28::new(0),
            kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
        });

        let smf = Smf {
            header: Header::new(Format::SingleTrack, Timing::Metrical(u15::new(ppq))),
            tracks: vec![track_events],
        };
        let mut file = File::create(path).map_err(|e| e.to_string())?;
        smf.write_std(&mut file).map_err(|e| e.to_string())
    }

    pub async fn init(&mut self) {
        let max_threads = num_cpus::get();
        for id in 0..max_threads {
            let (tx, rx) = channel::<Message>(32);
            let tx_thread = self.tx.clone();
            let handler = tokio::spawn(async move {
                let wrk = Worker::new(id, rx, tx_thread);
                wrk.await.work().await;
            });
            self.workers.push(WorkerData::new(tx.clone(), handler));
        }
    }

    async fn notify_clients(&self, action: Result<Action, String>) {
        for client in &self.clients {
            client
                .send(Message::Response(action.clone()))
                .await
                .expect("Error sending response to client");
        }
    }

    async fn request_hw_cycle(&mut self) {
        if self.awaiting_hwfinished {
            return;
        }
        self.apply_hw_out_gain_and_meter().await;
        if let Some(worker) = &self.oss_worker {
            if !self.pending_hw_midi_out_events.is_empty() {
                let out_events = std::mem::take(&mut self.pending_hw_midi_out_events);
                if let Err(e) = worker.tx.send(Message::HWMidiOutEvents(out_events)).await {
                    eprintln!("Error sending HWMidiOutEvents {e}");
                }
            }
            match worker.tx.send(Message::TracksFinished).await {
                Ok(_) => {
                    self.awaiting_hwfinished = true;
                }
                Err(e) => {
                    eprintln!("Error sending TracksFinished {e}");
                }
            }
        }
    }

    async fn apply_hw_out_gain_and_meter(&self) {
        let Some(oss_out) = &self.oss_out else {
            return;
        };
        let gain = if self.hw_out_muted {
            0.0
        } else {
            10.0_f32.powf(self.hw_out_level_db / 20.0)
        };
        {
            let hw_out = oss_out.lock();
            for channel in &hw_out.channels {
                let buf = channel.buffer.lock();
                for sample in buf.iter_mut() {
                    *sample *= gain;
                }
            }
        }
        if !VU_METERS_ENABLED {
            return;
        }
        let meter_db = {
            let hw_out = oss_out.lock();
            hw_out
                .channels
                .iter()
                .map(|channel| {
                    let buf = channel.buffer.lock();
                    let peak = buf.iter().fold(0.0_f32, |acc, sample| acc.max(sample.abs()));
                    if peak <= 1.0e-6 {
                        -90.0
                    } else {
                        (20.0 * peak.log10()).clamp(-90.0, 6.0)
                    }
                })
                .collect()
        };
        self.notify_clients(Ok(Action::TrackMeters {
            track_name: "hw:out".to_string(),
            output_db: meter_db,
        }))
        .await;
    }

    async fn send_tracks(&mut self) -> bool {
        let mut finished = true;
        for track in self.state.lock().tracks.values() {
            let t = track.lock();
            finished &= t.audio.finished;
            if !t.audio.finished && !t.audio.processing && t.audio.ready() {
                if self.ready_workers.is_empty() {
                    return false;
                }
                let worker_index = self.ready_workers.remove(0);
                t.audio.processing = true;
                let worker = &self.workers[worker_index];
                if let Err(e) = worker.tx.send(Message::ProcessTrack(track.clone())).await {
                    t.audio.processing = false;
                    self.notify_clients(Err(format!("Failed to send track to worker: {}", e)))
                        .await;
                }
            }
        }
        finished
    }

    async fn publish_track_meters(&self) {
        if !VU_METERS_ENABLED {
            return;
        }
        let meters: Vec<(String, Vec<f32>)> = self
            .state
            .lock()
            .tracks
            .iter()
            .map(|(name, track)| (name.clone(), track.lock().output_meter_db()))
            .collect();

        for (track_name, output_db) in meters {
            self.notify_clients(Ok(Action::TrackMeters {
                track_name,
                output_db,
            }))
            .await;
        }
    }

    pub fn check_if_leads_to_kind(
        &self,
        kind: Kind,
        current_track_name: &str,
        target_track_name: &str,
    ) -> bool {
        routing::would_create_cycle(
            &target_track_name.to_string(),
            &current_track_name.to_string(),
            |track_name| self.connected_neighbors(kind, track_name),
        )
    }

    fn connected_neighbors(&self, kind: Kind, current_track_name: &str) -> Vec<String> {
        let state = self.state.lock();
        let mut found_neighbors = Vec::new();

        if let Some(current_track_handle) = state.tracks.get(current_track_name) {
            let current_track = current_track_handle.lock();

            match kind {
                Kind::Audio => {
                    for out_port in &current_track.audio.outs {
                        let conns = out_port.connections.lock();
                        for conn in conns.iter() {
                            for (name, next_track_handle) in &state.tracks {
                                let next_track = next_track_handle.lock();
                                let is_connected =
                                    next_track.audio.ins.iter().any(|ins_port| {
                                        Arc::ptr_eq(&ins_port.buffer, &conn.buffer)
                                    });

                                if is_connected {
                                    found_neighbors.push(name.clone());
                                }
                            }
                        }
                    }
                }
                Kind::MIDI => {
                    for out_port in &current_track.midi.outs {
                        let conns = out_port.lock().connections.clone();
                        for conn in conns.iter() {
                            for (name, next_track_handle) in &state.tracks {
                                let next_track = next_track_handle.lock();
                                let is_connected = next_track
                                    .midi
                                    .ins
                                    .iter()
                                    .any(|ins_port| Arc::ptr_eq(ins_port, conn));

                                if is_connected {
                                    found_neighbors.push(name.clone());
                                }
                            }
                        }
                    }
                }
            }
        }
        found_neighbors
    }

    async fn handle_request(&mut self, a: Action) {
        match a {
            Action::Play => {
                self.playing = true;
                self.notify_clients(Ok(Action::TransportPosition(self.transport_sample)))
                    .await;
            }
            Action::Stop => {
                self.playing = false;
                self.flush_recordings().await;
                self.notify_clients(Ok(Action::TransportPosition(self.transport_sample)))
                    .await;
            }
            Action::TransportPosition(_) => {}
            Action::SetRecordEnabled(enabled) => {
                self.record_enabled = enabled;
                if !enabled {
                    self.flush_recordings().await;
                } else if self.session_dir.is_none() {
                    self.notify_clients(Err(
                        "Recording enabled but session path is not set".to_string(),
                    ))
                    .await;
                }
            }
            Action::SetSessionPath(ref path) => {
                self.session_dir = Some(Path::new(path).to_path_buf());
                self.ensure_session_subdirs();
                let lv2_dir = self.session_plugins_dir();
                for track in self.state.lock().tracks.values() {
                    track.lock().set_lv2_state_base_dir(lv2_dir.clone());
                }
            }
            Action::Quit => {
                self.flush_recordings().await;
                if let Some(worker) = self.oss_worker.take() {
                    worker
                        .tx
                        .send(Message::Request(a.clone()))
                        .await
                        .expect("Failed sending quit message to OSS worker");
                    worker
                        .handle
                        .await
                        .expect("Failed waiting for OSS worker to quit");
                }

                // Then shut down regular workers
                while !self.workers.is_empty() {
                    let worker = self.workers.remove(0);
                    worker
                        .tx
                        .send(Message::Request(a.clone()))
                        .await
                        .expect("Failed sending quit message to worker");
                    worker
                        .handle
                        .await
                        .expect("Failed waiting for worker to quit");
                }
            }
            Action::AddTrack {
                ref name,
                audio_ins,
                midi_ins,
                audio_outs,
                midi_outs,
            } => {
                let tracks = &mut self.state.lock().tracks;
                if tracks.contains_key(name) {
                    self.notify_clients(Err(format!("Track {} already exists", name)))
                        .await;
                    return;
                }
                match &self.oss_out {
                    Some(oss) => {
                        let chsamples = oss.lock().chsamples;
                        tracks.insert(
                            name.clone(),
                            Arc::new(UnsafeMutex::new(Box::new(Track::new(
                                name.clone(),
                                audio_ins,
                                audio_outs,
                                midi_ins,
                                midi_outs,
                                chsamples,
                                oss.lock().rate as f64,
                            )))),
                        );
                        if let Some(track) = tracks.get(name) {
                            track.lock().ensure_default_audio_passthrough();
                            track.lock().ensure_default_midi_passthrough();
                            let lv2_dir = self.session_plugins_dir();
                            track.lock().set_lv2_state_base_dir(lv2_dir);
                        }
                    }
                    None => {
                        self.notify_clients(Err(
                            "Engine needs to open audio device before adding audio track"
                                .to_string(),
                        ))
                        .await;
                    }
                }
            }
            Action::RemoveTrack(ref name) => {
                self.state.lock().tracks.remove(name);
                self.audio_recordings.remove(name);
                self.midi_recordings.remove(name);
            }
            Action::TrackLevel(ref name, level) => {
                if name == "hw:out" {
                    self.hw_out_level_db = level;
                } else if let Some(track) = self.state.lock().tracks.get(name) {
                    track.lock().set_level(level);
                }
            }
            Action::TrackMeters { .. } => {}
            Action::TrackToggleArm(ref name) => {
                if let Some(track) = self.state.lock().tracks.get(name).cloned() {
                    track.lock().arm();
                    if !track.lock().armed
                        && self.audio_recordings.contains_key(name)
                    {
                        self.flush_track_recording(name).await;
                    }
                }
            }
            Action::TrackToggleMute(ref name) => {
                if name == "hw:out" {
                    self.hw_out_muted = !self.hw_out_muted;
                } else if let Some(track) = self.state.lock().tracks.get(name) {
                    track.lock().mute();
                }
            }
            Action::TrackToggleSolo(ref name) => {
                if name == "hw:out" {
                    return;
                }
                if let Some(track) = self.state.lock().tracks.get(name) {
                    track.lock().solo();
                }
            }
            Action::TrackLoadLv2Plugin {
                ref track_name,
                ref plugin_uri,
            } => {
                let track_handle = self.state.lock().tracks.get(track_name).cloned();
                match track_handle {
                    Some(track) => {
                        if let Err(e) = track.lock().load_lv2_plugin(plugin_uri) {
                            self.notify_clients(Err(e)).await;
                            return;
                        }
                    }
                    None => {
                        self.notify_clients(Err(format!("Track not found: {track_name}")))
                            .await;
                        return;
                    }
                }
            }
            Action::TrackClearDefaultPassthrough { ref track_name } => {
                let track_handle = self.state.lock().tracks.get(track_name).cloned();
                match track_handle {
                    Some(track) => {
                        track.lock().clear_default_passthrough();
                    }
                    None => {
                        self.notify_clients(Err(format!("Track not found: {track_name}")))
                            .await;
                        return;
                    }
                }
            }
            Action::TrackSetLv2PluginState {
                ref track_name,
                instance_id,
                ref state,
            } => {
                let track_handle = self.state.lock().tracks.get(track_name).cloned();
                match track_handle {
                    Some(track) => {
                        if let Err(e) = track.lock().set_lv2_plugin_state(instance_id, state.clone())
                        {
                            self.notify_clients(Err(e)).await;
                            return;
                        }
                    }
                    None => {
                        self.notify_clients(Err(format!("Track not found: {track_name}")))
                            .await;
                        return;
                    }
                }
            }
            Action::TrackUnloadLv2PluginInstance {
                ref track_name,
                instance_id,
            } => {
                let track_handle = self.state.lock().tracks.get(track_name).cloned();
                match track_handle {
                    Some(track) => {
                        if let Err(e) = track.lock().unload_lv2_plugin_instance(instance_id) {
                            self.notify_clients(Err(e)).await;
                            return;
                        }
                    }
                    None => {
                        self.notify_clients(Err(format!("Track not found: {track_name}")))
                            .await;
                        return;
                    }
                }
            }
            Action::TrackShowLv2PluginUiInstance {
                ref track_name,
                instance_id,
            } => {
                let track_handle = self.state.lock().tracks.get(track_name).cloned();
                match track_handle {
                    Some(track) => {
                        if let Err(e) = track.lock().show_lv2_plugin_ui_instance(instance_id) {
                            self.notify_clients(Err(e)).await;
                            return;
                        }
                    }
                    None => {
                        self.notify_clients(Err(format!("Track not found: {track_name}")))
                            .await;
                        return;
                    }
                }
            }
            Action::TrackGetLv2Graph { ref track_name } => {
                let track_handle = self.state.lock().tracks.get(track_name).cloned();
                match track_handle {
                    Some(track) => {
                        let (plugins, connections) = {
                            let track = track.lock();
                            (track.lv2_graph_plugins(), track.lv2_graph_connections())
                        };
                        self.notify_clients(Ok(Action::TrackLv2Graph {
                            track_name: track_name.clone(),
                            plugins,
                            connections,
                        }))
                        .await;
                        return;
                    }
                    None => {
                        self.notify_clients(Err(format!("Track not found: {track_name}")))
                            .await;
                        return;
                    }
                }
            }
            Action::TrackLv2Graph { .. } => {}
            Action::TrackConnectLv2Audio {
                ref track_name,
                ref from_node,
                from_port,
                ref to_node,
                to_port,
            } => {
                let track_handle = self.state.lock().tracks.get(track_name).cloned();
                match track_handle {
                    Some(track) => {
                        if let Err(e) = track.lock().connect_lv2_audio(
                            from_node.clone(),
                            from_port,
                            to_node.clone(),
                            to_port,
                        ) {
                            self.notify_clients(Err(e)).await;
                            return;
                        }
                    }
                    None => {
                        self.notify_clients(Err(format!("Track not found: {track_name}")))
                            .await;
                        return;
                    }
                }
            }
            Action::TrackConnectLv2Midi {
                ref track_name,
                ref from_node,
                from_port,
                ref to_node,
                to_port,
            } => {
                let track_handle = self.state.lock().tracks.get(track_name).cloned();
                match track_handle {
                    Some(track) => {
                        if let Err(e) = track.lock().connect_lv2_midi(
                            from_node.clone(),
                            from_port,
                            to_node.clone(),
                            to_port,
                        ) {
                            self.notify_clients(Err(e)).await;
                            return;
                        }
                    }
                    None => {
                        self.notify_clients(Err(format!("Track not found: {track_name}")))
                            .await;
                        return;
                    }
                }
            }
            Action::TrackDisconnectLv2Audio {
                ref track_name,
                ref from_node,
                from_port,
                ref to_node,
                to_port,
            } => {
                let track_handle = self.state.lock().tracks.get(track_name).cloned();
                match track_handle {
                    Some(track) => {
                        if let Err(e) = track.lock().disconnect_lv2_audio(
                            from_node.clone(),
                            from_port,
                            to_node.clone(),
                            to_port,
                        ) {
                            self.notify_clients(Err(e)).await;
                            return;
                        }
                    }
                    None => {
                        self.notify_clients(Err(format!("Track not found: {track_name}")))
                            .await;
                        return;
                    }
                }
            }
            Action::TrackDisconnectLv2Midi {
                ref track_name,
                ref from_node,
                from_port,
                ref to_node,
                to_port,
            } => {
                let track_handle = self.state.lock().tracks.get(track_name).cloned();
                match track_handle {
                    Some(track) => {
                        if let Err(e) = track.lock().disconnect_lv2_midi(
                            from_node.clone(),
                            from_port,
                            to_node.clone(),
                            to_port,
                        ) {
                            self.notify_clients(Err(e)).await;
                            return;
                        }
                    }
                    None => {
                        self.notify_clients(Err(format!("Track not found: {track_name}")))
                            .await;
                        return;
                    }
                }
            }
            Action::ListLv2Plugins => {
                let plugins = {
                    let host = crate::lv2::Lv2Host::new(48_000.0);
                    host.list_plugins()
                };
                self.notify_clients(Ok(Action::Lv2Plugins(plugins))).await;
                return;
            }
            Action::Lv2Plugins(_) => {}
            Action::ClipMove {
                ref kind,
                ref from,
                ref to,
                copy,
            } => {
                if let Some(from_track_handle) = self.state.lock().tracks.get(&from.track_name)
                    && let Some(to_track_handle) = self.state.lock().tracks.get(&to.track_name)
                {
                    let from_track = from_track_handle.lock();
                    let to_track = to_track_handle.lock();
                    match kind {
                        Kind::Audio => {
                            if from.clip_index >= from_track.audio.clips.len() {
                                self.notify_clients(Err(format!(
                                    "Clip index {} is too high, as track {} has only {} clips!",
                                    from.clip_index,
                                    from_track.name.clone(),
                                    from_track.audio.clips.len(),
                                )))
                                .await;
                                return;
                            }
                            let clip_copy = from_track.audio.clips[from.clip_index].clone();
                            if !copy {
                                from_track.audio.clips.remove(from.clip_index);
                            }
                            to_track.audio.clips.push(clip_copy);
                        }
                        Kind::MIDI => {
                            if from.clip_index >= from_track.midi.clips.len() {
                                self.notify_clients(Err(format!(
                                    "Clip index {} is too high, as track {} has only {} clips!",
                                    from.clip_index,
                                    from_track.name.clone(),
                                    from_track.midi.clips.len(),
                                )))
                                .await;
                                return;
                            }
                            let clip_copy = from_track.midi.clips[from.clip_index].clone();
                            if !copy {
                                from_track.midi.clips.remove(from.clip_index);
                            }
                            to_track.midi.clips.push(clip_copy);
                        }
                    }
                }
            }
            Action::AddClip {
                ref name,
                ref track_name,
                start,
                length,
                offset,
                kind,
            } => {
                if let Some(track) = self.state.lock().tracks.get(track_name) {
                    match kind {
                        Kind::Audio => {
                            let mut clip = AudioClip::new(name.clone(), start, length);
                            clip.offset = offset;
                            track.lock().audio.clips.push(clip);
                        }
                        Kind::MIDI => {
                            let mut clip = MIDIClip::new(name.clone(), start, length);
                            clip.offset = offset;
                            track.lock().midi.clips.push(clip);
                        }
                    }
                }
            }
            Action::Connect {
                ref from_track,
                from_port,
                ref to_track,
                to_port,
                kind,
            } => {
                match kind {
                    Kind::Audio => {
                        let from_audio_io = if from_track == "hw:in" {
                            self.oss_in
                                .as_ref()
                                .and_then(|h| h.lock().channels.get(from_port).cloned())
                        } else {
                            self.state
                                .lock()
                                .tracks
                                .get(from_track)
                                .and_then(|t| t.lock().audio.outs.get(from_port).cloned())
                        };
                        let to_audio_io = if to_track == "hw:out" {
                            self.oss_out
                                .as_ref()
                                .and_then(|h| h.lock().channels.get(to_port).cloned())
                        } else {
                            self.state
                                .lock()
                                .tracks
                                .get(to_track)
                                .and_then(|t| t.lock().audio.ins.get(to_port).cloned())
                        };
                        match (from_audio_io, to_audio_io) {
                            (Some(source), Some(target)) => {
                                if from_track != "hw:in"
                                    && to_track != "hw:out"
                                    && self.check_if_leads_to_kind(
                                        Kind::Audio,
                                        to_track,
                                        from_track,
                                    )
                                {
                                    self.notify_clients(Err(
                                        "Circular routing is not allowed!".into()
                                    ))
                                    .await;
                                    return;
                                }
                                crate::audio::io::AudioIO::connect(&source, &target);
                            }
                            (None, _) => {
                                self.notify_clients(Err(format!(
                                    "Source track '{}' not found",
                                    from_track
                                )))
                                .await;
                                return;
                            }
                            (_, None) => {
                                self.notify_clients(Err(format!(
                                    "Destination track '{}' not found",
                                    to_track
                                )))
                                .await;
                                return;
                            }
                        }
                    }
                    Kind::MIDI => {
                        if from_track == "hw:in" || to_track == "hw:out" {
                            self.notify_clients(Err(
                                "Hardware MIDI connections are not supported!".into(),
                            ))
                            .await;
                            return;
                        }
                        if self.check_if_leads_to_kind(Kind::MIDI, to_track, from_track) {
                            self.notify_clients(Err("Circular routing is not allowed!".into()))
                                .await;
                            return;
                        }

                        let state = self.state.lock();
                        let from_track_handle = state.tracks.get(from_track);
                        let to_track_handle = state.tracks.get(to_track);

                        match (from_track_handle, to_track_handle) {
                            (Some(f_t), Some(t_t)) => {
                                let to_in_res = t_t.lock().midi.ins.get(to_port).cloned();
                                if let Some(to_in) = to_in_res {
                                    if let Err(e) = f_t.lock().midi.connect_out(from_port, to_in) {
                                        self.notify_clients(Err(e)).await;
                                    }
                                } else {
                                    self.notify_clients(Err(format!(
                                        "MIDI input port {} not found on track '{}',",
                                        to_port, to_track
                                    )))
                                    .await;
                                }
                            }
                            _ => {
                                self.notify_clients(Err(format!(
                                    "MIDI tracks not found: {} or {}",
                                    from_track, to_track
                                )))
                                .await;
                            }
                        }
                    }
                };
            }
            Action::Disconnect {
                ref from_track,
                from_port,
                ref to_track,
                to_port,
                kind,
            } => {
                let from_audio_io = if from_track == "hw:in" {
                    self.oss_in
                        .as_ref()
                        .and_then(|h| h.lock().channels.get(from_port).cloned())
                } else {
                    let state = self.state.lock();
                    state
                        .tracks
                        .get(from_track)
                        .and_then(|t| t.lock().audio.outs.get(from_port).cloned())
                };
                let to_audio_io = if to_track == "hw:out" {
                    self.oss_out
                        .as_ref()
                        .and_then(|h| h.lock().channels.get(to_port).cloned())
                } else {
                    let state = self.state.lock();
                    state
                        .tracks
                        .get(to_track)
                        .and_then(|t| t.lock().audio.ins.get(to_port).cloned())
                };

                if kind == Kind::Audio {
                    match (from_audio_io, to_audio_io) {
                        (Some(source), Some(target)) => {
                            if let Err(e) = crate::audio::io::AudioIO::disconnect(&source, &target)
                            {
                                self.notify_clients(Err(format!("Disconnect failed: {}", e)))
                                    .await;
                                return;
                            }
                        }
                        _ => {
                            self.notify_clients(Err(format!(
                                "Disconnect failed: Port not found ({} -> {})",
                                from_track, to_track
                            )))
                            .await;
                        }
                    }
                } else if kind == Kind::MIDI && from_track != "hw:in" && to_track != "hw:out" {
                    let state = self.state.lock();
                    if let (Some(f_t), Some(t_t)) =
                        (state.tracks.get(from_track), state.tracks.get(to_track))
                        && let Some(to_in) = t_t.lock().midi.ins.get(to_port).cloned()
                    {
                        if let Err(e) = f_t.lock().midi.disconnect_out(from_port, &to_in) {
                            self.notify_clients(Err(e)).await;
                        } else {
                            self.notify_clients(Ok(a.clone())).await;
                        }
                    }
                }
            }

            Action::OpenAudioDevice(ref device) => {
                match oss::Audio::new(device, 48000, 32, true) {
                    Ok(d) => {
                        let channels = d.channels.len();
                        let rate = d.rate as usize;
                        self.oss_in = Some(Arc::new(UnsafeMutex::new(d)));
                        self.notify_clients(Ok(Action::HWInfo {
                            channels,
                            rate,
                            input: true,
                        }))
                        .await;
                    }
                    Err(e) => {
                        self.notify_clients(Err(e.to_string())).await;
                    }
                }
                match oss::Audio::new(device, 48000, 32, false) {
                    Ok(d) => {
                        let channels = d.channels.len();
                        let rate = d.rate as usize;
                        self.oss_out = Some(Arc::new(UnsafeMutex::new(d)));
                        self.notify_clients(Ok(Action::HWInfo {
                            channels,
                            rate,
                            input: false,
                        }))
                        .await;
                    }
                    Err(e) => {
                        self.notify_clients(Err(e.to_string())).await;
                    }
                }

                if let (Some(oss_in), Some(oss_out)) = (&self.oss_in, &self.oss_out) {
                    let in_fd = oss_in.lock().fd();
                    let out_fd = oss_out.lock().fd();
                    let mut group = 0;
                    let in_group = oss::add_to_sync_group(in_fd, group, true);
                    if in_group > 0 {
                        group = in_group;
                    }
                    let out_group = oss::add_to_sync_group(out_fd, group, false);
                    if out_group > 0 {
                        group = out_group;
                    }
                    let sync_started = if group > 0 {
                        oss::start_sync_group(in_fd, group).is_ok()
                    } else {
                        false
                    };
                    if !sync_started {
                        let _ = oss_in.lock().start_trigger();
                        let _ = oss_out.lock().start_trigger();
                    }
                }

                if self.oss_worker.is_none() && self.oss_in.is_some() && self.oss_out.is_some() {
                    let (tx, rx) = channel::<Message>(32);
                    let oss_in = self.oss_in.clone().unwrap();
                    let oss_out = self.oss_out.clone().unwrap();
                    let midi_hub = self.midi_hub.clone();
                    let tx_engine = self.tx.clone();
                    let handler = tokio::spawn(async move {
                        let worker = OssWorker::new(oss_in, oss_out, midi_hub, rx, tx_engine);
                        worker.work().await;
                    });
                    self.oss_worker = Some(WorkerData::new(tx, handler));
                    self.request_hw_cycle().await;
                }

                for device in Self::discover_midi_hw_devices() {
                    let (opened_in, opened_out) = {
                        let midi_hub = self.midi_hub.lock();
                        let opened_in = midi_hub.open_input(&device).is_ok();
                        let opened_out = midi_hub.open_output(&device).is_ok();
                        (opened_in, opened_out)
                    };

                    if opened_in {
                        self.notify_clients(Ok(Action::OpenMidiInputDevice(device.clone())))
                            .await;
                    }
                    if opened_out {
                        self.notify_clients(Ok(Action::OpenMidiOutputDevice(device.clone())))
                            .await;
                    }
                }
            }
            Action::OpenMidiInputDevice(ref device) => {
                let midi_hub = self.midi_hub.lock();
                if let Err(e) = midi_hub.open_input(device) {
                    self.notify_clients(Err(e)).await;
                    return;
                }
            }
            Action::OpenMidiOutputDevice(ref device) => {
                let midi_hub = self.midi_hub.lock();
                if let Err(e) = midi_hub.open_output(device) {
                    self.notify_clients(Err(e)).await;
                    return;
                }
            }
            Action::HWInfo { .. } => {}
        }
        self.notify_clients(Ok(a.clone())).await;
    }

    pub async fn work(&mut self) {
        while let Some(message) = self.rx.recv().await {
            match message {
                Message::Ready(id) => {
                    self.ready_workers.push(id);
                }
                Message::Finished(workid) => {
                    self.ready_workers.push(workid);
                    let all_finished = self.send_tracks().await;
                    if all_finished {
                        self.pending_hw_midi_out_events = self.collect_hw_midi_output_events();
                        self.request_hw_cycle().await;
                    }
                }
                Message::Channel(s) => {
                    self.clients.push(s);
                }

                Message::Request(a) => match a {
                    Action::OpenAudioDevice(_)
                    | Action::OpenMidiInputDevice(_)
                    | Action::OpenMidiOutputDevice(_)
                    | Action::Quit
                    | Action::ListLv2Plugins
                    | Action::Play
                    | Action::Stop
                    | Action::SetRecordEnabled(_)
                    | Action::SetSessionPath(_) => {
                        self.handle_request(a).await;
                    }
                    _ => {
                        self.pending_requests.push_back(a);
                        self.request_hw_cycle().await;
                    }
                },
                Message::HWFinished => {
                    if !self.awaiting_hwfinished {
                        continue;
                    }
                    self.awaiting_hwfinished = false;
                    while let Some(a) = self.pending_requests.pop_front() {
                        self.handle_request(a).await;
                    }
                    self.apply_mute_solo_policy();
                    self.append_recorded_cycle();
                    for track in self.state.lock().tracks.values() {
                        if !self.pending_hw_midi_events.is_empty() {
                            track
                                .lock()
                                .push_hw_midi_events(&self.pending_hw_midi_events);
                        }
                        track.lock().setup();
                    }
                    self.publish_track_meters().await;
                    self.pending_hw_midi_events.clear();
                    if self.playing {
                        self.transport_sample =
                            self.transport_sample.saturating_add(self.current_cycle_samples());
                    }
                    if self.send_tracks().await {
                        self.request_hw_cycle().await;
                    }
                }
                Message::HWMidiEvents(events) => {
                    self.pending_hw_midi_events.extend(events);
                }
                _ => {}
            }
        }
    }

    fn collect_hw_midi_output_events(&self) -> Vec<MidiEvent> {
        let mut events = vec![];
        for track in self.state.lock().tracks.values() {
            events.extend(track.lock().take_hw_midi_out_events());
        }
        events.sort_by(|a, b| a.frame.cmp(&b.frame));
        events
    }
}
