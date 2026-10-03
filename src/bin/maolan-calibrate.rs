//! `maolan-calibrate` — loopback latency calibration over FreeBSD OSS mmap
//! duplex, MTDM content-inferred (the engine counterpart of the root
//! `latency.c` tool).
//!
//! ```text
//! maolan-calibrate --input-device /dev/dsp5 --input-channel 8 \
//!     --output-device /dev/dsp5 --output-channel 3
//! ```
//!
//! Runs the same engine, hardware routing, and IO Delay calibration action as
//! the GUI for two seconds per supported OSS period. Saves each successful
//! calibration before proceeding to the next period.

#![cfg(target_os = "freebsd")]

use maolan_engine::{
    kind::Kind,
    message::{Action, Event, Message},
};
use nix::sys::signal::{SigHandler, Signal, signal};
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::{
    sync::mpsc::{Receiver, Sender, channel},
    time::{Duration, Instant, timeout},
};

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
    bits: usize,
    nperiods: usize,
    sync_mode: bool,
    exclusive: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut input_device = None;
    let mut input_channel = None;
    let mut output_device = None;
    let mut output_channel = None;
    let mut rate = 48_000;
    let mut gain = 1.0f32;
    let mut bits = 32;
    let mut nperiods = 2;
    let mut sync_mode = false;
    let mut exclusive = false;
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
            "--bits" => bits = value("--bits")?.parse().map_err(|_| "invalid bit depth")?,
            "--nperiods" => {
                nperiods = value("--nperiods")?
                    .parse()
                    .map_err(|_| "invalid period count")?
            }
            "--sync-mode" => sync_mode = true,
            "--exclusive" => exclusive = true,
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
            "-h" | "--help" => {
                println!(
                    "usage: maolan-calibrate --input-device <path> --input-channel <n> \
                     --output-device <path> --output-channel <n> [--rate <hz>] \
                     [--gain <f>] [--bits <8|16|24|32>] [--nperiods <n>] [--sync-mode] [--exclusive]"
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
    if !gain.is_finite() || gain <= 0.0 {
        return Err("--gain must be positive".to_string());
    }
    if rate == 0 || rate > i32::MAX as usize || nperiods == 0 || ![8, 16, 24, 32].contains(&bits) {
        return Err("invalid rate, bit depth, or period count".into());
    }
    Ok(Args {
        input_device: input_device.ok_or_else(|| "--input-device is required".to_string())?,
        input_channel,
        output_device: output_device.ok_or_else(|| "--output-device is required".to_string())?,
        output_channel,
        rate,
        gain,
        bits,
        nperiods,
        sync_mode,
        exclusive,
    })
}

fn period_options(max_channels: usize, max_buffer_bytes: usize, bits: usize) -> Vec<usize> {
    if max_channels == 0 || max_buffer_bytes == 0 {
        return Vec::new();
    }
    let frame_bytes = max_channels.max(1).saturating_mul(bits / 8);
    let min_bytes = frame_bytes.next_power_of_two();
    let max_bytes = max_buffer_bytes.min(1 << 16).max(min_bytes);
    let mut sizes = Vec::new();
    let mut bytes = min_bytes;
    while bytes <= max_bytes {
        sizes.push(bytes.div_ceil(frame_bytes).max(1).next_power_of_two());
        let Some(next) = bytes.checked_mul(2) else {
            break;
        };
        bytes = next;
    }
    sizes.sort_unstable();
    sizes.dedup();
    sizes
}

fn save_calibration(args: &Args, period: usize, input: usize, output: usize) -> Result<(), String> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .ok_or_else(|| "cannot determine home directory".to_string())?;
    let path = std::path::PathBuf::from(home).join(".config/maolan/daw/config.toml");
    save_calibration_to_path(args, period, input, output, &path)
}

