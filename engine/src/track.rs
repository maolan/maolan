use super::{audio::track::AudioTrack, midi::track::MIDITrack};
use crate::lv2::Lv2Processor;

#[derive(Debug)]
pub struct Track {
    pub name: String,
    pub level: f32,
    pub armed: bool,
    pub muted: bool,
    pub soloed: bool,
    pub audio: AudioTrack,
    pub midi: MIDITrack,
    pub lv2_processors: Vec<Lv2Processor>,
    pub sample_rate: f64,
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
            armed: false,
            muted: false,
            soloed: false,
            audio: AudioTrack::new(audio_ins, audio_outs, buffer_size),
            midi: MIDITrack::new(midi_ins, midi_outs),
            lv2_processors: Vec::new(),
            sample_rate,
        }
    }

    pub fn setup(&mut self) {
        self.audio.setup();
    }

    pub fn process(&mut self) {
        self.midi.process();
        for audio_in in &self.audio.ins {
            audio_in.process();
        }

        let frames = self
            .audio
            .ins
            .first()
            .map(|audio_in| audio_in.buffer.lock().len())
            .or_else(|| self.audio.outs.first().map(|audio_out| audio_out.buffer.lock().len()))
            .unwrap_or(0);

        let mut stage: Vec<Vec<f32>> = self
            .audio
            .ins
            .iter()
            .map(|audio_in| audio_in.buffer.lock().as_ref().to_vec())
            .collect();

        if self.lv2_processors.is_empty() {
            for audio_out in &self.audio.outs {
                let out_samples = audio_out.buffer.lock();
                out_samples.fill(0.0);
            }
            for (channel, audio_out) in self.audio.outs.iter().enumerate() {
                if let Some(input) = stage.get(channel) {
                    let out_samples = audio_out.buffer.lock();
                    let copy_len = out_samples.len().min(input.len());
                    out_samples[..copy_len].copy_from_slice(&input[..copy_len]);
                }
                *audio_out.finished.lock() = true;
            }
        } else {
            for processor in &mut self.lv2_processors {
                stage = processor.process(&stage, frames);
            }

            for audio_out in &self.audio.outs {
                let out_samples = audio_out.buffer.lock();
                out_samples.fill(0.0);
            }
            for (channel, audio_out) in self.audio.outs.iter().enumerate() {
                if let Some(output) = stage.get(channel) {
                    let out_samples = audio_out.buffer.lock();
                    let copy_len = out_samples.len().min(output.len());
                    out_samples[..copy_len].copy_from_slice(&output[..copy_len]);
                }
                *audio_out.finished.lock() = true;
            }
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

    pub fn arm(&mut self) {
        self.armed = !self.armed;
    }
    pub fn mute(&mut self) {
        self.muted = !self.muted;
    }
    pub fn solo(&mut self) {
        self.soloed = !self.soloed;
    }

    pub fn load_lv2_plugin(&mut self, uri: &str) -> Result<(), String> {
        if self
            .lv2_processors
            .iter()
            .any(|processor| processor.uri() == uri)
        {
            return Err(format!(
                "Track '{}' already has LV2 plugin loaded: {uri}",
                self.name
            ));
        }

        let processor = Lv2Processor::new(self.sample_rate, uri)?;
        self.lv2_processors.push(processor);
        Ok(())
    }

    pub fn unload_lv2_plugin(&mut self, uri: &str) -> Result<(), String> {
        let original_len = self.lv2_processors.len();
        self.lv2_processors.retain(|processor| processor.uri() != uri);
        if self.lv2_processors.len() == original_len {
            return Err(format!(
                "Track '{}' does not have LV2 plugin loaded: {uri}",
                self.name
            ));
        }
        Ok(())
    }

    pub fn loaded_lv2_plugins(&self) -> Vec<String> {
        self.lv2_processors
            .iter()
            .map(|processor| processor.uri().to_string())
            .collect()
    }
}
