use super::{audio::track::AudioTrack, midi::track::MIDITrack};
#[cfg(target_os = "macos")]
use crate::clap::ClapMidiOutputEvent;
use crate::clap::{ClapProcessor, ClapTransportInfo};
#[cfg(all(unix, not(target_os = "macos")))]
use crate::lv2::{Lv2Processor, Lv2TransportInfo};
#[cfg(all(unix, not(target_os = "macos")))]
use crate::message::Lv2ControlPortInfo;
#[cfg(unix)]
use crate::message::Lv2PluginState;
#[cfg(unix)]
use crate::message::{PluginGraphConnection, PluginGraphNode, PluginGraphPlugin};
use crate::mutex::UnsafeMutex;
use crate::vst3::Vst3Processor;
use crate::{
    audio::io::AudioIO,
    midi::io::{MIDIIO, MidiEvent},
};
#[cfg(unix)]
use crate::{kind::Kind, routing};
use midly::{MetaMessage, Smf, Timing, TrackEventKind, live::LiveEvent};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, atomic::Ordering},
};

/// MIDI clip events: vector of (timestamp, midi_bytes) pairs
type MidiClipEvents = Arc<Vec<(usize, Vec<u8>)>>;

#[cfg(all(unix, not(target_os = "macos")))]
#[derive(Debug)]
pub struct Lv2Instance {
    pub id: usize,
    pub processor: Lv2Processor,
}

#[derive(Debug)]
pub struct Vst3Instance {
    pub id: usize,
    pub processor: Vst3Processor,
}

#[derive(Debug, Clone)]
pub struct ClapInstance {
    pub id: usize,
    pub processor: ClapProcessor,
}

#[derive(Debug, Clone)]
struct AudioClipBuffer {
    channels: usize,
    samples: Vec<f32>,
}

#[derive(Debug, Clone)]
pub struct HwMidiOutEvent {
    pub port: usize,
    pub event: MidiEvent,
}

#[derive(Debug)]
pub struct Track {
    pub name: String,
    pub level: f32,
    pub balance: f32,
    pub armed: bool,
    pub muted: bool,
    pub soloed: bool,
    pub input_monitor: bool,
    pub disk_monitor: bool,
    pub midi_learn_volume: Option<crate::message::MidiLearnBinding>,
    pub midi_learn_balance: Option<crate::message::MidiLearnBinding>,
    pub midi_learn_mute: Option<crate::message::MidiLearnBinding>,
    pub midi_learn_solo: Option<crate::message::MidiLearnBinding>,
    pub midi_learn_arm: Option<crate::message::MidiLearnBinding>,
    pub midi_learn_input_monitor: Option<crate::message::MidiLearnBinding>,
    pub midi_learn_disk_monitor: Option<crate::message::MidiLearnBinding>,
    pub vca_master: Option<String>,
    pub frozen: bool,
    pub midi_lane_channels: Vec<Option<u8>>,
    primary_audio_ins: usize,
    primary_audio_outs: usize,
    pub audio: AudioTrack,
    pub midi: MIDITrack,
    #[cfg(all(unix, not(target_os = "macos")))]
    pub lv2_processors: Vec<Lv2Instance>,
    pub vst3_processors: Vec<Vst3Instance>,
    pub clap_plugins: Vec<ClapInstance>,
    #[cfg(unix)]
    pub plugin_midi_connections: Vec<PluginGraphConnection>,
    pub pending_hw_midi_out_events: Vec<HwMidiOutEvent>,
    #[cfg(all(unix, not(target_os = "macos")))]
    pub next_lv2_instance_id: usize,
    pub next_vst3_instance_id: usize,
    pub next_clap_instance_id: usize,
    pub next_plugin_instance_id: usize,
    pub sample_rate: f64,
    process_block_size: usize,
    pub output_enabled: bool,
    pub process_epoch: usize,
    pub transport_sample: usize,
    pub loop_enabled: bool,
    pub loop_range_samples: Option<(usize, usize)>,
    pub tempo_bpm: f64,
    pub tsig_num: u16,
    pub tsig_denom: u16,
    pub clip_playback_enabled: bool,
    pub metronome_enabled: bool,
    output_meter_linear_cache: Vec<f32>,
    meter_peak_hold_linear: Vec<f32>,
    pub record_tap_outs: Vec<Vec<f32>>,
    pub record_tap_midi_in: Vec<MidiEvent>,
    #[cfg(all(unix, not(target_os = "macos")))]
    pub lv2_state_base_dir: Option<PathBuf>,
    pub session_base_dir: Option<PathBuf>,
    record_tap_enabled: bool,
    audio_clip_cache: HashMap<String, Arc<AudioClipBuffer>>,
    midi_clip_cache: HashMap<String, MidiClipEvents>,
    internal_output_routes_cache: Vec<Vec<Arc<AudioIO>>>,
    audio_route_cache_dirty: bool,
    metronome_source: Option<Arc<AudioIO>>,
    midi_input_to_out_routes_cache: Vec<Vec<usize>>,
    midi_out_external_targets_cache: Vec<Vec<Arc<UnsafeMutex<Box<MIDIIO>>>>>,
    midi_route_cache_dirty: bool,
}

impl Track {
    const METRONOME_DEFAULT_LEVEL_DB: f32 = -10.0;

    pub fn new(
        name: String,
        audio_ins: usize,
        audio_outs: usize,
        midi_ins: usize,
        midi_outs: usize,
        buffer_size: usize,
        sample_rate: f64,
    ) -> Self {
        Self {
            name,
            level: 0.0,
            balance: 0.0,
            armed: false,
            muted: false,
            soloed: false,
            input_monitor: false,
            disk_monitor: true,
            midi_learn_volume: None,
            midi_learn_balance: None,
            midi_learn_mute: None,
            midi_learn_solo: None,
            midi_learn_arm: None,
            midi_learn_input_monitor: None,
            midi_learn_disk_monitor: None,
            vca_master: None,
            frozen: false,
            midi_lane_channels: vec![None; midi_ins],
            primary_audio_ins: audio_ins,
            primary_audio_outs: audio_outs,
            audio: AudioTrack::new(audio_ins, audio_outs, buffer_size),
            midi: MIDITrack::new(midi_ins, midi_outs),
            #[cfg(all(unix, not(target_os = "macos")))]
            lv2_processors: Vec::new(),
            vst3_processors: Vec::new(),
            clap_plugins: Vec::new(),
            #[cfg(unix)]
            plugin_midi_connections: Vec::new(),
            pending_hw_midi_out_events: vec![],
            #[cfg(all(unix, not(target_os = "macos")))]
            next_lv2_instance_id: 0,
            next_vst3_instance_id: 0,
            next_clap_instance_id: 0,
            next_plugin_instance_id: 0,
            sample_rate,
            process_block_size: buffer_size.max(1),
            output_enabled: true,
            process_epoch: 0,
            transport_sample: 0,
            loop_enabled: false,
            loop_range_samples: None,
            tempo_bpm: 120.0,
            tsig_num: 4,
            tsig_denom: 4,
            clip_playback_enabled: true,
            metronome_enabled: false,
            output_meter_linear_cache: vec![0.0; audio_outs],
            meter_peak_hold_linear: vec![0.0; audio_outs],
            record_tap_outs: vec![vec![0.0; buffer_size]; audio_outs],
            record_tap_midi_in: vec![],
            #[cfg(all(unix, not(target_os = "macos")))]
            lv2_state_base_dir: None,
            session_base_dir: None,
            record_tap_enabled: false,
            audio_clip_cache: HashMap::new(),
            midi_clip_cache: HashMap::new(),
            internal_output_routes_cache: Vec::new(),
            audio_route_cache_dirty: true,
            metronome_source: None,
            midi_input_to_out_routes_cache: Vec::new(),
            midi_out_external_targets_cache: Vec::new(),
            midi_route_cache_dirty: true,
        }
        .with_default_passthrough()
    }

    fn alloc_plugin_instance_id(&mut self) -> usize {
        let id = self.next_plugin_instance_id;
        self.next_plugin_instance_id = self.next_plugin_instance_id.saturating_add(1);
        id
    }

    pub fn setup(&mut self) {
        self.audio.setup();
        #[cfg(all(unix, not(target_os = "macos")))]
        for instance in &self.lv2_processors {
            instance.processor.setup_audio_ports();
        }
        for instance in &self.vst3_processors {
            instance.processor.setup_audio_ports();
        }
        for instance in &self.clap_plugins {
            instance.processor.setup_audio_ports();
        }
    }

    fn connect_directed_audio(from: &Arc<AudioIO>, to: &Arc<AudioIO>) {
        let new_len = {
            let conns = to.connections.lock();
            if !conns.iter().any(|conn| Arc::ptr_eq(conn, from)) {
                conns.push(from.clone());
            }
            conns.len()
        };
        to.connection_count.store(new_len, Ordering::Relaxed);
    }

    fn invalidate_audio_route_cache(&mut self) {
        self.audio_route_cache_dirty = true;
    }

    pub fn primary_audio_ins(&self) -> usize {
        self.primary_audio_ins.min(self.audio.ins.len())
    }

    pub fn primary_audio_outs(&self) -> usize {
        self.primary_audio_outs.min(self.audio.outs.len())
    }

    fn ensure_audio_route_cache(&mut self) {
        if !self.audio_route_cache_dirty
            && self.internal_output_routes_cache.len() == self.audio.outs.len()
        {
            return;
        }
        let internal_sources = self.internal_audio_sources();
        let mut routes = Vec::with_capacity(self.audio.outs.len());
        for audio_out in &self.audio.outs {
            let connections = audio_out.connections.lock();
            let mut route_sources = Vec::new();
            for source in connections.iter() {
                if internal_sources
                    .iter()
                    .any(|candidate| Arc::ptr_eq(candidate, source))
                {
                    route_sources.push(source.clone());
                }
            }
            routes.push(route_sources);
        }
        self.internal_output_routes_cache = routes;
        self.audio_route_cache_dirty = false;
    }

    pub fn invalidate_midi_route_cache(&mut self) {
        self.midi_route_cache_dirty = true;
    }

    fn ensure_midi_route_cache(&mut self) {
        if !self.midi_route_cache_dirty
            && self.midi_input_to_out_routes_cache.len() == self.midi.ins.len()
            && self.midi_out_external_targets_cache.len() == self.midi.outs.len()
        {
            return;
        }

        let mut input_to_out = vec![Vec::<usize>::new(); self.midi.ins.len()];
        let mut out_external_targets =
            vec![Vec::<Arc<UnsafeMutex<Box<MIDIIO>>>>::new(); self.midi.outs.len()];

        for (out_idx, out) in self.midi.outs.iter().enumerate() {
            let out_lock = out.lock();
            for target in &out_lock.connections {
                if let Some(input_idx) = self
                    .midi
                    .ins
                    .iter()
                    .position(|input| Arc::ptr_eq(input, target))
                {
                    input_to_out[input_idx].push(out_idx);
                } else {
                    out_external_targets[out_idx].push(target.clone());
                }
            }
        }

        self.midi_input_to_out_routes_cache = input_to_out;
        self.midi_out_external_targets_cache = out_external_targets;
        self.midi_route_cache_dirty = false;
    }

    #[inline(always)]
    fn copy_unity_with_zero_tail(dst: &mut [f32], src: &[f32]) {
        let len = dst.len().min(src.len());
        unsafe {
            std::ptr::copy_nonoverlapping(src.as_ptr(), dst.as_mut_ptr(), len);
        }
        if len < dst.len() {
            dst[len..].fill(0.0);
        }
    }

    #[inline(always)]
    fn copy_scaled_with_zero_tail(dst: &mut [f32], src: &[f32], gain: f32) {
        let len = dst.len().min(src.len());
        unsafe {
            let mut i = 0usize;
            let dp = dst.as_mut_ptr();
            let sp = src.as_ptr();
            while i < len {
                *dp.add(i) = *sp.add(i) * gain;
                i += 1;
            }
        }
        if len < dst.len() {
            dst[len..].fill(0.0);
        }
    }

    #[inline(always)]
    fn add_unity(dst: &mut [f32], src: &[f32]) {
        let len = dst.len().min(src.len());
        unsafe {
            let mut i = 0usize;
            let dp = dst.as_mut_ptr();
            let sp = src.as_ptr();
            while i < len {
                *dp.add(i) += *sp.add(i);
                i += 1;
            }
        }
    }

    #[inline(always)]
    fn add_scaled(dst: &mut [f32], src: &[f32], gain: f32) {
        let len = dst.len().min(src.len());
        unsafe {
            let mut i = 0usize;
            let dp = dst.as_mut_ptr();
            let sp = src.as_ptr();
            while i < len {
                *dp.add(i) += *sp.add(i) * gain;
                i += 1;
            }
        }
    }

    fn ensure_metronome_source(&mut self, frames: usize) -> Option<Arc<AudioIO>> {
        if self.name != "metronome" || self.audio.outs.is_empty() {
            return None;
        }
        let needed = frames.max(1);
        let needs_new = self
            .metronome_source
            .as_ref()
            .map_or(true, |src| src.buffer.lock().len() < needed);
        if needs_new {
            self.metronome_source = Some(Arc::new(AudioIO::new(needed)));
            self.invalidate_audio_route_cache();
        }
        let Some(src) = self.metronome_source.clone() else {
            return None;
        };
        let mut route_changed = false;
        for out in &self.audio.outs {
            if !out
                .connections
                .lock()
                .iter()
                .any(|conn| Arc::ptr_eq(conn, &src))
            {
                Self::connect_directed_audio(&src, out);
                route_changed = true;
            }
        }
        if route_changed {
            self.invalidate_audio_route_cache();
        }
        Some(src)
    }

