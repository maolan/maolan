mod clip;
mod connection;
mod track;

#[cfg(target_os = "linux")]
use alsa::Direction;
#[cfg(target_os = "linux")]
use alsa::pcm::{Access, Format, HwParams, PCM};
pub use clip::{AudioClip, MIDIClip};
pub use connection::Connection;
#[cfg(target_os = "windows")]
use cpal::traits::{DeviceTrait, HostTrait};
use iced::{Length, Point};
use maolan_engine::kind::Kind;
#[cfg(not(target_os = "macos"))]
use maolan_engine::lv2::Lv2PluginInfo;
#[cfg(not(target_os = "macos"))]
use maolan_engine::message::{Lv2GraphConnection, Lv2GraphNode, Lv2GraphPlugin};
#[cfg(target_os = "freebsd")]
use nvtree::{Nvtree, Nvtvalue, nvtree_find, nvtree_unpack};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Instant,
};
#[cfg(target_os = "freebsd")]
use std::{ffi::c_void, fs::File, os::fd::AsRawFd};
use tokio::sync::RwLock;
pub use track::Track;

pub const HW_IN_ID: &str = "hw:in";
pub const HW_OUT_ID: &str = "hw:out";
pub const MIDI_HW_IN_ID: &str = "midi:hw:in";
pub const MIDI_HW_OUT_ID: &str = "midi:hw:out";

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum AudioBackendOption {
    #[cfg(unix)]
    Jack,
    #[cfg(target_os = "freebsd")]
    Oss,
    #[cfg(target_os = "openbsd")]
    Sndio,
    #[cfg(target_os = "linux")]
    Alsa,
    #[cfg(target_os = "windows")]
    Wasapi,
    #[cfg(target_os = "macos")]
    CoreAudio,
}

impl std::fmt::Display for AudioBackendOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            #[cfg(unix)]
            Self::Jack => "JACK",
            #[cfg(target_os = "freebsd")]
            Self::Oss => "OSS",
            #[cfg(target_os = "openbsd")]
            Self::Sndio => "sndio",
            #[cfg(target_os = "linux")]
            Self::Alsa => "ALSA",
            #[cfg(target_os = "windows")]
            Self::Wasapi => "WASAPI",
            #[cfg(target_os = "macos")]
            Self::CoreAudio => "CoreAudio",
        };
        f.write_str(label)
    }
}

#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
#[derive(Clone, Debug)]
pub struct AudioDeviceOption {
    pub id: String,
    pub label: String,
    pub supported_bits: Vec<usize>,
}

#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
impl AudioDeviceOption {
    #[cfg(target_os = "linux")]
    pub fn with_supported_bits(
        id: impl Into<String>,
        label: impl Into<String>,
        mut supported_bits: Vec<usize>,
    ) -> Self {
        supported_bits.sort_by(|a, b| b.cmp(a));
        supported_bits.dedup();
        Self {
            id: id.into(),
            label: label.into(),
            supported_bits,
        }
    }

    #[cfg(target_os = "freebsd")]
    pub fn with_supported_bits(
        id: impl Into<String>,
        label: impl Into<String>,
        mut supported_bits: Vec<usize>,
    ) -> Self {
        supported_bits.sort_by(|a, b| b.cmp(a));
        supported_bits.dedup();
        Self {
            id: id.into(),
            label: label.into(),
            supported_bits,
        }
    }

    #[cfg(target_os = "openbsd")]
    pub fn with_supported_bits(
        id: impl Into<String>,
        label: impl Into<String>,
        mut supported_bits: Vec<usize>,
    ) -> Self {
        supported_bits.sort_by(|a, b| b.cmp(a));
        supported_bits.dedup();
        Self {
            id: id.into(),
            label: label.into(),
            supported_bits,
        }
    }

    pub fn preferred_bits(&self) -> Option<usize> {
        self.supported_bits.first().copied()
    }
}

#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
impl std::fmt::Display for AudioDeviceOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.supported_bits.is_empty() {
            return f.write_str(&self.label);
        }
        let formats = self
            .supported_bits
            .iter()
            .map(|b| format!("{b}"))
            .collect::<Vec<_>>()
            .join("/");
        write!(f, "{} [{}-bit]", self.label, formats)
    }
}

