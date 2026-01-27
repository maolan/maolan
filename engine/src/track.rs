pub trait Track: Send {
    fn process(&mut self);

    fn name(&self) -> String; 
    fn set_name(&mut self, name: String);

    fn level(&self) -> f32; 
    fn set_level(&mut self, level: f32);

    fn ins(&self) -> usize;
    fn set_ins(&mut self, ins: usize);

    fn audio_outs(&self) -> usize;
    fn set_audio_outs(&mut self, outs: usize);

    fn midi_outs(&self) -> usize;
    fn set_midi_outs(&mut self, outs: usize);

    fn arm(&mut self);
    fn mute(&mut self);
    fn solo(&mut self);
}
