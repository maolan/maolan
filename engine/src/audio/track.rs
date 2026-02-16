use super::{clip::AudioClip, io::AudioIO};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct AudioTrack {
    pub clips: Vec<AudioClip>,
    pub ins: Vec<Arc<AudioIO>>,
    pub outs: Vec<Arc<AudioIO>>,
    pub finished: bool,
}

impl AudioTrack {
    pub fn new(ins_count: usize, outs_count: usize, buffer_size: usize) -> Self {
        let mut ret = Self {
            clips: vec![],
            ins: Vec::with_capacity(ins_count),
            outs: Vec::with_capacity(outs_count),
            finished: false,
        };
        for _ in 0..ins_count {
            ret.ins.push(Arc::new(AudioIO::new(buffer_size)));
        }
        for _ in 0..outs_count {
            ret.outs.push(Arc::new(AudioIO::new(buffer_size)));
        }
        ret
    }

    pub fn connect_in(&self, index: usize, from: Arc<AudioIO>) -> Result<(), String> {
        if let Some(audio_in) = self.ins.get(index) {
            AudioIO::connect(&from, audio_in);
            Ok(())
        } else {
            Err(format!("Audio input index {} too high", index))
        }
    }

    pub fn connect_out(&self, index: usize, to: Arc<AudioIO>) -> Result<(), String> {
        if let Some(audio_out) = self.outs.get(index) {
            AudioIO::connect(audio_out, &to);
            Ok(())
        } else {
            Err(format!("Audio output index {} too high", index))
        }
    }

    pub fn disconnect_in(&self, index: usize, from: &Arc<AudioIO>) -> Result<(), String> {
        if let Some(audio_in) = self.ins.get(index) {
            AudioIO::disconnect(from, audio_in)
        } else {
            Err(format!("Audio input index {} too high", index))
        }
    }

    pub fn disconnect_out(&self, index: usize, to: &Arc<AudioIO>) -> Result<(), String> {
        if let Some(audio_out) = self.outs.get(index) {
            AudioIO::disconnect(audio_out, to)
        } else {
            Err(format!("Audio output index {} too high", index))
        }
    }

    pub fn process(&mut self) {
        for audio_in in &self.ins {
            audio_in.process();
        }
        for (audio_in, audio_out) in self.ins.iter().zip(self.outs.iter()) {
            let in_samples = audio_in.buffer.lock();
            let out_samples = audio_out.buffer.lock();

            out_samples.copy_from_slice(in_samples);
            *audio_out.finished.lock() = true;
        }
        self.finished = true;
    }

    pub fn setup(&mut self) {
        self.finished = false;
        for input in &self.ins {
            input.setup();
        }
        for output in &self.outs {
            output.setup();
        }
    }

    pub fn ready(&self) -> bool {
        for input in &self.ins {
            if !input.ready() {
                return false;
            }
        }
        true
    }
}
