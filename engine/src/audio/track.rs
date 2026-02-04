use super::clip::AudioClip;
use crate::{clip::Clip, track::Track};

pub struct AudioTrack {
    name: String,
    ins: usize,
    audio_outs: usize,
    midi_outs: usize,
    level: f32,
    armed: bool,
    muted: bool,
    soloed: bool,
    clips: Vec<AudioClip>,
}

impl AudioTrack {
    pub fn new(name: String, ins: usize, audio_outs: usize, midi_outs: usize) -> Self {
        Self {
            name,
            ins,
            audio_outs,
            midi_outs,
            level: 0.0,
            armed: false,
            muted: false,
            soloed: false,
            clips: vec![AudioClip::new("".to_string(), 0, 60)],
        }
    }
}

impl Track for AudioTrack {
    fn process(&mut self) {
        // self.buffer.clear();
    }

    fn name(&self) -> String {
        self.name.clone()
    }
    fn set_name(&mut self, name: String) {
        self.name = name;
    }

    fn level(&self) -> f32 {
        self.level
    }
    fn set_level(&mut self, level: f32) {
        self.level = level;
    }

    fn ins(&self) -> usize {
        self.ins
    }
    fn set_ins(&mut self, ins: usize) {
        self.ins = ins;
    }

    fn audio_outs(&self) -> usize {
        self.audio_outs
    }
    fn set_audio_outs(&mut self, outs: usize) {
        self.audio_outs = outs;
    }

    fn midi_outs(&self) -> usize {
        self.midi_outs
    }
    fn set_midi_outs(&mut self, outs: usize) {
        self.midi_outs = outs;
    }

    fn arm(&mut self) {
        self.armed = !self.armed;
    }
    fn mute(&mut self) {
        self.muted = !self.muted;
    }
    fn solo(&mut self) {
        self.soloed = !self.soloed;
    }

    fn add(&mut self, clip: Clip) -> Result<usize, String> {
        match clip {
            Clip::AudioClip(AudioClip {
                name,
                start,
                end,
                offset,
            }) => {
                let index = self
                    .clips
                    .iter()
                    .position(|t| t.start >= start)
                    .unwrap_or_default();
                let mut clip = AudioClip::new(name, start, end);
                clip.offset = offset;
                self.clips.insert(index, clip);
                Ok(index)
            }
            Clip::MIDIClip { .. } => Err("Tried to add audio clip to MIDI track".to_string()),
        }
    }
    fn remove(&mut self, index: usize) -> Result<usize, String> {
        if index < self.clips.len() {
            self.clips.remove(index);
            return Ok(index);
        }
        Err(format!(
            "Can not remove index {} from track {} as it has {} clips",
            index,
            self.name,
            self.clips.len()
        ))
    }

    fn at(&self, index: usize) -> Result<Clip, String> {
        if index < self.clips.len() {
            Ok(Clip::AudioClip(self.clips[index].clone()))
        } else {
            Err(format!(
                "Clip with index {} not found in track {}. There are totally {} clips in that track",
                index,
                self.name.clone(),
                self.clips.len()
            ))
        }
    }
}
