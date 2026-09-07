//! Hand-rolled WASAPI device discovery for the GUI preferences
//! (replaces the `cpal` crate, which was only used for device metadata).
//!
//! The actual audio streaming is done by maolan-engine's own WASAPI backend;
//! this module only lists devices and probes which sample rates / bit depths
//! a device accepts, mirroring what cpal's `supported_output_configs` did.

#[cfg(target_os = "windows")]
use windows::Win32::Devices::Properties::DEVPKEY_Device_FriendlyName;
#[cfg(target_os = "windows")]
use windows::Win32::Media::Audio::{
    AUDCLNT_SHAREMODE_SHARED, DEVICE_STATE_ACTIVE, IAudioClient, IMMDevice, IMMDeviceCollection,
    IMMDeviceEnumerator, MMDeviceEnumerator, WAVE_FORMAT_PCM, WAVEFORMATEX, eCapture, eConsole,
    eRender,
};
#[cfg(target_os = "windows")]
use windows::Win32::Media::Multimedia::WAVE_FORMAT_IEEE_FLOAT;
#[cfg(target_os = "windows")]
use windows::Win32::System::Com::StructuredStorage::PropVariantClear;
#[cfg(target_os = "windows")]
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
    CoUninitialize, STGM_READ,
};
#[cfg(target_os = "windows")]
use windows::Win32::System::Variant::VT_LPWSTR;
#[cfg(target_os = "windows")]
use windows::core::PWSTR;

#[cfg(target_os = "windows")]
const RPC_E_CHANGED_MODE: i32 = -2_147_417_850; // 0x80010106: thread already has a COM apartment

#[cfg(target_os = "windows")]
struct ComApartment {
    /// Only true when this call actually initialized COM (S_OK); S_FALSE and
    /// RPC_E_CHANGED_MODE mean the thread already had an apartment and must
    /// not be uninitialized here.
    initialized: bool,
}

#[cfg(target_os = "windows")]
impl ComApartment {
    fn new() -> Option<Self> {
        let rc = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let code = rc.0;
        if code == 0 {
            // S_OK: we initialized COM and must uninitialize on drop.
            Some(Self { initialized: true })
        } else if code == 1 || code == RPC_E_CHANGED_MODE {
            // S_FALSE or "already initialized with another concurrency model":
            // COM is usable on this thread either way; leave it as-is.
            Some(Self { initialized: false })
        } else {
            None
        }
    }
}

#[cfg(target_os = "windows")]
impl Drop for ComApartment {
    fn drop(&mut self) {
        if self.initialized {
            unsafe {
                CoUninitialize();
            }
        }
    }
}

#[cfg(target_os = "windows")]
enum WindowsDeviceDirection {
    Input,
    Output,
}

#[cfg(target_os = "windows")]
fn discover_windows_devices(direction: WindowsDeviceDirection) -> Vec<String> {
    let mut out = vec!["wasapi:default".to_string()];
    let Some(_com) = ComApartment::new() else {
        return out;
    };
    let Ok(enumerator) = create_enumerator() else {
        return out;
    };
    let flow = match direction {
        WindowsDeviceDirection::Input => eCapture,
        WindowsDeviceDirection::Output => eRender,
    };
    let Ok(devices) = (unsafe { enumerator.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE) }) else {
        return out;
    };
    let Ok(count) = (unsafe { devices.GetCount() }) else {
        return out;
    };
    for idx in 0..count {
        let Ok(device) = (unsafe { devices.Item(idx) }) else {
            continue;
        };
        if let Some(name) = device_friendly_name(&device) {
            out.push(format!("wasapi:{name}"));
        }
    }
    out.sort();
    out.dedup();
    out
}

#[cfg(target_os = "windows")]
pub(crate) fn discover_windows_audio_devices() -> Vec<String> {
    discover_windows_devices(WindowsDeviceDirection::Output)
}

#[cfg(target_os = "windows")]
pub(crate) fn discover_windows_input_devices() -> Vec<String> {
    discover_windows_devices(WindowsDeviceDirection::Input)
}

#[cfg(target_os = "windows")]
fn fallback_sample_rates() -> Vec<i32> {
    vec![
        8_000, 11_025, 16_000, 22_050, 32_000, 44_100, 48_000, 88_200, 96_000, 176_400, 192_000,
        384_000,
    ]
}

#[cfg(target_os = "windows")]
pub(crate) fn discover_windows_output_sample_rates(device_id: &str) -> Vec<i32> {
    let fallback = fallback_sample_rates();
    let Some(requested_name) = device_id.strip_prefix("wasapi:") else {
        return fallback;
    };
    let Some(_com) = ComApartment::new() else {
        return fallback;
    };
    let Some(client) = open_output_client(requested_name) else {
        return fallback;
    };
    let Some(channels) = mix_channels(&client) else {
        return fallback;
    };
    let mut rates = Vec::new();
    for rate in &fallback {
        if probe_format(&client, *rate as u32, 32, WAVE_FORMAT_IEEE_FLOAT, channels)
            || probe_format(&client, *rate as u32, 16, WAVE_FORMAT_PCM, channels)
        {
            rates.push(*rate);
        }
    }
    if rates.is_empty() { fallback } else { rates }
}