fn save_calibration_to_path(
    args: &Args,
    period: usize,
    input: usize,
    output: usize,
    path: &std::path::Path,
) -> Result<(), String> {
    let mut config: toml::Value = if path.exists() {
        let contents = std::fs::read_to_string(path)
            .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        toml::from_str(&contents).map_err(|e| format!("failed to parse {}: {e}", path.display()))?
    } else {
        toml::Value::Table(toml::map::Map::new())
    };
    let table = config
        .as_table_mut()
        .ok_or_else(|| "config TOML root must be a table".to_string())?;
    let records_value = table
        .entry("oss_calibrations")
        .or_insert_with(|| toml::Value::Array(Vec::new()));
    let records = records_value
        .as_array_mut()
        .ok_or_else(|| "oss_calibrations must be an array".to_string())?;
    records.retain(|entry| {
        !(entry.get("output_device_id").and_then(toml::Value::as_str) == Some(&args.output_device)
            && entry.get("input_device_id").and_then(toml::Value::as_str)
                == Some(&args.input_device)
            && entry.get("period_frames").and_then(toml::Value::as_integer) == Some(period as i64)
            && entry
                .get("sample_rate_hz")
                .and_then(toml::Value::as_integer)
                == Some(args.rate as i64)
            && (entry.get("measurement_path").and_then(toml::Value::as_str)
                != Some("engine_io_delay_v1")
                || (entry.get("bits").and_then(toml::Value::as_integer) == Some(args.bits as i64)
                    && entry.get("nperiods").and_then(toml::Value::as_integer)
                        == Some(args.nperiods as i64)
                    && entry.get("sync_mode").and_then(toml::Value::as_bool)
                        == Some(args.sync_mode)
                    && entry.get("exclusive").and_then(toml::Value::as_bool)
                        == Some(args.exclusive))))
    });
    let mut record = toml::map::Map::new();
    record.insert(
        "measurement_path".into(),
        toml::Value::String("engine_io_delay_v1".into()),
    );
    record.insert("bits".into(), toml::Value::Integer(args.bits as i64));
    record.insert(
        "nperiods".into(),
        toml::Value::Integer(args.nperiods as i64),
    );
    record.insert("sync_mode".into(), toml::Value::Boolean(args.sync_mode));
    record.insert("exclusive".into(), toml::Value::Boolean(args.exclusive));
    record.insert(
        "input_device_id".into(),
        toml::Value::String(args.input_device.clone()),
    );
    record.insert(
        "output_device_id".into(),
        toml::Value::String(args.output_device.clone()),
    );
    record.insert("period_frames".into(), toml::Value::Integer(period as i64));
    record.insert(
        "sample_rate_hz".into(),
        toml::Value::Integer(args.rate as i64),
    );
    record.insert(
        "input_latency_frames".into(),
        toml::Value::Integer(input as i64),
    );
    record.insert(
        "output_latency_frames".into(),
        toml::Value::Integer(output as i64),
    );
    records.push(toml::Value::Table(record));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }
    let contents = toml::to_string_pretty(&config).map_err(|e| e.to_string())?;
    std::fs::write(path, contents).map_err(|e| format!("failed to write {}: {e}", path.display()))
}

/// Commands are serialized so an error or acknowledgement belongs to this request.
async fn request(
    client: &Sender<Message>,
    rx: &mut Receiver<Message>,
    action: Action,
) -> Result<Action, String> {
    let expected = std::mem::discriminant(&action);
    client
        .send(Message::Request(action))
        .await
        .map_err(|e| e.to_string())?;
    timeout(Duration::from_secs(15), async {
        loop {
            match rx.recv().await {
                Some(Message::Response(Err(error))) => return Err(error),
                Some(Message::Response(Ok(action)))
                    if std::mem::discriminant(&action) == expected =>
                {
                    return Ok(action);
                }
                None => return Err("engine disconnected".into()),
                _ => {}
            }
        }
    })
    .await
    .map_err(|_| "engine request timed out".to_string())?
}