#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
impl PartialEq for AudioDeviceOption {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
impl Eq for AudioDeviceOption {}

#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
impl std::hash::Hash for AudioDeviceOption {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClipId {
    pub track_idx: String,
    pub clip_idx: usize,
    pub kind: Kind,
}

#[derive(Debug, Clone)]
pub enum Resizing {
    Clip {
        kind: Kind,
        track_name: String,
        index: usize,
        is_right_side: bool,
        initial_value: f32,
        initial_mouse_x: f32,
        initial_length: f32,
    },
    Mixer(f32, f32),
    Track(String, f32, f32),
    Tracks(f32, f32),
}

#[derive(Debug, Clone)]
pub struct Connecting {
    pub from_track: String,
    pub from_port: usize,
    pub kind: Kind,
    pub point: Point,
    pub is_input: bool,
}

#[derive(Debug, Clone)]
pub struct MovingTrack {
    pub track_idx: String,
    pub offset_x: f32,
    pub offset_y: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Hovering {
    Port {
        track_idx: String,
        port_idx: usize,
        is_input: bool,
    },
    Track(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionViewSelection {
    Tracks(HashSet<String>),
    Connections(HashSet<usize>),
    None,
}

#[derive(Debug, Clone)]
pub enum View {
    Workspace,
    Connections,
    TrackPlugins,
}

#[derive(Debug, Clone)]
pub struct HW {
    pub channels: usize,
}

#[cfg(not(target_os = "macos"))]
#[derive(Debug, Clone)]
pub struct Lv2Connecting {
    pub from_node: Lv2GraphNode,
    pub from_port: usize,
    pub kind: Kind,
    pub point: Point,
    pub is_input: bool,
}

#[cfg(not(target_os = "macos"))]
#[derive(Debug, Clone)]
pub struct MovingPlugin {
    pub instance_id: usize,
    pub offset_x: f32,
    pub offset_y: f32,
}

#[derive(Debug, Clone)]
pub struct ClipRenameDialog {
    pub track_idx: String,
    pub clip_idx: usize,
    pub kind: Kind,
    pub new_name: String,
}

#[derive(Debug, Clone)]
pub struct TrackRenameDialog {
    pub old_name: String,
    pub new_name: String,
}

#[derive(Debug)]
pub struct StateData {
    pub shift: bool,
    pub ctrl: bool,
    pub tracks: Vec<Track>,
    pub connections: Vec<Connection>,
    pub selected: HashSet<String>,
    pub selected_clips: HashSet<ClipId>,
    pub clip_click_consumed: bool,
    pub message: String,
    pub resizing: Option<Resizing>,
    pub connecting: Option<Connecting>,
    pub moving_track: Option<MovingTrack>,
    pub hovering: Option<Hovering>,
    pub connection_view_selection: ConnectionViewSelection,
    pub cursor: Point,
    pub mouse_left_down: bool,
    pub clip_marquee_start: Option<Point>,
    pub clip_marquee_end: Option<Point>,
    pub mixer_height: Length,
    pub tracks_width: Length,
    pub view: View,
    pub pending_track_positions: HashMap<String, Point>,
    pub pending_track_heights: HashMap<String, f32>,
    pub hovered_track_resize_handle: Option<String>,
    pub hw_loaded: bool,
    pub available_backends: Vec<AudioBackendOption>,
    pub selected_backend: AudioBackendOption,
    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
    pub available_hw: Vec<AudioDeviceOption>,
    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
    pub selected_hw: Option<AudioDeviceOption>,
    #[cfg(not(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd")))]
    pub available_hw: Vec<String>,
    #[cfg(not(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd")))]
    pub selected_hw: Option<String>,
    pub oss_exclusive: bool,
    pub oss_bits: usize,
    pub oss_period_frames: usize,
    pub oss_nperiods: usize,
    pub oss_sync_mode: bool,
    pub opened_midi_in_hw: Vec<String>,
    pub opened_midi_out_hw: Vec<String>,
    pub midi_hw_labels: HashMap<String, String>,
    pub midi_hw_in_positions: HashMap<String, Point>,
    pub midi_hw_out_positions: HashMap<String, Point>,
    pub hw_in: Option<HW>,
    pub hw_out: Option<HW>,
    pub hw_out_level: f32,
    pub hw_out_balance: f32,
    pub hw_out_muted: bool,
    pub hw_out_meter_db: Vec<f32>,
    #[cfg(not(target_os = "macos"))]
    pub lv2_plugins: Vec<Lv2PluginInfo>,
    pub lv2_graph_track: Option<String>,
    #[cfg(not(target_os = "macos"))]
    pub lv2_graph_plugins: Vec<Lv2GraphPlugin>,
    #[cfg(not(target_os = "macos"))]
    pub lv2_graph_connections: Vec<Lv2GraphConnection>,
    #[cfg(not(target_os = "macos"))]
    pub lv2_graphs_by_track: HashMap<String, (Vec<Lv2GraphPlugin>, Vec<Lv2GraphConnection>)>,
    pub lv2_graph_selected_connections: std::collections::HashSet<usize>,
    pub lv2_graph_selected_plugin: Option<usize>,
    pub lv2_graph_plugin_positions: HashMap<usize, Point>,
    #[cfg(not(target_os = "macos"))]
    pub lv2_graph_connecting: Option<Lv2Connecting>,
    #[cfg(not(target_os = "macos"))]
    pub lv2_graph_moving_plugin: Option<MovingPlugin>,
    pub lv2_graph_last_plugin_click: Option<(usize, Instant)>,
    pub connections_last_track_click: Option<(String, Instant)>,
    pub clip_rename_dialog: Option<ClipRenameDialog>,
    pub track_rename_dialog: Option<TrackRenameDialog>,
}

impl Default for StateData {
    fn default() -> Self {
        let available_backends = supported_audio_backends();
        let selected_backend = default_audio_backend();
        #[cfg(target_os = "freebsd")]
        let hw: Vec<AudioDeviceOption> = discover_freebsd_audio_devices();
        #[cfg(target_os = "openbsd")]
        let hw: Vec<AudioDeviceOption> = discover_openbsd_audio_devices();
        #[cfg(target_os = "linux")]
        let hw: Vec<AudioDeviceOption> = { discover_alsa_devices() };
        #[cfg(target_os = "windows")]
        let hw: Vec<String> = discover_windows_audio_devices();
        #[cfg(target_os = "macos")]
        let hw: Vec<String> = maolan_engine::discover_coreaudio_devices();
        #[cfg(not(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "openbsd",
            target_os = "windows",
            target_os = "macos"
        )))]
        let hw: Vec<String> = vec![];
        Self {
            shift: false,
            ctrl: false,
            tracks: vec![],
            connections: vec![],
            selected: HashSet::new(),
            selected_clips: HashSet::new(),
            clip_click_consumed: false,
            message: "Thank you for using Maolan!".to_string(),
            resizing: None,
            connecting: None,
            moving_track: None,
            hovering: None,
            connection_view_selection: ConnectionViewSelection::None,
            cursor: Point::new(0.0, 0.0),
            mouse_left_down: false,
            clip_marquee_start: None,
            clip_marquee_end: None,
            mixer_height: Length::Fixed(300.0),
            tracks_width: Length::Fixed(200.0),
            view: View::Workspace,
            pending_track_positions: HashMap::new(),
            pending_track_heights: HashMap::new(),
            hovered_track_resize_handle: None,
            hw_loaded: false,
            available_backends,
            selected_backend,
            available_hw: hw,
            selected_hw: None,
            oss_exclusive: true,
            oss_bits: 32,
            oss_period_frames: 1024,
            oss_nperiods: 1,
            oss_sync_mode: false,
            opened_midi_in_hw: vec![],
            opened_midi_out_hw: vec![],
            midi_hw_labels: HashMap::new(),
            midi_hw_in_positions: HashMap::new(),
            midi_hw_out_positions: HashMap::new(),
            hw_in: None,
            hw_out: None,
            hw_out_level: 0.0,
            hw_out_balance: 0.0,
            hw_out_muted: false,
            hw_out_meter_db: vec![],
            #[cfg(not(target_os = "macos"))]
            lv2_plugins: vec![],
            lv2_graph_track: None,
            #[cfg(not(target_os = "macos"))]
            lv2_graph_plugins: vec![],
            #[cfg(not(target_os = "macos"))]
            lv2_graph_connections: vec![],
            #[cfg(not(target_os = "macos"))]
            lv2_graphs_by_track: HashMap::new(),
            lv2_graph_selected_connections: HashSet::new(),
            lv2_graph_selected_plugin: None,
            lv2_graph_plugin_positions: HashMap::new(),
            #[cfg(not(target_os = "macos"))]
            lv2_graph_connecting: None,
            #[cfg(not(target_os = "macos"))]
            lv2_graph_moving_plugin: None,
            lv2_graph_last_plugin_click: None,
            connections_last_track_click: None,
            clip_rename_dialog: None,
            track_rename_dialog: None,
        }
    }
}

pub type State = Arc<RwLock<StateData>>;

fn supported_audio_backends() -> Vec<AudioBackendOption> {
    let mut out = vec![];
    #[cfg(unix)]
    out.push(AudioBackendOption::Jack);
    #[cfg(target_os = "freebsd")]
    out.push(AudioBackendOption::Oss);
    #[cfg(target_os = "openbsd")]
    out.push(AudioBackendOption::Sndio);
    #[cfg(target_os = "linux")]
    out.push(AudioBackendOption::Alsa);
    #[cfg(target_os = "windows")]
    out.push(AudioBackendOption::Wasapi);
    #[cfg(target_os = "macos")]
    out.push(AudioBackendOption::CoreAudio);
    out
}

fn default_audio_backend() -> AudioBackendOption {
    #[cfg(target_os = "freebsd")]
    {
        AudioBackendOption::Oss
    }
    #[cfg(target_os = "openbsd")]
    {
        AudioBackendOption::Sndio
    }
    #[cfg(target_os = "linux")]
    {
        AudioBackendOption::Alsa
    }
    #[cfg(target_os = "windows")]
    {
        AudioBackendOption::Wasapi
    }
    #[cfg(target_os = "macos")]
    {
        AudioBackendOption::CoreAudio
    }
    #[cfg(all(
        unix,
        not(any(
            target_os = "freebsd",
            target_os = "linux",
            target_os = "openbsd",
            target_os = "macos"
        ))
    ))]
    {
        AudioBackendOption::Jack
    }
    #[cfg(not(any(unix, target_os = "windows")))]
    {
        unreachable!("no default audio backend for this target")
    }
}

