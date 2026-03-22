#[cfg(unix)]
use maolan_engine::message::PluginGraphNode;
use maolan_engine::{
    kind::Kind,
    message::{
        Action, AudioClipData, GlobalMidiLearnTarget, MidiClipData, MidiLearnBinding,
        PitchCorrectionPointData, TrackMidiLearnTarget,
    },
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct CliConfig {
    pub default_audio_bit_depth: usize,
    pub default_export_sample_rate_hz: u32,
    pub osc_enabled: bool,
    pub default_output_device_id: Option<String>,
    pub default_input_device_id: Option<String>,
}

impl CliConfig {
    pub fn load() -> Result<Self, String> {
        let config_path = config_path()?;
        if !config_path.exists() {
            return Ok(Self::default());
        }
        let contents = std::fs::read_to_string(&config_path)
            .map_err(|err| format!("Failed to read {}: {err}", config_path.display()))?;
        toml::from_str(&contents)
            .map_err(|err| format!("Failed to parse {}: {err}", config_path.display()))
    }
}

fn config_path() -> Result<PathBuf, String> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| "Could not determine home directory".to_string())?;
    Ok(PathBuf::from(home)
        .join(".config")
        .join("maolan")
        .join("config.toml"))
}

struct SavedConnection {
    from_track: String,
    from_port: usize,
    to_track: String,
    to_port: usize,
    kind: Kind,
}

#[derive(Debug, Clone, Default)]
pub struct ExportMetadata {
    pub author: String,
    pub album: String,
    pub year: Option<u32>,
    pub track_number: Option<u32>,
    pub genre: String,
}

#[derive(Debug, Clone)]
pub struct ExportConnection {
    pub from_track: String,
    pub from_port: usize,
    pub to_track: String,
    pub to_port: usize,
    pub kind: Kind,
}

#[derive(Debug, Clone)]
pub struct ExportTrack {
    pub name: String,
    pub level: f32,
    pub balance: f32,
    pub muted: bool,
    pub soloed: bool,
    pub output_ports: usize,
    pub audio_clips: Vec<AudioClipData>,
}

#[derive(Debug, Clone, Default)]
pub struct ExportSessionData {
    pub metadata: ExportMetadata,
    pub tracks: Vec<ExportTrack>,
    pub connections: Vec<ExportConnection>,
}

pub fn load_session_restore_actions(session_dir: &Path) -> Result<Vec<Action>, String> {
    let session = load_session_json(session_dir)?;

    let mut actions = vec![
        Action::BeginSessionRestore,
        Action::SetSessionPath(session_dir.to_string_lossy().to_string()),
        Action::SetGlobalMidiLearnBinding {
            target: GlobalMidiLearnTarget::PlayPause,
            binding: None,
        },
        Action::SetGlobalMidiLearnBinding {
            target: GlobalMidiLearnTarget::Stop,
            binding: None,
        },
        Action::SetGlobalMidiLearnBinding {
            target: GlobalMidiLearnTarget::RecordToggle,
            binding: None,
        },
    ];

    if let Some(global_ml) = session.get("midi_learn_global").and_then(Value::as_object) {
        push_global_binding(
            &mut actions,
            GlobalMidiLearnTarget::PlayPause,
            global_ml.get("play_pause"),
        );
        push_global_binding(
            &mut actions,
            GlobalMidiLearnTarget::Stop,
            global_ml.get("stop"),
        );
        push_global_binding(
            &mut actions,
            GlobalMidiLearnTarget::RecordToggle,
            global_ml.get("record_toggle"),
        );
    }

    if let Some(transport) = session.get("transport") {
        let loop_range = parse_optional_range(transport.get("loop_range_samples"))?;
        let punch_range = parse_optional_range(transport.get("punch_range_samples"))?;
        let loop_enabled = transport
            .get("loop_enabled")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let punch_enabled = transport
            .get("punch_enabled")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let tempo = transport
            .get("tempo")
            .and_then(Value::as_f64)
            .unwrap_or(120.0);
        let numerator = transport
            .get("time_signature_num")
            .and_then(Value::as_u64)
            .unwrap_or(4) as u16;
        let denominator = transport
            .get("time_signature_denom")
            .and_then(Value::as_u64)
            .unwrap_or(4) as u16;
        actions.push(Action::SetLoopRange(loop_range));
        actions.push(Action::SetLoopEnabled(loop_enabled));
        actions.push(Action::SetPunchRange(punch_range));
        actions.push(Action::SetPunchEnabled(punch_enabled));
        actions.push(Action::SetTempo(tempo));
        actions.push(Action::SetTimeSignature {
            numerator,
            denominator,
        });
    }

    let tracks = session
        .get("tracks")
        .and_then(Value::as_array)
        .ok_or_else(|| "Session is missing 'tracks' array".to_string())?;

    for track in tracks {
        push_track_restore_actions(&mut actions, track)?;
    }

    push_track_graph_restore_actions(&mut actions, session.get("graphs"))?;
    push_connection_restore_actions(&mut actions, session.get("connections"))?;

    actions.push(Action::EndSessionRestore);
    Ok(actions)
}

