//! `maolan-calibrate` — loopback latency calibration using the engine's
//! IO Delay measurement path.
//!
//! ```text
//! maolan-calibrate --input-device <device-id> --input-channel 1 \
//!     --output-device <device-id> --output-channel 1
//! ```
//!
//! Runs the same engine, hardware routing, and IO Delay calibration action as
//! the GUI for two seconds per selected device period. Saves each successful
//! calibration before proceeding to the next period.

mod calibrator {

    use maolan_engine::{
        kind::Kind,
        message::{Action, Event, Message},
    };
    #[cfg(unix)]
    use nix::sys::signal::{SigHandler, Signal, signal};
    use std::sync::Arc;
    #[cfg(unix)]
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::{
        sync::mpsc::{Receiver, Sender, channel},
        time::{Duration, Instant, timeout},
    };

    /// Bridge between the C signal handler and the options' stop flag.
    #[cfg(unix)]
    static STOP: OnceLock<Arc<AtomicBool>> = OnceLock::new();

    #[cfg(unix)]
    extern "C" fn handle_signal(_sig: i32) {
        if let Some(flag) = STOP.get() {
            flag.store(true, Ordering::Relaxed);
        }
    }

    /// WASAPI device enumeration and period-range probing. Mirrors the engine's
    /// `hw/wasapi.rs` format negotiation so the periods offered here are the
    /// periods the engine can actually open.
    #[cfg(target_os = "windows")]
    mod wasapi_probe {
        use windows::Win32::Devices::Properties::DEVPKEY_Device_FriendlyName;
        use windows::Win32::Media::Audio::{
            AUDCLNT_SHAREMODE_EXCLUSIVE, AUDCLNT_SHAREMODE_SHARED, DEVICE_STATE_ACTIVE, EDataFlow,
            IAudioClient, IAudioClient3, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
            WAVEFORMATEX, WAVEFORMATEXTENSIBLE, eConsole, eRender,
        };
        use windows::Win32::Media::Multimedia::KSDATAFORMAT_SUBTYPE_IEEE_FLOAT;
        use windows::Win32::System::Com::StructuredStorage::PropVariantClear;
        use windows::Win32::System::Com::{
            CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
            CoUninitialize, STGM_READ,
        };
        use windows::Win32::System::Variant::VT_LPWSTR;
        use windows::core::{Interface, PWSTR};

        const REFTIME_PER_SEC: i64 = 10_000_000;
        const WAVE_FORMAT_EXTENSIBLE_TAG: u16 = 0xFFFE;

        /// Only true when this call actually initialized COM (S_OK); S_FALSE
        /// and RPC_E_CHANGED_MODE mean the thread already had an apartment.
        pub struct ComApartment {
            initialized: bool,
        }

        impl ComApartment {
            pub fn new() -> Option<Self> {
                let code = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.0;
                if code == 0 {
                    Some(Self { initialized: true })
                } else if code == 1 || code == -2_147_417_850 {
                    Some(Self { initialized: false })
                } else {
                    None
                }
            }
        }

        impl Drop for ComApartment {
            fn drop(&mut self) {
                if self.initialized {
                    unsafe {
                        CoUninitialize();
                    }
                }
            }
        }

        pub struct ListedDevice {
            pub id: String,
            pub channels: usize,
            pub mix_rate: u32,
        }

        fn create_enumerator() -> Result<IMMDeviceEnumerator, String> {
            unsafe {
                CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL)
                    .map_err(|e| format!("Failed to create WASAPI device enumerator: {e}"))
            }
        }

        fn friendly_name(device: &IMMDevice) -> Option<String> {
            // SAFETY: `device` is a live COM reference; the property store and
            // the returned PROPVARIANT are released/cleared before returning.
            let store = unsafe { device.OpenPropertyStore(STGM_READ) }.ok()?;
            let mut value =
                unsafe { store.GetValue(&DEVPKEY_Device_FriendlyName as *const _ as *const _) }
                    .ok()?;
            let variant = unsafe { &value.Anonymous.Anonymous };
            if variant.vt != VT_LPWSTR {
                unsafe {
                    let _ = PropVariantClear(&mut value);
                }
                return None;
            }
            // SAFETY: the variant type was checked to be VT_LPWSTR.
            let text = unsafe { pwstr_to_string(variant.Anonymous.pwszVal) };
            unsafe {
                let _ = PropVariantClear(&mut value);
            }
            Some(text)
        }

        fn pwstr_to_string(value: PWSTR) -> String {
            if value.is_null() {
                return String::new();
            }
            let mut len = 0usize;
            // SAFETY: `value` points at a NUL-terminated wide string we only read.
            unsafe {
                while *value.0.add(len) != 0 {
                    len += 1;
                }
                String::from_utf16_lossy(std::slice::from_raw_parts(value.0, len))
            }
        }