#[cfg(target_os = "windows")]
fn discover_windows_audio_devices() -> Vec<String> {
    let host = cpal::default_host();
    let mut out = vec!["default".to_string()];
    if let Ok(devices) = host.output_devices() {
        for dev in devices {
            if let Ok(name) = dev.name() {
                out.push(format!("wasapi:{name}"));
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

#[cfg(target_os = "openbsd")]
pub(crate) fn discover_openbsd_audio_devices() -> Vec<AudioDeviceOption> {
    let mut out = vec![AudioDeviceOption::with_supported_bits(
        "default",
        "Default (sndio)",
        vec![32, 24, 16, 8],
    )];

    let mut paths: Vec<String> = std::fs::read_dir("/dev")
        .map(|rd| {
            rd.filter_map(Result::ok)
                .map(|e| e.path())
                .filter_map(|path| {
                    let name = path.file_name()?.to_str()?;
                    if !name.starts_with("audio") || name.starts_with("audioctl") {
                        return None;
                    }
                    if name[5..].chars().all(|c| c.is_ascii_digit()) {
                        Some(path.to_string_lossy().into_owned())
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    paths.sort();
    paths.dedup();

    for dev in paths {
        out.push(AudioDeviceOption::with_supported_bits(
            dev.clone(),
            format!("{dev} (sndio sun)"),
            vec![32, 24, 16, 8],
        ));
    }

    out
}

#[cfg(target_os = "freebsd")]
pub(crate) fn discover_freebsd_audio_devices() -> Vec<AudioDeviceOption> {
    let mut devices = discover_sndstat_dsp_devices().unwrap_or_default();
    devices.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));
    devices.dedup_by(|a, b| a.id == b.id);
    devices
}

#[cfg(target_os = "freebsd")]
fn discover_sndstat_dsp_devices() -> Option<Vec<AudioDeviceOption>> {
    let file = File::open("/dev/sndstat").ok()?;
    let fd = file.as_raw_fd();

    unsafe {
        if sndst_refresh_devs(fd).is_err() {
            return None;
        }
    }

    let mut arg = SndstIoctlNvArg {
        nbytes: 0,
        buf: std::ptr::null_mut(),
    };
    unsafe {
        if sndst_get_devs(fd, &mut arg).is_err() {
            return None;
        }
    }
    if arg.nbytes == 0 {
        return None;
    }

    let mut buf = vec![0_u8; arg.nbytes];
    arg.buf = buf.as_mut_ptr().cast::<c_void>();
    unsafe {
        if sndst_get_devs(fd, &mut arg).is_err() {
            return None;
        }
    }
    if arg.nbytes == 0 || arg.nbytes > buf.len() {
        return None;
    }

    parse_sndstat_nvlist(&buf[..arg.nbytes])
}

#[cfg(target_os = "freebsd")]
fn parse_sndstat_nvlist(buf: &[u8]) -> Option<Vec<AudioDeviceOption>> {
    let root = match nvtree_unpack(buf) {
        Ok(root) => root,
        Err(_) => {
            return None;
        }
    };
    let dsps_pair = nvtree_find(&root, "dsps")?;
    let Nvtvalue::NestedArray(dsps) = &dsps_pair.value else {
        return None;
    };
    if dsps.is_empty() {
        return None;
    }

    let out = dsps
        .iter()
        .filter_map(|dsp| {
            let devnode_pair = nvtree_find(dsp, "devnode")?;
            let Nvtvalue::String(devnode) = &devnode_pair.value else {
                return None;
            };

            let devpath = if devnode.starts_with('/') {
                devnode.to_string()
            } else {
                format!("/dev/{devnode}")
            };

            if !devpath.starts_with("/dev/dsp") {
                return None;
            }

            let label_prefix = nvtree_find(dsp, "desc")
                .and_then(|pair| match &pair.value {
                    Nvtvalue::String(s) if !s.is_empty() => Some(s.clone()),
                    _ => None,
                })
                .or_else(|| {
                    nvtree_find(dsp, "nameunit").and_then(|pair| match &pair.value {
                        Nvtvalue::String(s) if !s.is_empty() => Some(s.clone()),
                        _ => None,
                    })
                });
            let label = label_prefix
                .map(|prefix| format!("{prefix} ({devpath})"))
                .unwrap_or_else(|| devpath.clone());
            let mut supported_bits = decode_supported_bits_from_dsp(dsp);
            if supported_bits.is_empty() {
                supported_bits = probe_oss_supported_bits(&devpath);
            }
            Some(AudioDeviceOption::with_supported_bits(
                devpath,
                label,
                supported_bits,
            ))
        })
        .collect::<Vec<_>>();

    (!out.is_empty()).then_some(out)
}

#[cfg(target_os = "freebsd")]
const AFMT_S16_LE: u64 = 0x00000010;
#[cfg(target_os = "freebsd")]
const AFMT_S16_BE: u64 = 0x00000020;
#[cfg(target_os = "freebsd")]
const AFMT_S8: u64 = 0x00000040;
#[cfg(target_os = "freebsd")]
const AFMT_S32_LE: u64 = 0x00001000;
#[cfg(target_os = "freebsd")]
const AFMT_S32_BE: u64 = 0x00002000;
#[cfg(target_os = "freebsd")]
const AFMT_S24_LE: u64 = 0x00010000;
#[cfg(target_os = "freebsd")]
const AFMT_S24_BE: u64 = 0x00020000;

#[cfg(target_os = "freebsd")]
fn decode_supported_bits_from_dsp(dsp: &Nvtree) -> Vec<usize> {
    fn parse_number_text(s: &str) -> Option<u64> {
        let trimmed = s.trim();
        if let Some(hex) = trimmed
            .strip_prefix("0x")
            .or_else(|| trimmed.strip_prefix("0X"))
        {
            return u64::from_str_radix(hex, 16).ok();
        }
        trimmed.parse::<u64>().ok()
    }

    fn format_mask_from_value(value: &Nvtvalue) -> Option<u64> {
        match value {
            Nvtvalue::Number(n) => Some(*n),
            Nvtvalue::String(s) => parse_number_text(s),
            Nvtvalue::NumberArray(arr) => Some(arr.iter().copied().fold(0_u64, |acc, n| acc | n)),
            Nvtvalue::StringArray(arr) => Some(
                arr.iter()
                    .filter_map(|s| parse_number_text(s))
                    .fold(0_u64, |acc, n| acc | n),
            ),
            _ => None,
        }
    }

    fn format_mask_from_tree(tree: &Nvtree) -> Option<u64> {
        const DIRECT_KEYS: [&str; 7] = [
            "formats",
            "iformats",
            "oformats",
            "pformats",
            "rformats",
            "playformats",
            "recformats",
        ];
        for key in DIRECT_KEYS {
            if let Some(pair) = nvtree_find(tree, key)
                && let Some(mask) = format_mask_from_value(&pair.value)
            {
                return Some(mask);
            }
        }
        None
    }

    let mut mask = format_mask_from_tree(dsp).unwrap_or(0);
    for nested_name in ["play", "playback", "record", "capture"] {
        if let Some(pair) = nvtree_find(dsp, nested_name)
            && let Nvtvalue::Nested(nested) = &pair.value
        {
            mask |= format_mask_from_tree(nested).unwrap_or(0);
        }
    }

    bits_from_format_mask(mask)
}

#[cfg(target_os = "freebsd")]
fn bits_from_format_mask(mask: u64) -> Vec<usize> {
    let mut bits = Vec::with_capacity(4);
    if (mask & (AFMT_S32_LE | AFMT_S32_BE)) != 0 {
        bits.push(32);
    }
    if (mask & (AFMT_S24_LE | AFMT_S24_BE)) != 0 {
        bits.push(24);
    }
    if (mask & (AFMT_S16_LE | AFMT_S16_BE)) != 0 {
        bits.push(16);
    }
    if (mask & AFMT_S8) != 0 {
        bits.push(8);
    }
    bits
}

#[cfg(target_os = "freebsd")]
fn probe_oss_supported_bits(devpath: &str) -> Vec<usize> {
    let Ok(file) = std::fs::OpenOptions::new().read(true).open(devpath) else {
        return Vec::new();
    };
    let fd = file.as_raw_fd();
    let mut formats = 0_i32;
    let ok = unsafe { oss_get_formats(fd, &mut formats).is_ok() };
    if !ok {
        return Vec::new();
    }
    bits_from_format_mask(formats as u64)
}

#[cfg(target_os = "freebsd")]
#[repr(C)]
struct SndstIoctlNvArg {
    nbytes: usize,
    buf: *mut c_void,
}

#[cfg(target_os = "freebsd")]
nix::ioctl_none!(sndst_refresh_devs, b'D', 100);
#[cfg(target_os = "freebsd")]
nix::ioctl_readwrite!(sndst_get_devs, b'D', 101, SndstIoctlNvArg);
#[cfg(target_os = "freebsd")]
nix::ioctl_read!(oss_get_formats, b'P', 11, i32);

#[cfg(target_os = "linux")]
fn read_alsa_card_labels() -> std::collections::HashMap<u32, String> {
    let mut labels = std::collections::HashMap::new();
    let Ok(contents) = std::fs::read_to_string("/proc/asound/cards") else {
        return labels;
    };
    for line in contents.lines() {
        let line = line.trim_start();
        let Some((num_str, rest)) = line.split_once(' ') else {
            continue;
        };
        let Ok(card) = num_str.parse::<u32>() else {
            continue;
        };
        let Some((_, desc)) = rest.split_once("]:") else {
            continue;
        };
        let desc = desc.trim();
        if !desc.is_empty() {
            labels.insert(card, desc.to_string());
        }
    }
    labels
}

#[cfg(target_os = "linux")]
fn probe_alsa_supported_bits(device: &str) -> Vec<usize> {
    let Ok(capture) = PCM::new(device, Direction::Capture, false) else {
        return Vec::new();
    };
    let Ok(playback) = PCM::new(device, Direction::Playback, false) else {
        return Vec::new();
    };
    let Ok(cap_hwp) = HwParams::any(&capture) else {
        return Vec::new();
    };
    let Ok(pb_hwp) = HwParams::any(&playback) else {
        return Vec::new();
    };
    if cap_hwp.set_access(Access::RWInterleaved).is_err() {
        return Vec::new();
    }
    if pb_hwp.set_access(Access::RWInterleaved).is_err() {
        return Vec::new();
    }

    fn supports(hwp: &HwParams<'_>, fmt: Format) -> bool {
        hwp.test_format(fmt).is_ok()
    }

    let candidates: Vec<(usize, Vec<Format>)> = vec![
        (32, vec![native_s32(), foreign_s32()]),
        (24, vec![native_s24(), foreign_s24()]),
        (16, vec![native_s16(), foreign_s16()]),
        (8, vec![Format::S8]),
    ];

    let mut supported = Vec::new();
    for (bits, formats) in candidates {
        let capture_ok = formats.iter().any(|f| supports(&cap_hwp, *f));
        let playback_ok = formats.iter().any(|f| supports(&pb_hwp, *f));
        if capture_ok && playback_ok {
            supported.push(bits);
        }
    }
    supported
}

#[cfg(target_os = "linux")]
#[cfg(target_endian = "little")]
fn native_s16() -> Format {
    Format::S16LE
}
#[cfg(target_os = "linux")]
#[cfg(target_endian = "big")]
fn native_s16() -> Format {
    Format::S16BE
}
#[cfg(target_os = "linux")]
#[cfg(target_endian = "little")]
fn foreign_s16() -> Format {
    Format::S16BE
}
#[cfg(target_os = "linux")]
#[cfg(target_endian = "big")]
fn foreign_s16() -> Format {
    Format::S16LE
}

#[cfg(target_os = "linux")]
#[cfg(target_endian = "little")]
fn native_s24() -> Format {
    Format::S24LE
}
#[cfg(target_os = "linux")]
#[cfg(target_endian = "big")]
fn native_s24() -> Format {
    Format::S24BE
}
#[cfg(target_os = "linux")]
#[cfg(target_endian = "little")]
fn foreign_s24() -> Format {
    Format::S24BE
}
#[cfg(target_os = "linux")]
#[cfg(target_endian = "big")]
fn foreign_s24() -> Format {
    Format::S24LE
}

#[cfg(target_os = "linux")]
#[cfg(target_endian = "little")]
fn native_s32() -> Format {
    Format::S32LE
}
#[cfg(target_os = "linux")]
#[cfg(target_endian = "big")]
fn native_s32() -> Format {
    Format::S32BE
}
#[cfg(target_os = "linux")]
#[cfg(target_endian = "little")]
fn foreign_s32() -> Format {
    Format::S32BE
}
#[cfg(target_os = "linux")]
#[cfg(target_endian = "big")]
fn foreign_s32() -> Format {
    Format::S32LE
}

#[cfg(target_os = "linux")]
fn discover_alsa_devices() -> Vec<AudioDeviceOption> {
    let mut devices = Vec::new();
    let card_labels = read_alsa_card_labels();
    if let Ok(contents) = std::fs::read_to_string("/proc/asound/pcm") {
        for line in contents.lines() {
            let Some((card_dev, rest)) = line.split_once(':') else {
                continue;
            };
            if !rest.contains("playback") && !rest.contains("capture") {
                continue;
            }
            let mut parts = card_dev.trim().split('-');
            let (Some(card), Some(dev)) = (parts.next(), parts.next()) else {
                continue;
            };
            let Ok(card) = card.parse::<u32>() else {
                continue;
            };
            let Ok(dev) = dev.parse::<u32>() else {
                continue;
            };
            let device_name = rest.split(':').next().unwrap_or("").trim();
            let card_label = card_labels
                .get(&card)
                .cloned()
                .unwrap_or_else(|| format!("Card {card}"));
            let base_label = if device_name.is_empty() {
                card_label
            } else {
                format!("{card_label} - {device_name}")
            };
            let id = format!("hw:{card},{dev}");
            let label = format!("{base_label} (hw:{card},{dev})");
            let supported_bits = probe_alsa_supported_bits(&id);
            devices.push(AudioDeviceOption::with_supported_bits(
                id,
                label,
                supported_bits,
            ));
        }
    }
    devices.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));
    devices.dedup_by(|a, b| a.id == b.id);
    devices
}
