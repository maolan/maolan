#[path = "../audio_defaults.rs"]
mod audio_defaults;

use maolan_engine::{
    client::Client,
    message::{Action, Message as EngineMessage},
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use tokio::sync::mpsc::Receiver;

#[derive(Debug, Clone)]
struct TestOptions {
    plugin_path: PathBuf,
    device: String,
    input_device: Option<String>,
    duration_secs: u64,
    sample_rate_hz: i32,
    period_frames: usize,
    nperiods: usize,
    track_name: String,
    param_id: Option<u32>,
    param_value: Option<f64>,
    verbose: bool,
}

impl Default for TestOptions {
    fn default() -> Self {
        Self {
            plugin_path: PathBuf::new(),
            device: "/dev/dsp6".to_string(),
            input_device: Some("/dev/dsp6".to_string()),
            duration_secs: 5,
            sample_rate_hz: audio_defaults::SAMPLE_RATE_HZ,
            period_frames: audio_defaults::PERIOD_FRAMES,
            nperiods: audio_defaults::NPERIODS,
            track_name: "test".to_string(),
            param_id: None,
            param_value: None,
            verbose: false,
        }
    }
}

fn print_verbose(verbose: bool, _msg: &str) {
    if verbose {}
}

fn parse_options(args: impl IntoIterator<Item = String>) -> Result<TestOptions, String> {
    let mut options = TestOptions::default();
    let mut args = args.into_iter();
    let _ = args.next();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--plugin-path" => {
                options.plugin_path = PathBuf::from(
                    args.next()
                        .ok_or_else(|| "--plugin-path requires a value".to_string())?,
                );
            }
            "--device" => {
                options.device = args
                    .next()
                    .ok_or_else(|| "--device requires a value".to_string())?;
            }
            "--input-device" => {
                options.input_device = Some(
                    args.next()
                        .ok_or_else(|| "--input-device requires a value".to_string())?,
                );
            }
            "--duration-secs" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--duration-secs requires a value".to_string())?;
                options.duration_secs = value
                    .parse()
                    .map_err(|_| format!("Invalid --duration-secs value: {value}"))?;
            }
            "--sample-rate" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--sample-rate requires a value".to_string())?;
                options.sample_rate_hz = value
                    .parse()
                    .map_err(|_| format!("Invalid --sample-rate value: {value}"))?;
            }
            "--period-frames" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--period-frames requires a value".to_string())?;
                options.period_frames = value
                    .parse()
                    .map_err(|_| format!("Invalid --period-frames value: {value}"))?;
            }
            "--nperiods" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--nperiods requires a value".to_string())?;
                options.nperiods = value
                    .parse()
                    .map_err(|_| format!("Invalid --nperiods value: {value}"))?;
            }
            "--track-name" => {
                options.track_name = args
                    .next()
                    .ok_or_else(|| "--track-name requires a value".to_string())?;
            }
            "--param-id" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--param-id requires a value".to_string())?;
                options.param_id = Some(
                    value
                        .parse()
                        .map_err(|_| format!("Invalid --param-id value: {value}"))?,
                );
            }
            "--param-value" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--param-value requires a value".to_string())?;
                options.param_value = Some(
                    value
                        .parse()
                        .map_err(|_| format!("Invalid --param-value value: {value}"))?,
                );
            }
            "--verbose" | "-v" => {
                options.verbose = true;
            }
            "--help" | "-h" => {
                return Err(help_text());
            }
            other => {
                if other.starts_with('-') {
                    return Err(format!("Unknown argument: {other}\n\n{}", help_text()));
                }
                if !options.plugin_path.as_os_str().is_empty() {
                    return Err(format!(
                        "Only one plugin path may be provided.\n\n{}",
                        help_text()
                    ));
                }
                options.plugin_path = PathBuf::from(other);
            }
        }
    }
    if options.plugin_path.as_os_str().is_empty() {
        return Err(format!("--plugin-path is required.\n\n{}", help_text()));
    }
    Ok(options)
}

fn help_text() -> String {
    "Usage: maolan-test --plugin-path <PATH> [options]

Options:
  --plugin-path <PATH>     Path to CLAP plugin binary (required)
  --device <PATH>          Output OSS device (default: /dev/dsp6)
  --input-device <PATH>    Input OSS device (default: /dev/dsp6)
  --duration-secs <N>      How many seconds to run playback (default: 5)
  --sample-rate <HZ>       Sample rate (default: 48000)
  --period-frames <N>      Period size in frames (default: 1024)
  --nperiods <N>           Number of periods (default: 1)
  --track-name <NAME>      Track name (default: test)
  --param-id <ID>          Parameter ID to set after load
  --param-value <VALUE>    Parameter value to set (requires --param-id)
  --verbose, -v            Print detailed progress
  --help, -h               Show this help"
        .to_string()
}

#[derive(Debug, Default)]
struct TestState {
    hw_ready: bool,
    hw_channels: usize,
    hw_rate: usize,
    clap_instance_count: usize,
    workers_ready: usize,
    workers_total: usize,
    playing: bool,
    plugin_load_error: Option<String>,
}

async fn wait_for_condition<F>(
    rx: &mut Receiver<EngineMessage>,
    timeout: Duration,
    mut condition: F,
) -> Result<bool, String>
where
    F: FnMut(&EngineMessage) -> bool,
{
    let start = Instant::now();
    loop {
        if start.elapsed() > timeout {
            return Ok(false);
        }
        match tokio::time::timeout(Duration::from_millis(100), rx.recv()).await {
            Ok(Some(msg)) => {
                if condition(&msg) {
                    return Ok(true);
                }
            }
            Ok(None) => return Err("Engine channel closed".to_string()),
            Err(_) => continue,
        }
    }
}