    fn synthesize_metronome_into(&mut self, dst: &Arc<AudioIO>, frames: usize) {
        let buf = dst.buffer.lock();
        buf.fill(0.0);
        if !self.metronome_enabled || !self.clip_playback_enabled || frames == 0 {
            return;
        }
        let metronome_gain = 10.0_f32.powf((-Self::METRONOME_DEFAULT_LEVEL_DB) / 20.0);
        let sample_rate = self.sample_rate.max(1.0);
        let denom = self.tsig_denom.max(1) as f64;
        let beats_per_bar = self.tsig_num.max(1) as u64;
        let samples_per_beat = ((sample_rate * 60.0) / self.tempo_bpm.max(1.0)) * (4.0 / denom);
        if !samples_per_beat.is_finite() || samples_per_beat <= 1.0 {
            return;
        }
        let segments = self.cycle_segments(frames);
        for (seg_start, seg_end, frame_offset) in segments {
            if seg_end <= seg_start {
                continue;
            }
            let mut beat_idx = ((seg_start as f64) / samples_per_beat).ceil() as u64;
            loop {
                let beat_sample = (beat_idx as f64 * samples_per_beat).round() as usize;
                if beat_sample >= seg_end {
                    break;
                }
                if beat_sample >= seg_start {
                    let hit_frame = frame_offset + (beat_sample - seg_start);
                    if hit_frame < frames {
                        let accented = beat_idx % beats_per_bar == 0;
                        let freq = if accented { 1_760.0_f32 } else { 1_320.0_f32 };
                        let amp = if accented { 0.30_f32 } else { 0.22_f32 } * metronome_gain;
                        let click_len = ((sample_rate as usize) / 50).max(64);
                        let phase_step = core::f32::consts::TAU * (freq / sample_rate as f32);
                        let end = (hit_frame + click_len).min(frames).min(buf.len());
                        for n in hit_frame..end {
                            let t = (n - hit_frame) as f32;
                            let env = (-t / (click_len as f32 * 0.28)).exp();
                            let s = (t * phase_step).sin() * amp * env;
                            buf[n] = (buf[n] + s).clamp(-1.0, 1.0);
                        }
                    }
                }
                beat_idx = beat_idx.saturating_add(1);
            }
        }
    }

    pub fn process(&mut self) {
        for audio_in in &self.audio.ins {
            audio_in.process();
        }
        let frames = self
            .audio
            .ins
            .first()
            .map(|audio_in| audio_in.buffer.lock().len())
            .or_else(|| {
                self.audio
                    .outs
                    .first()
                    .map(|audio_out| audio_out.buffer.lock().len())
            })
            .unwrap_or(self.process_block_size);
        if let Some(source) = self.ensure_metronome_source(frames) {
            self.synthesize_metronome_into(&source, frames);
        }
        let clip_playback_active = self.disk_monitor && self.clip_playback_enabled;
        if clip_playback_active {
            self.preload_audio_clip_cache();
            self.preload_midi_clip_cache();
        }
        let mut track_input_midi_events = self.collect_track_input_midi_events();
        if clip_playback_active {
            self.mix_clip_midi_into_inputs(&mut track_input_midi_events, frames);
            if !self.input_monitor {
                for audio_in in &self.audio.ins {
                    audio_in.buffer.lock().fill(0.0);
                }
            }
            self.mix_clip_audio_into_inputs();
        }

        #[cfg(target_os = "macos")]
        let mut plugin_midi_events = track_input_midi_events.first().cloned().unwrap_or_default();
        #[cfg(target_os = "macos")]
        let mut last_clap_output: Vec<ClapMidiOutputEvent> = Vec::new();

        #[cfg(all(unix, not(target_os = "macos")))]
        {
            let mut lv2_processed = vec![false; self.lv2_processors.len()];
            let mut vst3_processed = vec![false; self.vst3_processors.len()];
            let mut clap_processed = vec![false; self.clap_plugins.len()];
            let mut remaining = lv2_processed.len() + vst3_processed.len() + clap_processed.len();
            let mut processed_midi_plugins = HashSet::<PluginGraphNode>::new();
            let mut midi_node_events = HashMap::<(PluginGraphNode, usize), Vec<MidiEvent>>::new();

            while remaining > 0 {
                let mut progressed = false;

                for (idx, already_processed) in lv2_processed.iter_mut().enumerate() {
                    if *already_processed {
                        continue;
                    }
                    let all_inputs_ready = self.lv2_processors[idx]
                        .processor
                        .audio_inputs()
                        .iter()
                        .all(|audio_in| audio_in.ready());
                    if !all_inputs_ready {
                        continue;
                    }
                    let instance_id = self.lv2_processors[idx].id;
                    let node = PluginGraphNode::Lv2PluginInstance(instance_id);
                    if !self.plugin_midi_ready(&node, &processed_midi_plugins) {
                        continue;
                    }

                    for audio_in in self.lv2_processors[idx].processor.audio_inputs() {
                        audio_in.process();
                    }
                    let midi_inputs = self.plugin_midi_input_events(
                        &node,
                        self.lv2_processors[idx].processor.midi_input_count(),
                        &track_input_midi_events,
                        &midi_node_events,
                    );
                    let midi_outputs = self.lv2_processors[idx].processor.process_with_audio_io(
                        frames,
                        &midi_inputs,
                        Lv2TransportInfo {
                            transport_sample: self.transport_sample,
                            playing: self.disk_monitor && self.clip_playback_enabled,
                            bpm: self.tempo_bpm,
                            tsig_num: u32::from(self.tsig_num),
                            tsig_denom: u32::from(self.tsig_denom),
                        },
                    );
                    for (port, events) in midi_outputs.into_iter().enumerate() {
                        if !events.is_empty() {
                            midi_node_events.insert((node.clone(), port), events);
                        }
                    }
                    *already_processed = true;
                    remaining = remaining.saturating_sub(1);
                    processed_midi_plugins.insert(node);
                    progressed = true;
                }

                for (idx, already_processed) in vst3_processed.iter_mut().enumerate() {
                    if *already_processed {
                        continue;
                    }
                    let ready = self.vst3_processors[idx]
                        .processor
                        .audio_inputs()
                        .iter()
                        .all(|audio_in| audio_in.ready());
                    if !ready {
                        continue;
                    }
                    let node = PluginGraphNode::Vst3PluginInstance(self.vst3_processors[idx].id);
                    if !self.plugin_midi_ready(&node, &processed_midi_plugins) {
                        continue;
                    }
                    let midi_inputs = self.plugin_midi_input_events(
                        &node,
                        self.vst3_processors[idx].processor.midi_input_count(),
                        &track_input_midi_events,
                        &midi_node_events,
                    );
                    let vst3_input = midi_inputs.first().cloned().unwrap_or_default();
                    let outputs = self.vst3_processors[idx]
                        .processor
                        .process_with_midi(frames, &vst3_input);
                    if !outputs.is_empty() {
                        midi_node_events.insert((node.clone(), 0), outputs);
                    }
                    *already_processed = true;
                    remaining = remaining.saturating_sub(1);
                    processed_midi_plugins.insert(node);
                    progressed = true;
                }

                for (idx, already_processed) in clap_processed.iter_mut().enumerate() {
                    if *already_processed {
                        continue;
                    }
                    let ready = self.clap_plugins[idx]
                        .processor
                        .audio_inputs()
                        .iter()
                        .all(|audio_in| audio_in.ready());
                    if !ready {
                        continue;
                    }
                    let node = PluginGraphNode::ClapPluginInstance(self.clap_plugins[idx].id);
                    if !self.plugin_midi_ready(&node, &processed_midi_plugins) {
                        continue;
                    }
                    let midi_inputs = self.plugin_midi_input_events(
                        &node,
                        self.clap_plugins[idx].processor.midi_input_count(),
                        &track_input_midi_events,
                        &midi_node_events,
                    );
                    let clap_input = midi_inputs.first().cloned().unwrap_or_default();
                    let outputs = self.clap_plugins[idx].processor.process_with_midi(
                        frames,
                        &clap_input,
                        ClapTransportInfo {
                            transport_sample: self.transport_sample,
                            playing: self.disk_monitor && self.clip_playback_enabled,
                            loop_enabled: self.loop_enabled,
                            loop_range_samples: self.loop_range_samples,
                            bpm: self.tempo_bpm,
                            tsig_num: self.tsig_num,
                            tsig_denom: self.tsig_denom,
                        },
                    );
                    for evt in outputs {
                        midi_node_events
                            .entry((node.clone(), evt.port))
                            .or_default()
                            .push(evt.event);
                    }
                    *already_processed = true;
                    remaining = remaining.saturating_sub(1);
                    processed_midi_plugins.insert(node);
                    progressed = true;
                }

                if !progressed {
                    break;
                }
            }

            for (idx, done) in lv2_processed.iter().enumerate() {
                if *done {
                    continue;
                }
                for audio_in in self.lv2_processors[idx].processor.audio_inputs() {
                    audio_in.process();
                }
                let instance_id = self.lv2_processors[idx].id;
                let node = PluginGraphNode::Lv2PluginInstance(instance_id);
                let midi_inputs = self.plugin_midi_input_events(
                    &node,
                    self.lv2_processors[idx].processor.midi_input_count(),
                    &track_input_midi_events,
                    &midi_node_events,
                );
                let midi_outputs = self.lv2_processors[idx].processor.process_with_audio_io(
                    frames,
                    &midi_inputs,
                    Lv2TransportInfo {
                        transport_sample: self.transport_sample,
                        playing: self.disk_monitor && self.clip_playback_enabled,
                        bpm: self.tempo_bpm,
                        tsig_num: u32::from(self.tsig_num),
                        tsig_denom: u32::from(self.tsig_denom),
                    },
                );
                for (port, events) in midi_outputs.into_iter().enumerate() {
                    if !events.is_empty() {
                        midi_node_events.insert((node.clone(), port), events);
                    }
                }
            }
            for (idx, done) in vst3_processed.iter().enumerate() {
                if *done {
                    continue;
                }
                let node = PluginGraphNode::Vst3PluginInstance(self.vst3_processors[idx].id);
                let midi_inputs = self.plugin_midi_input_events(
                    &node,
                    self.vst3_processors[idx].processor.midi_input_count(),
                    &track_input_midi_events,
                    &midi_node_events,
                );
                let vst3_input = midi_inputs.first().cloned().unwrap_or_default();
                let outputs = self.vst3_processors[idx]
                    .processor
                    .process_with_midi(frames, &vst3_input);
                if !outputs.is_empty() {
                    midi_node_events.insert((node, 0), outputs);
                }
            }
            for (idx, done) in clap_processed.iter().enumerate() {
                if *done {
                    continue;
                }
                let node = PluginGraphNode::ClapPluginInstance(self.clap_plugins[idx].id);
                let midi_inputs = self.plugin_midi_input_events(
                    &node,
                    self.clap_plugins[idx].processor.midi_input_count(),
                    &track_input_midi_events,
                    &midi_node_events,
                );
                let clap_input = midi_inputs.first().cloned().unwrap_or_default();
                let outputs = self.clap_plugins[idx].processor.process_with_midi(
                    frames,
                    &clap_input,
                    ClapTransportInfo {
                        transport_sample: self.transport_sample,
                        playing: self.disk_monitor && self.clip_playback_enabled,
                        loop_enabled: self.loop_enabled,
                        loop_range_samples: self.loop_range_samples,
                        bpm: self.tempo_bpm,
                        tsig_num: self.tsig_num,
                        tsig_denom: self.tsig_denom,
                    },
                );
                for evt in outputs {
                    midi_node_events
                        .entry((node.clone(), evt.port))
                        .or_default()
                        .push(evt.event);
                }
            }

            self.route_plugin_midi_to_track_outputs_graph(
                &track_input_midi_events,
                &midi_node_events,
            );
        }

        #[cfg(target_os = "macos")]
        {
            if !self.vst3_processors.is_empty() {
                for instance in &self.vst3_processors {
                    let ready = instance
                        .processor
                        .audio_inputs()
                        .iter()
                        .all(|audio_in| audio_in.ready());
                    if ready {
                        plugin_midi_events = instance
                            .processor
                            .process_with_midi(frames, &plugin_midi_events);
                    }
                }
            }
            if !self.clap_plugins.is_empty() {
                let mut clap_midi_events = plugin_midi_events.clone();
                for instance in &self.clap_plugins {
                    let ready = instance
                        .processor
                        .audio_inputs()
                        .iter()
                        .all(|audio_in| audio_in.ready());
                    if ready {
                        last_clap_output = instance.processor.process_with_midi(
                            frames,
                            &clap_midi_events,
                            ClapTransportInfo {
                                transport_sample: self.transport_sample,
                                playing: self.disk_monitor && self.clip_playback_enabled,
                                loop_enabled: self.loop_enabled,
                                loop_range_samples: self.loop_range_samples,
                                bpm: self.tempo_bpm,
                                tsig_num: self.tsig_num,
                                tsig_denom: self.tsig_denom,
                            },
                        );
                        clap_midi_events = last_clap_output
                            .iter()
                            .map(|evt| evt.event.clone())
                            .collect();
                    }
                }
            }
        }

        self.ensure_midi_route_cache();
        self.route_track_inputs_to_track_outputs(&track_input_midi_events);
        #[cfg(target_os = "macos")]
        {
            if self.clap_plugins.is_empty() {
                self.route_plugin_midi_to_track_outputs(&plugin_midi_events);
            } else {
                self.route_clap_midi_to_track_outputs(&last_clap_output);
            }
        }
        self.collect_hw_midi_output_events();
        self.dispatch_track_output_midi_to_connected_inputs();
        self.clear_local_midi_inputs();
        let linear_gain = 10.0_f32.powf(self.level / 20.0);
        let (left_balance, right_balance) = if self.audio.outs.len() == 2 {
            let b = self.balance.clamp(-1.0, 1.0);
            ((1.0 - b).clamp(0.0, 1.0), (1.0 + b).clamp(0.0, 1.0))
        } else {
            (1.0, 1.0)
        };

        self.ensure_audio_route_cache();
        if self.output_meter_linear_cache.len() != self.audio.outs.len() {
            self.output_meter_linear_cache
                .resize(self.audio.outs.len(), 0.0);
        }
        if self.meter_peak_hold_linear.len() != self.audio.outs.len() {
            self.meter_peak_hold_linear
                .resize(self.audio.outs.len(), 0.0);
        }
        for out_idx in 0..self.audio.outs.len() {
            let audio_out = self.audio.outs[out_idx].clone();
            let out_samples = audio_out.buffer.lock();
            let capture_record_tap = self.armed && self.record_tap_enabled;
            if capture_record_tap {
                if self.record_tap_outs.len() <= out_idx {
                    self.record_tap_outs.push(vec![0.0; out_samples.len()]);
                }
                if self.record_tap_outs[out_idx].len() != out_samples.len() {
                    self.record_tap_outs[out_idx].resize(out_samples.len(), 0.0);
                }
            }
            let balance_gain = if self.audio.outs.len() == 2 {
                if out_idx == 0 {
                    left_balance
                } else {
                    right_balance
                }
            } else {
                1.0
            };
            let output_gain = linear_gain * balance_gain;
            let unity_output_gain = (output_gain - 1.0).abs() <= f32::EPSILON;
            let sources = self.internal_output_routes_cache.get(out_idx);
            let has_sources = sources.is_some_and(|s| !s.is_empty());
            out_samples.fill(0.0);
            if self.output_enabled
                && let Some(sources) = sources
            {
                let mut seeded = false;
                for source in sources {
                    if !self.input_monitor
                        && !clip_playback_active
                        && self.is_track_input_source(source)
                    {
                        continue;
                    }
                    let source_buf = source.buffer.lock();
                    if !seeded {
                        if unity_output_gain {
                            Self::copy_unity_with_zero_tail(out_samples, source_buf);
                        } else {
                            Self::copy_scaled_with_zero_tail(out_samples, source_buf, output_gain);
                        }
                        seeded = true;
                    } else if unity_output_gain {
                        Self::add_unity(out_samples, source_buf);
                    } else {
                        Self::add_scaled(out_samples, source_buf, output_gain);
                    }
                }
            }

            if capture_record_tap {
                let tap = &mut self.record_tap_outs[out_idx];
                if has_sources {
                    if let Some(sources) = sources {
                        let first = sources[0].buffer.lock();
                        Self::copy_unity_with_zero_tail(tap, first);
                        for source in &sources[1..] {
                            let source_buf = source.buffer.lock();
                            Self::add_unity(tap, source_buf);
                        }
                    }
                } else {
                    tap.fill(0.0);
                }
            }
            let peak_now = out_samples
                .iter()
                .fold(0.0_f32, |acc, sample| acc.max(sample.abs()));
            // Peak-hold with decay gives stable, readable VU behavior for short transients.
            let held = self.meter_peak_hold_linear[out_idx] * 0.92;
            let next = peak_now.max(held);
            self.meter_peak_hold_linear[out_idx] = next;
            self.output_meter_linear_cache[out_idx] = next;
            *audio_out.finished.lock() = true;
        }

        self.audio.finished = true;
        self.audio.processing = false;
    }