pub fn load_export_session_data(session_dir: &Path) -> Result<ExportSessionData, String> {
    let session = load_session_json(session_dir)?;
    let tracks = session
        .get("tracks")
        .and_then(Value::as_array)
        .ok_or_else(|| "Session is missing 'tracks' array".to_string())?
        .iter()
        .map(parse_export_track)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ExportSessionData {
        metadata: parse_export_metadata(session.get("metadata")),
        tracks,
        connections: parse_export_connections(session.get("connections"))?,
    })
}

pub fn load_session_end_sample(session_dir: &Path) -> Result<usize, String> {
    let session = load_session_json(session_dir)?;
    Ok(session
        .get("tracks")
        .and_then(Value::as_array)
        .map(|tracks| tracks.iter().map(track_end_sample).max().unwrap_or(0))
        .unwrap_or(0))
}

fn load_session_json(session_dir: &Path) -> Result<Value, String> {
    let session_path = session_dir.join("session.json");
    let file = File::open(&session_path)
        .map_err(|err| format!("Failed to open {}: {err}", session_path.display()))?;
    let reader = BufReader::new(file);
    serde_json::from_reader(reader)
        .map_err(|err| format!("Failed to parse {}: {err}", session_path.display()))
}

fn track_end_sample(track: &Value) -> usize {
    ["audio", "midi"]
        .into_iter()
        .filter_map(|kind| track.get(kind))
        .filter_map(|section| section.get("clips").and_then(Value::as_array))
        .flat_map(|clips| clips.iter())
        .map(clip_end_sample)
        .max()
        .unwrap_or(0)
}

fn clip_end_sample(clip: &Value) -> usize {
    let own_end = clip
        .get("start")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .saturating_add(clip.get("length").and_then(Value::as_u64).unwrap_or(0))
        as usize;
    let child_end = clip
        .get("grouped_clips")
        .and_then(Value::as_array)
        .map(|clips| clips.iter().map(clip_end_sample).max().unwrap_or(0))
        .unwrap_or(0);
    own_end.max(child_end)
}

fn push_global_binding(
    actions: &mut Vec<Action>,
    target: GlobalMidiLearnTarget,
    value: Option<&Value>,
) {
    let binding = value
        .cloned()
        .and_then(|value| serde_json::from_value::<Option<MidiLearnBinding>>(value).ok())
        .flatten();
    if let Some(binding) = binding {
        actions.push(Action::SetGlobalMidiLearnBinding {
            target,
            binding: Some(binding),
        });
    }
}

fn parse_optional_range(value: Option<&Value>) -> Result<Option<(usize, usize)>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let arr = value
        .as_array()
        .ok_or_else(|| "Transport range must be an array".to_string())?;
    if arr.len() != 2 {
        return Err("Transport range must have exactly two values".to_string());
    }
    let start = arr[0]
        .as_u64()
        .ok_or_else(|| "Transport range start must be an unsigned integer".to_string())?
        as usize;
    let end = arr[1]
        .as_u64()
        .ok_or_else(|| "Transport range end must be an unsigned integer".to_string())?
        as usize;
    if end <= start {
        return Err("Transport range end must be greater than start".to_string());
    }
    Ok(Some((start, end)))
}

