use super::{audio::track::AudioTrack, midi::track::MIDITrack};
#[cfg(all(unix, not(target_os = "macos")))]
use crate::lv2::Lv2Processor;
#[cfg(all(unix, not(target_os = "macos")))]
use crate::message::{Lv2GraphConnection, Lv2GraphNode, Lv2GraphPlugin, Lv2PluginState};
use crate::vst3::Vst3Processor;
use crate::{audio::io::AudioIO, midi::io::MidiEvent};
#[cfg(all(unix, not(target_os = "macos")))]
use crate::{kind::Kind, routing};
use midly::{MetaMessage, Smf, Timing, TrackEventKind, live::LiveEvent};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
};

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
struct AudioClipBuffer {
    channels: usize,
    samples: Vec<f32>,
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
    pub audio: AudioTrack,
    pub midi: MIDITrack,
    #[cfg(all(unix, not(target_os = "macos")))]
    pub lv2_processors: Vec<Lv2Instance>,
    pub vst3_processors: Vec<Vst3Instance>,
    #[cfg(all(unix, not(target_os = "macos")))]
    pub lv2_midi_connections: Vec<Lv2GraphConnection>,
    pub pending_hw_midi_out_events: Vec<MidiEvent>,
    #[cfg(all(unix, not(target_os = "macos")))]
    pub next_lv2_instance_id: usize,
    pub next_vst3_instance_id: usize,
    pub sample_rate: f64,
    pub output_enabled: bool,
    pub transport_sample: usize,
    pub loop_enabled: bool,
    pub loop_range_samples: Option<(usize, usize)>,
    pub record_tap_outs: Vec<Vec<f32>>,
    pub record_tap_midi_in: Vec<MidiEvent>,
    #[cfg(all(unix, not(target_os = "macos")))]
    pub lv2_state_base_dir: Option<PathBuf>,
    pub session_base_dir: Option<PathBuf>,
    audio_clip_cache: HashMap<String, AudioClipBuffer>,
    midi_clip_cache: HashMap<String, Vec<(usize, Vec<u8>)>>,
}