#[cfg(target_os = "windows")]
pub(crate) fn discover_windows_output_bit_depths(device_id: &str) -> Vec<usize> {
    let fallback_bits = vec![32, 24, 16, 8];
    let Some(requested_name) = device_id.strip_prefix("wasapi:") else {
        return fallback_bits;
    };
    let Some(_com) = ComApartment::new() else {
        return fallback_bits;
    };
    let Some(client) = open_output_client(requested_name) else {
        return fallback_bits;
    };
    let Some((channels, mix_rate)) = mix_format(&client) else {
        return fallback_bits;
    };
    let mut bits = Vec::new();
    if probe_format(&client, mix_rate, 32, WAVE_FORMAT_IEEE_FLOAT, channels) {
        bits.push(32);
    }
    if probe_format(&client, mix_rate, 16, WAVE_FORMAT_PCM, channels) {
        bits.push(16);
    }
    bits.sort_by(|a, b| b.cmp(a));
    bits.dedup();
    if bits.is_empty() { fallback_bits } else { bits }
}

#[cfg(target_os = "windows")]
fn create_enumerator() -> Result<IMMDeviceEnumerator, String> {
    unsafe {
        CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL)
            .map_err(|e| format!("Failed to create WASAPI device enumerator: {e}"))
    }
}

#[cfg(target_os = "windows")]
fn open_output_client(requested_name: &str) -> Option<IAudioClient> {
    let enumerator = create_enumerator().ok()?;
    let device = if requested_name == "default" {
        unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole) }.ok()?
    } else {
        let devices =
            unsafe { enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) }.ok()?;
        find_device_by_name(&devices, requested_name)?
    };
    unsafe { device.Activate::<IAudioClient>(CLSCTX_ALL, None) }.ok()
}

#[cfg(target_os = "windows")]
fn find_device_by_name(devices: &IMMDeviceCollection, requested: &str) -> Option<IMMDevice> {
    let count = unsafe { devices.GetCount() }.ok()?;
    for idx in 0..count {
        let Ok(device) = (unsafe { devices.Item(idx) }) else {
            continue;
        };
        if device_friendly_name(&device).is_some_and(|name| name == requested) {
            return Some(device);
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn mix_format(client: &IAudioClient) -> Option<(u32, u32)> {
    // SAFETY: `client` is a live COM reference; GetMixFormat returns a
    // CoTaskMem-allocated WAVEFORMATEX which we free below.
    let ptr = unsafe { client.GetMixFormat() }.ok()?;
    if ptr.is_null() {
        return None;
    }
    // SAFETY: non-null pointer owned by us until freed.
    let mix = unsafe { *ptr };
    // SAFETY: `ptr` came from GetMixFormat and is freed exactly once here.
    unsafe {
        CoTaskMemFree(Some(ptr.cast()));
    }
    Some((u32::from(mix.nChannels).max(1), mix.nSamplesPerSec))
}

#[cfg(target_os = "windows")]
fn mix_channels(client: &IAudioClient) -> Option<u32> {
    mix_format(client).map(|(channels, _)| channels)
}

/// Tests whether `client` accepts an interleaved stream with the given
/// parameters in shared mode (the mode the engine uses for playback).
#[cfg(target_os = "windows")]
fn probe_format(client: &IAudioClient, rate: u32, bits: u16, tag: u32, channels: u32) -> bool {
    let block_align = channels.saturating_mul(u32::from(bits) / 8) as u16;
    let format = WAVEFORMATEX {
        wFormatTag: tag as u16,
        nChannels: channels as u16,
        nSamplesPerSec: rate,
        nAvgBytesPerSec: rate.saturating_mul(u32::from(block_align)),
        nBlockAlign: block_align,
        wBitsPerSample: bits,
        cbSize: 0,
    };
    let mut closest: *mut WAVEFORMATEX = std::ptr::null_mut();
    // SAFETY: `format` is a plain stack struct valid for the call; `closest`
    // receives an optional CoTaskMem allocation which we free afterwards.
    let supported = unsafe {
        client.IsFormatSupported(
            AUDCLNT_SHAREMODE_SHARED,
            &format,
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

#[cfg(target_os = "windows")]
fn device_friendly_name(device: &IMMDevice) -> Option<String> {
    // SAFETY: `device` is a live COM reference; the property store and the
    // returned PROPVARIANT are released/cleared before returning.
    let store = unsafe { device.OpenPropertyStore(STGM_READ) }.ok()?;
    let mut value =
        unsafe { store.GetValue(&DEVPKEY_Device_FriendlyName as *const _ as *const _) }.ok()?;
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

#[cfg(target_os = "windows")]
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
