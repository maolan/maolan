//! `maolan-iodelay` — live loopback latency measurement over OSS mmap
//! duplex, MTDM content-inferred (the engine counterpart of the root
//! `latency.c` tool).
//!
//! ```text
//! maolan-iodelay --input-device /dev/dsp5 --input-channel 8 \
//!     --output-device /dev/dsp5 --output-channel 3
//! ```
//!
//! Plays a continuous multitone on the output channel, demodulates the
//! looped-back input channel, and reports the converged round trip every
//! 250 ms until interrupted (`--seconds` for a bounded run). The engine
//! crate (`maolan_engine::iodelay`) does all the work; this binary is
//! argument parsing, signal handling, and printing.

#![cfg(target_os = "freebsd")]

use maolan_engine::iodelay::{IoDelay, IoDelayOptions, IoDelayStatus};
use nix::sys::signal::{SigHandler, Signal, signal};
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

/// Bridge between the C signal handler and the options' stop flag.
static STOP: OnceLock<Arc<AtomicBool>> = OnceLock::new();

extern "C" fn handle_signal(_sig: i32) {
    if let Some(flag) = STOP.get() {
        flag.store(true, Ordering::Relaxed);
    }
}

struct Args {
    input_device: String,
    input_channel: usize,
    output_device: String,
    output_channel: usize,
    rate: usize,
    gain: f32,
    seconds: f64,
}

fn parse_args() -> Result<Args, String> {
    let mut input_device = None;
    let mut input_channel = None;
    let mut output_device = None;
    let mut output_channel = None;
    let mut rate = 48_000;
    let mut gain = 1.0f32;
    let mut seconds = 0.0f64;
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut value = |name: &str| it.next().ok_or_else(|| format!("{name} requires a value"));
        match arg.as_str() {
            "--input-device" => input_device = Some(value("--input-device")?),
            "--input-channel" => {
                input_channel = Some(
                    value("--input-channel")?
                        .parse()
                        .map_err(|_| "--input-channel must be a positive integer".to_string())?,
                )
            }
            "--output-device" => output_device = Some(value("--output-device")?),
            "--output-channel" => {
                output_channel = Some(
                    value("--output-channel")?
                        .parse()
                        .map_err(|_| "--output-channel must be a positive integer".to_string())?,
                )
            }
            "--rate" => {
                rate = value("--rate")?
                    .parse()
                    .map_err(|_| "--rate must be a positive integer".to_string())?
            }
            "--gain" => {
                gain = value("--gain")?
                    .parse()
                    .map_err(|_| "--gain must be a number".to_string())?
            }
            "--seconds" => {
                seconds = value("--seconds")?
                    .parse()
                    .map_err(|_| "--seconds must be a number".to_string())?
            }
            "-h" | "--help" => {
                println!(
                    "usage: maolan-iodelay --input-device <path> --input-channel <n> \
                     --output-device <path> --output-channel <n> [--rate <hz>] \
                     [--gain <f>] [--seconds <n>]"
                );
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    let input_channel = input_channel.ok_or_else(|| "--input-channel is required".to_string())?;
    let output_channel =
        output_channel.ok_or_else(|| "--output-channel is required".to_string())?;
    if input_channel == 0 || output_channel == 0 {
        return Err("channel indexes are 1-based and must be positive".to_string());
    }
    if gain <= 0.0 {
        return Err("--gain must be positive".to_string());
    }
    Ok(Args {
        input_device: input_device.ok_or_else(|| "--input-device is required".to_string())?,
        input_channel,
        output_device: output_device.ok_or_else(|| "--output-device is required".to_string())?,
        output_channel,
        rate,
        gain,
        seconds,
    })
}

fn main() {
    let args = match parse_args() {
        Ok(args) => args,
        Err(e) => {
            eprintln!("maolan-iodelay: {e}");
            eprintln!(
                "usage: maolan-iodelay --input-device <path> --input-channel <n> \
                 --output-device <path> --output-channel <n> [--rate <hz>] \
                 [--gain <f>] [--seconds <n>]"
            );
            std::process::exit(2);
        }
    };
    let options = IoDelayOptions {
        sample_rate: args.rate,
        gain: args.gain,
        seconds: args.seconds,
        ..IoDelayOptions::new(
            args.input_device.clone(),
            args.input_channel,
            args.output_device.clone(),
            args.output_channel,
        )
    };
    let _ = STOP.set(Arc::clone(&options.stop));
    for sig in [Signal::SIGINT, Signal::SIGTERM] {
        if let Err(e) = unsafe { signal(sig, SigHandler::Handler(handle_signal)) } {
            eprintln!("maolan-iodelay: warning: cannot install {sig} handler: {e}");
        }
    }

    let mut iodelay = match IoDelay::open(options) {
        Ok(iodelay) => iodelay,
        Err(e) => {
            eprintln!("maolan-iodelay: {e}");
            std::process::exit(1);
        }
    };
    let rate = iodelay.sample_rate() as f64;
    println!(
        "Measuring output {} -> input {}: {} Hz, {}-frame mmap ring, \
         MTDM content-inferred latency",
        args.output_channel,
        args.input_channel,
        iodelay.sample_rate(),
        iodelay.ring_frames()
    );
    let report = iodelay.run(|report| {
        let prefix = if report.final_report { "final:" } else { "  " };
        match report.status {
            IoDelayStatus::BelowThreshold => {
                println!("{prefix} signal below threshold...");
            }
            IoDelayStatus::Collecting => println!("{prefix} collecting..."),
            IoDelayStatus::Resolved => {
                let ms = report.delay_frames * 1000.0 / rate;
                print!(
                    "{prefix} {:10.3} frames {:10.3} ms roundtrip latency",
                    report.delay_frames, ms
                );
                if report.error > 0.2 {
                    print!(" ??");
                }
                if report.inverted {
                    print!(" Inv");
                }
                println!();
            }
        }
    });
    if report.status != IoDelayStatus::Resolved {
        std::process::exit(1);
    }
}