async fn measure_period(
    client: &Sender<Message>,
    rx: &mut Receiver<Message>,
    args: &Args,
    period: usize,
    stop: &AtomicBool,
) -> Result<(usize, usize), String> {
    request(
        client,
        rx,
        Action::IoDelayConfigure {
            enabled: false,
            gain: args.gain,
        },
    )
    .await?;
    let opened = request(
        client,
        rx,
        Action::OpenAudioDevice {
            device: args.output_device.clone(),
            input_device: Some(args.input_device.clone()),
            sample_rate_hz: args.rate as i32,
            bits: args.bits as i32,
            exclusive: args.exclusive,
            period_frames: period,
            nperiods: args.nperiods,
            sync_mode: args.sync_mode,
            io_latency_calibration: None,
            actual_period_frames: 0,
            input_channels: 0,
            output_channels: 0,
            bytes_per_frame: 0,
            ring_buffer_multiplier: 0,
            auto_open_midi_devices: false,
        },
    )
    .await?;
    if let Action::OpenAudioDevice {
        input_channels,
        output_channels,
        actual_period_frames,
        bits: actual_bits,
        sample_rate_hz,
        ..
    } = opened
    {
        if actual_bits != args.bits as i32 || sample_rate_hz != args.rate as i32 {
            return Err(format!(
                "device negotiated {actual_bits}-bit / {sample_rate_hz} Hz; rerun with those settings"
            ));
        }
        if args.input_channel > input_channels || args.output_channel > output_channels {
            return Err(format!(
                "channel out of range: device has {input_channels} inputs and {output_channels} outputs"
            ));
        }
        println!(
            "Measuring period {period} (actual {actual_period_frames}) frames: output {} -> input {}",
            args.output_channel, args.input_channel
        );
    }
    request(
        client,
        rx,
        Action::IoDelayConfigure {
            enabled: true,
            gain: args.gain,
        },
    )
    .await?;
    request(
        client,
        rx,
        Action::IoDelayAddMeasurement {
            measurement_id: 1,
            gain: args.gain,
        },
    )
    .await?;
    request(
        client,
        rx,
        Action::Connect {
            from_track: "hw:in".into(),
            from_port: args.input_channel - 1,
            to_track: "iodelay:measurement:1".into(),
            to_port: 0,
            kind: Kind::Audio,
        },
    )
    .await?;
    request(
        client,
        rx,
        Action::Connect {
            from_track: "iodelay".into(),
            from_port: 0,
            to_track: "hw:out".into(),
            to_port: args.output_channel - 1,
            kind: Kind::Audio,
        },
    )
    .await?;
    // Opening the device does not start continuous engine processing. Pause
    // enables monitoring with the timeline stopped, so IO Delay runs and its
    // calibration action remains permitted (Play would advance the timeline).
    request(client, rx, Action::Pause).await?;
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut tick = tokio::time::interval(Duration::from_millis(25));
    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => break,
            _ = tick.tick() => if stop.load(Ordering::Relaxed) { return Err("interrupted".into()); },
            message = rx.recv() => match message {
                Some(Message::Response(Err(error))) => return Err(error),
                None => return Err("engine disconnected".into()),
                _ => {}
            }
        }
    }
    client
        .send(Message::Request(Action::IoDelayCalibrate {
            measurement_id: 1,
        }))
        .await
        .map_err(|e| e.to_string())?;
    timeout(Duration::from_secs(10), async {
        loop {
            match rx.recv().await {
                Some(Message::Event(Event::IoDelayCalibrated { measurement_id: 1, frames, playback_lead, record_back })) => {
                    println!("  {frames} frames roundtrip: play ahead {playback_lead}, record back {record_back}");
                    return Ok((record_back, playback_lead));
                }
                Some(Message::Response(Err(error))) => return Err(error),
                None => return Err("engine disconnected".into()),
                _ => {}
            }
        }
    }).await.map_err(|_| "IO Delay calibration timed out".to_string())?
}