    pub fn name(&self) -> String {
        self.name.clone()
    }
    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }

    pub fn level(&self) -> f32 {
        self.level
    }
    pub fn set_level(&mut self, level: f32) {
        self.level = level;
    }
    pub fn set_balance(&mut self, balance: f32) {
        self.balance = balance.clamp(-1.0, 1.0);
    }

    pub fn output_meter_linear(&self) -> Vec<f32> {
        self.output_meter_linear_cache.clone()
    }

    pub fn arm(&mut self) {
        self.armed = !self.armed;
    }

    pub fn set_output_enabled(&mut self, enabled: bool) {
        self.output_enabled = enabled;
    }
    pub fn set_transport_sample(&mut self, sample: usize) {
        self.transport_sample = sample;
    }
    pub fn set_loop_config(&mut self, enabled: bool, range: Option<(usize, usize)>) {
        self.loop_enabled = enabled;
        self.loop_range_samples = range;
    }
    pub fn set_transport_timing(&mut self, tempo_bpm: f64, tsig_num: u16, tsig_denom: u16) {
        self.tempo_bpm = tempo_bpm.max(1.0);
        self.tsig_num = tsig_num.max(1);
        self.tsig_denom = tsig_denom.max(1);
    }
    pub fn set_clip_playback_enabled(&mut self, enabled: bool) {
        self.clip_playback_enabled = enabled;
    }
    pub fn set_metronome_enabled(&mut self, enabled: bool) {
        self.metronome_enabled = enabled;
    }
    pub fn set_record_tap_enabled(&mut self, enabled: bool) {
        self.record_tap_enabled = enabled;
    }

    pub fn set_midi_lane_channel(&mut self, lane: usize, channel: Option<u8>) {
        if let Some(slot) = self.midi_lane_channels.get_mut(lane) {
            *slot = channel.map(|channel| channel.min(15));
        }
    }
    pub fn mute(&mut self) {
        self.muted = !self.muted;
    }
    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
    }
    pub fn solo(&mut self) {
        self.soloed = !self.soloed;
    }
    pub fn toggle_input_monitor(&mut self) {
        self.input_monitor = !self.input_monitor;
    }
    pub fn toggle_disk_monitor(&mut self) {
        self.disk_monitor = !self.disk_monitor;
    }
    pub fn set_vca_master(&mut self, master: Option<String>) {
        self.vca_master = master;
    }
    pub fn vca_master(&self) -> Option<String> {
        self.vca_master.clone()
    }
    pub fn set_session_base_dir(&mut self, base_dir: Option<PathBuf>) {
        if self.session_base_dir != base_dir {
            self.session_base_dir = base_dir;
            // Clip names are relative in sessions; if base dir changes, cached buffers can go stale.
            self.audio_clip_cache.clear();
            self.midi_clip_cache.clear();
        }
    }

    pub fn frozen(&self) -> bool {
        self.frozen
    }

    pub fn set_frozen(&mut self, frozen: bool) {
        self.frozen = frozen;
    }

    pub fn offline_bounce_interleaved(
        &mut self,
        start_sample: usize,
        length_samples: usize,
    ) -> (usize, Vec<f32>) {
        let channels = self.audio.outs.len().max(1);
        if length_samples == 0 {
            return (channels, vec![]);
        }
        let block_size = self
            .audio
            .outs
            .first()
            .map(|io| io.buffer.lock().len())
            .or_else(|| self.audio.ins.first().map(|io| io.buffer.lock().len()))
            .unwrap_or(0)
            .max(1);

        let saved_transport = self.transport_sample;
        let saved_disk_monitor = self.disk_monitor;
        let saved_input_monitor = self.input_monitor;
        let saved_clip_playback_enabled = self.clip_playback_enabled;
        let saved_record_tap_enabled = self.record_tap_enabled;
        let saved_armed = self.armed;
        let saved_output_enabled = self.output_enabled;
        let saved_loop_enabled = self.loop_enabled;
        let saved_loop_range = self.loop_range_samples;
        let saved_pending_hw = self.pending_hw_midi_out_events.clone();

        self.disk_monitor = true;
        self.input_monitor = false;
        self.clip_playback_enabled = true;
        self.record_tap_enabled = false;
        self.armed = false;
        self.output_enabled = true;
        self.loop_enabled = false;
        self.loop_range_samples = None;
        self.pending_hw_midi_out_events.clear();

        let mut rendered = vec![0.0_f32; length_samples.saturating_mul(channels)];
        let mut cursor = 0usize;
        while cursor < length_samples {
            self.transport_sample = start_sample.saturating_add(cursor);
            self.process();
            let step = (length_samples - cursor).min(block_size);
            for ch in 0..channels {
                let out = self.audio.outs[ch].buffer.lock();
                let copy_len = step.min(out.len());
                for i in 0..copy_len {
                    let dst = (cursor + i) * channels + ch;
                    rendered[dst] = out[i];
                }
            }
            cursor = cursor.saturating_add(step);
            self.pending_hw_midi_out_events.clear();
        }

        self.transport_sample = saved_transport;
        self.disk_monitor = saved_disk_monitor;
        self.input_monitor = saved_input_monitor;
        self.clip_playback_enabled = saved_clip_playback_enabled;
        self.record_tap_enabled = saved_record_tap_enabled;
        self.armed = saved_armed;
        self.output_enabled = saved_output_enabled;
        self.loop_enabled = saved_loop_enabled;
        self.loop_range_samples = saved_loop_range;
        self.pending_hw_midi_out_events = saved_pending_hw;

        (channels, rendered)
    }

    pub(crate) fn resolve_clip_path(&self, clip_name: &str) -> PathBuf {
        let clip_path = Path::new(clip_name);
        if clip_path.is_absolute() {
            clip_path.to_path_buf()
        } else {
            if let Some(base) = &self.session_base_dir {
                let candidate = base.join(clip_path);
                if candidate.exists() {
                    return candidate;
                }
            }

            // Keep direct relative path support for runs started from the session directory.
            let cwd_candidate = clip_path.to_path_buf();
            if cwd_candidate.exists() {
                return cwd_candidate;
            }

            // Allow explicit session root override for diagnostics/recovery scenarios.
            if let Ok(session_root) = std::env::var("MAOLAN_SESSION_PATH") {
                let candidate = Path::new(&session_root).join(clip_path);
                if candidate.exists() {
                    return candidate;
                }
            }

            // Fallback to default recordings folder used by the app.
            if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
                let candidate = Path::new(&home).join("recordings").join(clip_path);
                if candidate.exists() {
                    return candidate;
                }
            }

            // Preserve original behavior for missing files: if we know session root, keep it.
            if let Some(base) = &self.session_base_dir {
                base.join(clip_path)
            } else {
                cwd_candidate
            }
        }
    }

    pub(crate) fn invalidate_midi_clip_cache(&mut self, clip_name: &str) {
        self.midi_clip_cache.remove(clip_name);
    }

    fn load_audio_clip_buffer(path: &Path) -> Option<AudioClipBuffer> {
        let mut wav = wavers::Wav::<f32>::from_path(path).ok()?;
        let channels = wav.n_channels().max(1) as usize;
        let samples: wavers::Samples<f32> = wav.read().ok()?;
        if samples.is_empty() {
            return None;
        }
        Some(AudioClipBuffer {
            channels,
            samples: samples.to_vec(),
        })
    }

    fn clip_buffer(&mut self, clip_name: &str) -> Option<Arc<AudioClipBuffer>> {
        if let Some(cached) = self.audio_clip_cache.get(clip_name) {
            return Some(cached.clone());
        }
        let path = self.resolve_clip_path(clip_name);
        let loaded = Self::load_audio_clip_buffer(&path)?;
        let loaded = Arc::new(loaded);
        self.audio_clip_cache
            .insert(clip_name.to_string(), loaded.clone());
        Some(loaded)
    }

    fn preload_audio_clip_cache(&mut self) {
        let missing: Vec<String> = self
            .audio
            .clips
            .iter()
            .filter_map(|clip| {
                if self.audio_clip_cache.contains_key(&clip.name) {
                    None
                } else {
                    Some(clip.name.clone())
                }
            })
            .collect();
        for clip_name in missing {
            let _ = self.clip_buffer(&clip_name);
        }
    }

    fn load_midi_clip_events(path: &Path, sample_rate: f64) -> Option<Vec<(usize, Vec<u8>)>> {
        let bytes = std::fs::read(path).ok()?;
        let smf = Smf::parse(&bytes).ok()?;
        let Timing::Metrical(ppq) = smf.header.timing else {
            return None;
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
                    (seg_ticks as u128).saturating_mul(current_tempo_us as u128) / (ppq as u128),
                );
                prev_tick = *change_tick;
                current_tempo_us = *tempo_us;
            }
            let tail_ticks = tick.saturating_sub(prev_tick);
            total_us = total_us.saturating_add(
                (tail_ticks as u128).saturating_mul(current_tempo_us as u128) / (ppq as u128),
            );
            ((total_us as f64) * (sample_rate / 1_000_000.0)).round() as usize
        };

        let mut out = Vec::<(usize, Vec<u8>)>::new();
        for track in &smf.tracks {
            let mut tick = 0_u64;
            for event in track {
                tick = tick.saturating_add(event.delta.as_int() as u64);
                let data = match event.kind {
                    TrackEventKind::Midi { channel, message } => {
                        let mut data = Vec::with_capacity(3);
                        if (LiveEvent::Midi { channel, message })
                            .write(&mut data)
                            .is_ok()
                        {
                            Some(data)
                        } else {
                            None
                        }
                    }
                    TrackEventKind::SysEx(payload) => {
                        let mut data = Vec::with_capacity(payload.len() + 2);
                        data.push(0xF0);
                        data.extend_from_slice(payload);
                        if data.last().copied() != Some(0xF7) {
                            data.push(0xF7);
                        }
                        Some(data)
                    }
                    // "Escape" packets are already prefixed with 0xF7 in SMF stream form.
                    TrackEventKind::Escape(payload) => {
                        let mut data = Vec::with_capacity(payload.len() + 1);
                        data.push(0xF7);
                        data.extend_from_slice(payload);
                        Some(data)
                    }
                    _ => None,
                };
                if let Some(data) = data {
                    out.push((ticks_to_samples(tick), data));
                }
            }
        }
        out.sort_by_key(|(sample, _)| *sample);
        Some(out)
    }

    fn midi_clip_events(&mut self, clip_name: &str) -> Option<MidiClipEvents> {
        if let Some(cached) = self.midi_clip_cache.get(clip_name) {
            return Some(cached.clone());
        }
        let path = self.resolve_clip_path(clip_name);
        let loaded = Self::load_midi_clip_events(&path, self.sample_rate)?;
        let loaded = Arc::new(loaded);
        self.midi_clip_cache
            .insert(clip_name.to_string(), loaded.clone());
        Some(loaded)
    }

    fn preload_midi_clip_cache(&mut self) {
        let missing: Vec<String> = self
            .midi
            .clips
            .iter()
            .filter_map(|clip| {
                if self.midi_clip_cache.contains_key(&clip.name) {
                    None
                } else {
                    Some(clip.name.clone())
                }
            })
            .collect();
        for clip_name in missing {
            let _ = self.midi_clip_events(&clip_name);
        }
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
            let segment_end_limit = loop_end;
            let take = segment_end_limit.saturating_sub(current).min(remaining);
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

    /// Ardour-like constant-power fade-in.
    /// gain = sin(t * pi/2)
    /// Input: t in range [0.0, 1.0] (position within fade-in region)
    /// Output: gain value in range [0.0, 1.0]
    #[inline]
    fn fade_in_curve(t: f32) -> f32 {
        (t.clamp(0.0, 1.0) * std::f32::consts::FRAC_PI_2).sin()
    }

    /// Ardour-like constant-power fade-out.
    /// gain = cos(t * pi/2)
    /// Input: t in range [0.0, 1.0] (position within fade-out region)
    /// Output: gain value in range [1.0, 0.0]
    #[inline]
    fn fade_out_curve(t: f32) -> f32 {
        (t.clamp(0.0, 1.0) * std::f32::consts::FRAC_PI_2).cos()
    }

    fn warped_source_sample_for_output(
        clip_len: usize,
        output_sample_in_clip: usize,
        warp_markers: &[crate::message::AudioWarpMarker],
    ) -> usize {
        if clip_len == 0 || warp_markers.is_empty() {
            return output_sample_in_clip;
        }

        let mut points: Vec<(usize, usize)> = Vec::with_capacity(warp_markers.len() + 2);
        points.push((0, 0));
        for marker in warp_markers {
            points.push((marker.timeline_sample.min(clip_len), marker.source_sample));
        }
        points.push((clip_len, clip_len));
        points.sort_unstable_by_key(|(timeline, _)| *timeline);
        points.dedup_by_key(|(timeline, _)| *timeline);
        if points.len() < 2 {
            return output_sample_in_clip;
        }

        let x = output_sample_in_clip.min(clip_len);
        for window in points.windows(2) {
            let (x0, y0) = window[0];
            let (x1, y1) = window[1];
            if x < x0 || x > x1 {
                continue;
            }
            if x1 == x0 {
                return y0;
            }
            let t = (x - x0) as f64 / (x1 - x0) as f64;
            let mapped = y0 as f64 + (y1 as f64 - y0 as f64) * t;
            return mapped.max(0.0).round() as usize;
        }
        output_sample_in_clip
    }

    fn mix_clip_audio_into_inputs(&mut self) {
        let frames = self
            .audio
            .ins
            .first()
            .map(|audio_in| audio_in.buffer.lock().len())
            .unwrap_or(0);
        if frames == 0 || self.audio.ins.is_empty() {
            return;
        }

        let segments = self.cycle_segments(frames);
        for clip in &self.audio.clips {
            if clip.muted {
                continue;
            }
            let clip_start = clip.start;
            let clip_len = clip.end;
            if clip_len == 0 {
                continue;
            }
            let clip_end = clip_start.saturating_add(clip_len);
            let Some(buffer) = self.audio_clip_cache.get(&clip.name) else {
                continue;
            };
            let channels = buffer.channels.max(1);
            let total_frames = buffer.samples.len() / channels;
            if total_frames == 0 {
                continue;
            }

            for in_channel in 0..self.audio.ins.len() {
                let source_channel = if channels == 1 {
                    0
                } else if in_channel < channels {
                    in_channel
                } else {
                    continue;
                };
                let in_samples = self.audio.ins[in_channel].buffer.lock();

                for (segment_start, segment_end, out_offset) in &segments {
                    if clip_end <= *segment_start || clip_start >= *segment_end {
                        continue;
                    }
                    let from = (*segment_start).max(clip_start);
                    let to = (*segment_end).min(clip_end);
                    for absolute_sample in from..to {
                        let track_idx = out_offset + (absolute_sample - *segment_start);
                        let output_sample_in_clip = absolute_sample - clip_start;
                        let warped_source_sample = Self::warped_source_sample_for_output(
                            clip_len,
                            output_sample_in_clip,
                            &clip.warp_markers,
                        );
                        let clip_idx = warped_source_sample + clip.offset;
                        if clip_idx >= total_frames || track_idx >= in_samples.len() {
                            break;
                        }
                        let mut sample = buffer.samples[clip_idx * channels + source_channel];

                        // Apply fade curves if enabled
                        if clip.fade_enabled {
                            let clip_sample_pos = absolute_sample - clip_start;

                            // Apply fade-in
                            if clip_sample_pos < clip.fade_in_samples {
                                let t = clip_sample_pos as f32 / clip.fade_in_samples.max(1) as f32;
                                sample *= Self::fade_in_curve(t);
                            }

                            // Apply fade-out
                            if clip_sample_pos >= clip_len.saturating_sub(clip.fade_out_samples) {
                                let fade_out_start = clip_len.saturating_sub(clip.fade_out_samples);
                                let t = (clip_sample_pos - fade_out_start) as f32
                                    / clip.fade_out_samples.max(1) as f32;
                                sample *= Self::fade_out_curve(t);
                            }
                        }

                        in_samples[track_idx] += sample;
                    }
                }
            }
        }
    }

    fn mix_clip_midi_into_inputs(&mut self, input_events: &mut [Vec<MidiEvent>], frames: usize) {
        if frames == 0 || input_events.is_empty() {
            return;
        }
        let segments = self.cycle_segments(frames);
        for clip in &self.midi.clips {
            if clip.muted {
                continue;
            }
            let clip_start = clip.start;
            let clip_len = clip.end;
            if clip_len == 0 {
                continue;
            }
            let input_lane = clip.input_channel.min(input_events.len().saturating_sub(1));
            let clip_end = clip_start.saturating_add(clip_len);
            let Some(events) = self.midi_clip_cache.get(&clip.name) else {
                continue;
            };
            for (segment_start, segment_end, out_offset) in &segments {
                if clip_end <= *segment_start || clip_start >= *segment_end {
                    continue;
                }
                let from = (*segment_start).max(clip_start);
                let to = (*segment_end).min(clip_end);
                let source_from = from.saturating_sub(clip_start).saturating_add(clip.offset);
                let source_to = to.saturating_sub(clip_start).saturating_add(clip.offset);
                for (source_sample, data) in events.iter() {
                    if *source_sample < source_from {
                        continue;
                    }
                    if *source_sample >= source_to {
                        break;
                    }
                    let absolute_sample =
                        clip_start.saturating_add(source_sample.saturating_sub(clip.offset));
                    let frame_idx = out_offset.saturating_add(absolute_sample - *segment_start);
                    if frame_idx >= frames {
                        continue;
                    }
                    input_events[input_lane].push(MidiEvent::new(frame_idx as u32, data.clone()));
                }
            }
        }
        for events in input_events.iter_mut() {
            events.sort_by_key(|event| event.frame);
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    pub fn load_lv2_plugin(&mut self, uri: &str) -> Result<(), String> {
        let buffer_size = self
            .audio
            .ins
            .first()
            .map(|io| io.buffer.lock().len())
            .or_else(|| self.audio.outs.first().map(|io| io.buffer.lock().len()))
            .unwrap_or(0);
        let processor = Lv2Processor::new(self.sample_rate, buffer_size, uri)?;
        let mut processor = processor;
        if let Some(base_dir) = &self.lv2_state_base_dir {
            processor.set_state_base_dir(base_dir.clone());
        }
        let id = self.alloc_plugin_instance_id();
        self.next_lv2_instance_id = self.next_lv2_instance_id.max(id.saturating_add(1));
        self.lv2_processors.push(Lv2Instance { id, processor });
        self.invalidate_audio_route_cache();
        Ok(())
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    pub fn unload_lv2_plugin(&mut self, uri: &str) -> Result<(), String> {
        let Some(index) = self
            .lv2_processors
            .iter()
            .position(|instance| instance.processor.uri() == uri)
        else {
            return Err(format!(
                "Track '{}' does not have LV2 plugin loaded: {uri}",
                self.name
            ));
        };

        self.remove_lv2_instance(index);
        Ok(())
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    pub fn unload_lv2_plugin_instance(&mut self, instance_id: usize) -> Result<(), String> {
        let Some(index) = self
            .lv2_processors
            .iter()
            .position(|instance| instance.id == instance_id)
        else {
            return Err(format!(
                "Track '{}' does not have LV2 instance id: {}",
                self.name, instance_id
            ));
        };

        self.remove_lv2_instance(index);
        Ok(())
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn remove_lv2_instance(&mut self, index: usize) {
        let removed = self.lv2_processors.remove(index);
        for port in removed.processor.audio_inputs() {
            Self::disconnect_all(port);
        }
        for port in removed.processor.audio_outputs() {
            Self::disconnect_all(port);
        }
        self.plugin_midi_connections.retain(|conn| {
            conn.from_node != PluginGraphNode::Lv2PluginInstance(removed.id)
                && conn.to_node != PluginGraphNode::Lv2PluginInstance(removed.id)
        });
        self.invalidate_audio_route_cache();
    }

    #[cfg(unix)]
    fn prune_plugin_midi_connections(&mut self, node: PluginGraphNode) {
        self.plugin_midi_connections
            .retain(|conn| conn.from_node != node && conn.to_node != node);
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    pub fn loaded_lv2_plugins(&self) -> Vec<String> {
        self.lv2_processors
            .iter()
            .map(|instance| instance.processor.uri().to_string())
            .collect()
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    pub fn loaded_lv2_instances(&self) -> Vec<(usize, String)> {
        self.lv2_processors
            .iter()
            .map(|instance| (instance.id, instance.processor.uri().to_string()))
            .collect()
    }

    #[cfg(unix)]
    fn push_plugin_graph_plugin(
        plugins: &mut Vec<PluginGraphPlugin>,
        node: PluginGraphNode,
        instance_id: usize,
        format: &'static str,
        uri: String,
        plugin_id: String,
        name: String,
        main_audio_inputs: usize,
        main_audio_outputs: usize,
        audio_inputs: usize,
        audio_outputs: usize,
        midi_inputs: usize,
        midi_outputs: usize,
        state: Option<Lv2PluginState>,
    ) {
        plugins.push(PluginGraphPlugin {
            node,
            instance_id,
            format: format.to_string(),
            uri,
            plugin_id,
            name,
            main_audio_inputs,
            main_audio_outputs,
            audio_inputs,
            audio_outputs,
            midi_inputs,
            midi_outputs,
            state,
        });
    }

    #[cfg(unix)]
    pub fn plugin_graph_plugins(&self) -> Vec<PluginGraphPlugin> {
        let mut plugins = Vec::new();
        #[cfg(all(unix, not(target_os = "macos")))]
        for instance in &self.lv2_processors {
            Self::push_plugin_graph_plugin(
                &mut plugins,
                PluginGraphNode::Lv2PluginInstance(instance.id),
                instance.id,
                "LV2",
                instance.processor.uri().to_string(),
                String::new(),
                instance.processor.name().to_string(),
                instance.processor.main_audio_input_count(),
                instance.processor.main_audio_output_count(),
                instance.processor.audio_input_count(),
                instance.processor.audio_output_count(),
                instance.processor.midi_input_count(),
                instance.processor.midi_output_count(),
                Some(instance.processor.snapshot_state()),
            );
        }
        for instance in &self.vst3_processors {
            Self::push_plugin_graph_plugin(
                &mut plugins,
                PluginGraphNode::Vst3PluginInstance(instance.id),
                instance.id,
                "VST3",
                instance.processor.path().to_string(),
                instance.processor.plugin_id().to_string(),
                instance.processor.name().to_string(),
                instance.processor.main_audio_input_count(),
                instance.processor.main_audio_output_count(),
                instance.processor.audio_inputs().len(),
                instance.processor.audio_outputs().len(),
                instance.processor.midi_input_count(),
                instance.processor.midi_output_count(),
                None,
            );
        }
        for instance in &self.clap_plugins {
            Self::push_plugin_graph_plugin(
                &mut plugins,
                PluginGraphNode::ClapPluginInstance(instance.id),
                instance.id,
                "CLAP",
                instance.processor.path().to_string(),
                String::new(),
                instance.processor.name().to_string(),
                instance.processor.main_audio_input_count(),
                instance.processor.main_audio_output_count(),
                instance.processor.audio_inputs().len(),
                instance.processor.audio_outputs().len(),
                instance.processor.midi_input_count(),
                instance.processor.midi_output_count(),
                None,
            );
        }
        plugins
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    pub fn set_lv2_plugin_state(
        &mut self,
        instance_id: usize,
        state: Lv2PluginState,
    ) -> Result<(), String> {
        let Some(instance) = self
            .lv2_processors
            .iter_mut()
            .find(|instance| instance.id == instance_id)
        else {
            return Err(format!(
                "Track '{}' does not have LV2 instance id: {}",
                self.name, instance_id
            ));
        };
        instance.processor.restore_state(&state)
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    pub fn lv2_plugin_controls(
        &self,
        instance_id: usize,
    ) -> Result<(Vec<Lv2ControlPortInfo>, Option<usize>), String> {
        let Some(instance) = self
            .lv2_processors
            .iter()
            .find(|instance| instance.id == instance_id)
        else {
            return Err(format!(
                "Track '{}' does not have LV2 instance id: {}",
                self.name, instance_id
            ));
        };
        Ok((
            instance.processor.control_ports_with_values(),
            instance.processor.instance_access_handle(),
        ))
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    pub fn set_lv2_control_value(
        &mut self,
        instance_id: usize,
        index: u32,
        value: f32,
    ) -> Result<(), String> {
        let Some(instance) = self
            .lv2_processors
            .iter_mut()
            .find(|instance| instance.id == instance_id)
        else {
            return Err(format!(
                "Track '{}' does not have LV2 instance id: {}",
                self.name, instance_id
            ));
        };
        instance.processor.set_control_value(index, value)
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    pub fn lv2_snapshot_state(&self, instance_id: usize) -> Result<Lv2PluginState, String> {
        let Some(instance) = self
            .lv2_processors
            .iter()
            .find(|instance| instance.id == instance_id)
        else {
            return Err(format!(
                "Track '{}' does not have LV2 instance id: {}",
                self.name, instance_id
            ));
        };
        Ok(instance.processor.snapshot_state())
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    pub fn get_lv2_midnam(&self) -> std::collections::HashMap<u8, String> {
        // Get midnam from the first LV2 plugin that has it
        for instance in &self.lv2_processors {
            let note_names = instance.processor.midnam_note_names();
            if !note_names.is_empty() {
                return note_names;
            }
        }
        std::collections::HashMap::new()
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    pub fn set_lv2_state_base_dir(&mut self, base_dir: Option<PathBuf>) {
        self.lv2_state_base_dir = base_dir.clone();
        if let Some(path) = base_dir {
            for instance in &mut self.lv2_processors {
                instance.processor.set_state_base_dir(path.clone());
            }
        }
    }

    pub fn load_vst3_plugin(&mut self, plugin_path: &str) -> Result<(), String> {
        let buffer_size = self
            .audio
            .ins
            .first()
            .map(|io| io.buffer.lock().len())
            .or_else(|| self.audio.outs.first().map(|io| io.buffer.lock().len()))
            .unwrap_or(64)
            .max(1);
        let input_count = self.audio.ins.len().max(1);
        let output_count = self.audio.outs.len().max(1);
        let processor = Vst3Processor::new_with_sample_rate(
            self.sample_rate,
            buffer_size,
            plugin_path,
            input_count,
            output_count,
        )?;
        let id = self.alloc_plugin_instance_id();
        self.next_vst3_instance_id = self.next_vst3_instance_id.max(id.saturating_add(1));
        self.vst3_processors.push(Vst3Instance { id, processor });
        self.invalidate_audio_route_cache();
        Ok(())
    }

    pub fn load_clap_plugin(&mut self, plugin_path: &str) -> Result<(), String> {
        let bundle_path = plugin_path
            .split_once("::")
            .map(|(path, _)| path)
            .unwrap_or(plugin_path);
        let path = Path::new(bundle_path);
        if !path.exists() {
            return Err(format!("CLAP plugin not found: {plugin_path}"));
        }
        if !path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("clap"))
        {
            return Err(format!("Not a CLAP plugin path: {plugin_path}"));
        }
        if self
            .clap_plugins
            .iter()
            .any(|plugin| plugin.processor.path().eq_ignore_ascii_case(plugin_path))
        {
            return Err(format!("CLAP plugin already loaded: {plugin_path}"));
        }

        let id = self.alloc_plugin_instance_id();
        self.next_clap_instance_id = self.next_clap_instance_id.max(id.saturating_add(1));
        let buffer_size = self
            .audio
            .ins
            .first()
            .map(|io| io.buffer.lock().len())
            .or_else(|| self.audio.outs.first().map(|io| io.buffer.lock().len()))
            .unwrap_or(0);
        let input_count = self.audio.ins.len().max(1);
        let output_count = self.audio.outs.len().max(1);
        let processor = ClapProcessor::new(
            self.sample_rate,
            buffer_size,
            plugin_path,
            input_count,
            output_count,
        )?;
        self.clap_plugins.push(ClapInstance { id, processor });
        self.invalidate_audio_route_cache();
        Ok(())
    }

    pub fn unload_clap_plugin_instance(&mut self, instance_id: usize) -> Result<(), String> {
        let Some(index) = self
            .clap_plugins
            .iter()
            .position(|instance| instance.id == instance_id)
        else {
            return Err(format!(
                "Track '{}' does not have CLAP instance id: {}",
                self.name, instance_id
            ));
        };
        self.clap_plugins.remove(index);
        #[cfg(unix)]
        self.prune_plugin_midi_connections(PluginGraphNode::ClapPluginInstance(instance_id));
        self.invalidate_audio_route_cache();
        Ok(())
    }

    pub fn unload_clap_plugin(&mut self, plugin_path: &str) -> Result<(), String> {
        let Some(index) = self
            .clap_plugins
            .iter()
            .position(|instance| instance.processor.path().eq_ignore_ascii_case(plugin_path))
        else {
            return Err(format!(
                "Track '{}' does not have CLAP plugin loaded: {}",
                self.name, plugin_path
            ));
        };
        let removed_id = self.clap_plugins[index].id;
        self.clap_plugins.remove(index);
        #[cfg(unix)]
        self.prune_plugin_midi_connections(PluginGraphNode::ClapPluginInstance(removed_id));
        self.invalidate_audio_route_cache();
        Ok(())
    }

    pub fn loaded_clap_instances(&self) -> Vec<(usize, String, String)> {
        self.clap_plugins
            .iter()
            .map(|instance| {
                (
                    instance.id,
                    instance.processor.path().to_string(),
                    instance.processor.name().to_string(),
                )
            })
            .collect()
    }

    pub fn set_clap_parameter(
        &self,
        instance_id: usize,
        param_id: u32,
        value: f64,
    ) -> Result<(), String> {
        let instance = self
            .clap_plugins
            .iter()
            .find(|instance| instance.id == instance_id)
            .ok_or_else(|| {
                format!(
                    "Track '{}' does not have CLAP instance id: {}",
                    self.name, instance_id
                )
            })?;
        instance.processor.set_parameter(param_id, value)
    }

    pub fn set_clap_parameter_at(
        &self,
        instance_id: usize,
        param_id: u32,
        value: f64,
        frame: u32,
    ) -> Result<(), String> {
        let instance = self
            .clap_plugins
            .iter()
            .find(|instance| instance.id == instance_id)
            .ok_or_else(|| {
                format!(
                    "Track '{}' does not have CLAP instance id: {}",
                    self.name, instance_id
                )
            })?;
        instance.processor.set_parameter_at(param_id, value, frame)
    }

    pub fn begin_clap_parameter_edit(
        &self,
        instance_id: usize,
        param_id: u32,
        frame: u32,
    ) -> Result<(), String> {
        let instance = self
            .clap_plugins
            .iter()
            .find(|instance| instance.id == instance_id)
            .ok_or_else(|| {
                format!(
                    "Track '{}' does not have CLAP instance id: {}",
                    self.name, instance_id
                )
            })?;
        instance.processor.begin_parameter_edit_at(param_id, frame)
    }

    pub fn end_clap_parameter_edit(
        &self,
        instance_id: usize,
        param_id: u32,
        frame: u32,
    ) -> Result<(), String> {
        let instance = self
            .clap_plugins
            .iter()
            .find(|instance| instance.id == instance_id)
            .ok_or_else(|| {
                format!(
                    "Track '{}' does not have CLAP instance id: {}",
                    self.name, instance_id
                )
            })?;
        instance.processor.end_parameter_edit_at(param_id, frame)
    }

    pub fn get_clap_parameters(
        &self,
        instance_id: usize,
    ) -> Result<Vec<crate::clap::ClapParameterInfo>, String> {
        let instance = self
            .clap_plugins
            .iter()
            .find(|instance| instance.id == instance_id)
            .ok_or_else(|| {
                format!(
                    "Track '{}' does not have CLAP instance id: {}",
                    self.name, instance_id
                )
            })?;
        Ok(instance.processor.parameter_infos())
    }

    pub fn clap_snapshot_state(
        &self,
        instance_id: usize,
    ) -> Result<crate::clap::ClapPluginState, String> {
        let instance = self
            .clap_plugins
            .iter()
            .find(|instance| instance.id == instance_id)
            .ok_or_else(|| {
                format!(
                    "Track '{}' does not have CLAP instance id: {}",
                    self.name, instance_id
                )
            })?;
        instance.processor.snapshot_state()
    }

    pub fn clap_restore_state(
        &self,
        instance_id: usize,
        state: &crate::clap::ClapPluginState,
    ) -> Result<(), String> {
        let instance = self
            .clap_plugins
            .iter()
            .find(|instance| instance.id == instance_id)
            .ok_or_else(|| {
                format!(
                    "Track '{}' does not have CLAP instance id: {}",
                    self.name, instance_id
                )
            })?;
        instance.processor.restore_state(state)
    }

    pub fn clap_snapshot_all_states(&self) -> Vec<(usize, String, crate::clap::ClapPluginState)> {
        self.clap_plugins
            .iter()
            .filter_map(|instance| {
                instance
                    .processor
                    .snapshot_state()
                    .ok()
                    .map(|state| (instance.id, instance.processor.path().to_string(), state))
            })
            .collect()
    }

    pub fn unload_vst3_plugin_instance(&mut self, instance_id: usize) -> Result<(), String> {
        let Some(index) = self
            .vst3_processors
            .iter()
            .position(|instance| instance.id == instance_id)
        else {
            return Err(format!(
                "Track '{}' does not have VST3 instance id: {}",
                self.name, instance_id
            ));
        };
        let removed = self.vst3_processors.remove(index);
        for port in removed.processor.audio_inputs() {
            Self::disconnect_all(port);
        }
        for port in removed.processor.audio_outputs() {
            Self::disconnect_all(port);
        }
        #[cfg(unix)]
        self.prune_plugin_midi_connections(PluginGraphNode::Vst3PluginInstance(instance_id));
        self.invalidate_audio_route_cache();
        Ok(())
    }

    pub fn loaded_vst3_instances(&self) -> Vec<(usize, String, String)> {
        self.vst3_processors
            .iter()
            .map(|instance| {
                (
                    instance.id,
                    instance.processor.path().to_string(),
                    instance.processor.name().to_string(),
                )
            })
            .collect()
    }

    pub fn vst3_graph_plugins(&self) -> Vec<crate::message::Vst3GraphPlugin> {
        use crate::message::Vst3GraphPlugin;

        self.vst3_processors
            .iter()
            .map(|instance| Vst3GraphPlugin {
                instance_id: instance.id,
                name: instance.processor.name().to_string(),
                path: instance.processor.path().to_string(),
                audio_inputs: instance.processor.audio_inputs().len(),
                audio_outputs: instance.processor.audio_outputs().len(),
                parameters: instance.processor.parameters().to_vec(),
            })
            .collect()
    }

    pub fn vst3_graph_connections(&self) -> Vec<crate::message::Vst3GraphConnection> {
        use crate::kind::Kind;
        use crate::message::{Vst3GraphConnection, Vst3GraphNode};

        let mut connections = Vec::new();

        // Build connections by inspecting AudioIO connections
        // Similar to plugin_graph_connections approach
        for instance in &self.vst3_processors {
            // Check audio input connections
            for (port_idx, input) in instance.processor.audio_inputs().iter().enumerate() {
                let conns = input.connections.lock();
                for conn in conns.iter() {
                    // Try to find source: could be track input, another VST3 output, or LV2 output
                    let from_node = self.find_vst3_audio_source_node(conn.as_ref());
                    if let Some((node, from_port)) = from_node {
                        connections.push(Vst3GraphConnection {
                            from_node: node,
                            from_port,
                            to_node: Vst3GraphNode::PluginInstance(instance.id),
                            to_port: port_idx,
                            kind: Kind::Audio,
                        });
                    }
                }
            }

            // Check audio output connections to track outputs
            for (port_idx, output) in instance.processor.audio_outputs().iter().enumerate() {
                let conns = output.connections.lock();
                for conn in conns.iter() {
                    // Check if connected to track outputs
                    if self.audio.outs.iter().any(|out| Arc::ptr_eq(out, conn)) {
                        let to_port = self
                            .audio
                            .outs
                            .iter()
                            .position(|out| Arc::ptr_eq(out, conn))
                            .unwrap();

                        connections.push(Vst3GraphConnection {
                            from_node: Vst3GraphNode::PluginInstance(instance.id),
                            from_port: port_idx,
                            to_node: Vst3GraphNode::TrackOutput,
                            to_port,
                            kind: Kind::Audio,
                        });
                    }
                }
            }
        }

        connections
    }

    fn find_vst3_audio_source_node(
        &self,
        audio_io: &crate::audio::io::AudioIO,
    ) -> Option<(crate::message::Vst3GraphNode, usize)> {
        use crate::message::Vst3GraphNode;

        // Check if it's a track input
        for (idx, input) in self.audio.ins.iter().enumerate() {
            if Arc::ptr_eq(
                input,
                &Arc::new(unsafe { std::ptr::read(audio_io as *const _) }),
            ) {
                return Some((Vst3GraphNode::TrackInput, idx));
            }
        }

        // Check if it's a VST3 output
        for instance in &self.vst3_processors {
            for (port_idx, output) in instance.processor.audio_outputs().iter().enumerate() {
                if Arc::ptr_eq(
                    output,
                    &Arc::new(unsafe { std::ptr::read(audio_io as *const _) }),
                ) {
                    return Some((Vst3GraphNode::PluginInstance(instance.id), port_idx));
                }
            }
        }

        None
    }

    pub fn set_vst3_parameter(
        &mut self,
        instance_id: usize,
        param_id: u32,
        value: f32,
    ) -> Result<(), String> {
        let instance = self
            .vst3_processors
            .iter_mut()
            .find(|i| i.id == instance_id)
            .ok_or_else(|| format!("VST3 instance {} not found", instance_id))?;

        instance.processor.set_parameter_value(param_id, value)
    }

    pub fn get_vst3_parameters(
        &self,
        instance_id: usize,
    ) -> Result<Vec<crate::vst3::port::ParameterInfo>, String> {
        let instance = self
            .vst3_processors
            .iter()
            .find(|i| i.id == instance_id)
            .ok_or_else(|| format!("VST3 instance {} not found", instance_id))?;

        Ok(instance.processor.parameters().to_vec())
    }

    pub fn vst3_snapshot_state(
        &self,
        instance_id: usize,
    ) -> Result<crate::vst3::state::Vst3PluginState, String> {
        let instance = self
            .vst3_processors
            .iter()
            .find(|i| i.id == instance_id)
            .ok_or_else(|| format!("VST3 instance {} not found", instance_id))?;

        instance.processor.snapshot_state()
    }

    pub fn vst3_restore_state(
        &mut self,
        instance_id: usize,
        state: &crate::vst3::state::Vst3PluginState,
    ) -> Result<(), String> {
        let instance = self
            .vst3_processors
            .iter_mut()
            .find(|i| i.id == instance_id)
            .ok_or_else(|| format!("VST3 instance {} not found", instance_id))?;

        instance.processor.restore_state(state)
    }

    pub fn connect_vst3_audio(
        &mut self,
        from_node: &crate::message::Vst3GraphNode,
        from_port: usize,
        to_node: &crate::message::Vst3GraphNode,
        to_port: usize,
    ) -> Result<(), String> {
        use crate::message::Vst3GraphNode;

        // Get source AudioIO and clone it immediately to avoid borrow issues
        let from_io = match from_node {
            Vst3GraphNode::TrackInput => self
                .audio
                .ins
                .get(from_port)
                .ok_or("Invalid track input port")?
                .clone(),
            Vst3GraphNode::PluginInstance(id) => {
                let instance = self
                    .vst3_processors
                    .iter()
                    .find(|i| i.id == *id)
                    .ok_or("VST3 instance not found")?;
                instance
                    .processor
                    .audio_outputs()
                    .get(from_port)
                    .ok_or("Invalid plugin output port")?
                    .clone()
            }
            Vst3GraphNode::TrackOutput => {
                return Err("Cannot connect from track output".to_string());
            }
        };

        // Get destination AudioIO
        let to_io = match to_node {
            Vst3GraphNode::PluginInstance(id) => {
                let instance = self
                    .vst3_processors
                    .iter()
                    .find(|i| i.id == *id)
                    .ok_or("VST3 instance not found")?;
                instance
                    .processor
                    .audio_inputs()
                    .get(to_port)
                    .ok_or("Invalid plugin input port")?
            }
            Vst3GraphNode::TrackOutput => self
                .audio
                .outs
                .get(to_port)
                .ok_or("Invalid track output port")?,
            Vst3GraphNode::TrackInput => return Err("Cannot connect to track input".to_string()),
        };

        // Add connection
        to_io.connections.lock().push(from_io);
        self.invalidate_audio_route_cache();
        Ok(())
    }

    pub fn disconnect_vst3_audio(
        &mut self,
        from_node: &crate::message::Vst3GraphNode,
        from_port: usize,
        to_node: &crate::message::Vst3GraphNode,
        to_port: usize,
    ) -> Result<(), String> {
        use crate::message::Vst3GraphNode;

        // Get source AudioIO and clone to avoid borrow issues
        let from_io = match from_node {
            Vst3GraphNode::TrackInput => self
                .audio
                .ins
                .get(from_port)
                .ok_or("Invalid track input port")?
                .clone(),
            Vst3GraphNode::PluginInstance(id) => {
                let instance = self
                    .vst3_processors
                    .iter()
                    .find(|i| i.id == *id)
                    .ok_or("VST3 instance not found")?;
                instance
                    .processor
                    .audio_outputs()
                    .get(from_port)
                    .ok_or("Invalid plugin output port")?
                    .clone()
            }
            Vst3GraphNode::TrackOutput => {
                return Err("Cannot disconnect from track output".to_string());
            }
        };

        // Get destination AudioIO
        let to_io = match to_node {
            Vst3GraphNode::PluginInstance(id) => {
                let instance = self
                    .vst3_processors
                    .iter()
                    .find(|i| i.id == *id)
                    .ok_or("VST3 instance not found")?;
                instance
                    .processor
                    .audio_inputs()
                    .get(to_port)
                    .ok_or("Invalid plugin input port")?
            }
            Vst3GraphNode::TrackOutput => self
                .audio
                .outs
                .get(to_port)
                .ok_or("Invalid track output port")?,
            Vst3GraphNode::TrackInput => return Err("Cannot disconnect to track input".to_string()),
        };

        // Remove connection
        to_io
            .connections
            .lock()
            .retain(|conn| !Arc::ptr_eq(conn, &from_io));
        self.invalidate_audio_route_cache();
        Ok(())
    }

    pub fn clear_default_passthrough(&mut self) {
        for (audio_in, audio_out) in self.audio.ins.iter().zip(self.audio.outs.iter()) {
            let _ = AudioIO::disconnect(audio_in, audio_out);
            let _ = AudioIO::disconnect(audio_out, audio_in);
        }
        for (midi_in, midi_out) in self.midi.ins.iter().zip(self.midi.outs.iter()) {
            let _ = midi_out.lock().disconnect(midi_in);
        }
        self.invalidate_audio_route_cache();
        self.invalidate_midi_route_cache();
    }

    fn current_buffer_size(&self) -> usize {
        self.audio
            .ins
            .first()
            .map(|io| io.buffer.lock().len())
            .or_else(|| self.audio.outs.first().map(|io| io.buffer.lock().len()))
            .unwrap_or(self.process_block_size)
    }

    pub fn add_audio_input(&mut self) -> Result<(), String> {
        let buffer_size = self.current_buffer_size();
        if buffer_size == 0 {
            return Err(format!("Track '{}' has no audio buffer size", self.name));
        }
        let _ = self.audio.add_input(buffer_size);
        self.invalidate_audio_route_cache();
        Ok(())
    }

    pub fn add_audio_output(&mut self) -> Result<(), String> {
        let buffer_size = self.current_buffer_size();
        if buffer_size == 0 {
            return Err(format!("Track '{}' has no audio buffer size", self.name));
        }
        let _ = self.audio.add_output(buffer_size);
        self.record_tap_outs.push(vec![0.0; buffer_size]);
        self.output_meter_linear_cache.push(0.0);
        self.meter_peak_hold_linear.push(0.0);
        self.invalidate_audio_route_cache();
        Ok(())
    }

    pub fn remove_audio_input(&mut self) -> Result<(), String> {
        if self.audio.ins.len() <= self.primary_audio_ins() {
            return Err(format!(
                "Track '{}' has no removable return inputs",
                self.name
            ));
        }
        if let Some(input) = self.audio.ins.pop() {
            Self::disconnect_all(&input);
            for output in &self.audio.outs {
                let conns = output.connections.lock();
                conns.retain(|source| !Arc::ptr_eq(source, &input));
            }
            self.invalidate_audio_route_cache();
            Ok(())
        } else {
            Err(format!("Track '{}' input removal failed", self.name))
        }
    }

    pub fn remove_audio_output(
        &mut self,
        hw_outputs: &[Arc<AudioIO>],
        track_inputs: &[Arc<AudioIO>],
    ) -> Result<(), String> {
        if self.audio.outs.len() <= self.primary_audio_outs() {
            return Err(format!(
                "Track '{}' has no removable send outputs",
                self.name
            ));
        }
        let Some(output) = self.audio.outs.pop() else {
            return Err(format!("Track '{}' output removal failed", self.name));
        };
        for target in hw_outputs.iter().chain(track_inputs.iter()) {
            let _ = AudioIO::disconnect(&output, target);
        }
        self.record_tap_outs.truncate(self.audio.outs.len());
        self.output_meter_linear_cache
            .truncate(self.audio.outs.len());
        self.meter_peak_hold_linear.truncate(self.audio.outs.len());
        self.invalidate_audio_route_cache();
        Ok(())
    }

    #[cfg(unix)]
    pub fn plugin_graph_connections(&self) -> Vec<PluginGraphConnection> {
        let mut source_ports: Vec<(PluginGraphNode, usize, Arc<AudioIO>)> = self
            .audio
            .ins
            .iter()
            .enumerate()
            .map(|(idx, io)| (PluginGraphNode::TrackInput, idx, io.clone()))
            .collect();
        #[cfg(all(unix, not(target_os = "macos")))]
        for instance in &self.lv2_processors {
            source_ports.extend(instance.processor.audio_outputs().iter().enumerate().map(
                |(idx, io)| {
                    (
                        PluginGraphNode::Lv2PluginInstance(instance.id),
                        idx,
                        io.clone(),
                    )
                },
            ));
        }
        for instance in &self.vst3_processors {
            source_ports.extend(instance.processor.audio_outputs().iter().enumerate().map(
                |(idx, io)| {
                    (
                        PluginGraphNode::Vst3PluginInstance(instance.id),
                        idx,
                        io.clone(),
                    )
                },
            ));
        }
        for instance in &self.clap_plugins {
            source_ports.extend(instance.processor.audio_outputs().iter().enumerate().map(
                |(idx, io)| {
                    (
                        PluginGraphNode::ClapPluginInstance(instance.id),
                        idx,
                        io.clone(),
                    )
                },
            ));
        }

        let mut connections = vec![];
        for (to_port, to_io) in self.audio.outs.iter().enumerate() {
            for conn in to_io.connections.lock().iter() {
                if let Some((from_node, from_port, _)) = source_ports
                    .iter()
                    .find(|(_, _, source_io)| Arc::ptr_eq(source_io, conn))
                {
                    connections.push(PluginGraphConnection {
                        from_node: from_node.clone(),
                        from_port: *from_port,
                        to_node: PluginGraphNode::TrackOutput,
                        to_port,
                        kind: Kind::Audio,
                    });
                }
            }
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        for instance in &self.lv2_processors {
            for (to_port, to_io) in instance.processor.audio_inputs().iter().enumerate() {
                for conn in to_io.connections.lock().iter() {
                    if let Some((from_node, from_port, _)) = source_ports
                        .iter()
                        .find(|(_, _, source_io)| Arc::ptr_eq(source_io, conn))
                    {
                        connections.push(PluginGraphConnection {
                            from_node: from_node.clone(),
                            from_port: *from_port,
                            to_node: PluginGraphNode::Lv2PluginInstance(instance.id),
                            to_port,
                            kind: Kind::Audio,
                        });
                    }
                }
            }
        }
        for instance in &self.vst3_processors {
            for (to_port, to_io) in instance.processor.audio_inputs().iter().enumerate() {
                for conn in to_io.connections.lock().iter() {
                    if let Some((from_node, from_port, _)) = source_ports
                        .iter()
                        .find(|(_, _, source_io)| Arc::ptr_eq(source_io, conn))
                    {
                        connections.push(PluginGraphConnection {
                            from_node: from_node.clone(),
                            from_port: *from_port,
                            to_node: PluginGraphNode::Vst3PluginInstance(instance.id),
                            to_port,
                            kind: Kind::Audio,
                        });
                    }
                }
            }
        }
        for instance in &self.clap_plugins {
            for (to_port, to_io) in instance.processor.audio_inputs().iter().enumerate() {
                for conn in to_io.connections.lock().iter() {
                    if let Some((from_node, from_port, _)) = source_ports
                        .iter()
                        .find(|(_, _, source_io)| Arc::ptr_eq(source_io, conn))
                    {
                        connections.push(PluginGraphConnection {
                            from_node: from_node.clone(),
                            from_port: *from_port,
                            to_node: PluginGraphNode::ClapPluginInstance(instance.id),
                            to_port,
                            kind: Kind::Audio,
                        });
                    }
                }
            }
        }
        for (to_port, to_io) in self.midi.outs.iter().enumerate() {
            for conn in to_io.lock().connections.iter() {
                if let Some((from_port, _)) = self
                    .midi
                    .ins
                    .iter()
                    .enumerate()
                    .find(|(_, in_io)| Arc::ptr_eq(in_io, conn))
                {
                    connections.push(PluginGraphConnection {
                        from_node: PluginGraphNode::TrackInput,
                        from_port,
                        to_node: PluginGraphNode::TrackOutput,
                        to_port,
                        kind: Kind::MIDI,
                    });
                }
            }
        }
        connections.extend(self.plugin_midi_connections.iter().cloned());
        connections
    }

    #[cfg(unix)]
    pub fn connect_plugin_audio(
        &mut self,
        from_node: PluginGraphNode,
        from_port: usize,
        to_node: PluginGraphNode,
        to_port: usize,
    ) -> Result<(), String> {
        let source = self.plugin_source_io(&from_node, from_port)?;
        let target = self.plugin_target_io(&to_node, to_port)?;
        if routing::would_create_cycle(&from_node, &to_node, |node| {
            self.plugin_connected_neighbors(Kind::Audio, node)
        }) {
            return Err("Circular routing is not allowed!".to_string());
        }
        if matches!(from_node, PluginGraphNode::TrackInput) {
            Self::connect_directed_audio(&source, &target);
        } else {
            AudioIO::connect(&source, &target);
        }
        self.invalidate_audio_route_cache();
        Ok(())
    }

    #[cfg(unix)]
    pub fn disconnect_plugin_audio(
        &mut self,
        from_node: PluginGraphNode,
        from_port: usize,
        to_node: PluginGraphNode,
        to_port: usize,
    ) -> Result<(), String> {
        let source = self.plugin_source_io(&from_node, from_port)?;
        let target = self.plugin_target_io(&to_node, to_port)?;
        AudioIO::disconnect(&source, &target)?;
        self.invalidate_audio_route_cache();
        Ok(())
    }

    #[cfg(unix)]
    pub fn connect_plugin_midi(
        &mut self,
        from_node: PluginGraphNode,
        from_port: usize,
        to_node: PluginGraphNode,
        to_port: usize,
    ) -> Result<(), String> {
        self.validate_plugin_midi_source(&from_node, from_port)?;
        self.validate_plugin_midi_target(&to_node, to_port)?;
        if from_node == to_node && from_port == to_port {
            return Err("Cannot connect a MIDI port to itself".to_string());
        }
        if matches!(from_node, PluginGraphNode::TrackInput)
            && matches!(to_node, PluginGraphNode::TrackOutput)
        {
            let Some(midi_in) = self.midi.ins.get(from_port).cloned() else {
                return Err(format!("Track MIDI input port {from_port} not found"));
            };
            let Some(midi_out) = self.midi.outs.get(to_port).cloned() else {
                return Err(format!("Track MIDI output port {to_port} not found"));
            };
            let out = midi_out.lock();
            let exists = out
                .connections
                .iter()
                .any(|conn| Arc::ptr_eq(conn, &midi_in));
            if !exists {
                out.connect(midi_in);
            }
            self.invalidate_midi_route_cache();
            return Ok(());
        }
        if routing::would_create_cycle(&from_node, &to_node, |node| {
            self.plugin_connected_neighbors(Kind::MIDI, node)
        }) {
            return Err("Circular routing is not allowed!".to_string());
        }
        let new_conn = PluginGraphConnection {
            from_node,
            from_port,
            to_node,
            to_port,
            kind: Kind::MIDI,
        };
        if self.plugin_midi_connections.iter().any(|c| c == &new_conn) {
            return Ok(());
        }
        self.plugin_midi_connections.push(new_conn);
        Ok(())
    }

    #[cfg(unix)]
    pub fn disconnect_plugin_midi(
        &mut self,
        from_node: PluginGraphNode,
        from_port: usize,
        to_node: PluginGraphNode,
        to_port: usize,
    ) -> Result<(), String> {
        if matches!(from_node, PluginGraphNode::TrackInput)
            && matches!(to_node, PluginGraphNode::TrackOutput)
        {
            let Some(midi_in) = self.midi.ins.get(from_port).cloned() else {
                return Err(format!("Track MIDI input port {from_port} not found"));
            };
            let Some(midi_out) = self.midi.outs.get(to_port).cloned() else {
                return Err(format!("Track MIDI output port {to_port} not found"));
            };
            midi_out.lock().disconnect(&midi_in)?;
            self.invalidate_midi_route_cache();
            return Ok(());
        }
        let before = self.plugin_midi_connections.len();
        self.plugin_midi_connections.retain(|c| {
            !(c.kind == Kind::MIDI
                && c.from_node == from_node
                && c.from_port == from_port
                && c.to_node == to_node
                && c.to_port == to_port)
        });
        if self.plugin_midi_connections.len() == before {
            Err("MIDI plugin graph connection not found".to_string())
        } else {
            Ok(())
        }
    }

    fn with_default_passthrough(mut self) -> Self {
        self.ensure_default_audio_passthrough();
        self.ensure_default_midi_passthrough();
        self
    }

    pub(crate) fn ensure_default_audio_passthrough(&mut self) {
        if self.audio.ins.is_empty() {
            self.invalidate_audio_route_cache();
            return;
        }

        for audio_in in &self.audio.ins {
            audio_in
                .connections
                .lock()
                .retain(|conn| !self.audio.outs.iter().any(|out| Arc::ptr_eq(out, conn)));
        }

        for (out_idx, audio_out) in self.audio.outs.iter().enumerate() {
            let source_idx = out_idx.min(self.audio.ins.len().saturating_sub(1));
            let audio_in = &self.audio.ins[source_idx];
            let conns = audio_out.connections.lock();
            conns.retain(|conn| !self.audio.ins.iter().any(|input| Arc::ptr_eq(input, conn)));
            if !conns.iter().any(|conn| Arc::ptr_eq(conn, audio_in)) {
                conns.push(audio_in.clone());
            }
        }
        self.invalidate_audio_route_cache();
    }

    pub(crate) fn ensure_default_midi_passthrough(&mut self) {
        for (midi_in, midi_out) in self.midi.ins.iter().zip(self.midi.outs.iter()) {
            let out = midi_out.lock();
            let exists = out
                .connections
                .iter()
                .any(|conn| Arc::ptr_eq(conn, midi_in));
            if !exists {
                out.connect(midi_in.clone());
            }
        }
        self.invalidate_midi_route_cache();
    }

    fn internal_audio_sources(&self) -> Vec<Arc<AudioIO>> {
        let mut sources = self.audio.ins.clone();
        if let Some(src) = &self.metronome_source {
            sources.push(src.clone());
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        for instance in &self.lv2_processors {
            sources.extend(instance.processor.audio_outputs().iter().cloned());
        }
        for instance in &self.vst3_processors {
            sources.extend(instance.processor.audio_outputs().iter().cloned());
        }
        for instance in &self.clap_plugins {
            sources.extend(instance.processor.audio_outputs().iter().cloned());
        }
        sources
    }

    fn is_track_input_source(&self, source: &Arc<AudioIO>) -> bool {
        self.audio
            .ins
            .iter()
            .any(|input| Arc::ptr_eq(input, source))
    }

    fn disconnect_all(port: &Arc<AudioIO>) {
        let connections = port.connections.lock().clone();
        for other in connections {
            let _ = AudioIO::disconnect(&other, port);
        }
    }

    #[cfg(not(all(unix, not(target_os = "macos"))))]
    fn lv2_unsupported_error(instance_id: usize) -> String {
        format!("LV2 instance {instance_id} is not supported on this platform")
    }

    #[cfg(unix)]
    fn lv2_audio_output_io(
        &self,
        instance_id: usize,
        _port: usize,
    ) -> Result<Arc<AudioIO>, String> {
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            self.lv2_processors
                .iter()
                .find(|instance| instance.id == instance_id)
                .and_then(|instance| instance.processor.audio_outputs().get(_port).cloned())
                .ok_or_else(|| format!("Plugin instance {instance_id} output port {_port} missing"))
        }
        #[cfg(not(all(unix, not(target_os = "macos"))))]
        {
            Err(Self::lv2_unsupported_error(instance_id))
        }
    }

    #[cfg(unix)]
    fn lv2_audio_input_io(&self, instance_id: usize, _port: usize) -> Result<Arc<AudioIO>, String> {
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            self.lv2_processors
                .iter()
                .find(|instance| instance.id == instance_id)
                .and_then(|instance| instance.processor.audio_inputs().get(_port).cloned())
                .ok_or_else(|| format!("Plugin instance {instance_id} input port {_port} missing"))
        }
        #[cfg(not(all(unix, not(target_os = "macos"))))]
        {
            Err(Self::lv2_unsupported_error(instance_id))
        }
    }

    #[cfg(unix)]
    fn lv2_validate_midi_output(&self, instance_id: usize, _port: usize) -> Result<(), String> {
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            self.lv2_processors
                .iter()
                .find(|instance| instance.id == instance_id)
                .and_then(|instance| (_port < instance.processor.midi_output_count()).then_some(()))
                .ok_or_else(|| {
                    format!("Plugin instance {instance_id} MIDI output port {_port} missing")
                })
        }
        #[cfg(not(all(unix, not(target_os = "macos"))))]
        {
            Err(Self::lv2_unsupported_error(instance_id))
        }
    }

    #[cfg(unix)]
    fn lv2_validate_midi_input(&self, instance_id: usize, _port: usize) -> Result<(), String> {
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            self.lv2_processors
                .iter()
                .find(|instance| instance.id == instance_id)
                .and_then(|instance| (_port < instance.processor.midi_input_count()).then_some(()))
                .ok_or_else(|| {
                    format!("Plugin instance {instance_id} MIDI input port {_port} missing")
                })
        }
        #[cfg(not(all(unix, not(target_os = "macos"))))]
        {
            Err(Self::lv2_unsupported_error(instance_id))
        }
    }

    #[cfg(unix)]
    fn vst3_audio_output_io(
        &self,
        instance_id: usize,
        port: usize,
    ) -> Result<Arc<AudioIO>, String> {
        self.vst3_processors
            .iter()
            .find(|instance| instance.id == instance_id)
            .and_then(|instance| instance.processor.audio_outputs().get(port).cloned())
            .ok_or_else(|| format!("VST3 instance {instance_id} output port {port} missing"))
    }

    #[cfg(unix)]
    fn vst3_audio_input_io(&self, instance_id: usize, port: usize) -> Result<Arc<AudioIO>, String> {
        self.vst3_processors
            .iter()
            .find(|instance| instance.id == instance_id)
            .and_then(|instance| instance.processor.audio_inputs().get(port).cloned())
            .ok_or_else(|| format!("VST3 instance {instance_id} input port {port} missing"))
    }

    #[cfg(unix)]
    fn clap_audio_output_io(
        &self,
        instance_id: usize,
        port: usize,
    ) -> Result<Arc<AudioIO>, String> {
        self.clap_plugins
            .iter()
            .find(|instance| instance.id == instance_id)
            .and_then(|instance| instance.processor.audio_outputs().get(port).cloned())
            .ok_or_else(|| format!("CLAP instance {instance_id} output port {port} missing"))
    }

    #[cfg(unix)]
    fn clap_audio_input_io(&self, instance_id: usize, port: usize) -> Result<Arc<AudioIO>, String> {
        self.clap_plugins
            .iter()
            .find(|instance| instance.id == instance_id)
            .and_then(|instance| instance.processor.audio_inputs().get(port).cloned())
            .ok_or_else(|| format!("CLAP instance {instance_id} input port {port} missing"))
    }

    #[cfg(unix)]
    fn vst3_validate_midi_output(&self, instance_id: usize, port: usize) -> Result<(), String> {
        self.vst3_processors
            .iter()
            .find(|instance| instance.id == instance_id)
            .and_then(|instance| (port < instance.processor.midi_output_count()).then_some(()))
            .ok_or_else(|| format!("VST3 instance {instance_id} MIDI output port {port} missing"))
    }

    #[cfg(unix)]
    fn clap_validate_midi_output(&self, instance_id: usize, port: usize) -> Result<(), String> {
        self.clap_plugins
            .iter()
            .find(|instance| instance.id == instance_id)
            .and_then(|instance| (port < instance.processor.midi_output_count()).then_some(()))
            .ok_or_else(|| format!("CLAP instance {instance_id} MIDI output port {port} missing"))
    }

    #[cfg(unix)]
    fn vst3_validate_midi_input(&self, instance_id: usize, port: usize) -> Result<(), String> {
        self.vst3_processors
            .iter()
            .find(|instance| instance.id == instance_id)
            .and_then(|instance| (port < instance.processor.midi_input_count()).then_some(()))
            .ok_or_else(|| format!("VST3 instance {instance_id} MIDI input port {port} missing"))
    }

    #[cfg(unix)]
    fn clap_validate_midi_input(&self, instance_id: usize, port: usize) -> Result<(), String> {
        self.clap_plugins
            .iter()
            .find(|instance| instance.id == instance_id)
            .and_then(|instance| (port < instance.processor.midi_input_count()).then_some(()))
            .ok_or_else(|| format!("CLAP instance {instance_id} MIDI input port {port} missing"))
    }

    #[cfg(unix)]
    fn plugin_source_io(
        &self,
        node: &PluginGraphNode,
        port: usize,
    ) -> Result<Arc<AudioIO>, String> {
        match node {
            PluginGraphNode::TrackInput => self
                .audio
                .ins
                .get(port)
                .cloned()
                .ok_or_else(|| format!("Track input port {port} not found")),
            PluginGraphNode::TrackOutput => Err("Track output node cannot be source".to_string()),
            PluginGraphNode::Lv2PluginInstance(instance_id) => {
                self.lv2_audio_output_io(*instance_id, port)
            }
            PluginGraphNode::Vst3PluginInstance(instance_id) => {
                self.vst3_audio_output_io(*instance_id, port)
            }
            PluginGraphNode::ClapPluginInstance(instance_id) => {
                self.clap_audio_output_io(*instance_id, port)
            }
        }
    }

    #[cfg(unix)]
    fn plugin_target_io(
        &self,
        node: &PluginGraphNode,
        port: usize,
    ) -> Result<Arc<AudioIO>, String> {
        match node {
            PluginGraphNode::TrackInput => Err("Track input node cannot be target".to_string()),
            PluginGraphNode::TrackOutput => self
                .audio
                .outs
                .get(port)
                .cloned()
                .ok_or_else(|| format!("Track output port {port} not found")),
            PluginGraphNode::Lv2PluginInstance(instance_id) => {
                self.lv2_audio_input_io(*instance_id, port)
            }
            PluginGraphNode::Vst3PluginInstance(instance_id) => {
                self.vst3_audio_input_io(*instance_id, port)
            }
            PluginGraphNode::ClapPluginInstance(instance_id) => {
                self.clap_audio_input_io(*instance_id, port)
            }
        }
    }

    #[cfg(unix)]
    fn validate_plugin_midi_source(
        &self,
        node: &PluginGraphNode,
        port: usize,
    ) -> Result<(), String> {
        match node {
            PluginGraphNode::TrackInput => self
                .midi
                .ins
                .get(port)
                .map(|_| ())
                .ok_or_else(|| format!("Track MIDI input port {port} not found")),
            PluginGraphNode::TrackOutput => {
                Err("Track output node cannot be MIDI source".to_string())
            }
            PluginGraphNode::Lv2PluginInstance(instance_id) => {
                self.lv2_validate_midi_output(*instance_id, port)
            }
            PluginGraphNode::Vst3PluginInstance(instance_id) => {
                self.vst3_validate_midi_output(*instance_id, port)
            }
            PluginGraphNode::ClapPluginInstance(instance_id) => {
                self.clap_validate_midi_output(*instance_id, port)
            }
        }
    }

    #[cfg(unix)]
    fn validate_plugin_midi_target(
        &self,
        node: &PluginGraphNode,
        port: usize,
    ) -> Result<(), String> {
        match node {
            PluginGraphNode::TrackInput => {
                Err("Track input node cannot be MIDI target".to_string())
            }
            PluginGraphNode::TrackOutput => self
                .midi
                .outs
                .get(port)
                .map(|_| ())
                .ok_or_else(|| format!("Track MIDI output port {port} not found")),
            PluginGraphNode::Lv2PluginInstance(instance_id) => {
                self.lv2_validate_midi_input(*instance_id, port)
            }
            PluginGraphNode::Vst3PluginInstance(instance_id) => {
                self.vst3_validate_midi_input(*instance_id, port)
            }
            PluginGraphNode::ClapPluginInstance(instance_id) => {
                self.clap_validate_midi_input(*instance_id, port)
            }
        }
    }

    #[cfg(unix)]
    fn plugin_connected_neighbors(
        &self,
        kind: Kind,
        current_node: &PluginGraphNode,
    ) -> Vec<PluginGraphNode> {
        let mut nodes = HashSet::new();
        for conn in self.plugin_graph_connections() {
            if conn.kind == kind && &conn.from_node == current_node {
                nodes.insert(conn.to_node);
            }
        }
        nodes.into_iter().collect()
    }

    pub fn push_hw_midi_events(&mut self, events: &[MidiEvent]) {
        let Some(input) = self.midi.ins.first() else {
            return;
        };
        if events.is_empty() {
            return;
        }
        input.lock().buffer.extend_from_slice(events);
    }

    pub fn push_hw_midi_events_to_port(&mut self, port: usize, events: &[MidiEvent]) {
        let Some(input) = self.midi.ins.get(port) else {
            return;
        };
        if events.is_empty() {
            return;
        }
        input.lock().buffer.extend_from_slice(events);
    }

    fn collect_track_input_midi_events(&mut self) -> Vec<Vec<MidiEvent>> {
        let mut events: Vec<Vec<MidiEvent>> = Vec::with_capacity(self.midi.ins.len());
        self.record_tap_midi_in.clear();
        let should_filter = self.input_monitor || self.record_tap_enabled;
        for (lane, input) in self.midi.ins.iter().enumerate() {
            let input_lock = input.lock();
            let mut port_events = std::mem::take(&mut input_lock.buffer);
            if should_filter && let Some(Some(channel)) = self.midi_lane_channels.get(lane) {
                port_events.retain(|event| Self::event_matches_midi_channel(event, *channel));
            }
            self.record_tap_midi_in.extend(port_events.iter().cloned());
            events.push(port_events);
        }
        self.record_tap_midi_in.sort_by_key(|e| e.frame);
        events
    }

    fn event_matches_midi_channel(event: &MidiEvent, channel: u8) -> bool {
        let Some(status) = event.data.first().copied() else {
            return true;
        };
        if !(0x80..=0xEF).contains(&status) {
            return true;
        }
        (status & 0x0F) == channel.min(15)
    }

    fn route_track_inputs_to_track_outputs(&mut self, input_events: &[Vec<MidiEvent>]) {
        for out in &self.midi.outs {
            out.lock().buffer.clear();
        }
        if !self.output_enabled {
            return;
        }
        for (input_idx, events) in input_events.iter().enumerate() {
            if events.is_empty() {
                continue;
            }
            let Some(out_indices) = self.midi_input_to_out_routes_cache.get(input_idx) else {
                continue;
            };
            for out_idx in out_indices {
                if let Some(out) = self.midi.outs.get(*out_idx) {
                    out.lock().buffer.extend_from_slice(events);
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    fn route_plugin_midi_to_track_outputs(&self, plugin_events: &[MidiEvent]) {
        if !self.output_enabled || plugin_events.is_empty() {
            return;
        }
        for out in &self.midi.outs {
            out.lock().buffer.extend_from_slice(plugin_events);
        }
    }

    #[cfg(target_os = "macos")]
    fn route_clap_midi_to_track_outputs(&self, plugin_events: &[ClapMidiOutputEvent]) {
        if !self.output_enabled || plugin_events.is_empty() {
            return;
        }
        for event in plugin_events {
            let port = event.port.min(self.midi.outs.len().saturating_sub(1));
            let Some(out) = self.midi.outs.get(port) else {
                continue;
            };
            out.lock().buffer.push(event.event.clone());
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn plugin_midi_ready(
        &self,
        node: &PluginGraphNode,
        processed: &HashSet<PluginGraphNode>,
    ) -> bool {
        self.plugin_midi_connections
            .iter()
            .filter(|conn| {
                conn.kind == Kind::MIDI
                    && &conn.to_node == node
                    && matches!(
                        conn.from_node,
                        PluginGraphNode::Lv2PluginInstance(_)
                            | PluginGraphNode::Vst3PluginInstance(_)
                            | PluginGraphNode::ClapPluginInstance(_)
                    )
            })
            .all(|conn| processed.contains(&conn.from_node))
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn plugin_midi_input_events(
        &self,
        node: &PluginGraphNode,
        midi_inputs: usize,
        track_input_events: &[Vec<MidiEvent>],
        node_events: &HashMap<(PluginGraphNode, usize), Vec<MidiEvent>>,
    ) -> Vec<Vec<MidiEvent>> {
        let mut per_port = vec![Vec::new(); midi_inputs];
        for conn in self.plugin_midi_connections.iter().filter(|conn| {
            conn.kind == Kind::MIDI && &conn.to_node == node && conn.to_port < midi_inputs
        }) {
            let events_opt = if conn.from_node == PluginGraphNode::TrackInput {
                track_input_events.get(conn.from_port)
            } else {
                node_events.get(&(conn.from_node.clone(), conn.from_port))
            };
            if let Some(events) = events_opt {
                per_port[conn.to_port].extend_from_slice(events);
            }
        }
        per_port
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn route_plugin_midi_to_track_outputs_graph(
        &self,
        track_input_events: &[Vec<MidiEvent>],
        node_events: &HashMap<(PluginGraphNode, usize), Vec<MidiEvent>>,
    ) {
        if !self.output_enabled {
            return;
        }
        for conn in self
            .plugin_midi_connections
            .iter()
            .filter(|conn| conn.kind == Kind::MIDI && conn.to_node == PluginGraphNode::TrackOutput)
        {
            let Some(out) = self.midi.outs.get(conn.to_port) else {
                continue;
            };
            let events_opt = if conn.from_node == PluginGraphNode::TrackInput {
                track_input_events.get(conn.from_port)
            } else {
                node_events.get(&(conn.from_node.clone(), conn.from_port))
            };
            if let Some(events) = events_opt {
                out.lock().buffer.extend_from_slice(events);
            }
        }
    }

    fn dispatch_track_output_midi_to_connected_inputs(&mut self) {
        for (out_idx, out) in self.midi.outs.iter().enumerate() {
            let events = {
                let out_lock = out.lock();
                std::mem::take(&mut out_lock.buffer)
            };
            if events.is_empty() {
                continue;
            }
            let Some(targets) = self.midi_out_external_targets_cache.get(out_idx) else {
                continue;
            };
            for target in targets.iter() {
                target.lock().buffer.extend_from_slice(&events);
            }
        }
    }

    fn clear_local_midi_inputs(&self) {
        for input in &self.midi.ins {
            input.lock().buffer.clear();
        }
    }

    fn collect_hw_midi_output_events(&mut self) {
        self.pending_hw_midi_out_events.clear();
        for (port, out) in self.midi.outs.iter().enumerate() {
            self.pending_hw_midi_out_events.extend(
                out.lock()
                    .buffer
                    .iter()
                    .cloned()
                    .map(|event| HwMidiOutEvent { port, event }),
            );
        }
    }

    pub fn take_hw_midi_out_events(&mut self) -> Vec<HwMidiOutEvent> {
        std::mem::take(&mut self.pending_hw_midi_out_events)
    }
}

#[cfg(test)]
mod tests {
    use super::{AudioClipBuffer, HwMidiOutEvent, Track};
    use crate::audio::clip::AudioClip;
    use crate::audio::io::AudioIO;
    #[cfg(unix)]
    use crate::{kind::Kind, message::PluginGraphNode};
    use std::sync::Arc;

    #[test]
    fn default_audio_passthrough_uses_minimum_port_count() {
        let track = Track::new("t".to_string(), 1, 2, 0, 0, 64, 48_000.0);

        assert_eq!(track.audio.ins.len(), 1);
        assert_eq!(track.audio.outs.len(), 2);
        assert!(
            track.audio.outs[0]
                .connections
                .lock()
                .iter()
                .any(|conn| Arc::ptr_eq(conn, &track.audio.ins[0]))
        );
        assert!(
            track.audio.outs[1]
                .connections
                .lock()
                .iter()
                .any(|conn| Arc::ptr_eq(conn, &track.audio.ins[0]))
        );
    }

    #[test]
    fn default_midi_passthrough_uses_minimum_port_count() {
        let track = Track::new("t".to_string(), 0, 0, 1, 2, 64, 48_000.0);

        assert_eq!(track.midi.ins.len(), 1);
        assert_eq!(track.midi.outs.len(), 2);
        assert!(
            track.midi.outs[0]
                .lock()
                .connections
                .iter()
                .any(|conn| Arc::ptr_eq(conn, &track.midi.ins[0]))
        );
        assert!(
            track.midi.outs[1]
                .lock()
                .connections
                .iter()
                .all(|conn| !Arc::ptr_eq(conn, &track.midi.ins[0]))
        );
    }

    #[test]
    #[cfg(unix)]
    fn plugin_graph_includes_default_track_midi_passthrough() {
        let track = Track::new("t".to_string(), 0, 0, 1, 2, 64, 48_000.0);
        let connections = track.plugin_graph_connections();

        assert!(connections.iter().any(|c| {
            c.kind == Kind::MIDI
                && c.from_node == PluginGraphNode::TrackInput
                && c.from_port == 0
                && c.to_node == PluginGraphNode::TrackOutput
                && c.to_port == 0
        }));
        assert!(connections.iter().all(|c| {
            !(c.kind == Kind::MIDI
                && c.from_node == PluginGraphNode::TrackInput
                && c.from_port == 0
                && c.to_node == PluginGraphNode::TrackOutput
                && c.to_port == 1)
        }));
    }

    #[test]
    fn track_input_passthrough_respects_input_monitor() {
        let mut track = Track::new("t".to_string(), 1, 1, 0, 0, 8, 48_000.0);
        let source = Arc::new(AudioIO::new(8));
        source.buffer.lock()[0] = 0.5;
        source.buffer.lock()[1] = -0.25;
        AudioIO::connect(&source, &track.audio.ins[0]);

        track.input_monitor = false;
        track.process();
        let out = track.audio.outs[0].buffer.lock().to_vec();
        assert_eq!(out[0], 0.0);
        assert_eq!(out[1], 0.0);

        track.input_monitor = true;
        track.process();
        let out = track.audio.outs[0].buffer.lock().to_vec();
        assert_eq!(out[0], 0.5);
        assert_eq!(out[1], -0.25);
    }

    #[test]
    fn clip_playback_audible_with_input_monitor_off() {
        let mut track = Track::new("t".to_string(), 1, 1, 0, 0, 8, 48_000.0);
        track.input_monitor = false;
        track.disk_monitor = true;
        let mut clip = AudioClip::new("clip".to_string(), 0, 4);
        clip.fade_enabled = false;
        track.audio.clips.push(clip);
        track.audio_clip_cache.insert(
            "clip".to_string(),
            Arc::new(AudioClipBuffer {
                channels: 1,
                samples: vec![0.8, 0.0, 0.0, 0.0],
            }),
        );

        track.process();
        let out = track.audio.outs[0].buffer.lock().to_vec();
        assert_eq!(out[0], 0.8);
    }

    #[test]
    fn clip_playback_respects_clip_playback_enabled_flag() {
        let mut track = Track::new("t".to_string(), 1, 1, 0, 0, 8, 48_000.0);
        track.input_monitor = false;
        track.disk_monitor = true;
        track.clip_playback_enabled = false;
        let mut clip = AudioClip::new("clip".to_string(), 0, 4);
        clip.fade_enabled = false;
        track.audio.clips.push(clip);
        track.audio_clip_cache.insert(
            "clip".to_string(),
            Arc::new(AudioClipBuffer {
                channels: 1,
                samples: vec![0.8, 0.0, 0.0, 0.0],
            }),
        );

        track.process();
        let out = track.audio.outs[0].buffer.lock().to_vec();
        assert_eq!(out[0], 0.0);

        track.clip_playback_enabled = true;
        track.process();
        let out = track.audio.outs[0].buffer.lock().to_vec();
        assert_eq!(out[0], 0.8);
    }

    #[test]
    #[cfg(unix)]
    fn disconnecting_one_stereo_internal_channel_mutes_only_that_channel() {
        let mut track = Track::new("t".to_string(), 2, 2, 0, 0, 8, 48_000.0);
        let left = Arc::new(AudioIO::new(8));
        let right = Arc::new(AudioIO::new(8));
        left.buffer.lock()[0] = 0.25;
        right.buffer.lock()[0] = 0.75;
        AudioIO::connect(&left, &track.audio.ins[0]);
        AudioIO::connect(&right, &track.audio.ins[1]);
        track.input_monitor = true;
        track.disk_monitor = false;

        track.process();
        let out_l = track.audio.outs[0].buffer.lock().to_vec();
        let out_r = track.audio.outs[1].buffer.lock().to_vec();
        assert_eq!(out_l[0], 0.25);
        assert_eq!(out_r[0], 0.75);

        track
            .disconnect_plugin_audio(
                PluginGraphNode::TrackInput,
                1,
                PluginGraphNode::TrackOutput,
                1,
            )
            .unwrap();
        track.process();
        let out_l = track.audio.outs[0].buffer.lock().to_vec();
        let out_r = track.audio.outs[1].buffer.lock().to_vec();
        assert_eq!(out_l[0], 0.25);
        assert_eq!(out_r[0], 0.0);
    }

    #[test]
    fn transport_timing_and_loop_config_clamp_invalid_values() {
        let mut track = Track::new("t".to_string(), 1, 1, 0, 0, 8, 48_000.0);

        track.set_transport_timing(0.0, 0, 0);
        assert_eq!(track.tempo_bpm, 1.0);
        assert_eq!(track.tsig_num, 1);
        assert_eq!(track.tsig_denom, 1);

        track.set_loop_config(true, Some((128, 256)));
        assert!(track.loop_enabled);
        assert_eq!(track.loop_range_samples, Some((128, 256)));
    }

    #[test]
    fn cycle_segments_wrap_across_loop_boundary() {
        let mut track = Track::new("t".to_string(), 1, 1, 0, 0, 8, 48_000.0);
        track.transport_sample = 14;
        track.loop_enabled = true;
        track.loop_range_samples = Some((10, 16));

        let segments = track.cycle_segments(6);
        assert_eq!(segments, vec![(14, 16, 0), (10, 14, 2)]);
    }

    #[test]
    fn warped_source_sample_interpolates_between_markers() {
        use crate::message::AudioWarpMarker;

        let markers = vec![
            AudioWarpMarker {
                timeline_sample: 25,
                source_sample: 10,
            },
            AudioWarpMarker {
                timeline_sample: 75,
                source_sample: 90,
            },
        ];

        assert_eq!(Track::warped_source_sample_for_output(100, 0, &markers), 0);
        assert_eq!(
            Track::warped_source_sample_for_output(100, 25, &markers),
            10
        );
        assert_eq!(
            Track::warped_source_sample_for_output(100, 50, &markers),
            50
        );
        assert_eq!(
            Track::warped_source_sample_for_output(100, 75, &markers),
            90
        );
        assert_eq!(
            Track::warped_source_sample_for_output(100, 100, &markers),
            100
        );
    }

    #[test]
    fn offline_bounce_restores_transport_and_monitor_state() {
        let mut track = Track::new("t".to_string(), 1, 1, 0, 0, 8, 48_000.0);
        track.transport_sample = 123;
        track.disk_monitor = false;
        track.input_monitor = true;
        track.clip_playback_enabled = false;
        track.output_enabled = false;
        track.loop_enabled = true;
        track.loop_range_samples = Some((32, 64));
        track.armed = true;
        track.pending_hw_midi_out_events.push(HwMidiOutEvent {
            port: 0,
            event: crate::midi::io::MidiEvent::new(0, vec![0x90, 60, 100]),
        });

        let (channels, rendered) = track.offline_bounce_interleaved(0, 4);
        assert_eq!(channels, 1);
        assert_eq!(rendered.len(), 4);

        assert_eq!(track.transport_sample, 123);
        assert!(!track.disk_monitor);
        assert!(track.input_monitor);
        assert!(!track.clip_playback_enabled);
        assert!(!track.output_enabled);
        assert!(track.loop_enabled);
        assert_eq!(track.loop_range_samples, Some((32, 64)));
        assert!(track.armed);
        assert_eq!(track.pending_hw_midi_out_events.len(), 1);
    }

    #[test]
    fn midi_only_track_clip_playback_generates_hw_midi_events() {
        let mut track = Track::new("t".to_string(), 0, 0, 1, 1, 8, 48_000.0);
        track.disk_monitor = true;
        track.clip_playback_enabled = true;
        track.midi.clips.push(crate::midi::clip::MIDIClip::new(
            "clip.mid".to_string(),
            0,
            8,
        ));
        track.midi_clip_cache.insert(
            "clip.mid".to_string(),
            Arc::new(vec![(0, vec![0x90, 60, 100])]),
        );

        track.process();

        assert_eq!(track.pending_hw_midi_out_events.len(), 1);
        assert_eq!(track.pending_hw_midi_out_events[0].port, 0);
        assert_eq!(
            track.pending_hw_midi_out_events[0].event,
            crate::midi::io::MidiEvent::new(0, vec![0x90, 60, 100])
        );
    }

    #[test]
    fn midi_lane_channel_filters_monitored_input() {
        let mut track = Track::new("t".to_string(), 0, 0, 1, 1, 8, 48_000.0);
        track.input_monitor = true;
        track.set_midi_lane_channel(0, Some(1));
        track.push_hw_midi_events_to_port(
            0,
            &[
                crate::midi::io::MidiEvent::new(0, vec![0x90, 60, 100]),
                crate::midi::io::MidiEvent::new(1, vec![0x91, 61, 101]),
                crate::midi::io::MidiEvent::new(2, vec![0xF8]),
            ],
        );

        let events = track.collect_track_input_midi_events();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].len(), 2);
        assert_eq!(
            events[0][0],
            crate::midi::io::MidiEvent::new(1, vec![0x91, 61, 101])
        );
        assert_eq!(events[0][1], crate::midi::io::MidiEvent::new(2, vec![0xF8]));
        assert_eq!(track.record_tap_midi_in, events[0]);
    }

    #[test]
    fn midi_lane_channel_omni_does_not_filter_input() {
        let mut track = Track::new("t".to_string(), 0, 0, 1, 1, 8, 48_000.0);
        track.input_monitor = true;
        track.set_midi_lane_channel(0, None);
        track.push_hw_midi_events_to_port(
            0,
            &[
                crate::midi::io::MidiEvent::new(0, vec![0x90, 60, 100]),
                crate::midi::io::MidiEvent::new(1, vec![0x91, 61, 101]),
            ],
        );

        let events = track.collect_track_input_midi_events();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].len(), 2);
    }
}
