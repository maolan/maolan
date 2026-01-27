pub trait Track: Send {
    fn process(&mut self) -> ();
    fn name(&self) -> String; 
    fn set_name(&mut self, name: String) -> ();
    fn level(&self) -> f32; 
    fn set_level(&mut self, level: f32) -> ();
}