fn push_track_restore_actions(actions: &mut Vec<Action>, track: &Value) -> Result<(), String> {
    let name = get_required_str(track, "name")?.to_string();
    let audio = track
        .get("audio")
        .ok_or_else(|| format!("Track '{name}' is missing audio section"))?;
    let midi = track
        .get("midi")
        .ok_or_else(|| format!("Track '{name}' is missing midi section"))?;

    let audio_ins = get_required_usize(audio, "ins", &name)?;
    let audio_outs = get_required_usize(audio, "outs", &name)?;
    let midi_ins = get_required_usize(midi, "ins", &name)?;
    let midi_outs = get_required_usize(midi, "outs", &name)?;
    let primary_audio_ins = track
        .get("primary_audio_ins")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(audio_ins);
    let primary_audio_outs = track
        .get("primary_audio_outs")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(audio_outs);

    actions.push(Action::AddTrack {
        name: name.clone(),
        audio_ins: primary_audio_ins.min(audio_ins),
        midi_ins,
        audio_outs: primary_audio_outs.min(audio_outs),
        midi_outs,
    });
    for _ in primary_audio_ins.min(audio_ins)..audio_ins {
        actions.push(Action::TrackAddAudioInput(name.clone()));
    }
    for _ in primary_audio_outs.min(audio_outs)..audio_outs {
        actions.push(Action::TrackAddAudioOutput(name.clone()));
    }

    push_optional_f32(actions, track, "level", |value| {
        Action::TrackLevel(name.clone(), value)
    });
    push_optional_f32(actions, track, "balance", |value| {
        Action::TrackBalance(name.clone(), value)
    });
    push_optional_toggle(actions, track, "armed", || {
        Action::TrackToggleArm(name.clone())
    });
    push_optional_toggle(actions, track, "muted", || {
        Action::TrackToggleMute(name.clone())
    });
    push_optional_toggle(actions, track, "soloed", || {
        Action::TrackToggleSolo(name.clone())
    });
    push_optional_toggle(actions, track, "input_monitor", || {
        Action::TrackToggleInputMonitor(name.clone())
    });
    if matches!(
        track.get("disk_monitor").and_then(Value::as_bool),
        Some(false)
    ) {
        actions.push(Action::TrackToggleDiskMonitor(name.clone()));
    }

    if let Some(channels) = track.get("midi_lane_channels").and_then(Value::as_array) {
        for (lane, channel) in channels.iter().enumerate() {
            actions.push(Action::TrackSetMidiLaneChannel {
                track_name: name.clone(),
                lane,
                channel: channel.as_u64().map(|value| value.min(15) as u8),
            });
        }
    }

    push_track_midi_binding(
        actions,
        &name,
        TrackMidiLearnTarget::Volume,
        track.get("midi_learn_volume"),
    );
    push_track_midi_binding(
        actions,
        &name,
        TrackMidiLearnTarget::Balance,
        track.get("midi_learn_balance"),
    );
    push_track_midi_binding(
        actions,
        &name,
        TrackMidiLearnTarget::Mute,
        track.get("midi_learn_mute"),
    );
    push_track_midi_binding(
        actions,
        &name,
        TrackMidiLearnTarget::Solo,
        track.get("midi_learn_solo"),
    );
    push_track_midi_binding(
        actions,
        &name,
        TrackMidiLearnTarget::Arm,
        track.get("midi_learn_arm"),
    );
    push_track_midi_binding(
        actions,
        &name,
        TrackMidiLearnTarget::InputMonitor,
        track.get("midi_learn_input_monitor"),
    );
    push_track_midi_binding(
        actions,
        &name,
        TrackMidiLearnTarget::DiskMonitor,
        track.get("midi_learn_disk_monitor"),
    );

    if let Some(audio_clips) = audio.get("clips").and_then(Value::as_array) {
        for clip in audio_clips {
            if clip
                .get("grouped_clips")
                .and_then(Value::as_array)
                .is_some_and(|children| !children.is_empty())
            {
                actions.push(Action::AddGroupedClip {
                    track_name: name.clone(),
                    kind: Kind::Audio,
                    audio_clip: Some(parse_audio_clip_data(clip)?),
                    midi_clip: None,
                });
            } else {
                actions.push(Action::AddClip {
                    name: clip
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    track_name: name.clone(),
                    start: clip.get("start").and_then(Value::as_u64).unwrap_or(0) as usize,
                    length: clip.get("length").and_then(Value::as_u64).unwrap_or(0) as usize,
                    offset: clip.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize,
                    input_channel: clip
                        .get("input_channel")
                        .and_then(Value::as_u64)
                        .unwrap_or(0) as usize,
                    muted: clip.get("muted").and_then(Value::as_bool).unwrap_or(false),
                    peaks_file: clip
                        .get("peaks_file")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    kind: Kind::Audio,
                    fade_enabled: clip
                        .get("fade_enabled")
                        .and_then(Value::as_bool)
                        .unwrap_or(true),
                    fade_in_samples: clip
                        .get("fade_in_samples")
                        .and_then(Value::as_u64)
                        .unwrap_or(240) as usize,
                    fade_out_samples: clip
                        .get("fade_out_samples")
                        .and_then(Value::as_u64)
                        .unwrap_or(240) as usize,
                    source_name: clip
                        .get("pitch_correction_source_name")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    source_offset: clip
                        .get("pitch_correction_source_offset")
                        .and_then(Value::as_u64)
                        .map(|value| value as usize),
                    source_length: clip
                        .get("pitch_correction_source_length")
                        .and_then(Value::as_u64)
                        .map(|value| value as usize),
                    preview_name: clip
                        .get("pitch_correction_preview_name")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    pitch_correction_points: parse_pitch_correction_points(clip),
                    pitch_correction_frame_likeness: clip
                        .get("pitch_correction_frame_likeness")
                        .and_then(Value::as_f64)
                        .map(|value| value as f32),
                    pitch_correction_inertia_ms: clip
                        .get("pitch_correction_inertia_ms")
                        .and_then(Value::as_u64)
                        .map(|value| value as u16),
                    pitch_correction_formant_compensation: clip
                        .get("pitch_correction_formant_compensation")
                        .and_then(Value::as_bool),
                    plugin_graph_json: clip
                        .get("plugin_graph_json")
                        .filter(|value| !value.is_null())
                        .cloned()
                        .or_else(|| Some(default_clip_plugin_graph_json(audio_ins, audio_outs))),
                });
            }
        }
    }

    if let Some(midi_clips) = midi.get("clips").and_then(Value::as_array) {
        for clip in midi_clips {
            if clip
                .get("grouped_clips")
                .and_then(Value::as_array)
                .is_some_and(|children| !children.is_empty())
            {
                actions.push(Action::AddGroupedClip {
                    track_name: name.clone(),
                    kind: Kind::MIDI,
                    audio_clip: None,
                    midi_clip: Some(parse_midi_clip_data(clip)?),
                });
            } else {
                actions.push(Action::AddClip {
                    name: clip
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    track_name: name.clone(),
                    start: clip.get("start").and_then(Value::as_u64).unwrap_or(0) as usize,
                    length: clip.get("length").and_then(Value::as_u64).unwrap_or(0) as usize,
                    offset: clip.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize,
                    input_channel: clip
                        .get("input_channel")
                        .and_then(Value::as_u64)
                        .unwrap_or(0) as usize,
                    muted: clip.get("muted").and_then(Value::as_bool).unwrap_or(false),
                    peaks_file: None,
                    kind: Kind::MIDI,
                    fade_enabled: clip
                        .get("fade_enabled")
                        .and_then(Value::as_bool)
                        .unwrap_or(true),
                    fade_in_samples: clip
                        .get("fade_in_samples")
                        .and_then(Value::as_u64)
                        .unwrap_or(240) as usize,
                    fade_out_samples: clip
                        .get("fade_out_samples")
                        .and_then(Value::as_u64)
                        .unwrap_or(240) as usize,
                    source_name: None,
                    source_offset: None,
                    source_length: None,
                    preview_name: None,
                    pitch_correction_points: vec![],
                    pitch_correction_frame_likeness: None,
                    pitch_correction_inertia_ms: None,
                    pitch_correction_formant_compensation: None,
                    plugin_graph_json: None,
                });
            }
        }
    }

    Ok(())
}