        /// Lists active endpoints for `flow` as `wasapi:<friendly name>` ids
        /// with the channel count of the device's current mix format.
        pub fn list_devices(flow: EDataFlow) -> Result<Vec<ListedDevice>, String> {
            let enumerator = create_enumerator()?;
            let collection = unsafe { enumerator.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE) }
                .map_err(|e| format!("Failed to enumerate WASAPI endpoints: {e}"))?;
            let count = unsafe { collection.GetCount() }
                .map_err(|e| format!("Failed to count WASAPI endpoints: {e}"))?;
            let mut out = Vec::new();
            for idx in 0..count {
                let Ok(device) = (unsafe { collection.Item(idx) }) else {
                    continue;
                };
                let Some(name) = friendly_name(&device) else {
                    continue;
                };
                let (channels, mix_rate) = device
                    .activate_client()
                    .and_then(|client| mix_format(&client))
                    .unwrap_or((0, 0));
                out.push(ListedDevice {
                    id: format!("wasapi:{name}"),
                    channels: channels as usize,
                    mix_rate,
                });
            }
            out.sort_by(|a, b| a.id.cmp(&b.id));
            out.dedup_by(|a, b| a.id == b.id);
            Ok(out)
        }

        trait DeviceExt {
            fn activate_client(&self) -> Result<IAudioClient, String>;
        }

        impl DeviceExt for IMMDevice {
            fn activate_client(&self) -> Result<IAudioClient, String> {
                unsafe {
                    self.Activate::<IAudioClient>(CLSCTX_ALL, None)
                        .map_err(|e| format!("Failed to activate WASAPI client: {e}"))
                }
            }
        }

        /// Finds an endpoint by calibrate-style device id (`wasapi:<name>`,
        /// plain friendly name, or `default`). Matches the engine's
        /// exact-then-substring resolution in `hw/wasapi.rs`.
        fn find_device(
            enumerator: &IMMDeviceEnumerator,
            flow: EDataFlow,
            device_id: &str,
        ) -> Result<IMMDevice, String> {
            let requested = device_id.strip_prefix("wasapi:").unwrap_or(device_id);
            if requested.is_empty() || requested == "default" {
                return unsafe { enumerator.GetDefaultAudioEndpoint(flow, eConsole) }
                    .map_err(|e| format!("No default WASAPI endpoint: {e}"));
            }
            let collection = unsafe { enumerator.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE) }
                .map_err(|e| format!("Failed to enumerate WASAPI endpoints: {e}"))?;
            let count = unsafe { collection.GetCount() }.unwrap_or(0);
            let mut substring_match = None;
            for idx in 0..count {
                let Ok(device) = (unsafe { collection.Item(idx) }) else {
                    continue;
                };
                let Some(name) = friendly_name(&device) else {
                    continue;
                };
                if name == requested {
                    return Ok(device);
                }
                if substring_match.is_none() && name.contains(requested) {
                    substring_match = Some(device);
                }
            }
            substring_match.ok_or_else(|| {
                format!("WASAPI endpoint {device_id:?} not found (see --list-devices)")
            })
        }

        /// (channels, sample rate) of the device's current mix format — the
        /// channel count the engine will stream with.
        fn mix_format(client: &IAudioClient) -> Result<(u16, u32), String> {
            // SAFETY: `client` is a live COM reference; GetMixFormat returns a
            // CoTaskMem-allocated WAVEFORMATEX which we free below.
            let ptr = unsafe { client.GetMixFormat() }
                .map_err(|e| format!("Failed to query WASAPI mix format: {e}"))?;
            if ptr.is_null() {
                return Err("WASAPI returned a null mix format".to_string());
            }
            // SAFETY: non-null pointer owned by us until freed.
            let mix = unsafe { *ptr };
            // SAFETY: `ptr` came from GetMixFormat and is freed exactly once here.
            unsafe {
                CoTaskMemFree(Some(ptr.cast()));
            }
            Ok((mix.nChannels.max(1), mix.nSamplesPerSec))
        }

        /// The engine's stream format: float32 WAVEFORMATEXTENSIBLE at
        /// `rate` with the mix format's channel count.
        fn float_mix_format(channels: u16, rate: u32) -> WAVEFORMATEXTENSIBLE {
            let channels = u32::from(channels).max(1);
            let block_align = channels.saturating_mul(4) as u16;
            let mut format = WAVEFORMATEXTENSIBLE::default();
            format.Format.wFormatTag = WAVE_FORMAT_EXTENSIBLE_TAG;
            format.Format.nChannels = channels as u16;
            format.Format.nSamplesPerSec = rate;
            format.Format.nAvgBytesPerSec = rate.saturating_mul(u32::from(block_align));
            format.Format.nBlockAlign = block_align;
            format.Format.wBitsPerSample = 32;
            format.Format.cbSize = (std::mem::size_of::<WAVEFORMATEXTENSIBLE>()
                - std::mem::size_of::<WAVEFORMATEX>()) as u16;
            format.Samples.wValidBitsPerSample = 32;
            format.dwChannelMask = if channels <= 32 {
                (1_u32 << channels) - 1
            } else {
                0
            };
            format.SubFormat = KSDATAFORMAT_SUBTYPE_IEEE_FLOAT;
            format
        }

        fn is_format_supported(client: &IAudioClient, format: &WAVEFORMATEXTENSIBLE) -> bool {
            let mut closest: *mut WAVEFORMATEX = std::ptr::null_mut();
            // SAFETY: `format` is a plain stack struct valid for the call;
            // `closest` receives an optional CoTaskMem allocation freed below.
            let supported = unsafe {
                client.IsFormatSupported(
                    AUDCLNT_SHAREMODE_SHARED,
                    &format.Format,
                    Some(&mut closest as *mut *mut WAVEFORMATEX),
                )
            };
            if !closest.is_null() {
                // SAFETY: non-null `closest` was allocated by IsFormatSupported.
                unsafe {
                    CoTaskMemFree(Some(closest.cast()));
                }
            }
            supported.is_ok()
        }

        /// (default, fundamental, min, max) engine periods in frames for the
        /// float32 format at `rate`, via `IAudioClient3`. Falls back to the
        /// shared-mode device period (both in frames).
        pub fn period_range(
            client: &IAudioClient,
            format: &WAVEFORMATEXTENSIBLE,
            rate: u32,
        ) -> Result<(u32, u32, u32, u32), String> {
            let client3 = client
                .cast::<IAudioClient3>()
                .map_err(|_| "WASAPI shared low-latency mode requires IAudioClient3".to_string())?;
            let (mut default, mut fundamental, mut min, mut max) = (0, 0, 0, 0);
            unsafe {
                client3
                    .GetSharedModeEnginePeriod(
                        &format.Format,
                        &mut default,
                        &mut fundamental,
                        &mut min,
                        &mut max,
                    )
                    .map_err(|e| format!("Failed to query WASAPI engine periods: {e}"))?;
            }
            if min == 0 || max < min {
                // Very old stacks lack IAudioClient3 period granularity; use
                // the shared device period as [min, max].
                let (mut default_hns, mut min_hns) = (0_i64, 0_i64);
                unsafe {
                    client
                        .GetDevicePeriod(Some(&mut default_hns), Some(&mut min_hns))
                        .map_err(|e| format!("Failed to query WASAPI device period: {e}"))?;
                }
                let default = hns_to_frames(default_hns, rate).max(1);
                let min = hns_to_frames(min_hns, rate).max(1);
                return Ok((default, 0, min, default.max(min)));
            }
            Ok((default, fundamental, min, max))
        }

        fn hns_to_frames(hns: i64, rate: u32) -> u32 {
            ((hns * i64::from(rate) + REFTIME_PER_SEC - 1) / REFTIME_PER_SEC) as u32
        }

        pub struct ProbedDevice {
            pub channels: usize,
            pub default_period: u32,
            pub fundamental_period: u32,
            pub min_period: u32,
            pub max_period: u32,
        }

        /// Resolves `device_id` on `flow` and probes the period range the
        /// engine would negotiate for it at `rate`.
        pub fn probe_device(
            device_id: &str,
            flow: EDataFlow,
            rate: u32,
        ) -> Result<ProbedDevice, String> {
            let enumerator = create_enumerator()?;
            let device = find_device(&enumerator, flow, device_id)?;
            let client = device.activate_client()?;
            let (channels, _mix_rate) = mix_format(&client)?;
            let format = float_mix_format(channels, rate);
            if !is_format_supported(&client, &format) {
                return Err(format!(
                    "WASAPI endpoint {device_id:?} does not support 32-bit float at {rate} Hz"
                ));
            }
            let (default_period, fundamental_period, min_period, max_period) =
                period_range(&client, &format, rate)?;
            Ok(ProbedDevice {
                channels: u32::from(channels) as usize,
                default_period,
                fundamental_period,
                min_period,
                max_period,
            })
        }

        fn exclusive_is_supported(client: &IAudioClient, format: &WAVEFORMATEXTENSIBLE) -> bool {
            // Exclusive IsFormatSupported takes no "closest" output pointer.
            unsafe { client.IsFormatSupported(AUDCLNT_SHAREMODE_EXCLUSIVE, &format.Format, None) }
                .is_ok()
        }

        /// Whether the endpoint accepts the engine's float32 stream in
        /// exclusive mode, trying the positional channel mask first and
        /// DIRECTOUT second (mirrors the engine's `build_float_mix_format`).
        pub fn exclusive_format_supported(device_id: &str, rate: u32) -> Result<bool, String> {
            let enumerator = create_enumerator()?;
            let device = find_device(&enumerator, eRender, device_id)?;
            let client = device.activate_client()?;
            let (channels, _) = mix_format(&client)?;
            let mut format = float_mix_format(channels, rate);
            if exclusive_is_supported(&client, &format) {
                return Ok(true);
            }
            format.dwChannelMask = 0;
            Ok(exclusive_is_supported(&client, &format))
        }
    }

    #[derive(Clone)]
    struct Args {
        input_device: String,
        input_channel: usize,
        output_device: String,
        output_channel: usize,
        rate: usize,
        gain: f32,
        bits: usize,
        nperiods: usize,
        periods: Option<Vec<usize>>,
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
        let mut periods = None;
        let mut sync_mode = false;
        // Exclusive mode is the default: it keeps other applications out of
        // the device so the measurement reflects only the hardware path.
        let mut exclusive = true;
        let mut it = std::env::args().skip(1);
        while let Some(arg) = it.next() {
            let mut value =
                |name: &str| it.next().ok_or_else(|| format!("{name} requires a value"));
            match arg.as_str() {
                "--input-device" => input_device = Some(value("--input-device")?),
                "--input-channel" => {
                    input_channel =
                        Some(value("--input-channel")?.parse().map_err(|_| {
                            "--input-channel must be a positive integer".to_string()
                        })?)
                }
                "--output-device" => output_device = Some(value("--output-device")?),
                "--output-channel" => {
                    output_channel =
                        Some(value("--output-channel")?.parse().map_err(|_| {
                            "--output-channel must be a positive integer".to_string()
                        })?)
                }
                "--bits" => bits = value("--bits")?.parse().map_err(|_| "invalid bit depth")?,
                "--nperiods" => {
                    nperiods = value("--nperiods")?
                        .parse()
                        .map_err(|_| "invalid period count")?
                }
                "--periods" => periods = Some(parse_periods(&value("--periods")?)?),
                "--sync-mode" => sync_mode = true,
                "--exclusive" => exclusive = true,
                "--shared" => exclusive = false,
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
                        "usage: maolan-calibrate --input-device <id> --input-channel <n> \
                     --output-device <path> --output-channel <n> [--rate <hz>] \
                     [--gain <f>] [--bits <8|16|24|32>] [--nperiods <n>] \
                     [--periods <n>[,<n>...]] [--sync-mode] [--exclusive|--shared]\n\
                     (exclusive mode is the default; --shared allows other\n\
                     applications to use the device during calibration)\n\
                     maolan-calibrate --list-devices"
                    );
                    std::process::exit(0);
                }
                other => return Err(format!("unknown argument {other:?}")),
            }
        }
        let input_channel =
            input_channel.ok_or_else(|| "--input-channel is required".to_string())?;
        let output_channel =
            output_channel.ok_or_else(|| "--output-channel is required".to_string())?;
        if input_channel == 0 || output_channel == 0 {
            return Err("channel indexes are 1-based and must be positive".to_string());
        }
        if !gain.is_finite() || gain <= 0.0 {
            return Err("--gain must be positive".to_string());
        }
        if rate == 0
            || rate > i32::MAX as usize
            || nperiods == 0
            || ![8, 16, 24, 32].contains(&bits)
        {
            return Err("invalid rate, bit depth, or period count".into());
        }
        Ok(Args {
            input_device: input_device.ok_or_else(|| "--input-device is required".to_string())?,
            input_channel,
            output_device: output_device
                .ok_or_else(|| "--output-device is required".to_string())?,
            output_channel,
            rate,
            gain,
            bits,
            nperiods,
            periods,
            sync_mode,
            exclusive,
        })
    }

    fn parse_periods(value: &str) -> Result<Vec<usize>, String> {
        let mut periods = Vec::new();
        for item in value.split(',') {
            let period = item.trim().parse::<usize>().map_err(|_| {
                format!(
                    "--periods must be a comma-separated list of positive frame counts: {value:?}"
                )
            })?;
            if period == 0 {
                return Err("--periods values must be positive".into());
            }
            if periods.contains(&period) {
                return Err(format!("--periods contains duplicate period {period}"));
            }
            periods.push(period);
        }
        if periods.is_empty() {
            return Err("--periods requires at least one period".into());
        }
        Ok(periods)
    }

    #[cfg(target_os = "freebsd")]
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

    fn save_calibration(
        args: &Args,
        period: usize,
        input: usize,
        output: usize,
    ) -> Result<(), String> {
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
            toml::from_str(&contents)
                .map_err(|e| format!("failed to parse {}: {e}", path.display()))?
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
            !(entry.get("output_device_id").and_then(toml::Value::as_str)
                == Some(&args.output_device)
                && entry.get("input_device_id").and_then(toml::Value::as_str)
                    == Some(&args.input_device)
                && entry.get("period_frames").and_then(toml::Value::as_integer)
                    == Some(period as i64)
                && entry
                    .get("sample_rate_hz")
                    .and_then(toml::Value::as_integer)
                    == Some(args.rate as i64)
                && (entry.get("measurement_path").and_then(toml::Value::as_str)
                    != Some("engine_io_delay_v1")
                    || (entry.get("bits").and_then(toml::Value::as_integer)
                        == Some(args.bits as i64)
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
        std::fs::write(path, contents)
            .map_err(|e| format!("failed to write {}: {e}", path.display()))
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

    /// Wait for the engine's terminal Quit acknowledgement before aborting
    /// its event loop. Other responses can be in flight from hardware workers
    /// and do not mean that shutdown has completed.
    async fn shutdown_engine(
        client: &Sender<Message>,
        rx: &mut Receiver<Message>,
        handle: tokio::task::JoinHandle<()>,
    ) -> Result<(), String> {
        let shutdown = async {
            client
                .send(Message::Request(Action::Quit))
                .await
                .map_err(|error| error.to_string())?;
            timeout(Duration::from_secs(30), async {
                let mut last_error = None;
                loop {
                    match rx.recv().await {
                        Some(Message::Response(Ok(action)))
                            if std::mem::discriminant(&action)
                                == std::mem::discriminant(&Action::Quit) =>
                        {
                            return Ok(());
                        }
                        Some(Message::Response(Err(error))) => last_error = Some(error),
                        None => {
                            return Err(last_error
                                .unwrap_or_else(|| "engine disconnected during shutdown".into()));
                        }
                        _ => {}
                    }
                }
            })
            .await
            .map_err(|_| "engine shutdown timed out".to_string())?
        }
        .await;
        handle.abort();
        let _ = handle.await;
        shutdown
    }

    async fn measure_period(
        client: &Sender<Message>,
        rx: &mut Receiver<Message>,
        args: &Args,
        period: usize,
        stop: &AtomicBool,
    ) -> Result<(usize, usize, usize), String> {
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
                // ALSA opens the requested channel width. Requesting the
                // selected port count makes higher numbered hardware channels
                // available to the IO Delay graph.
                input_channels: args.input_channel,
                output_channels: args.output_channel,
                bytes_per_frame: 0,
                ring_buffer_multiplier: 0,
                auto_open_midi_devices: false,
            },
        )
        .await?;
        let mut negotiated_period = period;
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
            negotiated_period = actual_period_frames;
            if actual_period_frames != period {
                // WASAPI clamps and aligns the request to the device's engine
                // period range; the record is keyed by the negotiated size so
                // it matches whatever period the DAW requests later.
                #[cfg(not(target_os = "windows"))]
                return Err(format!(
                    "device negotiated {actual_period_frames} frames for requested period {period}; skipping to avoid saving a calibration under the wrong size"
                ));
                #[cfg(target_os = "windows")]
                println!(
                    "  note: device negotiated {actual_period_frames} frames for requested period {period}"
                );
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
        let mut last_report = None;
        let mut xrun_count = None;
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut tick = tokio::time::interval(Duration::from_millis(25));
        loop {
            tokio::select! {
                _ = tokio::time::sleep_until(deadline) => break,
                _ = tick.tick() => if stop.load(Ordering::Relaxed) { return Err("interrupted".into()); },
                message = rx.recv() => match message {
                    Some(Message::Response(Err(error))) => return Err(error),
                    Some(Message::Event(Event::IoDelayReport { measurement_id: 1, report })) => last_report = Some(report),
                    Some(Message::Event(Event::AudioXruns { count })) => xrun_count = Some(count),
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
                    return Ok((negotiated_period, record_back, playback_lead));
                }
                Some(Message::Response(Err(error))) => {
                    let report = last_report.map_or_else(
                        || "no IO Delay reports received".to_string(),
                        |report: maolan_engine::mtdm::IoDelayReport| format!(
                            "last IO Delay status {:?}, residual {:.3}, delay {:.1} frames, inverted={}",
                            report.status, report.error, report.delay_frames, report.inverted
                        ),
                    );
                    let xruns = xrun_count.map_or_else(
                        || "xrun count unavailable".to_string(),
                        |count| format!("{count} xruns"),
                    );
                    return Err(format!("{error} ({report}; {xruns})"));
                }
                Some(Message::Event(Event::IoDelayReport { measurement_id: 1, report })) => last_report = Some(report),
                Some(Message::Event(Event::AudioXruns { count })) => xrun_count = Some(count),
                None => return Err("engine disconnected".into()),
                _ => {}
            }
        }
    }).await.map_err(|_| "IO Delay calibration timed out".to_string())?
    }

    #[cfg(target_os = "freebsd")]
    fn supported_periods(args: &Args) -> Result<Vec<usize>, String> {
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
        let periods: Vec<_> =
            period_options(output.max_channels, output.max_buffer_bytes, args.bits)
                .into_iter()
                .filter(|p| in_periods.contains(p))
                .collect();
        if periods.is_empty() {
            return Err("devices have no shared supported periods".into());
        }
        Ok(periods)
    }

    #[cfg(target_os = "linux")]
    fn alsa_period_range(
        device: &str,
        direction: alsa::Direction,
        channels: usize,
        args: &Args,
    ) -> Result<(usize, usize), String> {
        use alsa::ValueOr;
        use alsa::pcm::{Access, Format, HwParams, PCM};

        let pcm = PCM::new(device, direction, false)
            .map_err(|error| format!("cannot open ALSA {direction:?} device {device}: {error}"))?;
        let params = HwParams::any(&pcm).map_err(|error| error.to_string())?;
        if params.set_access(Access::MMapInterleaved).is_err() {
            params
                .set_access(Access::RWInterleaved)
                .map_err(|error| error.to_string())?;
        }
        let formats = match args.bits {
            32 => vec![
                if cfg!(target_endian = "little") {
                    Format::S32LE
                } else {
                    Format::S32BE
                },
                if cfg!(target_endian = "little") {
                    Format::S32BE
                } else {
                    Format::S32LE
                },
                if cfg!(target_endian = "little") {
                    Format::S24LE
                } else {
                    Format::S24BE
                },
                if cfg!(target_endian = "little") {
                    Format::S24BE
                } else {
                    Format::S24LE
                },
                if cfg!(target_endian = "little") {
                    Format::S16LE
                } else {
                    Format::S16BE
                },
                if cfg!(target_endian = "little") {
                    Format::S16BE
                } else {
                    Format::S16LE
                },
                Format::S8,
            ],
            24 => vec![
                if cfg!(target_endian = "little") {
                    Format::S24LE
                } else {
                    Format::S24BE
                },
                if cfg!(target_endian = "little") {
                    Format::S24BE
                } else {
                    Format::S24LE
                },
                if cfg!(target_endian = "little") {
                    Format::S16LE
                } else {
                    Format::S16BE
                },
                if cfg!(target_endian = "little") {
                    Format::S16BE
                } else {
                    Format::S16LE
                },
                Format::S8,
            ],
            16 => vec![
                if cfg!(target_endian = "little") {
                    Format::S16LE
                } else {
                    Format::S16BE
                },
                if cfg!(target_endian = "little") {
                    Format::S16BE
                } else {
                    Format::S16LE
                },
                Format::S8,
            ],
            _ => vec![Format::S8],
        };
        if !formats
            .into_iter()
            .any(|format| params.set_format(format).is_ok())
        {
            return Err(format!(
                "ALSA {direction:?} device {device} has no supported integer format"
            ));
        }
        let actual_channels = params
            .set_channels_near(channels.max(1) as u32)
            .map_err(|error| error.to_string())?;
        if actual_channels < channels as u32 {
            return Err(format!(
                "ALSA {direction:?} device {device} supports {actual_channels} channels, but channel {channels} was requested"
            ));
        }
        params
            .set_rate(args.rate as u32, ValueOr::Nearest)
            .map_err(|error| error.to_string())?;
        let min = params
            .get_period_size_min()
            .map_err(|error| error.to_string())? as usize;
        let max = params
            .get_period_size_max()
            .map_err(|error| error.to_string())? as usize;
        if min == 0 || max < min {
            return Err(format!(
                "ALSA {direction:?} device {device} reported an invalid period range"
            ));
        }
        Ok((min, max))
    }

    #[cfg(target_os = "linux")]
    fn supported_periods(args: &Args) -> Result<Vec<usize>, String> {
        use alsa::Direction;

        let (input_min, input_max) = alsa_period_range(
            &args.input_device,
            Direction::Capture,
            args.input_channel,
            args,
        )?;
        let (output_min, output_max) = alsa_period_range(
            &args.output_device,
            Direction::Playback,
            args.output_channel,
            args,
        )?;
        let min = input_min.max(output_min);
        let max = input_max.min(output_max);
        if max < min {
            return Err(format!(
                "ALSA devices have no shared period range (capture {input_min}..{input_max}, playback {output_min}..{output_max} frames)"
            ));
        }
        println!(
            "ALSA period ranges: capture {input_min}..{input_max}, playback {output_min}..{output_max}; shared {min}..{max} frames"
        );

        // ALSA exposes a range rather than a portable list of discrete
        // periods. Probe the range edges and conventional powers of two;
        // opening the duplex device verifies each candidate and reports the
        // period the driver actually negotiated.
        let mut periods = vec![min, max];
        if let Some(mut period) = min.checked_next_power_of_two() {
            while period <= max {
                periods.push(period);
                let Some(next) = period.checked_mul(2) else {
                    break;
                };
                period = next;
            }
        }
        periods.sort_unstable();
        periods.dedup();
        Ok(periods)
    }

    #[cfg(target_os = "macos")]
    fn list_devices() {
        for device in maolan_engine::audio_devices::discover_coreaudio_audio_devices() {
            let input_periods = device
                .min_period_frames
                .zip(device.max_period_frames)
                .map(|(min, max)| format!("{min}..{max} frames"))
                .unwrap_or_else(|| "period range unknown".into());
            println!(
                "{}  {}  input={} output={}  rates={:?}  buffer={}",
                device.id,
                device.label,
                device.supports_input,
                device.supports_output,
                device.sample_rates,
                input_periods,
            );
        }
    }

    #[cfg(target_os = "freebsd")]
    fn list_devices() {
        for device in maolan_engine::audio_devices::discover_freebsd_audio_devices() {
            println!(
                "{}  {}  input={} output={}  rates={:?}  bits={:?}",
                device.id,
                device.label,
                device.supports_input,
                device.supports_output,
                device.supported_sample_rates,
                device.supported_bits,
            );
        }
    }

    #[cfg(target_os = "linux")]
    fn list_devices() {
        let Ok(pcms) = std::fs::read_to_string("/proc/asound/pcm") else {
            eprintln!("maolan-calibrate: cannot read /proc/asound/pcm; is ALSA available?");
            return;
        };
        for line in pcms.lines() {
            let Some((identity, description)) = line.split_once(':') else {
                continue;
            };
            let Some((card, device)) = identity.trim().split_once('-') else {
                continue;
            };
            let (Ok(card), Ok(device)) = (card.parse::<u32>(), device.parse::<u32>()) else {
                continue;
            };
            let has_input = description.contains("capture ");
            let has_output = description.contains("playback ");
            println!(
                "hw:{card},{device}  {}  input={} output={}",
                description.trim(),
                has_input,
                has_output,
            );
        }
    }

    #[cfg(target_os = "macos")]
    fn supported_periods(args: &Args) -> Result<Vec<usize>, String> {
        let devices = maolan_engine::audio_devices::discover_coreaudio_audio_devices();
        let input = devices
            .iter()
            .find(|device| device.id == args.input_device && device.supports_input)
            .ok_or("input device is not available in CoreAudio")?;
        let output = devices
            .iter()
            .find(|device| device.id == args.output_device && device.supports_output)
            .ok_or("output device is not available in CoreAudio")?;
        let min = output
            .min_period_frames
            .or(input.min_period_frames)
            .unwrap_or(32)
            .max(1);
        let max = output
            .max_period_frames
            .or(input.max_period_frames)
            .unwrap_or(8192)
            .max(min);
        let mut periods = vec![min];
        let mut period = min.next_power_of_two();
        while period < max {
            periods.push(period);
            let Some(next) = period.checked_mul(2) else {
                break;
            };
            period = next;
        }
        periods.push(max);
        periods.sort_unstable();
        periods.dedup();
        Ok(periods)
    }

    #[cfg(target_os = "windows")]
    fn list_devices() {
        use windows::Win32::Media::Audio::{eCapture, eRender};
        let Some(_com) = wasapi_probe::ComApartment::new() else {
            eprintln!("maolan-calibrate: cannot initialize COM");
            return;
        };
        println!("Output devices:");
        match wasapi_probe::list_devices(eRender) {
            Ok(devices) => {
                for device in devices {
                    println!(
                        "  {}  channels={} mix_rate={}",
                        device.id, device.channels, device.mix_rate
                    );
                }
            }
            Err(error) => eprintln!("maolan-calibrate: {error}"),
        }
        println!("Input devices:");
        match wasapi_probe::list_devices(eCapture) {
            Ok(devices) => {
                for device in devices {
                    println!(
                        "  {}  channels={} mix_rate={}",
                        device.id, device.channels, device.mix_rate
                    );
                }
            }
            Err(error) => eprintln!("maolan-calibrate: {error}"),
        }
        println!(
            "Pass the id (or a substring of it) to --input-device / --output-device.\n\
             The channel count is the device's current Windows format; loopback inputs\n\
             on higher-numbered channels require setting that format to enough channels."
        );
    }

    #[cfg(target_os = "windows")]
    fn supported_periods(args: &Args) -> Result<Vec<usize>, String> {
        use windows::Win32::Media::Audio::{eCapture, eRender};

        let _com = wasapi_probe::ComApartment::new().ok_or("cannot initialize COM")?;
        for (flow, device_id, direction) in [
            (eRender, args.output_device.as_str(), "output"),
            (eCapture, args.input_device.as_str(), "input"),
        ] {
            let probed = wasapi_probe::probe_device(device_id, flow, args.rate as u32)?;
            if probed.channels < args.input_channel.max(args.output_channel) {
                return Err(format!(
                    "{direction} device {device_id:?} exposes {} channels, but channel {} was requested; \
                     raise the channel count in the device's Windows format",
                    probed.channels,
                    args.input_channel.max(args.output_channel)
                ));
            }
            println!(
                "WASAPI {direction} {device_id:?}: {} channels, engine periods {}..{} frames \
                 (default {}, fundamental {})",
                probed.channels,
                probed.min_period,
                probed.max_period,
                probed.default_period,
                probed.fundamental_period
            );
        }
        // Candidate requests are powers of two; the engine negotiates each to
        // an effective period (round up, clamp to the engine range, align to
        // the fundamental), and records are saved under the negotiated size.
        // Keep the smallest request per distinct effective period.
        let mut by_effective: Vec<(usize, usize)> = Vec::new();
        let mut request = 64_usize;
        loop {
            if let Ok(effective) = maolan_engine::audio_devices::effective_period_frames(
                &args.output_device,
                request,
                args.rate as u32,
                args.exclusive,
            ) {
                match by_effective.iter_mut().find(|(eff, _)| *eff == effective) {
                    Some((_, best)) => *best = (*best).min(request),
                    None => by_effective.push((effective, request)),
                }
            }
            if request >= 8192 {
                break;
            }
            request *= 2;
        }
        if by_effective.is_empty() {
            return Err("no measurable engine periods for this device".into());
        }
        by_effective.sort_unstable();
        Ok(by_effective
            .into_iter()
            .map(|(_, request)| request)
            .collect())
    }

    async fn calibrate_in_child(
        args: &Args,
        period: usize,
        stop: &AtomicBool,
    ) -> Result<bool, String> {
        let executable = std::env::current_exe().map_err(|error| error.to_string())?;
        let mut command = tokio::process::Command::new(executable);
        command.args([
            "--input-device".to_string(),
            args.input_device.clone(),
            "--input-channel".to_string(),
            args.input_channel.to_string(),
            "--output-device".to_string(),
            args.output_device.clone(),
            "--output-channel".to_string(),
            args.output_channel.to_string(),
            "--rate".to_string(),
            args.rate.to_string(),
            "--gain".to_string(),
            args.gain.to_string(),
            "--bits".to_string(),
            args.bits.to_string(),
            "--nperiods".to_string(),
            args.nperiods.to_string(),
            "--periods".to_string(),
            period.to_string(),
        ]);
        if args.sync_mode {
            command.arg("--sync-mode");
        }
        // Children re-parse their arguments, so the mode must be passed
        // explicitly in both directions to survive the new exclusive default.
        if args.exclusive {
            command.arg("--exclusive");
        } else {
            command.arg("--shared");
        }
        let mut child = command
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| format!("cannot start calibration for period {period}: {error}"))?;
        let mut tick = tokio::time::interval(Duration::from_millis(25));
        let mut stopping = false;
        let status = loop {
            tokio::select! {
                status = child.wait() => break status.map_err(|error| error.to_string())?,
                _ = tick.tick(), if !stopping => {
                    if stop.load(Ordering::Relaxed) {
                        // SIGTERM also covers a signal delivered only to the
                        // parent. The child handles it through its normal stop
                        // flag, finishing device shutdown before we continue.
                        #[cfg(unix)]
                        if let Some(pid) = child.id() {
                            let _ = nix::sys::signal::kill(
                                nix::unistd::Pid::from_raw(pid as i32),
                                Signal::SIGTERM,
                            );
                        }
                        #[cfg(not(unix))]
                        let _ = child.kill().await;
                        stopping = true;
                    }
                }
            }
        };
        Ok(status.success())
    }

    async fn calibrate(args: &Args, stop: &AtomicBool) -> Result<(), String> {
        // Probe once up front so a device that rejects exclusive streams
        // (common for multichannel USB interfaces) doesn't waste a whole
        // sweep discovering it one period at a time.
        #[cfg(target_os = "windows")]
        let args = {
            let mut args = args.clone();
            if args.exclusive {
                // A failed COM init must not be mistaken for "exclusive
                // unsupported"; default to keeping exclusive and let the
                // sweep's fallback handle it.
                let probe = wasapi_probe::ComApartment::new()
                    .map(|_com| {
                        wasapi_probe::exclusive_format_supported(
                            &args.output_device,
                            args.rate as u32,
                        )
                    })
                    .unwrap_or(Ok(true));
                if matches!(probe, Ok(false)) {
                    eprintln!(
                        "maolan-calibrate: {} rejected exclusive mode; using shared mode",
                        args.output_device
                    );
                    args.exclusive = false;
                }
            }
            args
        };
        #[cfg(target_os = "windows")]
        let args = &args;
        match calibrate_sweep(args, stop).await {
            Ok(()) => Ok(()),
            Err(error) => {
                // Multichannel USB interfaces commonly reject WASAPI/OSS
                // exclusive streams outright; rather than leaving the user
                // with nothing, retry the whole sweep in shared mode.
                if !args.exclusive || stop.load(Ordering::Relaxed) {
                    return Err(error);
                }
                eprintln!("maolan-calibrate: {error}");
                eprintln!(
                    "maolan-calibrate: the device rejected exclusive mode; retrying in shared mode"
                );
                let mut shared_args = args.clone();
                shared_args.exclusive = false;
                calibrate_sweep(&shared_args, stop).await
            }
        }
    }

    async fn calibrate_sweep(args: &Args, stop: &AtomicBool) -> Result<(), String> {
        let periods = match &args.periods {
            Some(periods) => periods.clone(),
            None => supported_periods(args)?,
        };
        // Recreating Engine does not recreate the Tokio runtime or reset
        // process-wide/thread-local scheduling state. A fresh process per
        // period gives a sweep the same lifecycle as separate --periods runs.
        // Children receive exactly one period, so they never spawn recursively.
        let isolate_periods = periods.len() > 1;
        let mut saved = 0;
        let mut failure = None;
        for period in periods {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            if isolate_periods {
                if calibrate_in_child(args, period, stop).await? {
                    saved += 1;
                }
                continue;
            }
            let (client, handle, _, _, _) = maolan_engine::init();
            let (events, mut rx) = channel(1024);
            client
                .send(Message::Channel(events))
                .await
                .map_err(|e| e.to_string())?;
            let result = measure_period(&client, &mut rx, args, period, stop).await;
            shutdown_engine(&client, &mut rx, handle).await?;
            match result {
                Ok((negotiated, input, output)) => {
                    if let Err(error) = save_calibration(args, negotiated, input, output) {
                        failure = Some(error);
                        break;
                    }
                    saved += 1;
                    println!("  calibration for {negotiated} frames saved");
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
        println!("Saved {saved} calibrations to ~/.config/maolan/daw/config.toml");
        Ok(())
    }

    pub async fn run() {
        maolan_engine::enable_flush_denormals_to_zero();
        if std::env::args().any(|arg| arg == "--list-devices") {
            list_devices();
            return;
        }
        let args = match parse_args() {
            Ok(args) => args,
            Err(error) => {
                eprintln!("maolan-calibrate: {error}");
                std::process::exit(2);
            }
        };
        let stop = Arc::new(AtomicBool::new(false));
        #[cfg(unix)]
        {
            let _ = STOP.set(stop.clone());
            for sig in [Signal::SIGINT, Signal::SIGTERM] {
                if let Err(error) = unsafe { signal(sig, SigHandler::Handler(handle_signal)) } {
                    eprintln!("maolan-calibrate: cannot install {sig} handler: {error}");
                }
            }
        }
        #[cfg(not(unix))]
        {
            let ctrl_c_stop = stop.clone();
            tokio::spawn(async move {
                let _ = tokio::signal::ctrl_c().await;
                ctrl_c_stop.store(true, Ordering::Relaxed);
            });
        }
        #[cfg(target_os = "windows")]
        if args.bits != 32 {
            eprintln!("maolan-calibrate: WASAPI always streams 32-bit float; --bits must be 32");
            std::process::exit(2);
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
        fn periods_parse_as_positive_unique_comma_separated_frames() {
            assert_eq!(parse_periods("64, 128,512").unwrap(), [64, 128, 512]);
            assert!(parse_periods("64,,128").is_err());
            assert!(parse_periods("0,128").is_err());
            assert!(parse_periods("64,64").is_err());
        }

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
                periods: None,
                sync_mode: false,
                exclusive: false,
            };
            save_calibration_to_path(&args, 512, 615, 616, &path).unwrap();
            save_calibration_to_path(&args, 1024, 900, 901, &path).unwrap();
            save_calibration_to_path(&args, 512, 614, 615, &path).unwrap();
            let value: toml::Value =
                toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
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
}

#[tokio::main]
async fn main() {
    calibrator::run().await;
}
