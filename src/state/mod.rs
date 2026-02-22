mod clip;
mod connection;
mod track;

pub use clip::{AudioClip, MIDIClip};
pub use connection::Connection;
use iced::{Length, Point};
use maolan_engine::kind::Kind;
use maolan_engine::lv2::Lv2PluginInfo;
use maolan_engine::message::{Lv2GraphConnection, Lv2GraphNode, Lv2GraphPlugin};
#[cfg(target_os = "freebsd")]
use nvtree::{Nvtvalue, nvtree_find, nvtree_unpack};
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
    Jack,
    #[cfg(target_os = "freebsd")]
    Oss,
    #[cfg(target_os = "linux")]
    Alsa,
}

impl std::fmt::Display for AudioBackendOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::Jack => "JACK",
            #[cfg(target_os = "freebsd")]
            Self::Oss => "OSS",
            #[cfg(target_os = "linux")]
            Self::Alsa => "ALSA",
        };
        f.write_str(label)
    }
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
#[derive(Clone, Debug)]
pub struct AudioDeviceOption {
    pub id: String,
    pub label: String,
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
impl AudioDeviceOption {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
impl std::fmt::Display for AudioDeviceOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
impl PartialEq for AudioDeviceOption {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
impl Eq for AudioDeviceOption {}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
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

#[derive(Debug, Clone)]
pub struct Lv2Connecting {
    pub from_node: Lv2GraphNode,
    pub from_port: usize,
    pub kind: Kind,
    pub point: Point,
    pub is_input: bool,
}

#[derive(Debug, Clone)]
pub struct MovingPlugin {
    pub instance_id: usize,
    pub offset_x: f32,
    pub offset_y: f32,
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
    pub mixer_height: Length,
    pub tracks_width: Length,
    pub view: View,
    pub pending_track_positions: HashMap<String, Point>,
    pub pending_track_heights: HashMap<String, f32>,
    pub hovered_track_resize_handle: Option<String>,
    pub hw_loaded: bool,
    pub available_backends: Vec<AudioBackendOption>,
    pub selected_backend: AudioBackendOption,
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    pub available_hw: Vec<AudioDeviceOption>,
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    pub selected_hw: Option<AudioDeviceOption>,
    #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
    pub available_hw: Vec<String>,
    #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
    pub selected_hw: Option<String>,
    pub oss_exclusive: bool,
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
    pub lv2_plugins: Vec<Lv2PluginInfo>,
    pub lv2_graph_track: Option<String>,
    pub lv2_graph_plugins: Vec<Lv2GraphPlugin>,
    pub lv2_graph_connections: Vec<Lv2GraphConnection>,
    pub lv2_graphs_by_track: HashMap<String, (Vec<Lv2GraphPlugin>, Vec<Lv2GraphConnection>)>,
    pub lv2_graph_selected_connections: std::collections::HashSet<usize>,
    pub lv2_graph_selected_plugin: Option<usize>,
    pub lv2_graph_plugin_positions: HashMap<usize, Point>,
    pub lv2_graph_connecting: Option<Lv2Connecting>,
    pub lv2_graph_moving_plugin: Option<MovingPlugin>,
    pub lv2_graph_last_plugin_click: Option<(usize, Instant)>,
    pub connections_last_track_click: Option<(String, Instant)>,
}

impl Default for StateData {
    fn default() -> Self {
        let available_backends = supported_audio_backends();
        let selected_backend = default_audio_backend();
        #[cfg(target_os = "freebsd")]
        let hw: Vec<AudioDeviceOption> = discover_freebsd_audio_devices();
        #[cfg(target_os = "linux")]
        let hw: Vec<AudioDeviceOption> = { discover_alsa_devices() };
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
            lv2_plugins: vec![],
            lv2_graph_track: None,
            lv2_graph_plugins: vec![],
            lv2_graph_connections: vec![],
            lv2_graphs_by_track: HashMap::new(),
            lv2_graph_selected_connections: HashSet::new(),
            lv2_graph_selected_plugin: None,
            lv2_graph_plugin_positions: HashMap::new(),
            lv2_graph_connecting: None,
            lv2_graph_moving_plugin: None,
            lv2_graph_last_plugin_click: None,
            connections_last_track_click: None,
        }
    }
}

pub type State = Arc<RwLock<StateData>>;

fn supported_audio_backends() -> Vec<AudioBackendOption> {
    let mut out = Vec::new();
    out.push(AudioBackendOption::Jack);
    #[cfg(target_os = "freebsd")]
    out.push(AudioBackendOption::Oss);
    #[cfg(target_os = "linux")]
    out.push(AudioBackendOption::Alsa);
    out
}

fn default_audio_backend() -> AudioBackendOption {
    #[cfg(target_os = "freebsd")]
    {
        AudioBackendOption::Oss
    }
    #[cfg(target_os = "linux")]
    {
        AudioBackendOption::Alsa
    }
    #[cfg(not(any(target_os = "freebsd", target_os = "linux")))]
    {
        AudioBackendOption::Jack
    }
}

#[cfg(target_os = "freebsd")]
fn discover_freebsd_audio_devices() -> Vec<AudioDeviceOption> {
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
    let Some(dsps_pair) = nvtree_find(&root, "dsps") else {
        return None;
    };
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
            Some(AudioDeviceOption::new(devpath, label))
        })
        .collect::<Vec<_>>();

    (!out.is_empty()).then_some(out)
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
            devices.push(AudioDeviceOption::new(
                format!("hw:{card},{dev}"),
                format!("{base_label} (hw:{card},{dev})"),
            ));
        }
    }
    devices.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));
    devices.dedup_by(|a, b| a.id == b.id);
    devices
}