fn get_required_str<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Missing string field '{key}'"))
}

fn get_required_usize(value: &Value, key: &str, track_name: &str) -> Result<usize, String> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .ok_or_else(|| format!("Track '{track_name}' is missing numeric field '{key}'"))
}

fn parse_export_metadata(value: Option<&Value>) -> ExportMetadata {
    let Some(metadata) = value.and_then(Value::as_object) else {
        return ExportMetadata::default();
    };
    ExportMetadata {
        author: metadata
            .get("author")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        album: metadata
            .get("album")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        year: metadata
            .get("year")
            .and_then(Value::as_u64)
            .map(|value| value as u32),
        track_number: metadata
            .get("track_number")
            .and_then(Value::as_u64)
            .map(|value| value as u32),
        genre: metadata
            .get("genre")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    }
}

fn parse_export_track(track: &Value) -> Result<ExportTrack, String> {
    let name = get_required_str(track, "name")?.to_string();
    let audio = track
        .get("audio")
        .ok_or_else(|| format!("Track '{name}' is missing audio section"))?;
    let output_ports = get_required_usize(audio, "outs", &name)?.max(1);
    let audio_clips = audio
        .get("clips")
        .and_then(Value::as_array)
        .map(|clips| {
            clips
                .iter()
                .map(parse_audio_clip_data)
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();
    Ok(ExportTrack {
        name,
        level: track.get("level").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        balance: track.get("balance").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        muted: track.get("muted").and_then(Value::as_bool).unwrap_or(false),
        soloed: track
            .get("soloed")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        output_ports,
        audio_clips,
    })
}

fn parse_export_connections(value: Option<&Value>) -> Result<Vec<ExportConnection>, String> {
    let Some(connections) = value.and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    connections
        .iter()
        .map(|connection| {
            Ok(ExportConnection {
                from_track: get_required_str(connection, "from_track")?.to_string(),
                from_port: connection
                    .get("from_port")
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as usize,
                to_track: get_required_str(connection, "to_track")?.to_string(),
                to_port: connection
                    .get("to_port")
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as usize,
                kind: parse_kind(connection.get("kind"))
                    .ok_or_else(|| "Failed to parse connection kind".to_string())?,
            })
        })
        .collect()
}

fn push_optional_f32<F>(actions: &mut Vec<Action>, track: &Value, key: &str, build: F)
where
    F: FnOnce(f32) -> Action,
{
    if let Some(value) = track.get(key).and_then(Value::as_f64)
        && value.is_finite()
    {
        actions.push(build(value as f32));
    }
}

fn push_optional_toggle<F>(actions: &mut Vec<Action>, track: &Value, key: &str, build: F)
where
    F: FnOnce() -> Action,
{
    if track.get(key).and_then(Value::as_bool).unwrap_or(false) {
        actions.push(build());
    }
}

fn push_track_midi_binding(
    actions: &mut Vec<Action>,
    track_name: &str,
    target: TrackMidiLearnTarget,
    value: Option<&Value>,
) {
    let binding = value
        .cloned()
        .and_then(|value| serde_json::from_value::<Option<MidiLearnBinding>>(value).ok())
        .flatten();
    if let Some(binding) = binding {
        actions.push(Action::TrackSetMidiLearnBinding {
            track_name: track_name.to_string(),
            target,
            binding: Some(binding),
        });
    }
}

fn parse_pitch_correction_points(clip: &Value) -> Vec<PitchCorrectionPointData> {
    clip.get("pitch_correction_points")
        .and_then(Value::as_array)
        .map(|points| {
            points
                .iter()
                .map(|point| PitchCorrectionPointData {
                    start_sample: point
                        .get("start_sample")
                        .and_then(Value::as_u64)
                        .unwrap_or(0) as usize,
                    length_samples: point
                        .get("length_samples")
                        .and_then(Value::as_u64)
                        .unwrap_or(0) as usize,
                    detected_midi_pitch: point
                        .get("detected_midi_pitch")
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0) as f32,
                    target_midi_pitch: point
                        .get("target_midi_pitch")
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0) as f32,
                    clarity: point.get("clarity").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_audio_clip_data(clip: &Value) -> Result<AudioClipData, String> {
    let mut data = AudioClipData {
        name: clip
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        start: clip.get("start").and_then(Value::as_u64).unwrap_or(0) as usize,
        length: clip.get("length").and_then(Value::as_u64).unwrap_or(0) as usize,
        offset: clip.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize,
        input_channel: clip
            .get("input_channel")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize,
        muted: clip.get("muted").and_then(Value::as_bool).unwrap_or(false),
        peaks_file: clip
            .get("peaks_file")
            .and_then(Value::as_str)
            .map(str::to_string),
        fade_enabled: clip
            .get("fade_enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        fade_in_samples: clip
            .get("fade_in_samples")
            .and_then(Value::as_u64)
            .unwrap_or(240) as usize,
        fade_out_samples: clip
            .get("fade_out_samples")
            .and_then(Value::as_u64)
            .unwrap_or(240) as usize,
        preview_name: clip
            .get("pitch_correction_preview_name")
            .and_then(Value::as_str)
            .map(str::to_string),
        source_name: clip
            .get("pitch_correction_source_name")
            .and_then(Value::as_str)
            .map(str::to_string),
        source_offset: clip
            .get("pitch_correction_source_offset")
            .and_then(Value::as_u64)
            .map(|value| value as usize),
        source_length: clip
            .get("pitch_correction_source_length")
            .and_then(Value::as_u64)
            .map(|value| value as usize),
        pitch_correction_points: parse_pitch_correction_points(clip),
        pitch_correction_frame_likeness: clip
            .get("pitch_correction_frame_likeness")
            .and_then(Value::as_f64)
            .map(|value| value as f32),
        pitch_correction_inertia_ms: clip
            .get("pitch_correction_inertia_ms")
            .and_then(Value::as_u64)
            .map(|value| value as u16),
        pitch_correction_formant_compensation: clip
            .get("pitch_correction_formant_compensation")
            .and_then(Value::as_bool),
        plugin_graph_json: clip
            .get("plugin_graph_json")
            .filter(|value| !value.is_null())
            .cloned(),
        grouped_clips: Vec::new(),
    };
    if let Some(children) = clip.get("grouped_clips").and_then(Value::as_array) {
        for child in children {
            data.grouped_clips.push(parse_audio_clip_data(child)?);
        }
    }
    Ok(data)
}

fn parse_midi_clip_data(clip: &Value) -> Result<MidiClipData, String> {
    let mut data = MidiClipData {
        name: clip
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        start: clip.get("start").and_then(Value::as_u64).unwrap_or(0) as usize,
        length: clip.get("length").and_then(Value::as_u64).unwrap_or(0) as usize,
        offset: clip.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize,
        input_channel: clip
            .get("input_channel")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize,
        muted: clip.get("muted").and_then(Value::as_bool).unwrap_or(false),
        grouped_clips: Vec::new(),
    };
    if let Some(children) = clip.get("grouped_clips").and_then(Value::as_array) {
        for child in children {
            data.grouped_clips.push(parse_midi_clip_data(child)?);
        }
    }
    Ok(data)
}

fn default_clip_plugin_graph_json(audio_ins: usize, audio_outs: usize) -> Value {
    let ports = audio_ins.max(audio_outs).max(1);
    let mut connections = Vec::with_capacity(ports);
    for port in 0..ports {
        connections.push(serde_json::json!({
            "from_node": {"type": "track_input"},
            "from_port": port.min(audio_ins.saturating_sub(1)),
            "to_node": {"type": "track_output"},
            "to_port": port.min(audio_outs.saturating_sub(1)),
            "kind": "audio",
        }));
    }
    serde_json::json!({
        "plugins": [],
        "connections": connections,
    })
}

fn push_connection_restore_actions(
    actions: &mut Vec<Action>,
    connections: Option<&Value>,
) -> Result<(), String> {
    let Some(connections) = connections.and_then(Value::as_array) else {
        return Ok(());
    };
    let mut saved_connections = Vec::with_capacity(connections.len());
    for connection in connections {
        let kind = parse_kind(connection.get("kind"))
            .ok_or_else(|| "Failed to parse connection kind".to_string())?;
        let from_track = connection
            .get("from_track")
            .and_then(Value::as_str)
            .ok_or_else(|| "Connection is missing from_track".to_string())?
            .to_string();
        let to_track = connection
            .get("to_track")
            .and_then(Value::as_str)
            .ok_or_else(|| "Connection is missing to_track".to_string())?
            .to_string();
        let from_port = connection
            .get("from_port")
            .and_then(Value::as_u64)
            .ok_or_else(|| "Connection is missing from_port".to_string())?
            as usize;
        let to_port = connection
            .get("to_port")
            .and_then(Value::as_u64)
            .ok_or_else(|| "Connection is missing to_port".to_string())?
            as usize;
        saved_connections.push(SavedConnection {
            from_track,
            from_port,
            to_track,
            to_port,
            kind,
        });
    }
    let mut opened_midi_inputs = BTreeSet::new();
    let mut opened_midi_outputs = BTreeSet::new();
    for connection in &saved_connections {
        if let Some(device) = connection.from_track.strip_prefix("midi:hw:in:")
            && opened_midi_inputs.insert(device.to_string())
        {
            actions.push(Action::OpenMidiInputDevice(device.to_string()));
        }
        if let Some(device) = connection.to_track.strip_prefix("midi:hw:out:")
            && opened_midi_outputs.insert(device.to_string())
        {
            actions.push(Action::OpenMidiOutputDevice(device.to_string()));
        }
    }
    for connection in saved_connections {
        actions.push(Action::Connect {
            from_track: connection.from_track,
            from_port: connection.from_port,
            to_track: connection.to_track,
            to_port: connection.to_port,
            kind: connection.kind,
        });
    }
    Ok(())
}

#[cfg(unix)]
fn push_track_graph_restore_actions(
    actions: &mut Vec<Action>,
    graphs: Option<&Value>,
) -> Result<(), String> {
    let Some(graphs) = graphs.and_then(Value::as_object) else {
        return Ok(());
    };
    for (track_name, graph) in graphs {
        actions.push(Action::TrackClearDefaultPassthrough {
            track_name: track_name.clone(),
        });
        if let Some(connections) = graph.get("connections").and_then(Value::as_array) {
            for connection in connections {
                let Some(kind) = parse_kind(connection.get("kind")) else {
                    continue;
                };
                let Some(from_node) = parse_plugin_node(connection.get("from_node")) else {
                    continue;
                };
                let Some(to_node) = parse_plugin_node(connection.get("to_node")) else {
                    continue;
                };
                let from_port = connection
                    .get("from_port")
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as usize;
                let to_port = connection
                    .get("to_port")
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as usize;
                match kind {
                    Kind::Audio => actions.push(Action::TrackConnectPluginAudio {
                        track_name: track_name.clone(),
                        from_node,
                        from_port,
                        to_node,
                        to_port,
                    }),
                    Kind::MIDI => actions.push(Action::TrackConnectPluginMidi {
                        track_name: track_name.clone(),
                        from_node,
                        from_port,
                        to_node,
                        to_port,
                    }),
                }
            }
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn push_track_graph_restore_actions(
    _actions: &mut Vec<Action>,
    _graphs: Option<&Value>,
) -> Result<(), String> {
    Ok(())
}

#[cfg(unix)]
fn parse_kind(value: Option<&Value>) -> Option<Kind> {
    match value.and_then(Value::as_str) {
        Some("audio") | Some("Audio") => Some(Kind::Audio),
        Some("midi") | Some("MIDI") => Some(Kind::MIDI),
        _ => None,
    }
}

#[cfg(unix)]
fn parse_plugin_node(value: Option<&Value>) -> Option<PluginGraphNode> {
    let value = value?;
    if let Some(text) = value.as_str() {
        return match text {
            "TrackInput" => Some(PluginGraphNode::TrackInput),
            "TrackOutput" => Some(PluginGraphNode::TrackOutput),
            _ => None,
        };
    }
    match value.get("type").and_then(Value::as_str) {
        Some("track_input") => Some(PluginGraphNode::TrackInput),
        Some("track_output") => Some(PluginGraphNode::TrackOutput),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_session_restore_actions_parses_tracks_and_connections() {
        let dir =
            std::env::temp_dir().join(format!("maolan-cli-support-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        std::fs::write(
            dir.join("session.json"),
            serde_json::json!({
                "transport": {
                    "loop_range_samples": null,
                    "loop_enabled": false,
                    "punch_range_samples": null,
                    "punch_enabled": false,
                    "tempo": 140.0,
                    "time_signature_num": 7,
                    "time_signature_denom": 8
                },
                "tracks": [{
                    "name": "Track 1",
                    "audio": {"ins": 2, "outs": 2, "clips": []},
                    "midi": {"ins": 1, "outs": 1, "clips": []},
                    "level": 1.5,
                    "balance": -0.25,
                    "armed": true,
                    "muted": false,
                    "soloed": true,
                    "input_monitor": true,
                    "disk_monitor": false,
                    "midi_lane_channels": [null, 3],
                    "midi_learn_volume": null
                }],
                "connections": [{
                    "from_track": "midi:hw:in:dev-in",
                    "from_port": 0,
                    "to_track": "Track 1",
                    "to_port": 0,
                    "kind": "MIDI"
                },{
                    "from_track": "Track 1",
                    "from_port": 0,
                    "to_track": "midi:hw:out:dev-out",
                    "to_port": 0,
                    "kind": "MIDI"
                }],
                "graphs": {
                    "Track 1": {
                        "plugins": [],
                        "connections": [{
                            "from_node": {"type": "track_input"},
                            "from_port": 0,
                            "to_node": {"type": "track_output"},
                            "to_port": 0,
                            "kind": "audio"
                        }]
                    }
                }
            })
            .to_string(),
        )
        .expect("write session");

        let actions = load_session_restore_actions(&dir).expect("restore actions");

        assert!(actions.iter().any(|action| matches!(
            action,
            Action::AddTrack { name, audio_ins, audio_outs, midi_ins, midi_outs }
            if name == "Track 1" && *audio_ins == 2 && *audio_outs == 2 && *midi_ins == 1 && *midi_outs == 1
        )));
        assert!(actions.iter().any(
            |action| matches!(action, Action::OpenMidiInputDevice(device) if device == "dev-in")
        ));
        assert!(actions.iter().any(
            |action| matches!(action, Action::OpenMidiOutputDevice(device) if device == "dev-out")
        ));
        assert!(actions.iter().any(
            |action| matches!(action, Action::TrackConnectPluginAudio { track_name, .. } if track_name == "Track 1")
        ));
        assert_eq!(
            actions
                .iter()
                .filter(|action| matches!(action, Action::TrackToggleDiskMonitor(name) if name == "Track 1"))
                .count(),
            1
        );
        assert!(actions.iter().any(
            |action| matches!(action, Action::Connect { from_track, to_track, kind, .. } if from_track == "Track 1" && to_track == "midi:hw:out:dev-out" && *kind == Kind::MIDI)
        ));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_session_end_sample_uses_latest_audio_midi_and_grouped_clip_end() {
        let dir = std::env::temp_dir().join(format!(
            "maolan-cli-support-end-sample-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        std::fs::write(
            dir.join("session.json"),
            serde_json::json!({
                "tracks": [{
                    "name": "Track 1",
                    "audio": {"ins": 2, "outs": 2, "clips": [{
                        "name": "Audio A",
                        "start": 100,
                        "length": 50
                    }, {
                        "name": "Grouped",
                        "start": 10,
                        "length": 5,
                        "grouped_clips": [{
                            "name": "Child",
                            "start": 400,
                            "length": 25
                        }]
                    }]},
                    "midi": {"ins": 1, "outs": 1, "clips": [{
                        "name": "Midi A",
                        "start": 250,
                        "length": 100
                    }]}
                }]
            })
            .to_string(),
        )
        .expect("write session");

        assert_eq!(load_session_end_sample(&dir).expect("end sample"), 425);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