async fn calibrate(args: &Args, stop: &AtomicBool) -> Result<(), String> {
    let devices = maolan_engine::audio_devices::discover_freebsd_audio_devices();
    let input = devices
        .iter()
        .find(|d| d.id == args.input_device)
        .ok_or("input device absent from /dev/sndstat")?;
    let output = devices
        .iter()
        .find(|d| d.id == args.output_device)
        .ok_or("output device absent from /dev/sndstat")?;
    let in_periods = period_options(input.max_channels, input.max_buffer_bytes, args.bits);
    let periods: Vec<_> = period_options(output.max_channels, output.max_buffer_bytes, args.bits)
        .into_iter()
        .filter(|p| in_periods.contains(p))
        .collect();
    if periods.is_empty() {
        return Err("devices have no shared supported periods".into());
    }
    let mut saved = 0;
    let mut failure = None;
    for period in periods {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let (client, handle, _, _, _) = maolan_engine::init();
        let (events, mut rx) = channel(1024);
        client
            .send(Message::Channel(events))
            .await
            .map_err(|e| e.to_string())?;
        let result = measure_period(&client, &mut rx, args, period, stop).await;
        let cleanup = async {
            request(&client, &mut rx, Action::Quit).await?;
            // Quit acknowledges closed OSS descriptors before joining all workers.
            // This barrier waits for the full shutdown handler to finish.
            request(&client, &mut rx, Action::ClearHistory).await?;
            Ok::<(), String>(())
        }
        .await;
        handle.abort();
        let _ = handle.await;
        cleanup?;
        match result {
            Ok((input, output)) => {
                if let Err(error) = save_calibration(args, period, input, output) {
                    failure = Some(error);
                    break;
                }
                saved += 1;
                println!("  calibration for {period} frames saved");
            }
            Err(error) => eprintln!("maolan-calibrate: period {period}: {error}"),
        }
    }
    if let Some(error) = failure {
        return Err(error);
    }
    if saved == 0 {
        return Err("no period was calibrated".into());
    }
    println!("Saved {saved} OSS calibrations to ~/.config/maolan/daw/config.toml");
    Ok(())
}

#[tokio::main]
async fn main() {
    maolan_engine::enable_flush_denormals_to_zero();
    let args = match parse_args() {
        Ok(args) => args,
        Err(error) => {
            eprintln!("maolan-calibrate: {error}");
            std::process::exit(2);
        }
    };
    let stop = Arc::new(AtomicBool::new(false));
    let _ = STOP.set(stop.clone());
    for sig in [Signal::SIGINT, Signal::SIGTERM] {
        if let Err(error) = unsafe { signal(sig, SigHandler::Handler(handle_signal)) } {
            eprintln!("maolan-calibrate: cannot install {sig} handler: {error}");
        }
    }
    if let Err(error) = calibrate(&args, &stop).await {
        eprintln!("maolan-calibrate: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_engine_totals_preserve_settings_and_replace_legacy_calibration() {
        let dir = std::env::temp_dir().join(format!(
            "maolan-calibration-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            r#"font_size = 16.0
[[oss_calibrations]]
input_device_id = "/dev/dsp5"
output_device_id = "/dev/dsp5"
period_frames = 512
sample_rate_hz = 48000
input_latency_frames = 279
output_latency_frames = 280
"#,
        )
        .unwrap();
        let args = Args {
            input_device: "/dev/dsp5".into(),
            output_device: "/dev/dsp5".into(),
            input_channel: 8,
            output_channel: 3,
            rate: 48000,
            gain: 1.0,
            bits: 32,
            nperiods: 2,
            sync_mode: false,
            exclusive: false,
        };
        save_calibration_to_path(&args, 512, 615, 616, &path).unwrap();
        save_calibration_to_path(&args, 1024, 900, 901, &path).unwrap();
        save_calibration_to_path(&args, 512, 614, 615, &path).unwrap();
        let value: toml::Value = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["font_size"].as_float(), Some(16.0));
        let rows = value["oss_calibrations"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        let row = rows
            .iter()
            .find(|r| r["period_frames"].as_integer() == Some(512))
            .unwrap();
        assert_eq!(row["measurement_path"].as_str(), Some("engine_io_delay_v1"));
        assert_eq!(row["input_latency_frames"].as_integer(), Some(614));
        assert_eq!(row["output_latency_frames"].as_integer(), Some(615));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
