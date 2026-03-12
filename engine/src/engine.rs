use midly::{
    Arena, Format, Header, MetaMessage, Smf, Timing, TrackEvent, TrackEventKind,
    live::LiveEvent,
    num::{u15, u24, u28},
};
#[cfg(any(target_os = "freebsd", target_os = "linux"))]
use std::fs::read_dir;
use std::{
    collections::{HashMap, VecDeque},
    fs::File,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::mpsc::{Receiver, Sender, channel};
use tokio::task::JoinHandle;
use tracing::error;
use wavers::write as write_wav;

/// Hardware device information: (input_channels, output_channels, sample_rate, latency_ranges)
type HwDeviceInfo = (usize, usize, usize, ((usize, usize), (usize, usize)));

#[cfg(target_os = "linux")]
use crate::hw::alsa::{HwDriver, HwOptions, MidiHub};
#[cfg(target_os = "macos")]
use crate::hw::coreaudio::{HwDriver, HwOptions, MidiHub};
#[cfg(unix)]
use crate::hw::jack::JackRuntime;
#[cfg(target_os = "freebsd")]
use crate::hw::oss as hw;
#[cfg(target_os = "freebsd")]
use crate::hw::oss::{HwDriver, HwOptions, MidiHub};
#[cfg(target_os = "linux")]
use crate::workers::alsa_worker::HwWorker;
#[cfg(target_os = "macos")]
use crate::workers::coreaudio_worker::HwWorker;
#[cfg(target_os = "freebsd")]
use crate::workers::oss_worker::HwWorker;
use crate::{
    audio::clip::AudioClip,
    audio::io::AudioIO,
    history::{History, UndoEntry, create_inverse_actions, should_record},
    hw::{config, traits::HwDevice},
    kind::Kind,
    message::{Action, HwMidiEvent, Message, MidiControllerData, MidiNoteData},
    midi::clip::MIDIClip,
    midi::io::MidiEvent,
    mutex::UnsafeMutex,
    routing,
    state::State,
    track::Track,
    workers::worker::Worker,
};

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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct MidiHwInRoute {
    device: String,
    to_track: String,
    to_port: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct MidiHwOutRoute {
    from_track: String,
    from_port: usize,
    device: String,
}

struct OfflineBounceJob {
    track_name: String,
    cancel: Arc<AtomicBool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum MidiLearnSlot {
    Track(String, crate::message::TrackMidiLearnTarget),
    Global(crate::message::GlobalMidiLearnTarget),
}

pub struct Engine {
    clients: Vec<Sender<Message>>,
    rx: Receiver<Message>,
    state: Arc<UnsafeMutex<State>>,
    tx: Sender<Message>,
    workers: Vec<WorkerData>,
    hw_driver: Option<Arc<UnsafeMutex<HwDriver>>>,
    #[cfg(unix)]
    jack_runtime: Option<Arc<UnsafeMutex<JackRuntime>>>,
    midi_hub: Arc<UnsafeMutex<MidiHub>>,
    hw_worker: Option<WorkerData>,
    pending_hw_midi_events: Vec<MidiEvent>,
    pending_hw_midi_events_by_device: HashMap<String, Vec<MidiEvent>>,
    pending_hw_midi_out_events: Vec<MidiEvent>,
    pending_hw_midi_out_events_by_device: Vec<HwMidiEvent>,
    midi_hw_in_routes: Vec<MidiHwInRoute>,
    midi_hw_out_routes: Vec<MidiHwOutRoute>,
    ready_workers: Vec<usize>,
    pending_requests: VecDeque<Action>,
    awaiting_hwfinished: bool,
    transport_sample: usize,
    loop_enabled: bool,
    loop_range_samples: Option<(usize, usize)>,
    metronome_enabled: bool,
    tempo_bpm: f64,
    tsig_num: u16,
    tsig_denom: u16,
    punch_enabled: bool,
    punch_range_samples: Option<(usize, usize)>,
    audio_recordings: std::collections::HashMap<String, RecordingSession>,
    midi_recordings: std::collections::HashMap<String, MidiRecordingSession>,
    completed_audio_recordings: Vec<(String, RecordingSession)>,
    completed_midi_recordings: Vec<(String, MidiRecordingSession)>,
    playing: bool,
    clip_playback_enabled: bool,
    record_enabled: bool,
    session_dir: Option<PathBuf>,
    hw_out_level_db: f32,
    hw_out_balance: f32,
    hw_out_muted: bool,
    last_hw_out_meter_publish: Option<Instant>,
    #[cfg(any(target_os = "freebsd", target_os = "linux"))]
    last_hw_out_meter_linear: Vec<f32>,
    hw_out_peak_hold_linear: Vec<f32>,
    #[cfg(any(target_os = "freebsd", target_os = "linux"))]
    hw_out_meter_publish_phase: bool,
    last_track_meter_publish: Option<Instant>,
    track_meter_linear_by_track: HashMap<String, Vec<f32>>,
    track_meter_last_published_linear: HashMap<String, Vec<f32>>,
    latest_hw_out_meter_db: Arc<Vec<f32>>,
    latest_track_meter_snapshot: Arc<Vec<(String, Vec<f32>)>>,
    history: History,
    history_group: Option<UndoEntry>,
    history_suspended: bool,
    offline_bounce_job: Option<OfflineBounceJob>,
    pending_midi_learn: Option<(String, crate::message::TrackMidiLearnTarget, Option<String>)>,
    pending_global_midi_learn: Option<crate::message::GlobalMidiLearnTarget>,
    global_midi_learn_play_pause: Option<crate::message::MidiLearnBinding>,
    global_midi_learn_stop: Option<crate::message::MidiLearnBinding>,
    global_midi_learn_record_toggle: Option<crate::message::MidiLearnBinding>,
    midi_cc_gate: HashMap<(String, u8, u8), bool>,
}

type MidiEditParseResult = (
    Vec<MidiNoteData>,
    Vec<MidiControllerData>,
    Vec<(u64, Vec<u8>)>,
);

impl Engine {
    const METRONOME_TRACK: &'static str = "metronome";
    const METRONOME_DEFAULT_LEVEL_DB: f32 = -10.0;

    fn meter_linear_to_db(peak: f32) -> f32 {
        if peak <= 1.0e-6 {
            -90.0
        } else {
            (20.0 * peak.log10()).clamp(-90.0, 20.0)
        }
    }

    fn parse_midi_clip_for_edit(
        path: &Path,
        sample_rate: f64,
    ) -> Result<MidiEditParseResult, String> {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        let smf = Smf::parse(&bytes).map_err(|e| e.to_string())?;
        let Timing::Metrical(ppq) = smf.header.timing else {
            return Ok((vec![], vec![], vec![]));
        };
        let ppq = u64::from(ppq.as_int().max(1));

        let mut tempo_changes: Vec<(u64, u32)> = vec![(0, 500_000)];
        for track in &smf.tracks {
            let mut tick = 0_u64;
            for event in track {
                tick = tick.saturating_add(event.delta.as_int() as u64);
                if let TrackEventKind::Meta(MetaMessage::Tempo(us_per_q)) = event.kind {
                    tempo_changes.push((tick, us_per_q.as_int()));
                }
            }
        }
        tempo_changes.sort_by_key(|(tick, _)| *tick);
        let mut normalized_tempos: Vec<(u64, u32)> = Vec::with_capacity(tempo_changes.len());
        for (tick, tempo) in tempo_changes {
            if let Some(last) = normalized_tempos.last_mut()
                && last.0 == tick
            {
                last.1 = tempo;
            } else {
                normalized_tempos.push((tick, tempo));
            }
        }
        let tempo_changes = normalized_tempos;

        let ticks_to_samples = |tick: u64| -> usize {
            let mut total_us: u128 = 0;
            let mut prev_tick = 0_u64;
            let mut current_tempo_us = 500_000_u32;
            for (change_tick, tempo_us) in &tempo_changes {
                if *change_tick > tick {
                    break;
                }
                let seg_ticks = change_tick.saturating_sub(prev_tick);
                total_us = total_us.saturating_add(
                    u128::from(seg_ticks).saturating_mul(u128::from(current_tempo_us))
                        / u128::from(ppq),
                );
                prev_tick = *change_tick;
                current_tempo_us = *tempo_us;
            }
            let rem = tick.saturating_sub(prev_tick);
            total_us = total_us.saturating_add(
                u128::from(rem).saturating_mul(u128::from(current_tempo_us)) / u128::from(ppq),
            );
            ((total_us as f64 / 1_000_000.0) * sample_rate).round() as usize
        };

        let mut notes = Vec::<MidiNoteData>::new();
        let mut controllers = Vec::<MidiControllerData>::new();
        let mut passthrough_events = Vec::<(u64, Vec<u8>)>::new();
        let mut active_notes: HashMap<(u8, u8), Vec<(u64, u8)>> = HashMap::new();

        for track in &smf.tracks {
            let mut tick = 0_u64;
            for event in track {
                tick = tick.saturating_add(event.delta.as_int() as u64);
                match event.kind {
                    TrackEventKind::Midi { channel, message } => {
                        let channel_u8 = channel.as_int();
                        match message {
                            midly::MidiMessage::NoteOn { key, vel } => {
                                let pitch = key.as_int();
                                let velocity = vel.as_int();
                                if velocity == 0 {
                                    if let Some(starts) = active_notes.get_mut(&(channel_u8, pitch))
                                        && let Some((start_tick, start_vel)) = starts.pop()
                                    {
                                        let start_sample = ticks_to_samples(start_tick);
                                        let end_sample = ticks_to_samples(tick);
                                        notes.push(MidiNoteData {
                                            start_sample,
                                            length_samples: end_sample
                                                .saturating_sub(start_sample)
                                                .max(1),
                                            pitch,
                                            velocity: start_vel,
                                            channel: channel_u8,
                                        });
                                    }
                                } else {
                                    active_notes
                                        .entry((channel_u8, pitch))
                                        .or_default()
                                        .push((tick, velocity));
                                }
                            }
                            midly::MidiMessage::NoteOff { key, .. } => {
                                let pitch = key.as_int();
                                if let Some(starts) = active_notes.get_mut(&(channel_u8, pitch))
                                    && let Some((start_tick, start_vel)) = starts.pop()
                                {
                                    let start_sample = ticks_to_samples(start_tick);
                                    let end_sample = ticks_to_samples(tick);
                                    notes.push(MidiNoteData {
                                        start_sample,
                                        length_samples: end_sample
                                            .saturating_sub(start_sample)
                                            .max(1),
                                        pitch,
                                        velocity: start_vel,
                                        channel: channel_u8,
                                    });
                                }
                            }
                            midly::MidiMessage::Controller { controller, value } => {
                                controllers.push(MidiControllerData {
                                    sample: ticks_to_samples(tick),
                                    controller: controller.as_int(),
                                    value: value.as_int(),
                                    channel: channel_u8,
                                });
                            }
                            _ => {
                                let mut data = Vec::with_capacity(3);
                                if (LiveEvent::Midi { channel, message })
                                    .write(&mut data)
                                    .is_ok()
                                {
                                    passthrough_events.push((ticks_to_samples(tick) as u64, data));
                                }
                            }
                        }
                    }
                    TrackEventKind::SysEx(payload) => {
                        let mut data = Vec::with_capacity(payload.len() + 2);
                        data.push(0xF0);
                        data.extend_from_slice(payload);
                        if data.last().copied() != Some(0xF7) {
                            data.push(0xF7);
                        }
                        passthrough_events.push((ticks_to_samples(tick) as u64, data));
                    }
                    TrackEventKind::Escape(payload) => {
                        let mut data = Vec::with_capacity(payload.len() + 1);
                        data.push(0xF7);
                        data.extend_from_slice(payload);
                        passthrough_events.push((ticks_to_samples(tick) as u64, data));
                    }
                    _ => {}
                }
            }
        }

        for ((channel, pitch), starts) in active_notes {
            for (start_tick, velocity) in starts {
                let start_sample = ticks_to_samples(start_tick);
                let end_sample = ticks_to_samples(start_tick.saturating_add(ppq / 8));
                notes.push(MidiNoteData {
                    start_sample,
                    length_samples: end_sample.saturating_sub(start_sample).max(1),
                    pitch,
                    velocity,
                    channel,
                });
            }
        }

        notes.sort_by_key(|n| (n.start_sample, n.pitch));
        controllers.sort_by_key(|c| (c.sample, c.controller));
        passthrough_events.sort_by_key(|(sample, _)| *sample);
        Ok((notes, controllers, passthrough_events))
    }

    fn midi_events_from_notes_and_controllers(
        notes: &[MidiNoteData],
        controllers: &[MidiControllerData],
    ) -> Vec<(u64, Vec<u8>)> {
        let mut events: Vec<(u64, u8, Vec<u8>)> = Vec::new();
        for note in notes {
            let channel = note.channel.min(15);
            let pitch = note.pitch.min(127);
            let velocity = note.velocity.min(127);
            let start = note.start_sample as u64;
            let end = note.start_sample.saturating_add(note.length_samples).max(1) as u64;
            events.push((start, 2, vec![0x90 | channel, pitch, velocity]));
            events.push((end, 0, vec![0x80 | channel, pitch, 64]));
        }
        for ctrl in controllers {
            let channel = ctrl.channel.min(15);
            let controller = ctrl.controller.min(127);
            let value = ctrl.value.min(127);
            events.push((
                ctrl.sample as u64,
                1,
                vec![0xB0 | channel, controller, value],
            ));
        }
        events.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        events
            .into_iter()
            .map(|(sample, _, data)| (sample, data))
            .collect()
    }

    fn is_track_frozen(&self, track_name: &str) -> bool {
        self.state
            .lock()
            .tracks
            .get(track_name)
            .map(|track| track.lock().frozen())
            .unwrap_or(false)
    }

    async fn reject_if_track_frozen(&mut self, track_name: &str, operation: &str) -> bool {
        if self.is_track_frozen(track_name) {
            self.notify_clients(Err(format!(
                "Track '{track_name}' is frozen; {operation} is blocked"
            )))
            .await;
            true
        } else {
            false
        }
    }

    fn apply_midi_edit_action(&mut self, action: &Action) -> Result<(), String> {
        let (track_name, clip_index) = match action {
            Action::ModifyMidiNotes {
                track_name,
                clip_index,
                ..
            }
            | Action::InsertMidiNotes {
                track_name,
                clip_index,
                ..
            }
            | Action::DeleteMidiNotes {
                track_name,
                clip_index,
                ..
            }
            | Action::ModifyMidiControllers {
                track_name,
                clip_index,
                ..
            }
            | Action::InsertMidiControllers {
                track_name,
                clip_index,
                ..
            }
            | Action::DeleteMidiControllers {
                track_name,
                clip_index,
                ..
            }
            | Action::SetMidiSysExEvents {
                track_name,
                clip_index,
                ..
            } => (track_name, *clip_index),
            _ => return Ok(()),
        };

        let track_handle = self
            .state
            .lock()
            .tracks
            .get(track_name)
            .cloned()
            .ok_or_else(|| format!("Track not found: {track_name}"))?;
        let (clip_name, clip_path, sample_rate) = {
            let track = track_handle.lock();
            if clip_index >= track.midi.clips.len() {
                return Err(format!(
                    "Invalid MIDI clip index {clip_index} for '{track_name}'"
                ));
            }
            let clip_name = track.midi.clips[clip_index].name.clone();
            let clip_path = track.resolve_clip_path(&clip_name);
            (clip_name, clip_path, track.sample_rate)
        };

        let (mut notes, mut controllers, mut passthrough_events) =
            Self::parse_midi_clip_for_edit(&clip_path, sample_rate)?;

        match action {
            Action::ModifyMidiNotes {
                note_indices,
                new_notes,
                ..
            } => {
                for (idx, new_note) in note_indices.iter().zip(new_notes.iter()) {
                    if let Some(note) = notes.get_mut(*idx) {
                        *note = new_note.clone();
                    }
                }
            }
            Action::DeleteMidiNotes { note_indices, .. } => {
                let mut indices = note_indices.clone();
                indices.sort_unstable();
                indices.dedup();
                for idx in indices.into_iter().rev() {
                    if idx < notes.len() {
                        notes.remove(idx);
                    }
                }
            }
            Action::InsertMidiNotes {
                notes: inserted, ..
            } => {
                let mut sorted = inserted.clone();
                sorted.sort_unstable_by_key(|(idx, _)| *idx);
                for (idx, note) in sorted {
                    let at = idx.min(notes.len());
                    notes.insert(at, note);
                }
            }
            Action::ModifyMidiControllers {
                controller_indices,
                new_controllers,
                ..
            } => {
                for (idx, new_ctrl) in controller_indices.iter().zip(new_controllers.iter()) {
                    if let Some(ctrl) = controllers.get_mut(*idx) {
                        *ctrl = new_ctrl.clone();
                    }
                }
            }
            Action::DeleteMidiControllers {
                controller_indices, ..
            } => {
                let mut indices = controller_indices.clone();
                indices.sort_unstable();
                indices.dedup();
                for idx in indices.into_iter().rev() {
                    if idx < controllers.len() {
                        controllers.remove(idx);
                    }
                }
            }
            Action::InsertMidiControllers {
                controllers: inserted,
                ..
            } => {
                let mut sorted = inserted.clone();
                sorted.sort_unstable_by_key(|(idx, _)| *idx);
                for (idx, ctrl) in sorted {
                    let at = idx.min(controllers.len());
                    controllers.insert(at, ctrl);
                }
            }
            Action::SetMidiSysExEvents {
                new_sysex_events, ..
            } => {
                passthrough_events
                    .retain(|(_, data)| !matches!(data.first(), Some(0xF0) | Some(0xF7)));
                passthrough_events.extend(
                    new_sysex_events
                        .iter()
                        .map(|ev| (ev.sample as u64, ev.data.clone())),
                );
            }
            _ => {}
        }

        notes.sort_by_key(|n| (n.start_sample, n.pitch));
        controllers.sort_by_key(|c| (c.sample, c.controller));
        passthrough_events.sort_by_key(|(sample, _)| *sample);
        let mut events = Self::midi_events_from_notes_and_controllers(&notes, &controllers);
        events.extend(passthrough_events);
        events.sort_by_key(|(sample, _)| *sample);
        Self::write_midi_file(&clip_path, sample_rate.max(1.0) as u32, &events)?;
        track_handle.lock().invalidate_midi_clip_cache(&clip_name);
        Ok(())
    }

    const METER_PUBLISH_INTERVAL: Duration = Duration::from_millis(50);
    #[cfg(any(target_os = "freebsd", target_os = "linux"))]
    const HW_OUT_METER_LINEAR_EPSILON: f32 = 0.0025;
    #[cfg(not(any(target_os = "freebsd", target_os = "linux")))]
    const TRACK_METER_LINEAR_EPSILON: f32 = 0.0025;

    #[cfg(all(unix, not(target_os = "macos")))]
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

    fn finalize_midi_hw_devices(mut devices: Vec<String>) -> Vec<String> {
        devices.sort();
        devices.dedup();
        devices
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    fn discover_midi_hw_devices_from_dir(path: &str, prefixes: &[&str]) -> Vec<String> {
        let devices = read_dir(path)
            .map(|rd| {
                rd.filter_map(Result::ok)
                    .map(|e| e.path())
                    .filter_map(|path| {
                        let name = path.file_name()?.to_str()?;
                        prefixes
                            .iter()
                            .any(|prefix| name.starts_with(prefix))
                            .then(|| path.to_string_lossy().into_owned())
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self::finalize_midi_hw_devices(devices)
    }

    fn discover_midi_hw_devices() -> Vec<String> {
        #[cfg(target_os = "freebsd")]
        let devices = Self::discover_midi_hw_devices_from_dir("/dev", &["umidi", "midi"]);
        #[cfg(target_os = "linux")]
        let devices = Self::discover_midi_hw_devices_from_dir("/dev/snd", &["midiC"]);
        #[cfg(target_os = "macos")]
        let devices = {
            let mut devices = Vec::new();
            for source in coremidi::Sources {
                if let Some(name) = source.display_name() {
                    devices.push(name);
                }
            }
            for dest in coremidi::Destinations {
                if let Some(name) = dest.display_name() {
                    devices.push(name);
                }
            }
            Self::finalize_midi_hw_devices(devices)
        };
        #[cfg(not(any(target_os = "linux", target_os = "freebsd", target_os = "macos")))]
        let devices = Vec::new();
        devices
    }

    pub fn new(rx: Receiver<Message>, tx: Sender<Message>) -> Self {
        Self {
            rx,
            tx,
            clients: vec![],
            state: Arc::new(UnsafeMutex::new(State::default())),
            workers: vec![],
            hw_driver: None,
            #[cfg(unix)]
            jack_runtime: None,
            midi_hub: Arc::new(UnsafeMutex::new(MidiHub::default())),
            hw_worker: None,
            pending_hw_midi_events: vec![],
            pending_hw_midi_events_by_device: HashMap::new(),
            pending_hw_midi_out_events: vec![],
            pending_hw_midi_out_events_by_device: vec![],
            midi_hw_in_routes: vec![],
            midi_hw_out_routes: vec![],
            ready_workers: vec![],
            pending_requests: VecDeque::new(),
            awaiting_hwfinished: false,
            transport_sample: 0,
            loop_enabled: false,
            loop_range_samples: None,
            metronome_enabled: false,
            tempo_bpm: 120.0,
            tsig_num: 4,
            tsig_denom: 4,
            punch_enabled: false,
            punch_range_samples: None,
            audio_recordings: std::collections::HashMap::new(),
            midi_recordings: std::collections::HashMap::new(),
            completed_audio_recordings: Vec::new(),
            completed_midi_recordings: Vec::new(),
            playing: false,
            clip_playback_enabled: true,
            record_enabled: false,
            session_dir: None,
            hw_out_level_db: 0.0,
            hw_out_balance: 0.0,
            hw_out_muted: false,
            last_hw_out_meter_publish: None,
            #[cfg(any(target_os = "freebsd", target_os = "linux"))]
            last_hw_out_meter_linear: vec![],
            hw_out_peak_hold_linear: vec![],
            #[cfg(any(target_os = "freebsd", target_os = "linux"))]
            hw_out_meter_publish_phase: false,
            last_track_meter_publish: None,
            track_meter_linear_by_track: HashMap::new(),
            track_meter_last_published_linear: HashMap::new(),
            latest_hw_out_meter_db: Arc::new(Vec::new()),
            latest_track_meter_snapshot: Arc::new(Vec::new()),
            history: History::default(),
            history_group: None,
            history_suspended: false,
            offline_bounce_job: None,
            pending_midi_learn: None,
            pending_global_midi_learn: None,
            global_midi_learn_play_pause: None,
            global_midi_learn_stop: None,
            global_midi_learn_record_toggle: None,
            midi_cc_gate: HashMap::new(),
        }
    }

    fn hw_driver_cycle_samples(&self) -> Option<usize> {
        self.hw_driver.as_ref().map(|o| o.lock().cycle_samples())
    }

    #[cfg(unix)]
    fn jack_cycle_samples(&self) -> Option<usize> {
        self.jack_runtime.as_ref().map(|j| j.lock().buffer_size)
    }

    #[cfg(not(unix))]
    fn jack_cycle_samples(&self) -> Option<usize> {
        None
    }

    fn current_cycle_samples(&self) -> usize {
        self.hw_driver_cycle_samples()
            .or_else(|| self.jack_cycle_samples())
            .unwrap_or(0)
    }

    async fn ensure_metronome_track(&mut self) {
        if self.state.lock().tracks.contains_key(Self::METRONOME_TRACK) {
            return;
        }
        let (cycle_samples, sample_rate_hz, output_channels) = if let Some(hw) = &self.hw_driver {
            let hw = hw.lock();
            (
                hw.cycle_samples(),
                hw.sample_rate() as f64,
                hw.output_channels(),
            )
        } else {
            #[cfg(unix)]
            {
                if let Some(jack) = &self.jack_runtime {
                    let jack = jack.lock();
                    (
                        jack.buffer_size,
                        jack.sample_rate as f64,
                        jack.audio_outs.len(),
                    )
                } else {
                    return;
                }
            }
            #[cfg(not(unix))]
            {
                return;
            }
        };
        if output_channels == 0 {
            return;
        }
        self.state.lock().tracks.insert(
            Self::METRONOME_TRACK.to_string(),
            Arc::new(UnsafeMutex::new(Box::new(Track::new(
                Self::METRONOME_TRACK.to_string(),
                0,
                1,
                0,
                0,
                cycle_samples.max(1),
                sample_rate_hz.max(1.0),
            )))),
        );
        if let Some(track) = self.state.lock().tracks.get(Self::METRONOME_TRACK).cloned() {
            track.lock().set_level(Self::METRONOME_DEFAULT_LEVEL_DB);
            track.lock().set_metronome_enabled(self.metronome_enabled);
        }
        self.notify_clients(Ok(Action::AddTrack {
            name: Self::METRONOME_TRACK.to_string(),
            audio_ins: 0,
            midi_ins: 0,
            audio_outs: 1,
            midi_outs: 0,
        }))
        .await;
        self.notify_clients(Ok(Action::TrackLevel(
            Self::METRONOME_TRACK.to_string(),
            Self::METRONOME_DEFAULT_LEVEL_DB,
        )))
        .await;
    }

    fn open_hw_driver(
        device: &str,
        input_device: Option<&str>,
        sample_rate_hz: i32,
        bits: i32,
        hw_opts: HwOptions,
    ) -> Result<HwDriver, String> {
        #[cfg(any(target_os = "freebsd", target_os = "linux"))]
        {
            HwDriver::new_with_options(device, input_device, sample_rate_hz, bits, hw_opts)
                .map_err(|e| e.to_string())
        }
        #[cfg(not(any(target_os = "freebsd", target_os = "linux")))]
        {
            let _ = input_device;
            HwDriver::new_with_options(device, sample_rate_hz, bits, hw_opts)
                .map_err(|e| e.to_string())
        }
    }

    fn hw_profile_backend_label(device: &str) -> &'static str {
        let _ = device;
        #[cfg(target_os = "linux")]
        let label = "ALSA";
        #[cfg(target_os = "freebsd")]
        let label = "OSS";
        #[cfg(target_os = "macos")]
        let label = "CoreAudio";
        #[cfg(not(any(target_os = "linux", target_os = "freebsd", target_os = "macos")))]
        let label = "Unknown";
        label
    }

    #[cfg(target_os = "freebsd")]
    fn maybe_start_freebsd_sync_group(&self) {
        if let Some(oss) = &self.hw_driver {
            let in_fd = oss.lock().input_fd();
            let out_fd = oss.lock().output_fd();
            let mut group = 0;
            let in_group = hw::add_to_sync_group(in_fd, group, true);
            if in_group > 0 {
                group = in_group;
            }
            let out_group = hw::add_to_sync_group(out_fd, group, false);
            if out_group > 0 {
                group = out_group;
            }
            let sync_started = if group > 0 {
                hw::start_sync_group(in_fd, group).is_ok()
            } else {
                false
            };
            if !sync_started {
                let _ = oss.lock().start_input_trigger();
                let _ = oss.lock().start_output_trigger();
            }
        }
    }

    #[cfg(not(target_os = "freebsd"))]
    fn maybe_start_freebsd_sync_group(&self) {}

    async fn open_discovered_midi_hw_devices(&mut self) {
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

    #[cfg(unix)]
    async fn maybe_open_jack_runtime(
        &mut self,
        device: &str,
        input_device: Option<&str>,
        sample_rate_hz: i32,
        bits: i32,
        exclusive: bool,
        period_frames: usize,
        nperiods: usize,
        sync_mode: bool,
    ) -> Option<()> {
        if !device.eq_ignore_ascii_case("jack") {
            return None;
        }
        match JackRuntime::new(
            "maolan",
            crate::hw::jack::Config::default(),
            self.tx.clone(),
        ) {
            Ok(runtime) => {
                let input_channels = runtime.audio_ins.len();
                let output_channels = runtime.audio_outs.len();
                let midi_inputs = runtime.midi_input_devices();
                let midi_outputs = runtime.midi_output_devices();
                let rate = runtime.sample_rate;
                self.hw_driver = None;
                if let Some(worker) = self.hw_worker.take() {
                    let _ = worker.tx.send(Message::Request(Action::Quit)).await;
                    let _ = worker.handle.await;
                }
                self.jack_runtime = Some(Arc::new(UnsafeMutex::new(runtime)));
                self.publish_hw_infos(input_channels, output_channels, rate)
                    .await;
                for device in midi_inputs {
                    self.notify_clients(Ok(Action::OpenMidiInputDevice(device)))
                        .await;
                }
                for device in midi_outputs {
                    self.notify_clients(Ok(Action::OpenMidiOutputDevice(device)))
                        .await;
                }
                self.notify_clients(Ok(Action::OpenAudioDevice {
                    device: device.to_string(),
                    input_device: input_device.map(ToOwned::to_owned),
                    sample_rate_hz,
                    bits,
                    exclusive,
                    period_frames,
                    nperiods,
                    sync_mode,
                }))
                .await;
                self.awaiting_hwfinished = true;
            }
            Err(e) => {
                self.notify_clients(Err(e)).await;
            }
        }
        Some(())
    }

    #[cfg(not(unix))]
    async fn maybe_open_jack_runtime(
        &mut self,
        device: &str,
        _input_device: Option<&str>,
        _sample_rate_hz: i32,
        _bits: i32,
        _exclusive: bool,
        _period_frames: usize,
        _nperiods: usize,
        _sync_mode: bool,
    ) -> Option<()> {
        if !device.eq_ignore_ascii_case("jack") {
            return None;
        }
        self.notify_clients(Err(
            "JACK backend is not available on this platform build".to_string()
        ))
        .await;
        Some(())
    }

    fn hw_driver_input_audio_port(&self, from_port: usize) -> Option<Arc<AudioIO>> {
        self.hw_driver
            .as_ref()
            .and_then(|h| h.lock().input_port(from_port))
    }

    fn hw_driver_output_audio_port(&self, to_port: usize) -> Option<Arc<AudioIO>> {
        self.hw_driver
            .as_ref()
            .and_then(|h| h.lock().output_port(to_port))
    }

    #[cfg(unix)]
    fn jack_input_audio_port(&self, from_port: usize) -> Option<Arc<AudioIO>> {
        self.jack_runtime
            .as_ref()
            .and_then(|j| j.lock().audio_ins.get(from_port).cloned())
    }

    #[cfg(not(unix))]
    fn jack_input_audio_port(&self, _from_port: usize) -> Option<Arc<AudioIO>> {
        None
    }

    #[cfg(unix)]
    fn jack_output_audio_port(&self, to_port: usize) -> Option<Arc<AudioIO>> {
        self.jack_runtime
            .as_ref()
            .and_then(|j| j.lock().audio_outs.get(to_port).cloned())
    }

    #[cfg(not(unix))]
    fn jack_output_audio_port(&self, _to_port: usize) -> Option<Arc<AudioIO>> {
        None
    }

    fn normalize_transport_sample(&self, sample: usize) -> usize {
        if self.loop_enabled
            && let Some((loop_start, loop_end)) = self.loop_range_samples
            && loop_end > loop_start
            && sample >= loop_end
        {
            let loop_len = loop_end - loop_start;
            return loop_start + (sample - loop_start) % loop_len;
        }
        sample
    }

    fn cycle_segments(&self, frames: usize) -> Vec<(usize, usize, usize)> {
        if frames == 0 {
            return vec![];
        }
        if !self.loop_enabled {
            return vec![(
                self.transport_sample,
                self.transport_sample.saturating_add(frames),
                0,
            )];
        }
        let Some((loop_start, loop_end)) = self.loop_range_samples else {
            return vec![(
                self.transport_sample,
                self.transport_sample.saturating_add(frames),
                0,
            )];
        };
        if loop_end <= loop_start {
            return vec![(
                self.transport_sample,
                self.transport_sample.saturating_add(frames),
                0,
            )];
        }
        let mut segments = Vec::new();
        let mut remaining = frames;
        let mut out_offset = 0usize;
        let mut current = self.transport_sample;
        while remaining > 0 {
            let take = loop_end.saturating_sub(current).min(remaining);
            if take == 0 {
                current = loop_start;
                continue;
            }
            segments.push((current, current.saturating_add(take), out_offset));
            out_offset = out_offset.saturating_add(take);
            remaining -= take;
            current = if remaining > 0 {
                loop_start
            } else {
                current.saturating_add(take)
            };
        }
        segments
    }

    fn recording_segments_for_cycle(&self, frames: usize) -> Vec<(usize, usize, usize)> {
        let segments = self.cycle_segments(frames);
        if !self.punch_enabled {
            return segments;
        }
        let Some((punch_start, punch_end)) = self.punch_range_samples else {
            return vec![];
        };
        if punch_end <= punch_start {
            return vec![];
        }
        let mut clipped = Vec::new();
        for (segment_start, segment_end, frame_offset) in segments {
            let start = segment_start.max(punch_start);
            let end = segment_end.min(punch_end);
            if end <= start {
                continue;
            }
            let clipped_offset = frame_offset.saturating_add(start.saturating_sub(segment_start));
            clipped.push((start, end, clipped_offset));
        }
        clipped
    }

    fn hw_device_info<D: HwDevice>(d: &D) -> HwDeviceInfo {
        (
            d.input_channels(),
            d.output_channels(),
            d.sample_rate() as usize,
            d.latency_ranges(),
        )
    }

    async fn publish_hw_infos(&self, input_channels: usize, output_channels: usize, rate: usize) {
        self.notify_clients(Ok(Action::HWInfo {
            channels: input_channels,
            rate,
            input: true,
        }))
        .await;
        self.notify_clients(Ok(Action::HWInfo {
            channels: output_channels,
            rate,
            input: false,
        }))
        .await;
    }

    fn can_schedule_hw_cycle(&self) -> bool {
        #[cfg(unix)]
        let can_schedule = self.hw_worker.is_some() || self.jack_runtime.is_some();
        #[cfg(not(unix))]
        let can_schedule = self.hw_worker.is_some();
        can_schedule
    }

    async fn ensure_hw_worker_running(&mut self) {
        if self.hw_worker.is_some() || self.hw_driver.is_none() {
            return;
        }
        let (tx, rx) = channel::<Message>(32);
        let hw = self.hw_driver.clone().unwrap();
        let midi_hub = self.midi_hub.clone();
        let tx_engine = self.tx.clone();
        let handler = tokio::spawn(async move {
            let worker = HwWorker::new(hw, midi_hub, rx, tx_engine);
            worker.work().await;
        });
        self.hw_worker = Some(WorkerData::new(tx, handler));
    }

    fn build_hw_options(
        exclusive: bool,
        period_frames: usize,
        nperiods: usize,
        sync_mode: bool,
    ) -> HwOptions {
        HwOptions {
            exclusive,
            period_frames: period_frames.max(1).next_power_of_two(),
            nperiods: nperiods.max(1),
            sync_mode,
            ..Default::default()
        }
    }

    async fn open_non_jack_audio_device(
        &mut self,
        device: &str,
        input_device: Option<&str>,
        sample_rate_hz: i32,
        bits: i32,
        hw_opts: HwOptions,
    ) -> Result<(), String> {
        let hw_profile_enabled = config::env_flag(config::HW_PROFILE_ENV);
        let d = Self::open_hw_driver(device, input_device, sample_rate_hz, bits, hw_opts)?;
        let (in_channels, out_channels, rate, (in_lat, out_lat)) = Self::hw_device_info(&d);
        if hw_profile_enabled {
            let label = Self::hw_profile_backend_label(device);
            error!(
                "{} config: exclusive={}, period={}, nperiods={}, ignore_hwbuf={}, sync_mode={}, in_latency_extra={}, out_latency_extra={}, input_range={:?}, output_range={:?}",
                label,
                hw_opts.exclusive,
                hw_opts.period_frames,
                hw_opts.nperiods,
                hw_opts.ignore_hwbuf,
                hw_opts.sync_mode,
                hw_opts.input_latency_frames,
                hw_opts.output_latency_frames,
                in_lat,
                out_lat
            );
        }
        #[cfg(unix)]
        {
            self.jack_runtime = None;
        }
        self.hw_driver = Some(Arc::new(UnsafeMutex::new(d)));
        self.publish_hw_infos(in_channels, out_channels, rate).await;
        Ok(())
    }

    async fn finalize_open_audio_device(&mut self) {
        self.maybe_start_freebsd_sync_group();
        if self.metronome_enabled {
            self.ensure_metronome_track().await;
        }
        if self.hw_worker.is_none() && self.hw_driver.is_some() {
            self.ensure_hw_worker_running().await;
            self.request_hw_cycle().await;
        }
        self.open_discovered_midi_hw_devices().await;
    }

    fn hw_input_audio_port(&self, from_port: usize) -> Option<Arc<AudioIO>> {
        self.hw_driver_input_audio_port(from_port)
            .or_else(|| self.jack_input_audio_port(from_port))
    }

    fn hw_output_audio_port(&self, to_port: usize) -> Option<Arc<AudioIO>> {
        self.hw_driver_output_audio_port(to_port)
            .or_else(|| self.jack_output_audio_port(to_port))
    }

    fn all_hw_output_audio_ports(&self) -> Vec<Arc<AudioIO>> {
        if let Some(driver) = &self.hw_driver {
            let count = driver.lock().output_channels();
            return (0..count)
                .filter_map(|idx| self.hw_driver_output_audio_port(idx))
                .collect();
        }
        #[cfg(unix)]
        if let Some(jack) = &self.jack_runtime {
            return jack.lock().audio_outs.clone();
        }
        Vec::new()
    }

    fn midi_hw_in_device(track: &str) -> Option<&str> {
        track.strip_prefix("midi:hw:in:")
    }

    fn midi_hw_out_device(track: &str) -> Option<&str> {
        track.strip_prefix("midi:hw:out:")
    }

    fn midi_binding_matches(
        a: &crate::message::MidiLearnBinding,
        b: &crate::message::MidiLearnBinding,
    ) -> bool {
        if a.channel != b.channel || a.cc != b.cc {
            return false;
        }
        match (&a.device, &b.device) {
            (Some(ad), Some(bd)) => ad == bd,
            _ => true,
        }
    }

    fn midi_learn_slot_conflicts(
        &self,
        binding: &crate::message::MidiLearnBinding,
        ignore: Option<MidiLearnSlot>,
    ) -> Vec<String> {
        let mut conflicts = Vec::<String>::new();
        let state = self.state.lock();
        let mut push_conflict = |slot: MidiLearnSlot, label: String| {
            if ignore.as_ref().is_some_and(|i| i == &slot) {
                return;
            }
            conflicts.push(label);
        };
        let check_global =
            |current: &Option<crate::message::MidiLearnBinding>,
             target: crate::message::GlobalMidiLearnTarget,
             label: &str,
             push_conflict: &mut dyn FnMut(MidiLearnSlot, String)| {
                if let Some(existing) = current
                    && Self::midi_binding_matches(binding, existing)
                {
                    push_conflict(MidiLearnSlot::Global(target), format!("Global {label}"));
                }
            };
        check_global(
            &self.global_midi_learn_play_pause,
            crate::message::GlobalMidiLearnTarget::PlayPause,
            "PlayPause",
            &mut push_conflict,
        );
        check_global(
            &self.global_midi_learn_stop,
            crate::message::GlobalMidiLearnTarget::Stop,
            "Stop",
            &mut push_conflict,
        );
        check_global(
            &self.global_midi_learn_record_toggle,
            crate::message::GlobalMidiLearnTarget::RecordToggle,
            "RecordToggle",
            &mut push_conflict,
        );
        for (track_name, track) in state.tracks.iter() {
            let t = track.lock();
            let mut check_track = |current: &Option<crate::message::MidiLearnBinding>,
                                   target: crate::message::TrackMidiLearnTarget,
                                   label: &str| {
                if let Some(existing) = current
                    && Self::midi_binding_matches(binding, existing)
                {
                    push_conflict(
                        MidiLearnSlot::Track(track_name.clone(), target),
                        format!("{track_name} {label}"),
                    );
                }
            };
            check_track(
                &t.midi_learn_volume,
                crate::message::TrackMidiLearnTarget::Volume,
                "Volume",
            );
            check_track(
                &t.midi_learn_balance,
                crate::message::TrackMidiLearnTarget::Balance,
                "Balance",
            );
            check_track(
                &t.midi_learn_mute,
                crate::message::TrackMidiLearnTarget::Mute,
                "Mute",
            );
            check_track(
                &t.midi_learn_solo,
                crate::message::TrackMidiLearnTarget::Solo,
                "Solo",
            );
            check_track(
                &t.midi_learn_arm,
                crate::message::TrackMidiLearnTarget::Arm,
                "Arm",
            );
            check_track(
                &t.midi_learn_input_monitor,
                crate::message::TrackMidiLearnTarget::InputMonitor,
                "InputMonitor",
            );
            check_track(
                &t.midi_learn_disk_monitor,
                crate::message::TrackMidiLearnTarget::DiskMonitor,
                "DiskMonitor",
            );
        }
        conflicts
    }

    async fn handle_incoming_hw_cc(&mut self, device: &str, channel: u8, cc: u8, value: u8) {
        let gate_key = (device.to_string(), channel, cc);
        let high = value >= 64;
        let prev_high = self.midi_cc_gate.get(&gate_key).copied().unwrap_or(false);
        self.midi_cc_gate.insert(gate_key, high);
        let rising = high && !prev_high;

        if let Some((track_name, target, armed_device)) = self.pending_midi_learn.clone() {
            let binding = crate::message::MidiLearnBinding {
                device: armed_device.or(Some(device.to_string())),
                channel,
                cc,
            };
            let conflicts = self.midi_learn_slot_conflicts(
                &binding,
                Some(MidiLearnSlot::Track(track_name.clone(), target)),
            );
            if !conflicts.is_empty() {
                self.pending_midi_learn = None;
                self.notify_clients(Err(format!(
                    "MIDI learn conflict for '{}' {:?}: {}",
                    track_name,
                    target,
                    conflicts.join(", ")
                )))
                .await;
                return;
            }
            if let Some(track) = self.state.lock().tracks.get(&track_name) {
                match target {
                    crate::message::TrackMidiLearnTarget::Volume => {
                        track.lock().midi_learn_volume = Some(binding.clone());
                    }
                    crate::message::TrackMidiLearnTarget::Balance => {
                        track.lock().midi_learn_balance = Some(binding.clone());
                    }
                    crate::message::TrackMidiLearnTarget::Mute => {
                        track.lock().midi_learn_mute = Some(binding.clone());
                    }
                    crate::message::TrackMidiLearnTarget::Solo => {
                        track.lock().midi_learn_solo = Some(binding.clone());
                    }
                    crate::message::TrackMidiLearnTarget::Arm => {
                        track.lock().midi_learn_arm = Some(binding.clone());
                    }
                    crate::message::TrackMidiLearnTarget::InputMonitor => {
                        track.lock().midi_learn_input_monitor = Some(binding.clone());
                    }
                    crate::message::TrackMidiLearnTarget::DiskMonitor => {
                        track.lock().midi_learn_disk_monitor = Some(binding.clone());
                    }
                }
                self.pending_midi_learn = None;
                self.notify_clients(Ok(Action::TrackSetMidiLearnBinding {
                    track_name: track_name.clone(),
                    target,
                    binding: Some(binding),
                }))
                .await;
            } else {
                self.pending_midi_learn = None;
            }
        }
        if let Some(target) = self.pending_global_midi_learn.take() {
            let binding = crate::message::MidiLearnBinding {
                device: Some(device.to_string()),
                channel,
                cc,
            };
            let conflicts =
                self.midi_learn_slot_conflicts(&binding, Some(MidiLearnSlot::Global(target)));
            if !conflicts.is_empty() {
                self.notify_clients(Err(format!(
                    "Global MIDI learn conflict for {:?}: {}",
                    target,
                    conflicts.join(", ")
                )))
                .await;
                return;
            }
            match target {
                crate::message::GlobalMidiLearnTarget::PlayPause => {
                    self.global_midi_learn_play_pause = Some(binding.clone());
                }
                crate::message::GlobalMidiLearnTarget::Stop => {
                    self.global_midi_learn_stop = Some(binding.clone());
                }
                crate::message::GlobalMidiLearnTarget::RecordToggle => {
                    self.global_midi_learn_record_toggle = Some(binding.clone());
                }
            }
            self.notify_clients(Ok(Action::SetGlobalMidiLearnBinding {
                target,
                binding: Some(binding),
            }))
            .await;
        }

        let mut mapped_actions = Vec::<Action>::new();
        for (track_name, track) in self.state.lock().tracks.iter() {
            let t = track.lock();
            if let Some(binding) = t.midi_learn_volume.as_ref() {
                let device_matches = binding.device.as_ref().is_none_or(|d| d.as_str() == device);
                if device_matches && binding.channel == channel && binding.cc == cc {
                    let level = -90.0 + (value as f32 / 127.0) * 110.0;
                    mapped_actions.push(Action::TrackLevel(track_name.clone(), level));
                }
            }
            if let Some(binding) = t.midi_learn_balance.as_ref() {
                let device_matches = binding.device.as_ref().is_none_or(|d| d.as_str() == device);
                if device_matches && binding.channel == channel && binding.cc == cc {
                    let balance = (value as f32 / 127.0) * 2.0 - 1.0;
                    mapped_actions.push(Action::TrackBalance(track_name.clone(), balance));
                }
            }
            if let Some(binding) = t.midi_learn_mute.as_ref() {
                let device_matches = binding.device.as_ref().is_none_or(|d| d.as_str() == device);
                if device_matches && binding.channel == channel && binding.cc == cc {
                    let wanted = value >= 64;
                    if t.muted != wanted {
                        mapped_actions.push(Action::TrackToggleMute(track_name.clone()));
                    }
                }
            }
            if let Some(binding) = t.midi_learn_solo.as_ref() {
                let device_matches = binding.device.as_ref().is_none_or(|d| d.as_str() == device);
                if device_matches && binding.channel == channel && binding.cc == cc {
                    let wanted = value >= 64;
                    if t.soloed != wanted {
                        mapped_actions.push(Action::TrackToggleSolo(track_name.clone()));
                    }
                }
            }
            if let Some(binding) = t.midi_learn_arm.as_ref() {
                let device_matches = binding.device.as_ref().is_none_or(|d| d.as_str() == device);
                if device_matches && binding.channel == channel && binding.cc == cc {
                    let wanted = value >= 64;
                    if t.armed != wanted {
                        mapped_actions.push(Action::TrackToggleArm(track_name.clone()));
                    }
                }
            }
            if let Some(binding) = t.midi_learn_input_monitor.as_ref() {
                let device_matches = binding.device.as_ref().is_none_or(|d| d.as_str() == device);
                if device_matches && binding.channel == channel && binding.cc == cc {
                    let wanted = value >= 64;
                    if t.input_monitor != wanted {
                        mapped_actions.push(Action::TrackToggleInputMonitor(track_name.clone()));
                    }
                }
            }
            if let Some(binding) = t.midi_learn_disk_monitor.as_ref() {
                let device_matches = binding.device.as_ref().is_none_or(|d| d.as_str() == device);
                if device_matches && binding.channel == channel && binding.cc == cc {
                    let wanted = value >= 64;
                    if t.disk_monitor != wanted {
                        mapped_actions.push(Action::TrackToggleDiskMonitor(track_name.clone()));
                    }
                }
            }
        }
        let device_matches =
            |binding: &crate::message::MidiLearnBinding| binding.device.as_deref() == Some(device);
        let mut mapped_global_actions = Vec::<Action>::new();
        if let Some(binding) = self.global_midi_learn_play_pause.as_ref()
            && device_matches(binding)
            && binding.channel == channel
            && binding.cc == cc
            && rising
        {
            mapped_global_actions.push(if self.playing {
                Action::Stop
            } else {
                Action::Play
            });
        }
        if let Some(binding) = self.global_midi_learn_stop.as_ref()
            && device_matches(binding)
            && binding.channel == channel
            && binding.cc == cc
            && rising
            && self.playing
        {
            mapped_global_actions.push(Action::Stop);
        }
        if let Some(binding) = self.global_midi_learn_record_toggle.as_ref()
            && device_matches(binding)
            && binding.channel == channel
            && binding.cc == cc
            && rising
        {
            mapped_global_actions.push(Action::SetRecordEnabled(!self.record_enabled));
        }
        for action in mapped_actions {
            match action {
                Action::TrackLevel(ref track_name, level) => {
                    if let Some(track) = self.state.lock().tracks.get(track_name) {
                        track.lock().set_level(level);
                        self.notify_clients(Ok(Action::TrackLevel(track_name.clone(), level)))
                            .await;
                    }
                }
                Action::TrackBalance(ref track_name, balance) => {
                    if let Some(track) = self.state.lock().tracks.get(track_name) {
                        track.lock().set_balance(balance);
                        self.notify_clients(Ok(Action::TrackBalance(track_name.clone(), balance)))
                            .await;
                    }
                }
                Action::TrackToggleMute(ref track_name) => {
                    if let Some(track) = self.state.lock().tracks.get(track_name) {
                        track.lock().mute();
                        self.notify_clients(Ok(Action::TrackToggleMute(track_name.clone())))
                            .await;
                    }
                }
                Action::TrackToggleSolo(ref track_name) => {
                    if let Some(track) = self.state.lock().tracks.get(track_name) {
                        track.lock().solo();
                        self.notify_clients(Ok(Action::TrackToggleSolo(track_name.clone())))
                            .await;
                    }
                }
                Action::TrackToggleArm(ref track_name) => {
                    if let Some(track) = self.state.lock().tracks.get(track_name) {
                        track.lock().arm();
                        self.notify_clients(Ok(Action::TrackToggleArm(track_name.clone())))
                            .await;
                    }
                }
                Action::TrackToggleInputMonitor(ref track_name) => {
                    if let Some(track) = self.state.lock().tracks.get(track_name) {
                        track.lock().toggle_input_monitor();
                        self.notify_clients(Ok(Action::TrackToggleInputMonitor(
                            track_name.clone(),
                        )))
                        .await;
                    }
                }
                Action::TrackToggleDiskMonitor(ref track_name) => {
                    if let Some(track) = self.state.lock().tracks.get(track_name) {
                        track.lock().toggle_disk_monitor();
                        self.notify_clients(Ok(Action::TrackToggleDiskMonitor(track_name.clone())))
                            .await;
                    }
                }
                _ => {}
            }
        }
        for action in mapped_global_actions {
            self.handle_request_inner(action, false).await;
        }
    }

    fn vca_followers(&self, master_name: &str) -> Vec<String> {
        self.state
            .lock()
            .tracks
            .iter()
            .filter_map(|(name, track)| {
                if track.lock().vca_master.as_deref() == Some(master_name) {
                    Some(name.clone())
                } else {
                    None
                }
            })
            .collect()
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
            let audio_channels = track.record_tap_outs.len();
            let audio_frames = track
                .record_tap_outs
                .first()
                .map(|ch| ch.len())
                .unwrap_or(0);
            let frames = audio_frames.max(self.current_cycle_samples());
            if frames == 0 {
                continue;
            }
            let segments = self.recording_segments_for_cycle(frames);
            for (segment_start, segment_end, frame_offset) in segments {
                let segment_len = segment_end.saturating_sub(segment_start);
                if segment_len == 0 {
                    continue;
                }

                if audio_channels > 0 && audio_frames > 0 {
                    let audio_entry =
                        self.audio_recordings
                            .entry(name.clone())
                            .or_insert_with(|| RecordingSession {
                                start_sample: segment_start,
                                samples: Vec::with_capacity(segment_len * audio_channels * 2),
                                channels: audio_channels,
                                file_name: Self::next_recording_file_name(name),
                            });
                    if audio_entry.channels != audio_channels {
                        continue;
                    }
                    if let Some(entry) = self.audio_recordings.get_mut(name.as_str()) {
                        let from = frame_offset.min(audio_frames);
                        let to = frame_offset.saturating_add(segment_len).min(audio_frames);
                        for frame in from..to {
                            for ch in 0..audio_channels {
                                entry.samples.push(track.record_tap_outs[ch][frame]);
                            }
                        }
                    }
                }

                let entry = self.midi_recordings.entry(name.clone()).or_insert_with(|| {
                    MidiRecordingSession {
                        start_sample: segment_start,
                        events: Vec::new(),
                        file_name: Self::next_midi_recording_file_name(name),
                    }
                });
                let from = frame_offset;
                let to = frame_offset.saturating_add(segment_len);
                for event in &track.record_tap_midi_in {
                    let frame = event.frame as usize;
                    if frame < from || frame >= to {
                        continue;
                    }
                    let abs_sample = segment_start as u64 + (frame - from) as u64;
                    entry.events.push((abs_sample, event.data.clone()));
                }

                if self.punch_enabled
                    && let Some((_, punch_end)) = self.punch_range_samples
                    && segment_end == punch_end
                {
                    if let Some(done) = self.audio_recordings.remove(name.as_str()) {
                        self.completed_audio_recordings.push((name.clone(), done));
                    }
                    if let Some(done) = self.midi_recordings.remove(name.as_str()) {
                        self.completed_midi_recordings.push((name.clone(), done));
                    }
                } else if self.loop_enabled
                    && let Some((_, loop_end)) = self.loop_range_samples
                    && segment_end == loop_end
                {
                    if let Some(done) = self.audio_recordings.remove(name.as_str()) {
                        self.completed_audio_recordings.push((name.clone(), done));
                    }
                    if let Some(done) = self.midi_recordings.remove(name.as_str()) {
                        self.completed_midi_recordings.push((name.clone(), done));
                    }
                }
            }
        }
    }

    async fn flush_completed_recordings(&mut self) {
        if self.completed_audio_recordings.is_empty() && self.completed_midi_recordings.is_empty() {
            return;
        }
        let Some(audio_dir) = self.session_audio_dir() else {
            self.completed_audio_recordings.clear();
            self.completed_midi_recordings.clear();
            return;
        };
        let Some(midi_dir) = self.session_midi_dir() else {
            self.completed_audio_recordings.clear();
            self.completed_midi_recordings.clear();
            return;
        };
        if std::fs::create_dir_all(&audio_dir).is_err()
            || std::fs::create_dir_all(&midi_dir).is_err()
        {
            self.completed_audio_recordings.clear();
            self.completed_midi_recordings.clear();
            return;
        }
        let rate = self
            .hw_driver
            .as_ref()
            .map(|o| o.lock().sample_rate())
            .unwrap_or(48_000);
        let completed_audio = std::mem::take(&mut self.completed_audio_recordings);
        for (track_name, rec) in completed_audio {
            self.flush_recording_entry(&audio_dir, rate, track_name, rec)
                .await;
        }
        let completed_midi = std::mem::take(&mut self.completed_midi_recordings);
        for (track_name, rec) in completed_midi {
            self.flush_midi_recording_entry(&midi_dir, rate as u32, track_name, rec)
                .await;
        }
    }

    async fn flush_recordings(&mut self) {
        let Some(audio_dir) = self.session_audio_dir() else {
            if !self.audio_recordings.is_empty()
                || !self.midi_recordings.is_empty()
                || !self.completed_audio_recordings.is_empty()
                || !self.completed_midi_recordings.is_empty()
            {
                self.notify_clients(Err("Recording stopped: session path is not set".to_string()))
                    .await;
            }
            self.audio_recordings.clear();
            self.midi_recordings.clear();
            self.completed_audio_recordings.clear();
            self.completed_midi_recordings.clear();
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
            self.completed_audio_recordings.clear();
            self.completed_midi_recordings.clear();
            return;
        }
        let Some(midi_dir) = self.session_midi_dir() else {
            self.audio_recordings.clear();
            self.midi_recordings.clear();
            self.completed_audio_recordings.clear();
            self.completed_midi_recordings.clear();
            return;
        };
        if std::fs::create_dir_all(&midi_dir).is_err() {
            self.audio_recordings.clear();
            self.midi_recordings.clear();
            self.completed_audio_recordings.clear();
            self.completed_midi_recordings.clear();
            return;
        }
        let rate = self
            .hw_driver
            .as_ref()
            .map(|o| o.lock().sample_rate())
            .unwrap_or(48_000);
        let completed_audio = std::mem::take(&mut self.completed_audio_recordings);
        for (track_name, rec) in completed_audio {
            self.flush_recording_entry(&audio_dir, rate, track_name, rec)
                .await;
        }
        let completed_midi = std::mem::take(&mut self.completed_midi_recordings);
        for (track_name, rec) in completed_midi {
            self.flush_midi_recording_entry(&midi_dir, rate as u32, track_name, rec)
                .await;
        }
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
            track.lock().audio.clips.push(clip.clone());
        }
        self.notify_clients(Ok(Action::AddClip {
            name: clip_rel_name,
            track_name: track_name.clone(),
            start: rec.start_sample,
            length,
            offset: 0,
            input_channel: 0,
            muted: false,
            kind: Kind::Audio,
            fade_enabled: clip.fade_enabled,
            fade_in_samples: clip.fade_in_samples,
            fade_out_samples: clip.fade_out_samples,
            warp_markers: vec![],
        }))
        .await;
    }

    async fn flush_track_recording(&mut self, track_name: &str) {
        let Some(audio_dir) = self.session_audio_dir() else {
            self.audio_recordings.remove(track_name);
            self.midi_recordings.remove(track_name);
            self.completed_audio_recordings
                .retain(|(name, _)| name != track_name);
            self.completed_midi_recordings
                .retain(|(name, _)| name != track_name);
            return;
        };
        let Some(midi_dir) = self.session_midi_dir() else {
            self.audio_recordings.remove(track_name);
            self.midi_recordings.remove(track_name);
            self.completed_audio_recordings
                .retain(|(name, _)| name != track_name);
            self.completed_midi_recordings
                .retain(|(name, _)| name != track_name);
            return;
        };
        if std::fs::create_dir_all(&audio_dir).is_err()
            || std::fs::create_dir_all(&midi_dir).is_err()
        {
            return;
        }
        let rate = self
            .hw_driver
            .as_ref()
            .map(|o| o.lock().sample_rate())
            .unwrap_or(48_000);
        let mut i = 0;
        while i < self.completed_audio_recordings.len() {
            if self.completed_audio_recordings[i].0 == track_name {
                let (name, rec) = self.completed_audio_recordings.remove(i);
                self.flush_recording_entry(&audio_dir, rate, name, rec)
                    .await;
            } else {
                i += 1;
            }
        }
        let mut j = 0;
        while j < self.completed_midi_recordings.len() {
            if self.completed_midi_recordings[j].0 == track_name {
                let (name, rec) = self.completed_midi_recordings.remove(j);
                self.flush_midi_recording_entry(&midi_dir, rate as u32, name, rec)
                    .await;
            } else {
                j += 1;
            }
        }

        let Some(rec) = self.audio_recordings.remove(track_name) else {
            if let Some(mrec) = self.midi_recordings.remove(track_name) {
                self.flush_midi_recording_entry(
                    &midi_dir,
                    rate as u32,
                    track_name.to_string(),
                    mrec,
                )
                .await;
            }
            return;
        };
        self.flush_recording_entry(&audio_dir, rate, track_name.to_string(), rec)
            .await;
        if let Some(mrec) = self.midi_recordings.remove(track_name) {
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
            input_channel: 0,
            muted: false,
            kind: Kind::MIDI,
            fade_enabled: true,
            fade_in_samples: 240,
            fade_out_samples: 240,
            warp_markers: vec![],
        }))
        .await;
    }

    fn write_midi_file(
        path: &Path,
        sample_rate: u32,
        events: &[(u64, Vec<u8>)],
    ) -> Result<(), String> {
        let ppq: u16 = 480;
        let ticks_per_second: u64 = 960;
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

    fn track_handle_by_name(&self, track_name: &str) -> Option<Arc<UnsafeMutex<Box<Track>>>> {
        self.state.lock().tracks.get(track_name).cloned()
    }

    fn track_handle_or_err(
        &self,
        track_name: &str,
    ) -> Result<Arc<UnsafeMutex<Box<Track>>>, String> {
        self.track_handle_by_name(track_name)
            .ok_or_else(|| format!("Track not found: {track_name}"))
    }

    fn add_clip_to_track(
        &self,
        name: &str,
        track_name: &str,
        start: usize,
        length: usize,
        offset: usize,
        input_channel: usize,
        muted: bool,
        kind: Kind,
        fade_enabled: bool,
        fade_in_samples: usize,
        fade_out_samples: usize,
        warp_markers: &[crate::message::AudioWarpMarker],
    ) {
        if let Some(track) = self.state.lock().tracks.get(track_name) {
            let track = track.lock();
            match kind {
                Kind::Audio => {
                    let mut clip = AudioClip::new(name.to_string(), start, length);
                    clip.offset = offset;
                    let max_lane = track.audio.ins.len().saturating_sub(1);
                    clip.input_channel = input_channel.min(max_lane);
                    clip.muted = muted;
                    clip.fade_enabled = fade_enabled;
                    clip.fade_in_samples = fade_in_samples;
                    clip.fade_out_samples = fade_out_samples;
                    clip.warp_markers = warp_markers.to_vec();
                    track.audio.clips.push(clip);
                }
                Kind::MIDI => {
                    let mut clip = MIDIClip::new(name.to_string(), start, length);
                    clip.offset = offset;
                    let max_lane = track.midi.ins.len().saturating_sub(1);
                    clip.input_channel = input_channel.min(max_lane);
                    clip.muted = muted;
                    track.midi.clips.push(clip);
                }
            }
        }
    }

    fn remove_clips_from_track(&self, track_name: &str, kind: Kind, clip_indices: &[usize]) {
        if let Some(track) = self.state.lock().tracks.get(track_name) {
            let track = track.lock();
            let mut indices = clip_indices.to_vec();
            indices.sort_unstable();
            indices.dedup();
            match kind {
                Kind::Audio => {
                    for idx in indices.into_iter().rev() {
                        if idx < track.audio.clips.len() {
                            track.audio.clips.remove(idx);
                        }
                    }
                }
                Kind::MIDI => {
                    for idx in indices.into_iter().rev() {
                        if idx < track.midi.clips.len() {
                            track.midi.clips.remove(idx);
                        }
                    }
                }
            }
        }
    }

    fn rename_clip_references(
        &self,
        track_name: &str,
        kind: Kind,
        clip_index: usize,
        new_name: &str,
    ) {
        let Some(track) = self.state.lock().tracks.get(track_name) else {
            return;
        };
        let track = track.lock();
        let old_name = match kind {
            Kind::Audio => {
                if clip_index >= track.audio.clips.len() {
                    return;
                }
                track.audio.clips[clip_index].name.clone()
            }
            Kind::MIDI => {
                if clip_index >= track.midi.clips.len() {
                    return;
                }
                track.midi.clips[clip_index].name.clone()
            }
        };

        let new_file_name = match kind {
            Kind::Audio => format!("audio/{}.wav", new_name),
            Kind::MIDI => {
                let ext = std::path::Path::new(&old_name)
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|s| s.to_ascii_lowercase())
                    .filter(|e| e == "mid" || e == "midi")
                    .unwrap_or_else(|| "mid".to_string());
                format!("midi/{}.{}", new_name, ext)
            }
        };
        let _ = track;

        for (_, other_track) in self.state.lock().tracks.iter() {
            let other_track = other_track.lock();
            match kind {
                Kind::Audio => {
                    for clip in &mut other_track.audio.clips {
                        if clip.name == old_name {
                            clip.name = new_file_name.clone();
                        }
                    }
                }
                Kind::MIDI => {
                    for clip in &mut other_track.midi.clips {
                        if clip.name == old_name {
                            clip.name = new_file_name.clone();
                        }
                    }
                }
            }
        }
    }

    fn set_clip_fade(
        &self,
        track_name: &str,
        clip_index: usize,
        kind: Kind,
        fade_enabled: bool,
        fade_in_samples: usize,
        fade_out_samples: usize,
    ) {
        let Some(track) = self.state.lock().tracks.get(track_name) else {
            return;
        };
        let track = track.lock();
        match kind {
            Kind::Audio => {
                if let Some(clip) = track.audio.clips.get_mut(clip_index) {
                    clip.fade_enabled = fade_enabled;
                    clip.fade_in_samples = fade_in_samples;
                    clip.fade_out_samples = fade_out_samples;
                }
            }
            Kind::MIDI => {}
        }
    }

    fn set_clip_bounds(
        &self,
        track_name: &str,
        clip_index: usize,
        kind: Kind,
        start: usize,
        length: usize,
        offset: usize,
    ) {
        let Some(track) = self.state.lock().tracks.get(track_name) else {
            return;
        };
        let track = track.lock();
        match kind {
            Kind::Audio => {
                if let Some(clip) = track.audio.clips.get_mut(clip_index) {
                    clip.start = start;
                    clip.end = length.max(1);
                    clip.offset = offset;
                }
            }
            Kind::MIDI => {
                if let Some(clip) = track.midi.clips.get_mut(clip_index) {
                    clip.start = start;
                    clip.end = length.max(1);
                    clip.offset = offset;
                }
            }
        }
    }

    fn set_clip_muted(&self, track_name: &str, clip_index: usize, kind: Kind, muted: bool) {
        let Some(track) = self.state.lock().tracks.get(track_name) else {
            return;
        };
        let track = track.lock();
        match kind {
            Kind::Audio => {
                if let Some(clip) = track.audio.clips.get_mut(clip_index) {
                    clip.muted = muted;
                }
            }
            Kind::MIDI => {
                if let Some(clip) = track.midi.clips.get_mut(clip_index) {
                    clip.muted = muted;
                }
            }
        }
    }

    fn set_audio_clip_warp_markers(
        &self,
        track_name: &str,
        clip_index: usize,
        warp_markers: &[crate::message::AudioWarpMarker],
    ) -> Result<(), String> {
        let track = self.track_handle_or_err(track_name)?;
        let track = track.lock();
        let Some(clip) = track.audio.clips.get_mut(clip_index) else {
            return Err(format!(
                "Audio clip index {} not found on track '{}'",
                clip_index, track_name
            ));
        };
        clip.warp_markers = warp_markers.to_vec();
        Ok(())
    }

    async fn request_hw_cycle(&mut self) {
        if self.awaiting_hwfinished {
            return;
        }
        self.apply_hw_out_gain_and_meter().await;
        if let Some(worker) = &self.hw_worker {
            if !self.pending_hw_midi_out_events_by_device.is_empty() {
                let out_events = std::mem::take(&mut self.pending_hw_midi_out_events_by_device);
                if let Err(e) = worker.tx.send(Message::HWMidiOutEvents(out_events)).await {
                    error!("Error sending HWMidiOutEvents {e}");
                }
            }
            match worker.tx.send(Message::TracksFinished).await {
                Ok(_) => {
                    self.awaiting_hwfinished = true;
                }
                Err(e) => {
                    error!("Error sending TracksFinished {e}");
                }
            }
        }
    }

    fn should_publish_hw_out_meters(&mut self) -> bool {
        let now = Instant::now();
        match self.last_hw_out_meter_publish {
            Some(last) if now.duration_since(last) < Self::METER_PUBLISH_INTERVAL => false,
            _ => {
                self.last_hw_out_meter_publish = Some(now);
                true
            }
        }
    }

    fn should_publish_track_meters(&mut self) -> bool {
        let now = Instant::now();
        match self.last_track_meter_publish {
            Some(last) if now.duration_since(last) < Self::METER_PUBLISH_INTERVAL => false,
            _ => {
                self.last_track_meter_publish = Some(now);
                true
            }
        }
    }

    fn should_publish_hw_out_linear(&mut self, peaks_linear: &[f32]) -> bool {
        #[cfg(any(target_os = "freebsd", target_os = "linux"))]
        {
            self.hw_out_meter_publish_phase = !self.hw_out_meter_publish_phase;
            if !self.hw_out_meter_publish_phase {
                return false;
            }
            let changed = if self.last_hw_out_meter_linear.len() != peaks_linear.len() {
                true
            } else {
                self.last_hw_out_meter_linear
                    .iter()
                    .zip(peaks_linear.iter())
                    .any(|(prev, next)| (prev - next).abs() >= Self::HW_OUT_METER_LINEAR_EPSILON)
            };
            if !changed {
                return false;
            }
            self.last_hw_out_meter_linear.clear();
            self.last_hw_out_meter_linear
                .extend_from_slice(peaks_linear);
            return true;
        }
        #[cfg(not(any(target_os = "freebsd", target_os = "linux")))]
        {
            let _ = peaks_linear;
            true
        }
    }

    async fn maybe_notify_hw_out_meter(&mut self, meter_db: Vec<f32>) {
        #[cfg(any(target_os = "freebsd", target_os = "linux"))]
        {
            let _ = meter_db;
        }
        #[cfg(not(any(target_os = "freebsd", target_os = "linux")))]
        {
            self.notify_clients(Ok(Action::TrackMeters {
                track_name: "hw:out".to_string(),
                output_db: meter_db,
            }))
            .await;
        }
    }

    fn collect_changed_track_meters(
        &mut self,
        tracks: &[(String, Arc<UnsafeMutex<Box<Track>>>)],
    ) -> Vec<(String, Vec<f32>)> {
        #[cfg(any(target_os = "freebsd", target_os = "linux"))]
        {
            let _ = tracks;
            let _ = &self.track_meter_last_published_linear;
            Vec::new()
        }
        #[cfg(not(any(target_os = "freebsd", target_os = "linux")))]
        {
            let mut active_track_names = std::collections::HashSet::with_capacity(tracks.len());
            let meters: Vec<(String, Vec<f32>)> = tracks
                .iter()
                .filter_map(|(name, track)| {
                    active_track_names.insert(name.clone());
                    let linear = self
                        .track_meter_linear_by_track
                        .get(name)
                        .cloned()
                        .unwrap_or_else(|| track.lock().output_meter_linear());
                    let changed =
                        if let Some(prev) = self.track_meter_last_published_linear.get(name) {
                            if prev.len() != linear.len() {
                                true
                            } else {
                                prev.iter()
                                    .zip(linear.iter())
                                    .any(|(a, b)| (a - b).abs() >= Self::TRACK_METER_LINEAR_EPSILON)
                            }
                        } else {
                            true
                        };
                    if !changed {
                        return None;
                    }
                    self.track_meter_last_published_linear
                        .insert(name.clone(), linear.clone());
                    let output_db = linear.into_iter().map(Self::meter_linear_to_db).collect();
                    Some((name.clone(), output_db))
                })
                .collect();
            self.track_meter_last_published_linear
                .retain(|name, _| active_track_names.contains(name));
            self.track_meter_linear_by_track
                .retain(|name, _| active_track_names.contains(name));
            meters
        }
    }

    async fn apply_hw_out_gain_and_meter(&mut self) {
        let gain = if self.hw_out_muted {
            0.0
        } else {
            10.0_f32.powf(self.hw_out_level_db / 20.0)
        };
        let should_notify_interval = self.should_publish_hw_out_meters();
        let peaks_linear = if let Some(oss) = self.hw_driver.clone() {
            {
                let hw = oss.lock();
                hw.set_output_gain_balance(gain, self.hw_out_balance);
            }
            oss.lock().output_meter_linear(gain, self.hw_out_balance)
        } else {
            #[cfg(unix)]
            {
                if let Some(jack) = self.jack_runtime.clone() {
                    jack.lock().set_output_gain_linear(gain);
                    jack.lock().set_output_balance(self.hw_out_balance);
                    let outs = jack.lock().audio_outs.clone();
                    let out_count = outs.len();
                    let b = if out_count == 2 {
                        self.hw_out_balance.clamp(-1.0, 1.0)
                    } else {
                        0.0
                    };
                    let mut meters_linear = Vec::with_capacity(out_count);
                    for (channel_idx, channel) in outs.iter().enumerate() {
                        let balance_gain = if out_count == 2 {
                            if channel_idx == 0 {
                                (1.0 - b).clamp(0.0, 1.0)
                            } else {
                                (1.0 + b).clamp(0.0, 1.0)
                            }
                        } else {
                            1.0
                        };
                        let buf = channel.buffer.lock();
                        let mut peak = 0.0_f32;
                        for &sample in buf.iter() {
                            let v = if sample >= 0.0 { sample } else { -sample };
                            if v > peak {
                                peak = v;
                            }
                        }
                        let peak = peak * gain * balance_gain;
                        meters_linear.push(peak);
                    }
                    meters_linear
                } else {
                    return;
                }
            }
            #[cfg(not(unix))]
            {
                return;
            }
        };
        if self.hw_out_peak_hold_linear.len() != peaks_linear.len() {
            self.hw_out_peak_hold_linear.resize(peaks_linear.len(), 0.0);
        }
        let mut held_peaks = Vec::with_capacity(peaks_linear.len());
        for (idx, peak_now) in peaks_linear.iter().copied().enumerate() {
            let held = self.hw_out_peak_hold_linear[idx] * 0.92;
            let next = peak_now.max(held);
            self.hw_out_peak_hold_linear[idx] = next;
            held_peaks.push(next);
        }
        let should_notify =
            should_notify_interval && self.should_publish_hw_out_linear(&held_peaks);
        let meter_db: Vec<f32> = held_peaks
            .into_iter()
            .map(Self::meter_linear_to_db)
            .collect();
        self.latest_hw_out_meter_db = Arc::new(meter_db.clone());
        if should_notify {
            self.maybe_notify_hw_out_meter(meter_db).await;
        }
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
                t.set_transport_sample(self.transport_sample);
                t.set_loop_config(self.loop_enabled, self.loop_range_samples);
                t.set_transport_timing(self.tempo_bpm, self.tsig_num, self.tsig_denom);
                // Avoid continuously mixing clip audio/MIDI while transport is stopped.
                t.set_clip_playback_enabled(self.clip_playback_enabled && self.playing);
                // Tap buffers are only needed while actively recording.
                t.set_record_tap_enabled(self.playing && self.record_enabled);
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

    async fn publish_track_meters(&mut self) {
        if !self.should_publish_track_meters() {
            return;
        }
        let tracks: Vec<(String, Arc<UnsafeMutex<Box<Track>>>)> = self
            .state
            .lock()
            .tracks
            .iter()
            .map(|(name, track)| (name.clone(), track.clone()))
            .collect();
        let mut snapshot = Vec::with_capacity(tracks.len());
        for (name, track) in &tracks {
            let linear = self
                .track_meter_linear_by_track
                .get(name)
                .cloned()
                .unwrap_or_else(|| track.lock().output_meter_linear());
            let output_db = linear
                .iter()
                .copied()
                .map(Self::meter_linear_to_db)
                .collect::<Vec<_>>();
            snapshot.push((name.clone(), output_db));
        }
        self.latest_track_meter_snapshot = Arc::new(snapshot);
        let meters = self.collect_changed_track_meters(&tracks);
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
            Action::Undo => {
                let Some(actions) = self.history.undo() else {
                    return;
                };
                let was_suspended = self.history_suspended;
                self.history_suspended = true;
                for action in actions {
                    self.handle_request_inner(action, false).await;
                }
                self.history_suspended = was_suspended;
            }
            Action::Redo => {
                let Some(actions) = self.history.redo() else {
                    return;
                };
                let was_suspended = self.history_suspended;
                self.history_suspended = true;
                for action in actions {
                    self.handle_request_inner(action, false).await;
                }
                self.history_suspended = was_suspended;
            }
            other => {
                self.handle_request_inner(other, true).await;
            }
        }
    }

    async fn handle_request_inner(&mut self, action_to_process: Action, record_history: bool) {
        let a = action_to_process.clone();
        let suppress_timing_history = self.playing
            && matches!(
                &action_to_process,
                Action::SetTempo(_) | Action::SetTimeSignature { .. }
            );
        let mut extra_inverse_actions: Vec<Action> = Vec::new();
        if record_history
            && !self.history_suspended
            && let Action::RemoveTrack(ref track_name) = action_to_process
        {
            for route in self
                .midi_hw_in_routes
                .iter()
                .filter(|route| &route.to_track == track_name)
            {
                extra_inverse_actions.push(Action::Connect {
                    from_track: format!("midi:hw:in:{}", route.device),
                    from_port: 0,
                    to_track: route.to_track.clone(),
                    to_port: route.to_port,
                    kind: Kind::MIDI,
                });
            }
            for route in self
                .midi_hw_out_routes
                .iter()
                .filter(|route| &route.from_track == track_name)
            {
                extra_inverse_actions.push(Action::Connect {
                    from_track: route.from_track.clone(),
                    from_port: route.from_port,
                    to_track: format!("midi:hw:out:{}", route.device),
                    to_port: 0,
                    kind: Kind::MIDI,
                });
            }
        }
        if record_history
            && !self.history_suspended
            && matches!(action_to_process, Action::ClearAllMidiLearnBindings)
        {
            if let Some(binding) = self.global_midi_learn_play_pause.clone() {
                extra_inverse_actions.push(Action::SetGlobalMidiLearnBinding {
                    target: crate::message::GlobalMidiLearnTarget::PlayPause,
                    binding: Some(binding),
                });
            }
            if let Some(binding) = self.global_midi_learn_stop.clone() {
                extra_inverse_actions.push(Action::SetGlobalMidiLearnBinding {
                    target: crate::message::GlobalMidiLearnTarget::Stop,
                    binding: Some(binding),
                });
            }
            if let Some(binding) = self.global_midi_learn_record_toggle.clone() {
                extra_inverse_actions.push(Action::SetGlobalMidiLearnBinding {
                    target: crate::message::GlobalMidiLearnTarget::RecordToggle,
                    binding: Some(binding),
                });
            }
        }
        let mut inverse_actions = if record_history
            && !suppress_timing_history
            && should_record(&action_to_process)
            && !self.history_suspended
        {
            let state = self.state.lock();
            create_inverse_actions(&action_to_process, state).map(|mut actions| {
                actions.extend(extra_inverse_actions);
                actions
            })
        } else {
            None
        };
        if record_history && !suppress_timing_history && !self.history_suspended {
            match &action_to_process {
                Action::SetTempo(_) => {
                    inverse_actions = Some(vec![Action::SetTempo(self.tempo_bpm)]);
                }
                Action::SetLoopEnabled(_) => {
                    inverse_actions = Some(vec![Action::SetLoopEnabled(self.loop_enabled)]);
                }
                Action::SetLoopRange(_) => {
                    inverse_actions = Some(vec![
                        Action::SetLoopRange(self.loop_range_samples),
                        Action::SetLoopEnabled(self.loop_enabled),
                    ]);
                }
                Action::SetPunchEnabled(_) => {
                    inverse_actions = Some(vec![Action::SetPunchEnabled(self.punch_enabled)]);
                }
                Action::SetPunchRange(_) => {
                    inverse_actions = Some(vec![
                        Action::SetPunchRange(self.punch_range_samples),
                        Action::SetPunchEnabled(self.punch_enabled),
                    ]);
                }
                Action::SetMetronomeEnabled(_) => {
                    inverse_actions =
                        Some(vec![Action::SetMetronomeEnabled(self.metronome_enabled)]);
                }
                Action::SetTimeSignature { .. } => {
                    inverse_actions = Some(vec![Action::SetTimeSignature {
                        numerator: self.tsig_num,
                        denominator: self.tsig_denom,
                    }]);
                }
                Action::SetClipPlaybackEnabled(_) => {
                    inverse_actions = Some(vec![Action::SetClipPlaybackEnabled(
                        self.clip_playback_enabled,
                    )]);
                }
                Action::SetRecordEnabled(_) => {
                    inverse_actions = Some(vec![Action::SetRecordEnabled(self.record_enabled)]);
                }
                Action::SetGlobalMidiLearnBinding { target, .. } => {
                    let binding = match target {
                        crate::message::GlobalMidiLearnTarget::PlayPause => {
                            self.global_midi_learn_play_pause.clone()
                        }
                        crate::message::GlobalMidiLearnTarget::Stop => {
                            self.global_midi_learn_stop.clone()
                        }
                        crate::message::GlobalMidiLearnTarget::RecordToggle => {
                            self.global_midi_learn_record_toggle.clone()
                        }
                    };
                    inverse_actions = Some(vec![Action::SetGlobalMidiLearnBinding {
                        target: *target,
                        binding,
                    }]);
                }
                _ => {}
            }
        }

        match action_to_process {
            Action::Play => {
                self.playing = true;
                if let Some(driver) = self.hw_driver.as_mut() {
                    driver.lock().set_playing(true);
                }
                self.notify_clients(Ok(Action::TransportPosition(self.transport_sample)))
                    .await;
            }
            Action::Stop => {
                self.playing = false;
                if let Some(driver) = self.hw_driver.as_mut() {
                    driver.lock().set_playing(false);
                }
                self.flush_recordings().await;
                self.notify_clients(Ok(Action::TransportPosition(self.transport_sample)))
                    .await;
            }
            Action::SetClipPlaybackEnabled(enabled) => {
                self.clip_playback_enabled = enabled;
                for track in self.state.lock().tracks.values() {
                    track.lock().set_clip_playback_enabled(enabled);
                }
            }
            Action::TransportPosition(sample) => {
                self.transport_sample = self.normalize_transport_sample(sample);
            }
            Action::SetLoopEnabled(enabled) => {
                self.loop_enabled = enabled && self.loop_range_samples.is_some();
            }
            Action::SetLoopRange(range) => {
                self.loop_range_samples = range.and_then(|(start, end)| {
                    if end > start {
                        Some((start, end))
                    } else {
                        None
                    }
                });
                self.loop_enabled = self.loop_range_samples.is_some();
                if self.loop_enabled
                    && let Some((loop_start, loop_end)) = self.loop_range_samples
                    && self.transport_sample >= loop_end
                {
                    self.transport_sample = loop_start;
                    self.notify_clients(Ok(Action::TransportPosition(self.transport_sample)))
                        .await;
                }
            }
            Action::SetPunchEnabled(enabled) => {
                self.punch_enabled = enabled && self.punch_range_samples.is_some();
            }
            Action::SetPunchRange(range) => {
                self.punch_range_samples = range.and_then(|(start, end)| {
                    if end > start {
                        Some((start, end))
                    } else {
                        None
                    }
                });
                self.punch_enabled = self.punch_range_samples.is_some();
            }
            Action::SetMetronomeEnabled(enabled) => {
                self.metronome_enabled = enabled;
                if enabled {
                    self.ensure_metronome_track().await;
                }
                if let Some(track) = self.state.lock().tracks.get(Self::METRONOME_TRACK).cloned() {
                    track.lock().set_metronome_enabled(enabled);
                }
            }
            Action::SetTempo(bpm) => {
                self.tempo_bpm = bpm.max(1.0);
            }
            Action::SetTimeSignature {
                numerator,
                denominator,
            } => {
                self.tsig_num = numerator.max(1);
                self.tsig_denom = denominator.max(1);
            }
            Action::SetRecordEnabled(enabled) => {
                self.record_enabled = enabled;
                if !enabled {
                    // If a HW cycle is currently in-flight, capture its recorded taps
                    // before flushing recordings to disk.
                    if self.awaiting_hwfinished {
                        self.append_recorded_cycle();
                    }
                    self.flush_recordings().await;
                } else if self.session_dir.is_none() {
                    self.notify_clients(Err(
                        "Recording enabled but session path is not set".to_string()
                    ))
                    .await;
                }
            }
            Action::BeginHistoryGroup => {
                if self.history_group.is_none() {
                    self.history_group = Some(UndoEntry {
                        forward_actions: vec![],
                        inverse_actions: vec![],
                    });
                }
            }
            Action::EndHistoryGroup => {
                if let Some(group) = self.history_group.take()
                    && !group.forward_actions.is_empty()
                    && !group.inverse_actions.is_empty()
                {
                    self.history.record(group);
                }
            }
            Action::SetSessionPath(ref path) => {
                self.session_dir = Some(Path::new(path).to_path_buf());
                self.ensure_session_subdirs();
                #[cfg(all(unix, not(target_os = "macos")))]
                let lv2_dir = self.session_plugins_dir();
                for track in self.state.lock().tracks.values() {
                    #[cfg(all(unix, not(target_os = "macos")))]
                    track.lock().set_lv2_state_base_dir(lv2_dir.clone());
                    track.lock().set_session_base_dir(self.session_dir.clone());
                }
            }
            Action::ClearHistory => {
                self.history.clear();
            }
            Action::BeginSessionRestore => {
                self.history_suspended = true;
                self.history.clear();
            }
            Action::EndSessionRestore => {
                self.history.clear();
                self.history_suspended = false;
            }
            Action::Quit => {
                self.flush_recordings().await;
                if let Some(worker) = self.hw_worker.take() {
                    worker
                        .tx
                        .send(Message::Request(a.clone()))
                        .await
                        .expect("Failed sending quit message to HW worker");
                    worker
                        .handle
                        .await
                        .expect("Failed waiting for HW worker to quit");
                }
                #[cfg(unix)]
                {
                    self.jack_runtime = None;
                }

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
                let maybe_hw = if let Some(oss) = &self.hw_driver {
                    let hw = oss.lock();
                    Some((hw.cycle_samples(), hw.sample_rate() as f64))
                } else {
                    #[cfg(unix)]
                    {
                        if let Some(jack) = &self.jack_runtime {
                            let j = jack.lock();
                            Some((j.buffer_size, j.sample_rate as f64))
                        } else {
                            None
                        }
                    }
                    #[cfg(not(unix))]
                    {
                        None
                    }
                };

                if let Some((chsamples, sample_rate)) = maybe_hw {
                    tracks.insert(
                        name.clone(),
                        Arc::new(UnsafeMutex::new(Box::new(Track::new(
                            name.clone(),
                            audio_ins,
                            audio_outs,
                            midi_ins,
                            midi_outs,
                            chsamples,
                            sample_rate,
                        )))),
                    );
                    if let Some(track) = tracks.get(name) {
                        track.lock().ensure_default_audio_passthrough();
                        track.lock().ensure_default_midi_passthrough();
                        track
                            .lock()
                            .set_clip_playback_enabled(self.clip_playback_enabled);
                        track.lock().set_transport_timing(
                            self.tempo_bpm,
                            self.tsig_num,
                            self.tsig_denom,
                        );
                        #[cfg(all(unix, not(target_os = "macos")))]
                        {
                            let lv2_dir = self.session_plugins_dir();
                            track.lock().set_lv2_state_base_dir(lv2_dir);
                        }
                        track.lock().set_session_base_dir(self.session_dir.clone());
                    }
                } else {
                    self.notify_clients(Err(
                        "Engine needs to open audio device before adding audio track".to_string(),
                    ))
                    .await;
                }
            }
            Action::TrackAddAudioInput(ref name) => {
                let track = match self.track_handle_or_err(name) {
                    Ok(track) => track,
                    Err(e) => {
                        self.notify_clients(Err(e)).await;
                        return;
                    }
                };
                if let Err(e) = track.lock().add_audio_input() {
                    self.notify_clients(Err(e)).await;
                    return;
                }
            }
            Action::TrackAddAudioOutput(ref name) => {
                let track = match self.track_handle_or_err(name) {
                    Ok(track) => track,
                    Err(e) => {
                        self.notify_clients(Err(e)).await;
                        return;
                    }
                };
                if let Err(e) = track.lock().add_audio_output() {
                    self.notify_clients(Err(e)).await;
                    return;
                }
            }
            Action::TrackRemoveAudioInput(ref name) => {
                let track = match self.track_handle_or_err(name) {
                    Ok(track) => track,
                    Err(e) => {
                        self.notify_clients(Err(e)).await;
                        return;
                    }
                };
                if let Err(e) = track.lock().remove_audio_input() {
                    self.notify_clients(Err(e)).await;
                    return;
                }
            }
            Action::TrackRemoveAudioOutput(ref name) => {
                let track = match self.track_handle_or_err(name) {
                    Ok(track) => track,
                    Err(e) => {
                        self.notify_clients(Err(e)).await;
                        return;
                    }
                };
                let (hw_outputs, track_inputs) = {
                    let state = self.state.lock();
                    let hw_outputs = self.all_hw_output_audio_ports();
                    let track_inputs = state
                        .tracks
                        .iter()
                        .filter(|(track_name, _)| *track_name != name)
                        .flat_map(|(_, handle)| handle.lock().audio.ins.clone())
                        .collect::<Vec<_>>();
                    (hw_outputs, track_inputs)
                };
                if let Err(e) = track.lock().remove_audio_output(&hw_outputs, &track_inputs) {
                    self.notify_clients(Err(e)).await;
                    return;
                }
            }
            Action::RenameTrack {
                ref old_name,
                ref new_name,
            } => {
                // Check if new name already exists
                if self.state.lock().tracks.contains_key(new_name) {
                    self.notify_clients(Err(format!("Track '{}' already exists", new_name)))
                        .await;
                    return;
                }

                // Get the track and update its name
                let Some(track) = self.state.lock().tracks.remove(old_name) else {
                    self.notify_clients(Err(format!("Track '{}' not found", old_name)))
                        .await;
                    return;
                };

                track.lock().name = new_name.clone();
                self.state.lock().tracks.insert(new_name.clone(), track);
                for other in self.state.lock().tracks.values() {
                    let other = other.lock();
                    if other.vca_master.as_deref() == Some(old_name.as_str()) {
                        other.set_vca_master(Some(new_name.clone()));
                    }
                }

                // Update recording references
                if let Some(recording) = self.audio_recordings.remove(old_name) {
                    self.audio_recordings.insert(new_name.clone(), recording);
                }
                if let Some(recording) = self.midi_recordings.remove(old_name) {
                    self.midi_recordings.insert(new_name.clone(), recording);
                }

                // Update MIDI routing references
                for route in &mut self.midi_hw_in_routes {
                    if route.to_track == *old_name {
                        route.to_track = new_name.clone();
                    }
                }
                for route in &mut self.midi_hw_out_routes {
                    if route.from_track == *old_name {
                        route.from_track = new_name.clone();
                    }
                }
                if let Some((armed_track, target, device)) = self.pending_midi_learn.clone()
                    && armed_track == *old_name
                {
                    self.pending_midi_learn = Some((new_name.clone(), target, device));
                }

                self.notify_clients(Ok(Action::RenameTrack {
                    old_name: old_name.clone(),
                    new_name: new_name.clone(),
                }))
                .await;
            }
            Action::RemoveTrack(ref name) => {
                self.state.lock().tracks.remove(name);
                self.audio_recordings.remove(name);
                self.midi_recordings.remove(name);
                self.midi_hw_in_routes.retain(|r| r.to_track != *name);
                self.midi_hw_out_routes.retain(|r| r.from_track != *name);
                if self
                    .pending_midi_learn
                    .as_ref()
                    .is_some_and(|(track_name, _, _)| track_name == name)
                {
                    self.pending_midi_learn = None;
                }
                for track in self.state.lock().tracks.values() {
                    let track = track.lock();
                    if track.vca_master.as_deref() == Some(name.as_str()) {
                        track.set_vca_master(None);
                    }
                }
            }
            Action::TrackLevel(ref name, level) => {
                if name == "hw:out" {
                    self.hw_out_level_db = level;
                } else if let Some(track) = self.state.lock().tracks.get(name) {
                    let previous = track.lock().level();
                    track.lock().set_level(level);
                    let delta = level - previous;
                    if delta.abs() > f32::EPSILON {
                        for follower_name in self.vca_followers(name) {
                            if let Some(follower) = self.state.lock().tracks.get(&follower_name) {
                                let next = (follower.lock().level() + delta).clamp(-90.0, 20.0);
                                follower.lock().set_level(next);
                                self.notify_clients(Ok(Action::TrackLevel(
                                    follower_name.clone(),
                                    next,
                                )))
                                .await;
                            }
                        }
                    }
                }
            }
            Action::TrackBalance(ref name, balance) => {
                if name == "hw:out" {
                    self.hw_out_balance = balance.clamp(-1.0, 1.0);
                } else if let Some(track) = self.state.lock().tracks.get(name) {
                    track.lock().set_balance(balance);
                }
            }
            Action::TrackAutomationLevel(ref name, level) => {
                if let Some(track) = self.state.lock().tracks.get(name) {
                    let previous = track.lock().level();
                    track.lock().set_level(level);
                    let delta = level - previous;
                    if delta.abs() > f32::EPSILON {
                        for follower_name in self.vca_followers(name) {
                            if let Some(follower) = self.state.lock().tracks.get(&follower_name) {
                                let next = (follower.lock().level() + delta).clamp(-90.0, 20.0);
                                follower.lock().set_level(next);
                                self.notify_clients(Ok(Action::TrackAutomationLevel(
                                    follower_name.clone(),
                                    next,
                                )))
                                .await;
                            }
                        }
                    }
                }
            }
            Action::TrackAutomationBalance(ref name, balance) => {
                if let Some(track) = self.state.lock().tracks.get(name) {
                    track.lock().set_balance(balance);
                }
            }
            Action::TrackAutomationMute(ref name, muted) => {
                if let Some(track) = self.state.lock().tracks.get(name) {
                    track.lock().set_muted(muted);
                    for follower_name in self.vca_followers(name) {
                        if let Some(follower) = self.state.lock().tracks.get(&follower_name) {
                            follower.lock().set_muted(muted);
                            self.notify_clients(Ok(Action::TrackAutomationMute(
                                follower_name.clone(),
                                muted,
                            )))
                            .await;
                        }
                    }
                }
            }
            Action::RequestMeterSnapshot => {
                self.notify_clients(Ok(Action::MeterSnapshot {
                    hw_out_db: self.latest_hw_out_meter_db.clone(),
                    track_meters: self.latest_track_meter_snapshot.clone(),
                }))
                .await;
                return;
            }
            Action::TrackMeters { .. } => {}
            Action::MeterSnapshot { .. } => {}
            Action::TrackToggleArm(ref name) => {
                if self.reject_if_track_frozen(name, "arming/disarming").await {
                    return;
                }
                if let Some(track) = self.state.lock().tracks.get(name).cloned() {
                    track.lock().arm();
                    if !track.lock().armed && self.audio_recordings.contains_key(name) {
                        self.flush_track_recording(name).await;
                    }
                }
            }
            Action::TrackToggleMute(ref name) => {
                if name == "hw:out" {
                    self.hw_out_muted = !self.hw_out_muted;
                } else if let Some(track) = self.state.lock().tracks.get(name) {
                    track.lock().mute();
                    let muted = track.lock().muted;
                    for follower_name in self.vca_followers(name) {
                        if let Some(follower) = self.state.lock().tracks.get(&follower_name)
                            && follower.lock().muted != muted
                        {
                            follower.lock().set_muted(muted);
                            self.notify_clients(Ok(Action::TrackToggleMute(follower_name.clone())))
                                .await;
                        }
                    }
                }
            }
            Action::TrackToggleSolo(ref name) => {
                if name == "hw:out" {
                    return;
                }
                if let Some(track) = self.state.lock().tracks.get(name) {
                    track.lock().solo();
                    let soloed = track.lock().soloed;
                    for follower_name in self.vca_followers(name) {
                        if let Some(follower) = self.state.lock().tracks.get(&follower_name)
                            && follower.lock().soloed != soloed
                        {
                            follower.lock().solo();
                            self.notify_clients(Ok(Action::TrackToggleSolo(follower_name.clone())))
                                .await;
                        }
                    }
                }
            }
            Action::TrackToggleInputMonitor(ref name) => {
                if let Some(track) = self.state.lock().tracks.get(name) {
                    track.lock().toggle_input_monitor();
                }
            }
            Action::TrackToggleDiskMonitor(ref name) => {
                if let Some(track) = self.state.lock().tracks.get(name) {
                    track.lock().toggle_disk_monitor();
                }
            }
            Action::TrackArmMidiLearn {
                ref track_name,
                target,
            } => {
                if let Err(e) = self.track_handle_or_err(track_name) {
                    self.notify_clients(Err(e)).await;
                    return;
                }
                self.pending_midi_learn = Some((track_name.clone(), target, None));
            }
            Action::GlobalArmMidiLearn { target } => {
                self.pending_global_midi_learn = Some(target);
            }
            Action::TrackSetMidiLearnBinding {
                ref track_name,
                target,
                ref binding,
            } => {
                if let Some(binding) = binding.as_ref() {
                    let conflicts = self.midi_learn_slot_conflicts(
                        binding,
                        Some(MidiLearnSlot::Track(track_name.clone(), target)),
                    );
                    if !conflicts.is_empty() {
                        self.notify_clients(Err(format!(
                            "MIDI learn conflict for '{}' {:?}: {}",
                            track_name,
                            target,
                            conflicts.join(", ")
                        )))
                        .await;
                        return;
                    }
                }
                let track = match self.track_handle_or_err(track_name) {
                    Ok(track) => track,
                    Err(e) => {
                        self.notify_clients(Err(e)).await;
                        return;
                    }
                };
                match target {
                    crate::message::TrackMidiLearnTarget::Volume => {
                        track.lock().midi_learn_volume = binding.clone();
                    }
                    crate::message::TrackMidiLearnTarget::Balance => {
                        track.lock().midi_learn_balance = binding.clone();
                    }
                    crate::message::TrackMidiLearnTarget::Mute => {
                        track.lock().midi_learn_mute = binding.clone();
                    }
                    crate::message::TrackMidiLearnTarget::Solo => {
                        track.lock().midi_learn_solo = binding.clone();
                    }
                    crate::message::TrackMidiLearnTarget::Arm => {
                        track.lock().midi_learn_arm = binding.clone();
                    }
                    crate::message::TrackMidiLearnTarget::InputMonitor => {
                        track.lock().midi_learn_input_monitor = binding.clone();
                    }
                    crate::message::TrackMidiLearnTarget::DiskMonitor => {
                        track.lock().midi_learn_disk_monitor = binding.clone();
                    }
                }
            }
            Action::SetGlobalMidiLearnBinding {
                target,
                ref binding,
            } => {
                if let Some(binding) = binding.as_ref() {
                    let conflicts = self
                        .midi_learn_slot_conflicts(binding, Some(MidiLearnSlot::Global(target)));
                    if !conflicts.is_empty() {
                        self.notify_clients(Err(format!(
                            "Global MIDI learn conflict for {:?}: {}",
                            target,
                            conflicts.join(", ")
                        )))
                        .await;
                        return;
                    }
                }
                match target {
                    crate::message::GlobalMidiLearnTarget::PlayPause => {
                        self.global_midi_learn_play_pause = binding.clone();
                    }
                    crate::message::GlobalMidiLearnTarget::Stop => {
                        self.global_midi_learn_stop = binding.clone();
                    }
                    crate::message::GlobalMidiLearnTarget::RecordToggle => {
                        self.global_midi_learn_record_toggle = binding.clone();
                    }
                }
            }
            Action::TrackSetVcaMaster {
                ref track_name,
                ref master_track,
            } => {
                let track = match self.track_handle_or_err(track_name) {
                    Ok(track) => track,
                    Err(e) => {
                        self.notify_clients(Err(e)).await;
                        return;
                    }
                };
                if let Some(master_name) = master_track {
                    if master_name == track_name {
                        self.notify_clients(Err("Track cannot be its own VCA master".to_string()))
                            .await;
                        return;
                    }
                }
                track.lock().set_vca_master(master_track.clone());
            }
            Action::TrackSetFrozen {
                ref track_name,
                frozen,
            } => {
                let track = match self.track_handle_or_err(track_name) {
                    Ok(track) => track,
                    Err(e) => {
                        self.notify_clients(Err(e)).await;
                        return;
                    }
                };
                track.lock().set_frozen(frozen);
            }
            Action::TrackOfflineBounce {
                track_name,
                output_path,
                start_sample,
                length_samples,
                automation_lanes,
            } => {
                if self.offline_bounce_job.is_some() {
                    self.notify_clients(Err(
                        "Another offline bounce is already in progress".to_string()
                    ))
                    .await;
                    return;
                }
                if let Err(e) = self.track_handle_or_err(&track_name) {
                    self.notify_clients(Err(e)).await;
                    return;
                }
                if length_samples == 0 {
                    self.notify_clients(Err(format!(
                        "Track '{}' has no renderable content for offline bounce",
                        track_name
                    )))
                    .await;
                    return;
                }
                if self.ready_workers.is_empty() {
                    self.pending_requests
                        .push_front(Action::TrackOfflineBounce {
                            track_name,
                            output_path,
                            start_sample,
                            length_samples,
                            automation_lanes,
                        });
                    return;
                }
                let cancel = Arc::new(AtomicBool::new(false));
                self.offline_bounce_job = Some(OfflineBounceJob {
                    track_name: track_name.clone(),
                    cancel: cancel.clone(),
                });
                let worker_index = self.ready_workers.remove(0);
                let worker = &self.workers[worker_index];
                let job = crate::message::OfflineBounceWork {
                    state: self.state.clone(),
                    track_name,
                    output_path,
                    start_sample,
                    length_samples,
                    tempo_bpm: self.tempo_bpm,
                    tsig_num: self.tsig_num,
                    tsig_denom: self.tsig_denom,
                    automation_lanes,
                    cancel,
                };
                if let Err(e) = worker.tx.send(Message::ProcessOfflineBounce(job)).await {
                    self.offline_bounce_job = None;
                    self.notify_clients(Err(format!("Failed to schedule offline bounce: {e}")))
                        .await;
                }
                return;
            }
            Action::TrackOfflineBounceCancel { .. } => {}
            Action::TrackOfflineBounceCanceled { .. } => {}
            Action::TrackOfflineBounceProgress { .. } => {}
            Action::PianoKey {
                ref track_name,
                note,
                velocity,
                on,
            } => {
                if let Some(track) = self.state.lock().tracks.get(track_name) {
                    let status = if on { 0x90 } else { 0x80 };
                    let event = MidiEvent::new(0, vec![status, note.min(127), velocity.min(127)]);
                    track.lock().push_hw_midi_events(&[event]);
                }
            }
            Action::ModifyMidiNotes { .. }
            | Action::ModifyMidiControllers { .. }
            | Action::DeleteMidiControllers { .. }
            | Action::InsertMidiControllers { .. }
            | Action::DeleteMidiNotes { .. }
            | Action::InsertMidiNotes { .. } => {
                if let Err(e) = self.apply_midi_edit_action(&action_to_process) {
                    self.notify_clients(Err(e)).await;
                    return;
                }
            }
            Action::SetMidiSysExEvents { .. } => {
                if let Err(e) = self.apply_midi_edit_action(&action_to_process) {
                    self.notify_clients(Err(e)).await;
                    return;
                }
            }
            #[cfg(all(unix, not(target_os = "macos")))]
            Action::TrackLoadLv2Plugin {
                ref track_name,
                ref plugin_uri,
            } => {
                if self
                    .reject_if_track_frozen(track_name, "LV2 plugin loading")
                    .await
                {
                    return;
                }
                let track = match self.track_handle_or_err(track_name) {
                    Ok(track) => track,
                    Err(e) => {
                        self.notify_clients(Err(e)).await;
                        return;
                    }
                };
                if let Err(e) = track.lock().load_lv2_plugin(plugin_uri) {
                    self.notify_clients(Err(e)).await;
                    return;
                }
            }
            Action::TrackClearDefaultPassthrough { ref track_name } => {
                if self
                    .reject_if_track_frozen(track_name, "plugin graph editing")
                    .await
                {
                    return;
                }
                let track = match self.track_handle_or_err(track_name) {
                    Ok(track) => track,
                    Err(e) => {
                        self.notify_clients(Err(e)).await;
                        return;
                    }
                };
                track.lock().clear_default_passthrough();
            }
            #[cfg(all(unix, not(target_os = "macos")))]
            Action::TrackSetLv2PluginState {
                ref track_name,
                instance_id,
                ref state,
            } => {
                if self
                    .reject_if_track_frozen(track_name, "LV2 plugin state changes")
                    .await
                {
                    return;
                }
                let track = match self.track_handle_or_err(track_name) {
                    Ok(track) => track,
                    Err(e) => {
                        self.notify_clients(Err(e)).await;
                        return;
                    }
                };
                if let Err(e) = track
                    .lock()
                    .set_lv2_plugin_state(instance_id, state.clone())
                {
                    self.notify_clients(Err(e)).await;
                    return;
                }
            }
            #[cfg(all(unix, not(target_os = "macos")))]
            Action::TrackUnloadLv2PluginInstance {
                ref track_name,
                instance_id,
            } => {
                if self
                    .reject_if_track_frozen(track_name, "LV2 plugin unloading")
                    .await
                {
                    return;
                }
                let track = match self.track_handle_or_err(track_name) {
                    Ok(track) => track,
                    Err(e) => {
                        self.notify_clients(Err(e)).await;
                        return;
                    }
                };
                if let Err(e) = track.lock().unload_lv2_plugin_instance(instance_id) {
                    self.notify_clients(Err(e)).await;
                    return;
                }
            }
            #[cfg(all(unix, not(target_os = "macos")))]
            Action::TrackGetLv2Midnam { ref track_name } => {
                let track = match self.track_handle_or_err(track_name) {
                    Ok(track) => track,
                    Err(e) => {
                        self.notify_clients(Err(e)).await;
                        return;
                    }
                };
                let note_names = track.lock().get_lv2_midnam();
                self.notify_clients(Ok(Action::TrackLv2Midnam {
                    track_name: track_name.clone(),
                    note_names,
                }))
                .await;
            }
            #[cfg(all(unix, not(target_os = "macos")))]
            Action::TrackGetLv2PluginControls {
                ref track_name,
                instance_id,
            } => {
                let track = match self.track_handle_or_err(track_name) {
                    Ok(track) => track,
                    Err(e) => {
                        self.notify_clients(Err(e)).await;
                        return;
                    }
                };
                let (controls, instance_access_handle) =
                    match track.lock().lv2_plugin_controls(instance_id) {
                        Ok(result) => result,
                        Err(e) => {
                            self.notify_clients(Err(e)).await;
                            return;
                        }
                    };
                self.notify_clients(Ok(Action::TrackLv2PluginControls {
                    track_name: track_name.clone(),
                    instance_id,
                    controls,
                    instance_access_handle,
                }))
                .await;
                return;
            }
            #[cfg(all(unix, not(target_os = "macos")))]
            Action::TrackSetLv2ControlValue {
                ref track_name,
                instance_id,
                index,
                value,
            } => {
                if self
                    .reject_if_track_frozen(track_name, "LV2 parameter changes")
                    .await
                {
                    return;
                }
                let track = match self.track_handle_or_err(track_name) {
                    Ok(track) => track,
                    Err(e) => {
                        self.notify_clients(Err(e)).await;
                        return;
                    }
                };
                if let Err(e) = track
                    .lock()
                    .set_lv2_control_value(instance_id, index, value)
                {
                    self.notify_clients(Err(e)).await;
                    return;
                }
            }
            #[cfg(unix)]
            Action::TrackGetPluginGraph { ref track_name } => {
                let track = match self.track_handle_or_err(track_name) {
                    Ok(track) => track,
                    Err(e) => {
                        self.notify_clients(Err(e)).await;
                        return;
                    }
                };
                let (plugins, connections) = {
                    let track = track.lock();
                    (
                        track.plugin_graph_plugins(),
                        track.plugin_graph_connections(),
                    )
                };
                self.notify_clients(Ok(Action::TrackPluginGraph {
                    track_name: track_name.clone(),
                    plugins,
                    connections,
                }))
                .await;
                return;
            }
            #[cfg(unix)]
            Action::TrackPluginGraph { .. } => {}
            #[cfg(unix)]
            Action::TrackConnectPluginAudio {
                ref track_name,
                ref from_node,
                from_port,
                ref to_node,
                to_port,
            } => {
                if self
                    .reject_if_track_frozen(track_name, "plugin routing changes")
                    .await
                {
                    return;
                }
                let track = match self.track_handle_or_err(track_name) {
                    Ok(track) => track,
                    Err(e) => {
                        self.notify_clients(Err(e)).await;
                        return;
                    }
                };
                if let Err(e) = track.lock().connect_plugin_audio(
                    from_node.clone(),
                    from_port,
                    to_node.clone(),
                    to_port,
                ) {
                    self.notify_clients(Err(e)).await;
                    return;
                }
            }
            #[cfg(unix)]
            Action::TrackConnectPluginMidi {
                ref track_name,
                ref from_node,
                from_port,
                ref to_node,
                to_port,
            } => {
                if self
                    .reject_if_track_frozen(track_name, "plugin routing changes")
                    .await
                {
                    return;
                }
                let track = match self.track_handle_or_err(track_name) {
                    Ok(track) => track,
                    Err(e) => {
                        self.notify_clients(Err(e)).await;
                        return;
                    }
                };
                if let Err(e) = track.lock().connect_plugin_midi(
                    from_node.clone(),
                    from_port,
                    to_node.clone(),
                    to_port,
                ) {
                    self.notify_clients(Err(e)).await;
                    return;
                }
            }
            #[cfg(unix)]
            Action::TrackDisconnectPluginAudio {
                ref track_name,
                ref from_node,
                from_port,
                ref to_node,
                to_port,
            } => {
                if self
                    .reject_if_track_frozen(track_name, "plugin routing changes")
                    .await
                {
                    return;
                }
                let track = match self.track_handle_or_err(track_name) {
                    Ok(track) => track,
                    Err(e) => {
                        self.notify_clients(Err(e)).await;
                        return;
                    }
                };
                if let Err(e) = track.lock().disconnect_plugin_audio(
                    from_node.clone(),
                    from_port,
                    to_node.clone(),
                    to_port,
                ) {
                    self.notify_clients(Err(e)).await;
                    return;
                }
            }
            #[cfg(unix)]
            Action::TrackDisconnectPluginMidi {
                ref track_name,
                ref from_node,
                from_port,
                ref to_node,
                to_port,
            } => {
                if self
                    .reject_if_track_frozen(track_name, "plugin routing changes")
                    .await
                {
                    return;
                }
                let track = match self.track_handle_or_err(track_name) {
                    Ok(track) => track,
                    Err(e) => {
                        self.notify_clients(Err(e)).await;
                        return;
                    }
                };
                if let Err(e) = track.lock().disconnect_plugin_midi(
                    from_node.clone(),
                    from_port,
                    to_node.clone(),
                    to_port,
                ) {
                    self.notify_clients(Err(e)).await;
                    return;
                }
            }
            #[cfg(all(unix, not(target_os = "macos")))]
            Action::ListLv2Plugins => {
                let plugins = {
                    let host = crate::lv2::Lv2Host::new(48_000.0);
                    host.list_plugins()
                };
                self.notify_clients(Ok(Action::Lv2Plugins(plugins))).await;
                return;
            }
            #[cfg(all(unix, not(target_os = "macos")))]
            Action::Lv2Plugins(_) => {}
            Action::ListVst3Plugins => {
                self.notify_clients(Ok(Action::Vst3Plugins(crate::vst3::list_plugins())))
                    .await;
                return;
            }
            Action::Vst3Plugins(_) => {}
            Action::ListClapPlugins => {
                self.notify_clients(Ok(Action::ClapPlugins(crate::clap::list_plugins())))
                    .await;
                return;
            }
            Action::ListClapPluginsWithCapabilities => {
                self.notify_clients(Ok(Action::ClapPlugins(
                    crate::clap::list_plugins_with_capabilities(true),
                )))
                .await;
                return;
            }
            Action::ClapPlugins(_) => {}
            Action::TrackLoadClapPlugin {
                ref track_name,
                ref plugin_path,
            } => {
                if self
                    .reject_if_track_frozen(track_name, "CLAP plugin loading")
                    .await
                {
                    return;
                }
                let track = match self.track_handle_or_err(track_name) {
                    Ok(track) => track,
                    Err(e) => {
                        self.notify_clients(Err(e)).await;
                        return;
                    }
                };
                if let Err(e) = track.lock().load_clap_plugin(plugin_path) {
                    self.notify_clients(Err(e)).await;
                    return;
                }
            }
            Action::TrackUnloadClapPlugin {
                ref track_name,
                ref plugin_path,
            } => {
                if self
                    .reject_if_track_frozen(track_name, "CLAP plugin unloading")
                    .await
                {
                    return;
                }
                let track = match self.track_handle_or_err(track_name) {
                    Ok(track) => track,
                    Err(e) => {
                        self.notify_clients(Err(e)).await;
                        return;
                    }
                };
                if let Err(e) = track.lock().unload_clap_plugin(plugin_path) {
                    self.notify_clients(Err(e)).await;
                    return;
                }
            }
            Action::TrackSetClapParameter {
                ref track_name,
                instance_id,
                param_id,
                value,
            } => {
                if self
                    .reject_if_track_frozen(track_name, "CLAP parameter changes")
                    .await
                {
                    return;
                }
                match self.track_handle_or_err(track_name) {
                    Ok(track) => {
                        if let Err(e) =
                            track
                                .lock()
                                .set_clap_parameter(instance_id, param_id, value)
                        {
                            self.notify_clients(Err(e)).await;
                            return;
                        }
                        self.notify_clients(Ok(a.clone())).await;
                    }
                    Err(e) => {
                        self.notify_clients(Err(e)).await;
                    }
                }
            }
            Action::TrackSetClapParameterAt {
                ref track_name,
                instance_id,
                param_id,
                value,
                frame,
            } => {
                if self
                    .reject_if_track_frozen(track_name, "CLAP parameter changes")
                    .await
                {
                    return;
                }
                match self.track_handle_or_err(track_name) {
                    Ok(track) => {
                        if let Err(e) =
                            track
                                .lock()
                                .set_clap_parameter_at(instance_id, param_id, value, frame)
                        {
                            self.notify_clients(Err(e)).await;
                            return;
                        }
                        self.notify_clients(Ok(a.clone())).await;
                    }
                    Err(e) => {
                        self.notify_clients(Err(e)).await;
                    }
                }
            }
            Action::TrackBeginClapParameterEdit {
                ref track_name,
                instance_id,
                param_id,
                frame,
            } => {
                if self
                    .reject_if_track_frozen(track_name, "CLAP parameter edit gestures")
                    .await
                {
                    return;
                }
                match self.track_handle_or_err(track_name) {
                    Ok(track) => {
                        if let Err(e) =
                            track
                                .lock()
                                .begin_clap_parameter_edit(instance_id, param_id, frame)
                        {
                            self.notify_clients(Err(e)).await;
                            return;
                        }
                        self.notify_clients(Ok(a.clone())).await;
                    }
                    Err(e) => {
                        self.notify_clients(Err(e)).await;
                    }
                }
            }
            Action::TrackEndClapParameterEdit {
                ref track_name,
                instance_id,
                param_id,
                frame,
            } => {
                if self
                    .reject_if_track_frozen(track_name, "CLAP parameter edit gestures")
                    .await
                {
                    return;
                }
                match self.track_handle_or_err(track_name) {
                    Ok(track) => {
                        if let Err(e) =
                            track
                                .lock()
                                .end_clap_parameter_edit(instance_id, param_id, frame)
                        {
                            self.notify_clients(Err(e)).await;
                            return;
                        }
                        self.notify_clients(Ok(a.clone())).await;
                    }
                    Err(e) => {
                        self.notify_clients(Err(e)).await;
                    }
                }
            }
            Action::TrackGetClapParameters {
                ref track_name,
                instance_id,
            } => match self.track_handle_or_err(track_name) {
                Ok(track) => match track.lock().get_clap_parameters(instance_id) {
                    Ok(parameters) => {
                        self.notify_clients(Ok(Action::TrackClapParameters {
                            track_name: track_name.clone(),
                            instance_id,
                            parameters,
                        }))
                        .await;
                    }
                    Err(e) => {
                        self.notify_clients(Err(e)).await;
                    }
                },
                Err(e) => {
                    self.notify_clients(Err(e)).await;
                }
            },
            Action::TrackClapParameters { .. } => {}
            Action::TrackClapSnapshotState {
                ref track_name,
                instance_id,
            } => match self.track_handle_or_err(track_name) {
                Ok(track) => {
                    let plugin_path = track
                        .lock()
                        .loaded_clap_instances()
                        .into_iter()
                        .find(|(id, _, _)| *id == instance_id)
                        .map(|(_, path, _)| path)
                        .unwrap_or_default();
                    match track.lock().clap_snapshot_state(instance_id) {
                        Ok(state) => {
                            self.notify_clients(Ok(Action::TrackClapStateSnapshot {
                                track_name: track_name.clone(),
                                instance_id,
                                plugin_path,
                                state,
                            }))
                            .await;
                        }
                        Err(e) => {
                            self.notify_clients(Err(e)).await;
                        }
                    }
                }
                Err(e) => {
                    self.notify_clients(Err(e)).await;
                }
            },
            Action::TrackClapStateSnapshot { .. } => {}
            Action::TrackClapRestoreState {
                ref track_name,
                instance_id,
                ref state,
            } => {
                if self
                    .reject_if_track_frozen(track_name, "CLAP state restore")
                    .await
                {
                    return;
                }
                let track = match self.track_handle_or_err(track_name) {
                    Ok(track) => track,
                    Err(e) => {
                        self.notify_clients(Err(e)).await;
                        return;
                    }
                };
                if let Err(e) = track.lock().clap_restore_state(instance_id, state) {
                    self.notify_clients(Err(e)).await;
                    return;
                }
            }
            Action::TrackSnapshotAllClapStates { ref track_name } => {
                let track = match self.track_handle_or_err(track_name) {
                    Ok(track) => track,
                    Err(e) => {
                        self.notify_clients(Err(e)).await;
                        return;
                    }
                };
                for (instance_id, plugin_path, state) in track.lock().clap_snapshot_all_states() {
                    self.notify_clients(Ok(Action::TrackClapStateSnapshot {
                        track_name: track_name.clone(),
                        instance_id,
                        plugin_path,
                        state,
                    }))
                    .await;
                }
            }
            Action::TrackLoadVst3Plugin {
                ref track_name,
                ref plugin_path,
            } => {
                if self
                    .reject_if_track_frozen(track_name, "VST3 plugin loading")
                    .await
                {
                    return;
                }
                let track = match self.track_handle_or_err(track_name) {
                    Ok(track) => track,
                    Err(e) => {
                        self.notify_clients(Err(e)).await;
                        return;
                    }
                };
                if let Err(e) = track.lock().load_vst3_plugin(plugin_path) {
                    self.notify_clients(Err(e)).await;
                    return;
                }
            }
            Action::TrackUnloadVst3PluginInstance {
                ref track_name,
                instance_id,
            } => {
                if self
                    .reject_if_track_frozen(track_name, "VST3 plugin unloading")
                    .await
                {
                    return;
                }
                let track = match self.track_handle_or_err(track_name) {
                    Ok(track) => track,
                    Err(e) => {
                        self.notify_clients(Err(e)).await;
                        return;
                    }
                };
                if let Err(e) = track.lock().unload_vst3_plugin_instance(instance_id) {
                    self.notify_clients(Err(e)).await;
                    return;
                }
            }
            Action::TrackGetVst3Graph { ref track_name } => {
                match self.track_handle_or_err(track_name) {
                    Ok(track) => {
                        let t = track.lock();
                        let plugins = t.vst3_graph_plugins();
                        let connections = t.vst3_graph_connections();
                        self.notify_clients(Ok(Action::TrackVst3Graph {
                            track_name: track_name.clone(),
                            plugins,
                            connections,
                        }))
                        .await;
                    }
                    Err(e) => {
                        self.notify_clients(Err(e)).await;
                    }
                }
            }
            Action::TrackVst3Graph { .. } => {
                // Response action, no handling needed
            }
            Action::TrackSetVst3Parameter {
                ref track_name,
                instance_id,
                param_id,
                value,
            } => {
                if self
                    .reject_if_track_frozen(track_name, "VST3 parameter changes")
                    .await
                {
                    return;
                }
                match self.track_handle_or_err(track_name) {
                    Ok(track) => {
                        if let Err(e) =
                            track
                                .lock()
                                .set_vst3_parameter(instance_id, param_id, value)
                        {
                            self.notify_clients(Err(e)).await;
                            return;
                        }
                        self.notify_clients(Ok(a.clone())).await;
                    }
                    Err(e) => {
                        self.notify_clients(Err(e)).await;
                    }
                }
            }
            Action::TrackGetVst3Parameters {
                ref track_name,
                instance_id,
            } => match self.track_handle_or_err(track_name) {
                Ok(track) => match track.lock().get_vst3_parameters(instance_id) {
                    Ok(parameters) => {
                        self.notify_clients(Ok(Action::TrackVst3Parameters {
                            track_name: track_name.clone(),
                            instance_id,
                            parameters,
                        }))
                        .await;
                    }
                    Err(e) => {
                        self.notify_clients(Err(e)).await;
                    }
                },
                Err(e) => {
                    self.notify_clients(Err(e)).await;
                }
            },
            Action::TrackVst3Parameters { .. } => {
                // Response action, no handling needed
            }
            Action::TrackVst3SnapshotState {
                ref track_name,
                instance_id,
            } => match self.track_handle_or_err(track_name) {
                Ok(track) => match track.lock().vst3_snapshot_state(instance_id) {
                    Ok(state) => {
                        self.notify_clients(Ok(Action::TrackVst3StateSnapshot {
                            track_name: track_name.clone(),
                            instance_id,
                            state,
                        }))
                        .await;
                    }
                    Err(e) => {
                        self.notify_clients(Err(e)).await;
                    }
                },
                Err(e) => {
                    self.notify_clients(Err(e)).await;
                }
            },
            Action::TrackVst3StateSnapshot { .. } => {
                // Response action, no handling needed
            }
            Action::TrackVst3RestoreState {
                ref track_name,
                instance_id,
                ref state,
            } => match self.track_handle_or_err(track_name) {
                Ok(track) => {
                    if let Err(e) = track.lock().vst3_restore_state(instance_id, state) {
                        self.notify_clients(Err(e)).await;
                        return;
                    }
                    self.notify_clients(Ok(a.clone())).await;
                }
                Err(e) => {
                    self.notify_clients(Err(e)).await;
                }
            },
            Action::TrackConnectVst3Audio {
                ref track_name,
                ref from_node,
                from_port,
                ref to_node,
                to_port,
            } => {
                if self
                    .reject_if_track_frozen(track_name, "VST3 routing changes")
                    .await
                {
                    return;
                }
                match self.track_handle_or_err(track_name) {
                    Ok(track) => {
                        if let Err(e) = track
                            .lock()
                            .connect_vst3_audio(from_node, from_port, to_node, to_port)
                        {
                            self.notify_clients(Err(e)).await;
                            return;
                        }
                        self.notify_clients(Ok(a.clone())).await;
                    }
                    Err(e) => {
                        self.notify_clients(Err(e)).await;
                    }
                }
            }
            Action::TrackDisconnectVst3Audio {
                ref track_name,
                ref from_node,
                from_port,
                ref to_node,
                to_port,
            } => {
                if self
                    .reject_if_track_frozen(track_name, "VST3 routing changes")
                    .await
                {
                    return;
                }
                match self.track_handle_or_err(track_name) {
                    Ok(track) => {
                        if let Err(e) = track
                            .lock()
                            .disconnect_vst3_audio(from_node, from_port, to_node, to_port)
                        {
                            self.notify_clients(Err(e)).await;
                            return;
                        }
                        self.notify_clients(Ok(a.clone())).await;
                    }
                    Err(e) => {
                        self.notify_clients(Err(e)).await;
                    }
                }
            }
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
                            if from_track.audio.ins.len() != to_track.audio.ins.len() {
                                self.notify_clients(Err(format!(
                                    "Cannot move/copy audio clip from '{}' ({} inputs) to '{}' ({} inputs)",
                                    from_track.name,
                                    from_track.audio.ins.len(),
                                    to_track.name,
                                    to_track.audio.ins.len()
                                )))
                                .await;
                                return;
                            }
                            let clip_copy = from_track.audio.clips[from.clip_index].clone();
                            if !copy {
                                from_track.audio.clips.remove(from.clip_index);
                            }
                            let mut clip_copy = clip_copy;
                            clip_copy.start = to.sample_offset;
                            let max_lane = to_track.audio.ins.len().saturating_sub(1);
                            clip_copy.input_channel = to.input_channel.min(max_lane);
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
                            let mut clip_copy = clip_copy;
                            clip_copy.start = to.sample_offset;
                            let max_lane = to_track.midi.ins.len().saturating_sub(1);
                            clip_copy.input_channel = to.input_channel.min(max_lane);
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
                input_channel,
                muted,
                kind,
                fade_enabled,
                fade_in_samples,
                fade_out_samples,
                ref warp_markers,
            } => {
                self.add_clip_to_track(
                    name,
                    track_name,
                    start,
                    length,
                    offset,
                    input_channel,
                    muted,
                    kind,
                    fade_enabled,
                    fade_in_samples,
                    fade_out_samples,
                    warp_markers,
                );
            }
            Action::RemoveClip {
                ref track_name,
                kind,
                ref clip_indices,
            } => {
                self.remove_clips_from_track(track_name, kind, clip_indices);
            }
            Action::RenameClip {
                ref track_name,
                kind,
                clip_index,
                ref new_name,
            } => {
                self.rename_clip_references(track_name, kind, clip_index, new_name);
            }
            Action::SetClipFade {
                ref track_name,
                clip_index,
                kind,
                fade_enabled,
                fade_in_samples,
                fade_out_samples,
            } => {
                self.set_clip_fade(
                    track_name,
                    clip_index,
                    kind,
                    fade_enabled,
                    fade_in_samples,
                    fade_out_samples,
                );
            }
            Action::SetClipBounds {
                ref track_name,
                clip_index,
                kind,
                start,
                length,
                offset,
            } => {
                self.set_clip_bounds(track_name, clip_index, kind, start, length, offset);
            }
            Action::SetClipMuted {
                ref track_name,
                clip_index,
                kind,
                muted,
            } => {
                self.set_clip_muted(track_name, clip_index, kind, muted);
            }
            Action::SetAudioClipWarpMarkers {
                ref track_name,
                clip_index,
                ref warp_markers,
            } => {
                if let Err(e) =
                    self.set_audio_clip_warp_markers(track_name, clip_index, warp_markers)
                {
                    self.notify_clients(Err(e)).await;
                    return;
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
                            self.hw_input_audio_port(from_port)
                        } else {
                            self.state
                                .lock()
                                .tracks
                                .get(from_track)
                                .and_then(|t| t.lock().audio.outs.get(from_port).cloned())
                        };
                        let to_audio_io = if to_track == "hw:out" {
                            self.hw_output_audio_port(to_port)
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
                        let from_hw_in_device = Self::midi_hw_in_device(from_track);
                        let to_hw_out_device = Self::midi_hw_out_device(to_track);
                        let from_is_invalid_hw = Self::midi_hw_out_device(from_track).is_some();
                        let to_is_invalid_hw = Self::midi_hw_in_device(to_track).is_some();

                        if from_is_invalid_hw || to_is_invalid_hw {
                            self.notify_clients(Err(
                                "Invalid MIDI hardware connection direction".to_string()
                            ))
                            .await;
                            return;
                        }

                        if from_hw_in_device.is_none()
                            && to_hw_out_device.is_none()
                            && self.check_if_leads_to_kind(Kind::MIDI, to_track, from_track)
                        {
                            self.notify_clients(Err("Circular routing is not allowed!".into()))
                                .await;
                            return;
                        }

                        let state = self.state.lock();
                        let from_track_handle = state.tracks.get(from_track);
                        let to_track_handle = state.tracks.get(to_track);

                        if let Some(device) = from_hw_in_device {
                            if let Some(t_t) = to_track_handle {
                                if t_t.lock().midi.ins.get(to_port).is_none() {
                                    self.notify_clients(Err(format!(
                                        "MIDI input port {} not found on track '{}'",
                                        to_port, to_track
                                    )))
                                    .await;
                                    return;
                                }
                                let route = MidiHwInRoute {
                                    device: device.to_string(),
                                    to_track: to_track.to_string(),
                                    to_port,
                                };
                                if !self.midi_hw_in_routes.iter().any(|r| r == &route) {
                                    self.midi_hw_in_routes.push(route);
                                }
                            } else {
                                self.notify_clients(Err(format!(
                                    "MIDI destination track not found: {}",
                                    to_track
                                )))
                                .await;
                                return;
                            }
                        } else if let Some(device) = to_hw_out_device {
                            if let Some(f_t) = from_track_handle {
                                if f_t.lock().midi.outs.get(from_port).is_none() {
                                    self.notify_clients(Err(format!(
                                        "MIDI output port {} not found on track '{}'",
                                        from_port, from_track
                                    )))
                                    .await;
                                    return;
                                }
                                let route = MidiHwOutRoute {
                                    from_track: from_track.to_string(),
                                    from_port,
                                    device: device.to_string(),
                                };
                                if !self.midi_hw_out_routes.iter().any(|r| r == &route) {
                                    self.midi_hw_out_routes.push(route);
                                }
                            } else {
                                self.notify_clients(Err(format!(
                                    "MIDI source track not found: {}",
                                    from_track
                                )))
                                .await;
                                return;
                            }
                        } else {
                            match (from_track_handle, to_track_handle) {
                                (Some(f_t), Some(t_t)) => {
                                    let to_in_res = t_t.lock().midi.ins.get(to_port).cloned();
                                    if let Some(to_in) = to_in_res {
                                        let from_track = f_t.lock();
                                        if let Err(e) =
                                            from_track.midi.connect_out(from_port, to_in)
                                        {
                                            self.notify_clients(Err(e)).await;
                                            return;
                                        }
                                        from_track.invalidate_midi_route_cache();
                                    } else {
                                        self.notify_clients(Err(format!(
                                            "MIDI input port {} not found on track '{}'",
                                            to_port, to_track
                                        )))
                                        .await;
                                        return;
                                    }
                                }
                                _ => {
                                    self.notify_clients(Err(format!(
                                        "MIDI tracks not found: {} or {}",
                                        from_track, to_track
                                    )))
                                    .await;
                                    return;
                                }
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
                    self.hw_input_audio_port(from_port)
                } else {
                    let state = self.state.lock();
                    state
                        .tracks
                        .get(from_track)
                        .and_then(|t| t.lock().audio.outs.get(from_port).cloned())
                };
                let to_audio_io = if to_track == "hw:out" {
                    self.hw_output_audio_port(to_port)
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
                } else if kind == Kind::MIDI {
                    let from_hw_in_device = Self::midi_hw_in_device(from_track);
                    let to_hw_out_device = Self::midi_hw_out_device(to_track);

                    if let Some(device) = from_hw_in_device {
                        let before = self.midi_hw_in_routes.len();
                        self.midi_hw_in_routes.retain(|r| {
                            !(r.device == device && r.to_track == *to_track && r.to_port == to_port)
                        });
                        if self.midi_hw_in_routes.len() < before {
                            self.notify_clients(Ok(a.clone())).await;
                        } else {
                            self.notify_clients(Err(format!(
                                "Disconnect failed: MIDI route not found ({} -> {})",
                                from_track, to_track
                            )))
                            .await;
                        }
                        return;
                    }

                    if let Some(device) = to_hw_out_device {
                        let before = self.midi_hw_out_routes.len();
                        self.midi_hw_out_routes.retain(|r| {
                            !(r.from_track == *from_track
                                && r.from_port == from_port
                                && r.device == device)
                        });
                        if self.midi_hw_out_routes.len() < before {
                            self.notify_clients(Ok(a.clone())).await;
                        } else {
                            self.notify_clients(Err(format!(
                                "Disconnect failed: MIDI route not found ({} -> {})",
                                from_track, to_track
                            )))
                            .await;
                        }
                        return;
                    }

                    let state = self.state.lock();
                    if let (Some(f_t), Some(t_t)) =
                        (state.tracks.get(from_track), state.tracks.get(to_track))
                        && let Some(to_in) = t_t.lock().midi.ins.get(to_port).cloned()
                    {
                        let from_track = f_t.lock();
                        if let Err(e) = from_track.midi.disconnect_out(from_port, &to_in) {
                            self.notify_clients(Err(e)).await;
                        } else {
                            from_track.invalidate_midi_route_cache();
                            self.notify_clients(Ok(a.clone())).await;
                        }
                    } else {
                        self.notify_clients(Err(format!(
                            "Disconnect failed: MIDI ports not found ({} -> {})",
                            from_track, to_track
                        )))
                        .await;
                    }
                }
            }

            Action::OpenAudioDevice {
                ref device,
                ref input_device,
                sample_rate_hz,
                bits,
                exclusive,
                period_frames,
                nperiods,
                sync_mode,
            } => {
                if self
                    .maybe_open_jack_runtime(
                        device,
                        input_device.as_deref(),
                        sample_rate_hz,
                        bits,
                        exclusive,
                        period_frames,
                        nperiods,
                        sync_mode,
                    )
                    .await
                    .is_some()
                {
                    return;
                }
                let hw_opts = Self::build_hw_options(exclusive, period_frames, nperiods, sync_mode);
                let open_result = self
                    .open_non_jack_audio_device(
                        device,
                        input_device.as_deref(),
                        sample_rate_hz,
                        bits,
                        hw_opts,
                    )
                    .await;
                match open_result {
                    Ok(()) => {}
                    Err(e) => {
                        self.notify_clients(Err(e)).await;
                        return;
                    }
                }
                self.finalize_open_audio_device().await;
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
            Action::RequestSessionDiagnostics => {
                let (
                    track_count,
                    frozen_track_count,
                    audio_clip_count,
                    midi_clip_count,
                    lv2_instance_count,
                    vst3_instance_count,
                    clap_instance_count,
                ) = {
                    let tracks = &self.state.lock().tracks;
                    let mut track_count = 0usize;
                    let mut frozen_track_count = 0usize;
                    let mut audio_clip_count = 0usize;
                    let mut midi_clip_count = 0usize;
                    #[cfg(all(unix, not(target_os = "macos")))]
                    let mut lv2_instance_count = 0usize;
                    #[cfg(not(all(unix, not(target_os = "macos"))))]
                    let lv2_instance_count = 0usize;
                    let mut vst3_instance_count = 0usize;
                    let mut clap_instance_count = 0usize;
                    for track in tracks.values() {
                        let t = track.lock();
                        track_count += 1;
                        if t.frozen {
                            frozen_track_count += 1;
                        }
                        audio_clip_count += t.audio.clips.len();
                        midi_clip_count += t.midi.clips.len();
                        #[cfg(all(unix, not(target_os = "macos")))]
                        {
                            lv2_instance_count += t.lv2_processors.len();
                        }
                        vst3_instance_count += t.vst3_processors.len();
                        clap_instance_count += t.clap_plugins.len();
                    }
                    (
                        track_count,
                        frozen_track_count,
                        audio_clip_count,
                        midi_clip_count,
                        lv2_instance_count,
                        vst3_instance_count,
                        clap_instance_count,
                    )
                };
                #[cfg(not(all(unix, not(target_os = "macos"))))]
                let _ = lv2_instance_count;
                let pending_hw_midi_events = self.pending_hw_midi_events.len()
                    + self
                        .pending_hw_midi_events_by_device
                        .values()
                        .map(std::vec::Vec::len)
                        .sum::<usize>();
                let sample_rate_hz = if let Some(hw) = &self.hw_driver {
                    hw.lock().sample_rate() as usize
                } else {
                    #[cfg(unix)]
                    {
                        self.jack_runtime
                            .as_ref()
                            .map(|j| j.lock().sample_rate)
                            .unwrap_or(0)
                    }
                    #[cfg(not(unix))]
                    {
                        0
                    }
                };
                let cycle_samples = self.current_cycle_samples();
                self.notify_clients(Ok(Action::SessionDiagnosticsReport {
                    track_count,
                    frozen_track_count,
                    audio_clip_count,
                    midi_clip_count,
                    #[cfg(all(unix, not(target_os = "macos")))]
                    lv2_instance_count,
                    vst3_instance_count,
                    clap_instance_count,
                    pending_requests: self.pending_requests.len(),
                    workers_total: self.workers.len(),
                    workers_ready: self.ready_workers.len(),
                    pending_hw_midi_events,
                    playing: self.playing,
                    transport_sample: self.transport_sample,
                    tempo_bpm: self.tempo_bpm,
                    sample_rate_hz,
                    cycle_samples,
                }))
                .await;
            }
            Action::RequestMidiLearnMappingsReport => {
                let mut lines = Vec::<String>::new();
                let fmt_binding = |b: &crate::message::MidiLearnBinding| {
                    let device = b.device.as_deref().unwrap_or("*");
                    format!("{device} CH{} CC{}", b.channel + 1, b.cc)
                };
                if let Some(b) = self.global_midi_learn_play_pause.as_ref() {
                    lines.push(format!("Global PlayPause: {}", fmt_binding(b)));
                }
                if let Some(b) = self.global_midi_learn_stop.as_ref() {
                    lines.push(format!("Global Stop: {}", fmt_binding(b)));
                }
                if let Some(b) = self.global_midi_learn_record_toggle.as_ref() {
                    lines.push(format!("Global RecordToggle: {}", fmt_binding(b)));
                }
                for (track_name, track) in self.state.lock().tracks.iter() {
                    let t = track.lock();
                    if let Some(b) = t.midi_learn_volume.as_ref() {
                        lines.push(format!("{} Volume: {}", track_name, fmt_binding(b)));
                    }
                    if let Some(b) = t.midi_learn_balance.as_ref() {
                        lines.push(format!("{} Balance: {}", track_name, fmt_binding(b)));
                    }
                    if let Some(b) = t.midi_learn_mute.as_ref() {
                        lines.push(format!("{} Mute: {}", track_name, fmt_binding(b)));
                    }
                    if let Some(b) = t.midi_learn_solo.as_ref() {
                        lines.push(format!("{} Solo: {}", track_name, fmt_binding(b)));
                    }
                    if let Some(b) = t.midi_learn_arm.as_ref() {
                        lines.push(format!("{} Arm: {}", track_name, fmt_binding(b)));
                    }
                    if let Some(b) = t.midi_learn_input_monitor.as_ref() {
                        lines.push(format!("{} InputMonitor: {}", track_name, fmt_binding(b)));
                    }
                    if let Some(b) = t.midi_learn_disk_monitor.as_ref() {
                        lines.push(format!("{} DiskMonitor: {}", track_name, fmt_binding(b)));
                    }
                }
                if lines.is_empty() {
                    lines.push("No MIDI learn mappings configured".to_string());
                }
                self.notify_clients(Ok(Action::MidiLearnMappingsReport { lines }))
                    .await;
            }
            Action::ClearAllMidiLearnBindings => {
                self.pending_midi_learn = None;
                self.pending_global_midi_learn = None;
                self.global_midi_learn_play_pause = None;
                self.global_midi_learn_stop = None;
                self.global_midi_learn_record_toggle = None;
                self.midi_cc_gate.clear();
                for track in self.state.lock().tracks.values() {
                    let t = track.lock();
                    t.midi_learn_volume = None;
                    t.midi_learn_balance = None;
                    t.midi_learn_mute = None;
                    t.midi_learn_solo = None;
                    t.midi_learn_arm = None;
                    t.midi_learn_input_monitor = None;
                    t.midi_learn_disk_monitor = None;
                }
            }
            #[cfg(all(unix, not(target_os = "macos")))]
            Action::TrackLv2PluginControls { .. } => {}
            #[cfg(all(unix, not(target_os = "macos")))]
            Action::TrackLv2Midnam { .. } => {}
            Action::SessionDiagnosticsReport { .. } => {}
            Action::MidiLearnMappingsReport { .. } => {}
            Action::HWInfo { .. } => {}
            Action::Undo => {} // Already handled at the beginning
            Action::Redo => {} // Already handled at the beginning
        }

        // Record action in history after successful processing
        if let Some(inverse) = inverse_actions {
            if let Some(group) = self.history_group.as_mut() {
                group.forward_actions.push(action_to_process.clone());
                group.inverse_actions.splice(0..0, inverse);
            } else {
                self.history.record(UndoEntry {
                    forward_actions: vec![action_to_process.clone()],
                    inverse_actions: inverse,
                });
            }
        }

        // Notify clients with the actual action that was processed
        self.notify_clients(Ok(action_to_process)).await;
    }

    pub async fn work(&mut self) {
        while let Some(message) = self.rx.recv().await {
            match message {
                Message::Ready(id) => {
                    self.ready_workers.push(id);
                }
                Message::Finished {
                    worker_id,
                    track_name,
                    output_linear,
                } => {
                    self.ready_workers.push(worker_id);
                    self.track_meter_linear_by_track
                        .insert(track_name, output_linear);
                    let all_finished = self.send_tracks().await;
                    if all_finished {
                        if self.hw_worker.is_some() {
                            self.pending_hw_midi_out_events_by_device =
                                self.collect_hw_midi_output_events_by_device();
                        } else {
                            self.pending_hw_midi_out_events = self.collect_hw_midi_output_events();
                        }
                        self.request_hw_cycle().await;
                    }
                }
                Message::Channel(s) => {
                    self.clients.push(s);
                }

                Message::Request(a) => match a {
                    Action::TrackOfflineBounceCancel { track_name } => {
                        if let Some(job) = &self.offline_bounce_job
                            && job.track_name == track_name
                        {
                            job.cancel.store(true, Ordering::Relaxed);
                        }
                    }
                    _ if self.offline_bounce_job.is_some() => {
                        self.pending_requests.push_back(a);
                    }
                    Action::OpenAudioDevice { .. }
                    | Action::OpenMidiInputDevice(_)
                    | Action::OpenMidiOutputDevice(_)
                    | Action::RequestMeterSnapshot
                    | Action::Quit
                    | Action::Play
                    | Action::Stop
                    | Action::SetLoopEnabled(_)
                    | Action::SetLoopRange(_)
                    | Action::SetPunchEnabled(_)
                    | Action::SetPunchRange(_)
                    | Action::SetMetronomeEnabled(_)
                    | Action::SetTempo(_)
                    | Action::SetTimeSignature { .. }
                    | Action::SetClipPlaybackEnabled(_)
                    | Action::SetRecordEnabled(_)
                    | Action::BeginHistoryGroup
                    | Action::EndHistoryGroup
                    | Action::SetSessionPath(_)
                    | Action::ClearHistory
                    | Action::BeginSessionRestore
                    | Action::PianoKey { .. }
                    | Action::ModifyMidiNotes { .. }
                    | Action::ModifyMidiControllers { .. }
                    | Action::DeleteMidiControllers { .. }
                    | Action::InsertMidiControllers { .. }
                    | Action::DeleteMidiNotes { .. }
                    | Action::InsertMidiNotes { .. }
                    | Action::SetMidiSysExEvents { .. } => {
                        self.handle_request(a).await;
                    }
                    #[cfg(all(unix, not(target_os = "macos")))]
                    Action::ListLv2Plugins => {
                        self.handle_request(a).await;
                    }
                    Action::ListVst3Plugins => {
                        self.handle_request(a).await;
                    }
                    Action::ListClapPlugins => {
                        self.handle_request(a).await;
                    }
                    Action::ListClapPluginsWithCapabilities => {
                        self.handle_request(a).await;
                    }
                    _ => {
                        self.pending_requests.push_back(a);
                        if self.can_schedule_hw_cycle() {
                            self.request_hw_cycle().await;
                        } else {
                            while let Some(next) = self.pending_requests.pop_front() {
                                self.handle_request(next).await;
                            }
                        }
                    }
                },
                Message::OfflineBounceFinished { result } => {
                    self.offline_bounce_job = None;
                    self.notify_clients(result).await;
                    while let Some(next) = self.pending_requests.pop_front() {
                        self.handle_request(next).await;
                    }
                }
                Message::HWFinished => {
                    if !self.awaiting_hwfinished {
                        continue;
                    }
                    self.awaiting_hwfinished = false;
                    #[cfg(unix)]
                    {
                        if let Some(jack) = &self.jack_runtime {
                            if !self.pending_hw_midi_out_events.is_empty() {
                                let out_events =
                                    std::mem::take(&mut self.pending_hw_midi_out_events);
                                jack.lock().write_events(&out_events);
                            }
                            let mut in_events = vec![];
                            jack.lock().read_events_into(&mut in_events);
                            if !in_events.is_empty() {
                                self.pending_hw_midi_events.extend(in_events);
                            }
                        }
                    }
                    while let Some(a) = self.pending_requests.pop_front() {
                        self.handle_request(a).await;
                    }
                    self.apply_mute_solo_policy();
                    self.append_recorded_cycle();
                    self.flush_completed_recordings().await;
                    let hw_in_routes = self.midi_hw_in_routes.clone();
                    let pending_hw_in_by_device = self.pending_hw_midi_events_by_device.clone();
                    for (track_name, track) in self.state.lock().tracks.iter() {
                        let track_lock = track.lock();
                        #[cfg(unix)]
                        {
                            if self.jack_runtime.is_some() {
                                if !self.pending_hw_midi_events.is_empty() {
                                    track_lock.push_hw_midi_events(&self.pending_hw_midi_events);
                                }
                            } else {
                                for route in
                                    hw_in_routes.iter().filter(|r| &r.to_track == track_name)
                                {
                                    if let Some(events) = pending_hw_in_by_device.get(&route.device)
                                    {
                                        track_lock
                                            .push_hw_midi_events_to_port(route.to_port, events);
                                    }
                                }
                            }
                        }
                        #[cfg(not(unix))]
                        {
                            for route in hw_in_routes.iter().filter(|r| &r.to_track == track_name) {
                                if let Some(events) = pending_hw_in_by_device.get(&route.device) {
                                    track_lock.push_hw_midi_events_to_port(route.to_port, events);
                                }
                            }
                        }
                        track_lock.setup();
                    }
                    self.publish_track_meters().await;
                    self.pending_hw_midi_events.clear();
                    self.pending_hw_midi_events_by_device.clear();
                    if self.playing {
                        let next = self
                            .transport_sample
                            .saturating_add(self.current_cycle_samples());
                        let normalized = self.normalize_transport_sample(next);
                        let wrapped = normalized != next;
                        self.transport_sample = normalized;
                        if wrapped {
                            self.notify_clients(Ok(Action::TransportPosition(
                                self.transport_sample,
                            )))
                            .await;
                        }
                    }
                    if self.send_tracks().await && self.hw_worker.is_some() {
                        self.request_hw_cycle().await;
                    }
                    #[cfg(unix)]
                    {
                        if self.jack_runtime.is_some() {
                            self.awaiting_hwfinished = true;
                        }
                    }
                }
                Message::HWMidiEvents(events) => {
                    for hw_event in events {
                        if hw_event.event.data.len() >= 3 {
                            let status = hw_event.event.data[0];
                            if status & 0xF0 == 0xB0 {
                                let channel = status & 0x0F;
                                let cc = hw_event.event.data[1];
                                let value = hw_event.event.data[2];
                                self.handle_incoming_hw_cc(&hw_event.device, channel, cc, value)
                                    .await;
                            }
                        }
                        self.pending_hw_midi_events_by_device
                            .entry(hw_event.device)
                            .or_default()
                            .push(hw_event.event);
                    }
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

    fn collect_hw_midi_output_events_by_device(&self) -> Vec<HwMidiEvent> {
        let mut events = Vec::<HwMidiEvent>::new();
        let routes = self.midi_hw_out_routes.clone();
        let state = self.state.lock();
        for route in routes {
            let Some(track) = state.tracks.get(&route.from_track) else {
                continue;
            };
            let track_lock = track.lock();
            let Some(out_port) = track_lock.midi.outs.get(route.from_port) else {
                continue;
            };
            let port_events = out_port.lock().buffer.clone();
            for event in port_events {
                events.push(HwMidiEvent {
                    device: route.device.clone(),
                    event,
                });
            }
        }
        events.sort_by(|a, b| {
            a.event
                .frame
                .cmp(&b.event.frame)
                .then_with(|| a.device.cmp(&b.device))
        });
        events
    }
}