impl Track {
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
            audio: AudioTrack::new(audio_ins, audio_outs, buffer_size),
            midi: MIDITrack::new(midi_ins, midi_outs),
            #[cfg(all(unix, not(target_os = "macos")))]
            lv2_processors: Vec::new(),
            vst3_processors: Vec::new(),
            #[cfg(all(unix, not(target_os = "macos")))]
            lv2_midi_connections: Vec::new(),
            pending_hw_midi_out_events: vec![],
            #[cfg(all(unix, not(target_os = "macos")))]
            next_lv2_instance_id: 0,
            next_vst3_instance_id: 0,
            sample_rate,
            output_enabled: true,
            transport_sample: 0,
            loop_enabled: false,
            loop_range_samples: None,
            record_tap_outs: vec![vec![0.0; buffer_size]; audio_outs],
            record_tap_midi_in: vec![],
            #[cfg(all(unix, not(target_os = "macos")))]
            lv2_state_base_dir: None,
            session_base_dir: None,
            audio_clip_cache: HashMap::new(),
            midi_clip_cache: HashMap::new(),
        }
        .with_default_passthrough()
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
            .unwrap_or(0);
        let mut track_input_midi_events = self.collect_track_input_midi_events();
        if self.disk_monitor {
            self.mix_clip_midi_into_inputs(&mut track_input_midi_events, frames);
        }

        #[cfg(all(unix, not(target_os = "macos")))]
        if !self.lv2_processors.is_empty() {
            let mut processed = vec![false; self.lv2_processors.len()];
            let mut remaining = self.lv2_processors.len();
            let mut processed_midi_plugins = HashSet::new();
            let mut midi_node_events = HashMap::<(Lv2GraphNode, usize), Vec<MidiEvent>>::new();
            for (port, events) in track_input_midi_events.iter().enumerate() {
                midi_node_events.insert((Lv2GraphNode::TrackInput, port), events.clone());
            }

            while remaining > 0 {
                let mut progressed = false;
                for (idx, already_processed) in processed.iter_mut().enumerate() {
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
                    if !self.lv2_plugin_midi_ready(instance_id, &processed_midi_plugins) {
                        continue;
                    }

                    for audio_in in self.lv2_processors[idx].processor.audio_inputs() {
                        audio_in.process();
                    }
                    let midi_inputs = self.lv2_plugin_input_events(instance_id, &midi_node_events);
                    let midi_outputs = self.lv2_processors[idx]
                        .processor
                        .process_with_audio_io(frames, &midi_inputs);
                    for (port, events) in midi_outputs.into_iter().enumerate() {
                        if !events.is_empty() {
                            midi_node_events
                                .insert((Lv2GraphNode::PluginInstance(instance_id), port), events);
                        }
                    }
                    *already_processed = true;
                    remaining -= 1;
                    processed_midi_plugins.insert(instance_id);
                    progressed = true;
                }

                if !progressed {
                    break;
                }
            }

            if remaining > 0 {
                for (idx, already_processed) in processed.iter().enumerate() {
                    if *already_processed {
                        continue;
                    }
                    for audio_in in self.lv2_processors[idx].processor.audio_inputs() {
                        audio_in.process();
                    }
                    let instance_id = self.lv2_processors[idx].id;
                    let midi_inputs = self.lv2_plugin_input_events(instance_id, &midi_node_events);
                    let midi_outputs = self.lv2_processors[idx]
                        .processor
                        .process_with_audio_io(frames, &midi_inputs);
                    for (port, events) in midi_outputs.into_iter().enumerate() {
                        if !events.is_empty() {
                            midi_node_events
                                .insert((Lv2GraphNode::PluginInstance(instance_id), port), events);
                        }
                    }
                    processed_midi_plugins.insert(instance_id);
                }
            }

            self.route_lv2_midi_to_track_outputs(&midi_node_events);
        }

        if !self.vst3_processors.is_empty() {
            for instance in &self.vst3_processors {
                let ready = instance
                    .processor
                    .audio_inputs()
                    .iter()
                    .all(|audio_in| audio_in.ready());
                if ready {
                    instance.processor.process_with_audio_io(frames);
                }
            }
        }

        self.route_track_inputs_to_track_outputs(&track_input_midi_events);
        self.dispatch_track_output_midi_to_connected_inputs();
        self.collect_hw_midi_output_events();
        self.clear_local_midi_inputs();
        let linear_gain = 10.0_f32.powf(self.level / 20.0);
        let (left_balance, right_balance) = if self.audio.outs.len() == 2 {
            let b = self.balance.clamp(-1.0, 1.0);
            ((1.0 - b).clamp(0.0, 1.0), (1.0 + b).clamp(0.0, 1.0))
        } else {
            (1.0, 1.0)
        };

        let internal_sources = self.internal_audio_sources();
        let internal_source_ptrs: HashSet<*const AudioIO> =
            internal_sources.iter().map(Arc::as_ptr).collect();
        for out_idx in 0..self.audio.outs.len() {
            let audio_out = self.audio.outs[out_idx].clone();
            let out_samples = audio_out.buffer.lock();
            out_samples.fill(0.0);
            if self.record_tap_outs.len() <= out_idx {
                self.record_tap_outs.push(vec![0.0; out_samples.len()]);
            }
            let mut tap = std::mem::take(&mut self.record_tap_outs[out_idx]);
            if tap.len() != out_samples.len() {
                tap.resize(out_samples.len(), 0.0);
            }
            tap.fill(0.0);
            let balance_gain = if self.audio.outs.len() == 2 {
                if out_idx == 0 {
                    left_balance
                } else {
                    right_balance
                }
            } else {
                1.0
            };
            let connections = audio_out.connections.lock();
            for source in connections.iter() {
                if !internal_source_ptrs.contains(&Arc::as_ptr(source)) {
                    continue;
                }
                let source_buf = source.buffer.lock();

                if self.input_monitor && self.output_enabled {
                    for ((tap_sample, out_sample), in_sample) in tap
                        .iter_mut()
                        .zip(out_samples.iter_mut())
                        .zip(source_buf.iter())
                    {
                        *tap_sample += *in_sample;
                        *out_sample += *in_sample * linear_gain * balance_gain;
                    }
                } else {
                    for (tap_sample, in_sample) in tap.iter_mut().zip(source_buf.iter()) {
                        *tap_sample += *in_sample;
                    }
                }
            }
            if self.disk_monitor {
                self.mix_clip_audio_into_output(out_idx, out_samples, linear_gain, balance_gain);
            }
            self.record_tap_outs[out_idx] = tap;
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

    pub fn output_meter_db(&self) -> Vec<f32> {
        self.audio
            .outs
            .iter()
            .map(|audio_out| {
                let buffer = audio_out.buffer.lock();
                let peak = buffer
                    .iter()
                    .fold(0.0_f32, |acc, sample| acc.max(sample.abs()));
                if peak <= 1.0e-6 {
                    -90.0
                } else {
                    (20.0 * peak.log10()).clamp(-90.0, 20.0)
                }
            })
            .collect()
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
    pub fn mute(&mut self) {
        self.muted = !self.muted;
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
    pub fn set_session_base_dir(&mut self, base_dir: Option<PathBuf>) {
        self.session_base_dir = base_dir;
    }

    fn resolve_clip_path(&self, clip_name: &str) -> PathBuf {
        let clip_path = Path::new(clip_name);
        if clip_path.is_absolute() {
            clip_path.to_path_buf()
        } else if let Some(base) = &self.session_base_dir {
            base.join(clip_path)
        } else {
            clip_path.to_path_buf()
        }
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

    fn clip_buffer(&mut self, clip_name: &str) -> Option<AudioClipBuffer> {
        if let Some(cached) = self.audio_clip_cache.get(clip_name) {
            return Some(cached.clone());
        }
        let path = self.resolve_clip_path(clip_name);
        let loaded = Self::load_audio_clip_buffer(&path)?;
        self.audio_clip_cache
            .insert(clip_name.to_string(), loaded.clone());
        Some(loaded)
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
                if let TrackEventKind::Midi { channel, message } = event.kind {
                    let mut data = Vec::with_capacity(3);
                    if (LiveEvent::Midi { channel, message })
                        .write(&mut data)
                        .is_ok()
                    {
                        out.push((ticks_to_samples(tick), data));
                    }
                }
            }
        }
        out.sort_by_key(|(sample, _)| *sample);
        Some(out)
    }

    fn midi_clip_events(&mut self, clip_name: &str) -> Option<Vec<(usize, Vec<u8>)>> {
        if let Some(cached) = self.midi_clip_cache.get(clip_name) {
            return Some(cached.clone());
        }
        let path = self.resolve_clip_path(clip_name);
        let loaded = Self::load_midi_clip_events(&path, self.sample_rate)?;
        self.midi_clip_cache
            .insert(clip_name.to_string(), loaded.clone());
        Some(loaded)
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

    fn mix_clip_audio_into_output(
        &mut self,
        out_channel: usize,
        out_samples: &mut [f32],
        linear_gain: f32,
        balance_gain: f32,
    ) {
        let frames = out_samples.len();
        if frames == 0 {
            return;
        }
        let segments = self.cycle_segments(frames);
        let clips = self.audio.clips.clone();
        for clip in clips {
            let clip_start = clip.start;
            let clip_len = clip.end;
            if clip_len == 0 {
                continue;
            }
            let destination_lane = clip
                .input_channel
                .min(self.audio.outs.len().saturating_sub(1));
            if self.audio.outs.len() > 1 && out_channel != destination_lane {
                continue;
            }
            let clip_end = clip_start.saturating_add(clip_len);
            let Some(buffer) = self.clip_buffer(&clip.name) else {
                continue;
            };
            let channels = buffer.channels.max(1);
            let total_frames = buffer.samples.len() / channels;
            if total_frames == 0 {
                continue;
            }
            let source_channel = if channels == 1 {
                0
            } else {
                clip.input_channel.min(channels - 1)
            };
            for (segment_start, segment_end, out_offset) in &segments {
                if clip_end <= *segment_start || clip_start >= *segment_end {
                    continue;
                }
                let from = (*segment_start).max(clip_start);
                let to = (*segment_end).min(clip_end);
                for absolute_sample in from..to {
                    let track_idx = out_offset + (absolute_sample - *segment_start);
                    let clip_idx = absolute_sample - clip_start + clip.offset;
                    if clip_idx >= total_frames || track_idx >= out_samples.len() {
                        break;
                    }
                    let sample = buffer.samples[clip_idx * channels + source_channel];
                    if self.output_enabled {
                        out_samples[track_idx] += sample * linear_gain * balance_gain;
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
        let clips = self.midi.clips.clone();
        for clip in clips {
            let clip_start = clip.start;
            let clip_len = clip.end;
            if clip_len == 0 {
                continue;
            }
            let input_lane = clip
                .input_channel
                .min(input_events.len().saturating_sub(1));
            let clip_end = clip_start.saturating_add(clip_len);
            let Some(events) = self.midi_clip_events(&clip.name) else {
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
                for (source_sample, data) in &events {
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
        let id = self.next_lv2_instance_id;
        self.next_lv2_instance_id = self.next_lv2_instance_id.saturating_add(1);
        self.lv2_processors.push(Lv2Instance { id, processor });
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
        self.lv2_midi_connections.retain(|conn| {
            conn.from_node != Lv2GraphNode::PluginInstance(removed.id)
                && conn.to_node != Lv2GraphNode::PluginInstance(removed.id)
        });
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

    #[cfg(all(unix, not(target_os = "macos")))]
    pub fn lv2_graph_plugins(&self) -> Vec<Lv2GraphPlugin> {
        self.lv2_processors
            .iter()
            .map(|instance| Lv2GraphPlugin {
                instance_id: instance.id,
                uri: instance.processor.uri().to_string(),
                name: instance.processor.name().to_string(),
                audio_inputs: instance.processor.audio_input_count(),
                audio_outputs: instance.processor.audio_output_count(),
                midi_inputs: instance.processor.midi_input_count(),
                midi_outputs: instance.processor.midi_output_count(),
                state: instance.processor.snapshot_state(),
            })
            .collect()
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
            .unwrap_or(0);
        let input_count = self.audio.ins.len().max(1);
        let output_count = self.audio.outs.len().max(1);
        let processor = Vst3Processor::new(buffer_size, plugin_path, input_count, output_count);
        let id = self.next_vst3_instance_id;
        self.next_vst3_instance_id = self.next_vst3_instance_id.saturating_add(1);
        self.vst3_processors.push(Vst3Instance { id, processor });
        self.rewire_vst3_default_audio_graph();
        Ok(())
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
        self.rewire_vst3_default_audio_graph();
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
        use crate::message::{Vst3GraphConnection, Vst3GraphNode};
        use crate::kind::Kind;

        let mut connections = Vec::new();

        // Build connections by inspecting AudioIO connections
        // Similar to lv2_graph_connections approach
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
                        let to_port = self.audio.outs
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
            if Arc::ptr_eq(input, &Arc::new(unsafe { std::ptr::read(audio_io as *const _) })) {
                return Some((Vst3GraphNode::TrackInput, idx));
            }
        }

        // Check if it's a VST3 output
        for instance in &self.vst3_processors {
            for (port_idx, output) in instance.processor.audio_outputs().iter().enumerate() {
                if Arc::ptr_eq(output, &Arc::new(unsafe { std::ptr::read(audio_io as *const _) })) {
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
            Vst3GraphNode::TrackOutput => return Err("Cannot connect from track output".to_string()),
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
            Vst3GraphNode::TrackOutput => return Err("Cannot disconnect from track output".to_string()),
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
        to_io.connections.lock().retain(|conn| !Arc::ptr_eq(conn, &from_io));
        Ok(())
    }

    pub fn clear_default_passthrough(&self) {
        for (audio_in, audio_out) in self.audio.ins.iter().zip(self.audio.outs.iter()) {
            let _ = AudioIO::disconnect(audio_in, audio_out);
            let _ = AudioIO::disconnect(audio_out, audio_in);
        }
        for (midi_in, midi_out) in self.midi.ins.iter().zip(self.midi.outs.iter()) {
            let _ = midi_out.lock().disconnect(midi_in);
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    pub fn lv2_graph_connections(&self) -> Vec<Lv2GraphConnection> {
        let mut source_ports: Vec<(Lv2GraphNode, usize, Arc<AudioIO>)> = self
            .audio
            .ins
            .iter()
            .enumerate()
            .map(|(idx, io)| (Lv2GraphNode::TrackInput, idx, io.clone()))
            .collect();
        for instance in &self.lv2_processors {
            source_ports.extend(
                instance
                    .processor
                    .audio_outputs()
                    .iter()
                    .enumerate()
                    .map(|(idx, io)| (Lv2GraphNode::PluginInstance(instance.id), idx, io.clone())),
            );
        }

        let mut connections = vec![];
        for (to_port, to_io) in self.audio.outs.iter().enumerate() {
            for conn in to_io.connections.lock().iter() {
                if let Some((from_node, from_port, _)) = source_ports
                    .iter()
                    .find(|(_, _, source_io)| Arc::ptr_eq(source_io, conn))
                {
                    connections.push(Lv2GraphConnection {
                        from_node: from_node.clone(),
                        from_port: *from_port,
                        to_node: Lv2GraphNode::TrackOutput,
                        to_port,
                        kind: Kind::Audio,
                    });
                }
            }
        }
        for instance in &self.lv2_processors {
            for (to_port, to_io) in instance.processor.audio_inputs().iter().enumerate() {
                for conn in to_io.connections.lock().iter() {
                    if let Some((from_node, from_port, _)) = source_ports
                        .iter()
                        .find(|(_, _, source_io)| Arc::ptr_eq(source_io, conn))
                    {
                        connections.push(Lv2GraphConnection {
                            from_node: from_node.clone(),
                            from_port: *from_port,
                            to_node: Lv2GraphNode::PluginInstance(instance.id),
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
                    connections.push(Lv2GraphConnection {
                        from_node: Lv2GraphNode::TrackInput,
                        from_port,
                        to_node: Lv2GraphNode::TrackOutput,
                        to_port,
                        kind: Kind::MIDI,
                    });
                }
            }
        }
        connections.extend(self.lv2_midi_connections.iter().cloned());
        connections
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    pub fn connect_lv2_audio(
        &self,
        from_node: Lv2GraphNode,
        from_port: usize,
        to_node: Lv2GraphNode,
        to_port: usize,
    ) -> Result<(), String> {
        let source = self.lv2_source_io(&from_node, from_port)?;
        let target = self.lv2_target_io(&to_node, to_port)?;
        if routing::would_create_cycle(&from_node, &to_node, |node| {
            self.lv2_connected_neighbors(Kind::Audio, node)
        }) {
            return Err("Circular routing is not allowed!".to_string());
        }
        AudioIO::connect(&source, &target);

        if matches!(from_node, Lv2GraphNode::TrackInput) {
            source
                .connections
                .lock()
                .retain(|conn| !Arc::ptr_eq(conn, &target));
        }
        Ok(())
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    pub fn disconnect_lv2_audio(
        &self,
        from_node: Lv2GraphNode,
        from_port: usize,
        to_node: Lv2GraphNode,
        to_port: usize,
    ) -> Result<(), String> {
        let source = self.lv2_source_io(&from_node, from_port)?;
        let target = self.lv2_target_io(&to_node, to_port)?;
        AudioIO::disconnect(&source, &target)
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    pub fn connect_lv2_midi(
        &mut self,
        from_node: Lv2GraphNode,
        from_port: usize,
        to_node: Lv2GraphNode,
        to_port: usize,
    ) -> Result<(), String> {
        self.validate_lv2_midi_source(&from_node, from_port)?;
        self.validate_lv2_midi_target(&to_node, to_port)?;
        if from_node == to_node && from_port == to_port {
            return Err("Cannot connect a MIDI port to itself".to_string());
        }
        if routing::would_create_cycle(&from_node, &to_node, |node| {
            self.lv2_connected_neighbors(Kind::MIDI, node)
        }) {
            return Err("Circular routing is not allowed!".to_string());
        }
        let new_conn = Lv2GraphConnection {
            from_node,
            from_port,
            to_node,
            to_port,
            kind: Kind::MIDI,
        };
        if self.lv2_midi_connections.iter().any(|c| c == &new_conn) {
            return Ok(());
        }
        self.lv2_midi_connections.push(new_conn);
        Ok(())
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    pub fn disconnect_lv2_midi(
        &mut self,
        from_node: Lv2GraphNode,
        from_port: usize,
        to_node: Lv2GraphNode,
        to_port: usize,
    ) -> Result<(), String> {
        let before = self.lv2_midi_connections.len();
        self.lv2_midi_connections.retain(|c| {
            !(c.kind == Kind::MIDI
                && c.from_node == from_node
                && c.from_port == from_port
                && c.to_node == to_node
                && c.to_port == to_port)
        });
        if self.lv2_midi_connections.len() == before {
            Err("MIDI LV2 graph connection not found".to_string())
        } else {
            Ok(())
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    pub fn show_lv2_plugin_ui(&mut self, uri: &str) -> Result<(), String> {
        let Some(instance) = self
            .lv2_processors
            .iter_mut()
            .find(|instance| instance.processor.uri() == uri)
        else {
            return Err(format!(
                "Track '{}' does not have LV2 plugin loaded: {uri}",
                self.name
            ));
        };
        instance.processor.show_ui()
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    pub fn show_lv2_plugin_ui_instance(&mut self, instance_id: usize) -> Result<(), String> {
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
        instance.processor.show_ui()
    }

    fn with_default_passthrough(self) -> Self {
        self.ensure_default_audio_passthrough();
        self.ensure_default_midi_passthrough();
        self
    }

    pub(crate) fn ensure_default_audio_passthrough(&self) {
        for (audio_in, audio_out) in self.audio.ins.iter().zip(self.audio.outs.iter()) {
            audio_in
                .connections
                .lock()
                .retain(|conn| !Arc::ptr_eq(conn, audio_out));

            let exists = audio_out
                .connections
                .lock()
                .iter()
                .any(|conn| Arc::ptr_eq(conn, audio_in));
            if !exists {
                audio_out.connections.lock().push(audio_in.clone());
            }
        }
    }

    pub(crate) fn ensure_default_midi_passthrough(&self) {
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
    }

    fn internal_audio_sources(&self) -> Vec<Arc<AudioIO>> {
        let mut sources = self.audio.ins.clone();
        #[cfg(all(unix, not(target_os = "macos")))]
        for instance in &self.lv2_processors {
            sources.extend(instance.processor.audio_outputs().iter().cloned());
        }
        for instance in &self.vst3_processors {
            sources.extend(instance.processor.audio_outputs().iter().cloned());
        }
        sources
    }

    fn disconnect_all(port: &Arc<AudioIO>) {
        let connections = port.connections.lock().clone();
        for other in connections {
            let _ = AudioIO::disconnect(&other, port);
        }
    }

    fn rewire_vst3_default_audio_graph(&self) {
        for instance in &self.vst3_processors {
            for port in instance.processor.audio_inputs() {
                Self::disconnect_all(port);
            }
            for port in instance.processor.audio_outputs() {
                Self::disconnect_all(port);
            }
        }

        for out in &self.audio.outs {
            out.connections.lock().retain(|source| {
                !self.audio.ins.iter().any(|input| Arc::ptr_eq(source, input))
                    && !self.vst3_processors.iter().any(|instance| {
                        instance
                            .processor
                            .audio_outputs()
                            .iter()
                            .any(|port| Arc::ptr_eq(source, port))
                    })
            });
        }
        for input in &self.audio.ins {
            for out in &self.audio.outs {
                let _ = AudioIO::disconnect(input, out);
            }
        }

        if self.vst3_processors.is_empty() {
            self.ensure_default_audio_passthrough();
            return;
        }

        let first = &self.vst3_processors[0].processor;
        for (idx, input) in self.audio.ins.iter().enumerate() {
            if let Some(vin) = first.audio_inputs().get(idx % first.audio_inputs().len()) {
                AudioIO::connect(input, vin);
            }
        }

        for pair in self.vst3_processors.windows(2) {
            let from = &pair[0].processor;
            let to = &pair[1].processor;
            for (idx, out) in from.audio_outputs().iter().enumerate() {
                if let Some(next_in) = to.audio_inputs().get(idx % to.audio_inputs().len()) {
                    AudioIO::connect(out, next_in);
                }
            }
        }

        let last = &self.vst3_processors[self.vst3_processors.len() - 1].processor;
        for (idx, out) in self.audio.outs.iter().enumerate() {
            if let Some(vout) = last.audio_outputs().get(idx % last.audio_outputs().len()) {
                AudioIO::connect(vout, out);
            }
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn lv2_source_io(&self, node: &Lv2GraphNode, port: usize) -> Result<Arc<AudioIO>, String> {
        match node {
            Lv2GraphNode::TrackInput => self
                .audio
                .ins
                .get(port)
                .cloned()
                .ok_or_else(|| format!("Track input port {port} not found")),
            Lv2GraphNode::TrackOutput => Err("Track output node cannot be source".to_string()),
            Lv2GraphNode::PluginInstance(instance_id) => self
                .lv2_processors
                .iter()
                .find(|instance| instance.id == *instance_id)
                .and_then(|instance| instance.processor.audio_outputs().get(port).cloned())
                .ok_or_else(|| format!("Plugin instance {instance_id} output port {port} missing")),
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn lv2_target_io(&self, node: &Lv2GraphNode, port: usize) -> Result<Arc<AudioIO>, String> {
        match node {
            Lv2GraphNode::TrackInput => Err("Track input node cannot be target".to_string()),
            Lv2GraphNode::TrackOutput => self
                .audio
                .outs
                .get(port)
                .cloned()
                .ok_or_else(|| format!("Track output port {port} not found")),
            Lv2GraphNode::PluginInstance(instance_id) => self
                .lv2_processors
                .iter()
                .find(|instance| instance.id == *instance_id)
                .and_then(|instance| instance.processor.audio_inputs().get(port).cloned())
                .ok_or_else(|| format!("Plugin instance {instance_id} input port {port} missing")),
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn validate_lv2_midi_source(&self, node: &Lv2GraphNode, port: usize) -> Result<(), String> {
        match node {
            Lv2GraphNode::TrackInput => self
                .midi
                .ins
                .get(port)
                .map(|_| ())
                .ok_or_else(|| format!("Track MIDI input port {port} not found")),
            Lv2GraphNode::TrackOutput => Err("Track output node cannot be MIDI source".to_string()),
            Lv2GraphNode::PluginInstance(instance_id) => self
                .lv2_processors
                .iter()
                .find(|instance| instance.id == *instance_id)
                .and_then(|instance| (port < instance.processor.midi_output_count()).then_some(()))
                .ok_or_else(|| {
                    format!("Plugin instance {instance_id} MIDI output port {port} missing")
                }),
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn validate_lv2_midi_target(&self, node: &Lv2GraphNode, port: usize) -> Result<(), String> {
        match node {
            Lv2GraphNode::TrackInput => Err("Track input node cannot be MIDI target".to_string()),
            Lv2GraphNode::TrackOutput => self
                .midi
                .outs
                .get(port)
                .map(|_| ())
                .ok_or_else(|| format!("Track MIDI output port {port} not found")),
            Lv2GraphNode::PluginInstance(instance_id) => self
                .lv2_processors
                .iter()
                .find(|instance| instance.id == *instance_id)
                .and_then(|instance| (port < instance.processor.midi_input_count()).then_some(()))
                .ok_or_else(|| {
                    format!("Plugin instance {instance_id} MIDI input port {port} missing")
                }),
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn lv2_connected_neighbors(
        &self,
        kind: Kind,
        current_node: &Lv2GraphNode,
    ) -> Vec<Lv2GraphNode> {
        let mut nodes = HashSet::new();
        for conn in self.lv2_graph_connections() {
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
        let events: Vec<Vec<MidiEvent>> = self
            .midi
            .ins
            .iter()
            .map(|input| input.lock().buffer.clone())
            .collect();
        self.record_tap_midi_in = events
            .iter()
            .flat_map(|port_events| port_events.iter().cloned())
            .collect();
        self.record_tap_midi_in.sort_by_key(|e| e.frame);
        events
    }

    fn route_track_inputs_to_track_outputs(&self, input_events: &[Vec<MidiEvent>]) {
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
            let Some(local_input) = self.midi.ins.get(input_idx) else {
                continue;
            };
            for out in &self.midi.outs {
                let should_route = out
                    .lock()
                    .connections
                    .iter()
                    .any(|conn| Arc::ptr_eq(conn, local_input));
                if should_route {
                    out.lock().buffer.extend_from_slice(events);
                }
            }
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn lv2_plugin_midi_ready(&self, instance_id: usize, processed: &HashSet<usize>) -> bool {
        self.lv2_midi_connections
            .iter()
            .filter(|conn| {
                conn.kind == Kind::MIDI
                    && conn.to_node == Lv2GraphNode::PluginInstance(instance_id)
                    && matches!(conn.from_node, Lv2GraphNode::PluginInstance(_))
            })
            .all(|conn| match conn.from_node {
                Lv2GraphNode::PluginInstance(from_id) => processed.contains(&from_id),
                _ => true,
            })
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn lv2_plugin_input_events(
        &self,
        instance_id: usize,
        node_events: &HashMap<(Lv2GraphNode, usize), Vec<MidiEvent>>,
    ) -> Vec<Vec<MidiEvent>> {
        let midi_inputs = self
            .lv2_processors
            .iter()
            .find(|instance| instance.id == instance_id)
            .map(|instance| instance.processor.midi_input_count())
            .unwrap_or(0);
        let mut per_port = vec![Vec::new(); midi_inputs];
        for conn in self.lv2_midi_connections.iter().filter(|conn| {
            conn.kind == Kind::MIDI
                && conn.to_node == Lv2GraphNode::PluginInstance(instance_id)
                && conn.to_port < midi_inputs
        }) {
            if let Some(events) = node_events.get(&(conn.from_node.clone(), conn.from_port)) {
                per_port[conn.to_port].extend_from_slice(events);
            }
        }
        per_port
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn route_lv2_midi_to_track_outputs(
        &self,
        node_events: &HashMap<(Lv2GraphNode, usize), Vec<MidiEvent>>,
    ) {
        if !self.output_enabled {
            return;
        }
        for conn in self
            .lv2_midi_connections
            .iter()
            .filter(|conn| conn.kind == Kind::MIDI && conn.to_node == Lv2GraphNode::TrackOutput)
        {
            let Some(out) = self.midi.outs.get(conn.to_port) else {
                continue;
            };
            if let Some(events) = node_events.get(&(conn.from_node.clone(), conn.from_port)) {
                out.lock().buffer.extend_from_slice(events);
            }
        }
    }

    fn dispatch_track_output_midi_to_connected_inputs(&self) {
        for out in &self.midi.outs {
            let (events, targets) = {
                let out_lock = out.lock();
                (out_lock.buffer.clone(), out_lock.connections.clone())
            };
            if events.is_empty() {
                continue;
            }
            for target in targets {
                if self
                    .midi
                    .ins
                    .iter()
                    .any(|input| Arc::ptr_eq(input, &target))
                {
                    continue;
                }
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
        for out in &self.midi.outs {
            self.pending_hw_midi_out_events
                .extend(out.lock().buffer.iter().cloned());
        }
    }

    pub fn take_hw_midi_out_events(&mut self) -> Vec<MidiEvent> {
        std::mem::take(&mut self.pending_hw_midi_out_events)
    }
}

#[cfg(test)]
mod tests {
    use super::Track;
    #[cfg(all(unix, not(target_os = "macos")))]
    use crate::{kind::Kind, message::Lv2GraphNode};
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
                .all(|conn| !Arc::ptr_eq(conn, &track.audio.ins[0]))
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
    #[cfg(all(unix, not(target_os = "macos")))]
    fn lv2_graph_includes_default_track_midi_passthrough() {
        let track = Track::new("t".to_string(), 0, 0, 1, 2, 64, 48_000.0);
        let connections = track.lv2_graph_connections();

        assert!(connections.iter().any(|c| {
            c.kind == Kind::MIDI
                && c.from_node == Lv2GraphNode::TrackInput
                && c.from_port == 0
                && c.to_node == Lv2GraphNode::TrackOutput
                && c.to_port == 0
        }));
        assert!(connections.iter().all(|c| {
            !(c.kind == Kind::MIDI
                && c.from_node == Lv2GraphNode::TrackInput
                && c.from_port == 0
                && c.to_node == Lv2GraphNode::TrackOutput
                && c.to_port == 1)
        }));
    }
}
