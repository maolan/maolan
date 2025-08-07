// use oss_audio_midi::Config;
// use std::io::Write;
// use wavers::{Samples, Wav};
// use std::env;

use maolan::{Engine, run};
use std::thread;

fn main() {
    // let args: Vec<String> = env::args().collect();
    // if args.len() < 2 {
    //     panic!("Usage: oss <wav file>");
    // }
    // let fp = &args[1];
    // let mut oss = Config::new("/dev/dsp", 48000, 32, false);
    // let mut wav: Wav<i32> = Wav::from_path(fp).unwrap();
    // let samples: Samples<i32> = wav.read().unwrap();
    //
    // for frame in 0..samples.len() {
    //     oss.dsp.write_all(&samples[frame].to_ne_bytes()).expect("Error writing out");
    // }

    let mut engine = Engine::new();
    let thread = thread::spawn(move || {
        engine.read();
    });
    run();
    let _ = thread.join();
}