fn update_state_from_message(state: &mut TestState, msg: &EngineMessage) {
    if let EngineMessage::Response(Ok(Action::HWInfo { channels, rate, .. })) = msg {
        state.hw_ready = true;
        state.hw_channels = *channels;
        state.hw_rate = *rate;
    }
    if let EngineMessage::Response(Ok(Action::Play)) = msg {
        state.playing = true;
    }
    if let EngineMessage::Response(Ok(Action::Stop)) = msg {
        state.playing = false;
    }
    if let EngineMessage::Response(Err(err)) = msg {
        state.plugin_load_error = Some(err.clone());
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = match parse_options(std::env::args()) {
        Ok(options) => options,
        Err(message) if message.starts_with("Usage: ") => {
            return Ok(());
        }
        Err(_message) => {
            std::process::exit(1);
        }
    };

    print_verbose(options.verbose, "Starting maolan engine...");
    let client = Client::default();
    let mut rx = client.subscribe().await;

    let mut state = TestState::default();

    print_verbose(
        options.verbose,
        &format!("Opening audio device '{}'...", options.device),
    );
    client
        .send(EngineMessage::Request(Action::OpenAudioDevice {
            device: options.device.clone(),
            input_device: options.input_device.clone(),
            sample_rate_hz: options.sample_rate_hz,
            bits: audio_defaults::BIT_DEPTH as i32,
            exclusive: false,
            period_frames: options.period_frames,
            nperiods: options.nperiods,
            sync_mode: audio_defaults::SYNC_MODE,
            actual_period_frames: 0,
            input_channels: 0,
            output_channels: 0,
            bytes_per_frame: 0,
        }))
        .await?;

    let hw_ok = wait_for_condition(&mut rx, Duration::from_secs(10), |msg| {
        update_state_from_message(&mut state, msg);
        state.hw_ready
    })
    .await?;

    if !hw_ok {
        let _ = client.send(EngineMessage::Request(Action::Quit)).await;
        std::process::exit(1);
    }

    print_verbose(
        options.verbose,
        &format!(
            "Audio ready: {} channels @ {} Hz",
            state.hw_channels, state.hw_rate
        ),
    );

    print_verbose(
        options.verbose,
        &format!("Creating track '{}'...", options.track_name),
    );
    client
        .send(EngineMessage::Request(Action::AddTrack {
            name: options.track_name.clone(),
            audio_ins: 2,
            audio_outs: 2,
            midi_ins: 1,
            midi_outs: 1,
        }))
        .await?;

    tokio::time::sleep(Duration::from_millis(100)).await;

    let plugin_path = options.plugin_path.to_string_lossy().to_string();
    print_verbose(
        options.verbose,
        &format!("Loading CLAP plugin '{}'...", plugin_path),
    );
    client
        .send(EngineMessage::Request(Action::TrackLoadClapPlugin {
            track_name: options.track_name.clone(),
            plugin_path: plugin_path.clone(),
            instance_id: Some(0),
        }))
        .await?;

    tokio::time::sleep(Duration::from_millis(500)).await;

    let plugin_loaded = wait_for_condition(&mut rx, Duration::from_secs(30), |msg| {
        update_state_from_message(&mut state, msg);
        state.clap_instance_count > 0
    })
    .await?;

    if !plugin_loaded {
        if let Some(_err) = state.plugin_load_error {}
        let _ = client.send(EngineMessage::Request(Action::Quit)).await;
        std::process::exit(1);
    }

    print_verbose(
        options.verbose,
        &format!(
            "Plugin loaded. Workers: {}/{} ready",
            state.workers_ready, state.workers_total
        ),
    );

    if let (Some(param_id), Some(param_value)) = (options.param_id, options.param_value) {
        print_verbose(
            options.verbose,
            &format!("Setting parameter {param_id} = {param_value}..."),
        );
        client
            .send(EngineMessage::Request(Action::TrackSetClapParameter {
                track_name: options.track_name.clone(),
                instance_id: 0,
                param_id,
                value: param_value,
            }))
            .await?;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    print_verbose(options.verbose, "Starting playback...");
    client
        .send(EngineMessage::Request(Action::SetClipPlaybackEnabled(true)))
        .await?;
    client.send(EngineMessage::Request(Action::Play)).await?;

    let play_ok = wait_for_condition(&mut rx, Duration::from_secs(5), |msg| {
        update_state_from_message(&mut state, msg);
        state.playing
    })
    .await?;

    if !play_ok {
        let _ = client.send(EngineMessage::Request(Action::Quit)).await;
        std::process::exit(1);
    }

    print_verbose(
        options.verbose,
        &format!("Running for {} seconds...", options.duration_secs),
    );
    tokio::time::sleep(Duration::from_secs(options.duration_secs)).await;

    print_verbose(options.verbose, "Stopping playback...");
    client.send(EngineMessage::Request(Action::Stop)).await?;

    let _stop_ok = wait_for_condition(&mut rx, Duration::from_secs(5), |msg| {
        update_state_from_message(&mut state, msg);
        !state.playing
    })
    .await?;

    tokio::time::sleep(Duration::from_millis(500)).await;

    let success = state.clap_instance_count > 0
        && state.workers_ready == state.workers_total
        && state.workers_total > 0;

    if success {
    } else {
        if state.workers_ready != state.workers_total {}
    }

    print_verbose(options.verbose, "Unloading plugin and shutting down...");
    let _ = client
        .send(EngineMessage::Request(
            Action::TrackUnloadClapPluginInstance {
                track_name: options.track_name.clone(),
                instance_id: 0,
            },
        ))
        .await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let _ = client.send(EngineMessage::Request(Action::Quit)).await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    if success {
        std::process::exit(0);
    } else {
        std::process::exit(1);
    }
}
