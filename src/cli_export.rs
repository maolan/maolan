use crate::cli_support::{ExportMetadata, ExportSessionData, ExportTrack};
use maolan_engine::{kind::Kind, message::AudioClipData};
use std::{
    collections::{BTreeSet, HashMap},
    fmt, fs, io,
    num::{NonZeroU8, NonZeroU32},
    path::{Path, PathBuf},
    time::Duration,
};

use ebur128::{EbuR128, Mode as LoudnessMode};
use flacenc::bitsink::ByteSink;
use flacenc::component::BitRepr;
use flacenc::error::Verify;
use mp3lame_encoder::{
    Bitrate as Mp3Bitrate, Builder as Mp3Builder, FlushNoGap, InterleavedPcm, Quality as Mp3Quality,
};
use vorbis_rs::{VorbisBitrateManagementStrategy, VorbisEncoderBuilder};
use wavers::Wav;

pub const STANDARD_EXPORT_SAMPLE_RATES: [u32; 12] = [
    8000, 11025, 16000, 22050, 32000, 44100, 48000, 88200, 96000, 176400, 192000, 384000,
];
pub const EXPORT_MP3_BITRATES_KBPS: [u16; 7] = [96, 128, 160, 192, 224, 256, 320];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Wav,
    Mp3,
    Ogg,
    Flac,
}

impl fmt::Display for ExportFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Wav => write!(f, "WAV"),
            Self::Mp3 => write!(f, "MP3"),
            Self::Ogg => write!(f, "OGG"),
            Self::Flac => write!(f, "FLAC"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportMp3Mode {
    Cbr,
    Vbr,
}

impl fmt::Display for ExportMp3Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cbr => write!(f, "CBR"),
            Self::Vbr => write!(f, "VBR"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportNormalizeMode {
    Peak,
    Loudness,
}

impl fmt::Display for ExportNormalizeMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Peak => write!(f, "Peak"),
            Self::Loudness => write!(f, "Loudness"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportRenderMode {
    Mixdown,
    StemsPostFader,
    StemsPreFader,
}

impl fmt::Display for ExportRenderMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mixdown => write!(f, "Mixdown"),
            Self::StemsPostFader => write!(f, "Stems (Post-Fader)"),
            Self::StemsPreFader => write!(f, "Stems (Pre-Fader)"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportBitDepth {
    Int16,
    Int24,
    Int32,
    Float32,
}

impl fmt::Display for ExportBitDepth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int16 => write!(f, "16-bit PCM"),
            Self::Int24 => write!(f, "24-bit PCM"),
            Self::Int32 => write!(f, "32-bit PCM"),
            Self::Float32 => write!(f, "32-bit float"),
        }
    }
}

pub const EXPORT_MP3_MODE_ALL: [ExportMp3Mode; 2] = [ExportMp3Mode::Cbr, ExportMp3Mode::Vbr];
pub const EXPORT_RENDER_MODE_ALL: [ExportRenderMode; 3] = [
    ExportRenderMode::Mixdown,
    ExportRenderMode::StemsPostFader,
    ExportRenderMode::StemsPreFader,
];
pub const EXPORT_BIT_DEPTH_ALL: [ExportBitDepth; 4] = [
    ExportBitDepth::Int16,
    ExportBitDepth::Int24,
    ExportBitDepth::Int32,
    ExportBitDepth::Float32,
];
pub const EXPORT_NORMALIZE_MODE_ALL: [ExportNormalizeMode; 2] =
    [ExportNormalizeMode::Peak, ExportNormalizeMode::Loudness];

#[derive(Debug, Clone)]
pub struct ExportSettings {
    pub sample_rate_hz: u32,
    pub format_wav: bool,
    pub format_mp3: bool,
    pub format_ogg: bool,
    pub format_flac: bool,
    pub bit_depth: ExportBitDepth,
    pub mp3_mode: ExportMp3Mode,
    pub mp3_bitrate_kbps: u16,
    pub ogg_quality: f32,
    pub render_mode: ExportRenderMode,
    pub hw_out_ports: BTreeSet<usize>,
    pub realtime_fallback: bool,
    pub normalize: bool,
    pub normalize_mode: ExportNormalizeMode,
    pub normalize_dbfs: f32,
    pub normalize_lufs: f32,
    pub normalize_dbtp: f32,
    pub normalize_tp_limiter: bool,
    pub master_limiter: bool,
    pub master_limiter_ceiling_dbtp: f32,
}

impl ExportSettings {
    pub fn new(default_sample_rate_hz: u32, hw_output_channels: usize) -> Self {
        Self {
            sample_rate_hz: default_sample_rate_hz,
            format_wav: true,
            format_mp3: false,
            format_ogg: false,
            format_flac: false,
            bit_depth: ExportBitDepth::Int24,
            mp3_mode: ExportMp3Mode::Cbr,
            mp3_bitrate_kbps: 320,
            ogg_quality: 0.6,
            render_mode: ExportRenderMode::Mixdown,
            hw_out_ports: default_hw_out_ports(hw_output_channels),
            realtime_fallback: false,
            normalize: false,
            normalize_mode: ExportNormalizeMode::Peak,
            normalize_dbfs: 0.0,
            normalize_lufs: -23.0,
            normalize_dbtp: -1.0,
            normalize_tp_limiter: true,
            master_limiter: true,
            master_limiter_ceiling_dbtp: -1.0,
        }
    }

    pub fn selected_formats(&self) -> Vec<ExportFormat> {
        let mut formats = Vec::new();
        if self.format_wav {
            formats.push(ExportFormat::Wav);
        }
        if self.format_mp3 {
            formats.push(ExportFormat::Mp3);
        }
        if self.format_ogg {
            formats.push(ExportFormat::Ogg);
        }
        if self.format_flac {
            formats.push(ExportFormat::Flac);
        }
        formats
    }

    pub fn normalize_hw_out_ports(&mut self, hw_output_channels: usize) {
        let available: BTreeSet<usize> = (0..hw_output_channels).collect();
        self.hw_out_ports.retain(|port| available.contains(port));
        if self.hw_out_ports.is_empty() {
            self.hw_out_ports = default_hw_out_ports(hw_output_channels);
        }
    }
}

fn default_hw_out_ports(hw_output_channels: usize) -> BTreeSet<usize> {
    (0..hw_output_channels).take(2).collect()
}

pub fn export_bit_depth_options(formats: &[ExportFormat]) -> Vec<ExportBitDepth> {
    if formats
        .iter()
        .any(|f| matches!(f, ExportFormat::Wav | ExportFormat::Flac))
    {
        EXPORT_BIT_DEPTH_ALL.to_vec()
    } else {
        vec![ExportBitDepth::Float32]
    }
}

pub fn export_mp3_supported(settings: &ExportSettings, session: &ExportSessionData) -> bool {
    export_max_channels(settings, session) <= 2
}

pub fn export_max_channels(settings: &ExportSettings, session: &ExportSessionData) -> usize {
    if matches!(settings.render_mode, ExportRenderMode::Mixdown) {
        settings.hw_out_ports.len()
    } else {
        session
            .tracks
            .iter()
            .map(|track| track.output_ports.max(1))
            .max()
            .unwrap_or(0)
    }
}

pub fn validate_export_settings(
    settings: &ExportSettings,
    session: &ExportSessionData,
) -> Result<(), String> {
    if settings.selected_formats().is_empty() {
        return Err("Select at least one export format".to_string());
    }
    if settings.format_mp3 && !export_mp3_supported(settings, session) {
        return Err("MP3 export supports only mono or stereo".to_string());
    }
    if matches!(settings.render_mode, ExportRenderMode::Mixdown) && settings.hw_out_ports.is_empty()
    {
        return Err("Select at least one hw:out port for mixdown export".to_string());
    }
    if !(-20.0..=0.0).contains(&settings.master_limiter_ceiling_dbtp) {
        return Err("Master limiter ceiling must be between -20.0 and 0.0 dBTP".to_string());
    }
    if !(-0.1..=1.0).contains(&settings.ogg_quality) {
        return Err("OGG quality must be between -0.1 and 1.0".to_string());
    }
    if settings.normalize {
        match settings.normalize_mode {
            ExportNormalizeMode::Peak => {
                if !(-60.0..=0.0).contains(&settings.normalize_dbfs) {
                    return Err("Normalize target must be between -60.0 and 0.0 dBFS".to_string());
                }
            }
            ExportNormalizeMode::Loudness => {
                if !(-70.0..=-5.0).contains(&settings.normalize_lufs) {
                    return Err("LUFS target must be between -70.0 and -5.0".to_string());
                }
                if !(-20.0..=0.0).contains(&settings.normalize_dbtp) {
                    return Err("dBTP ceiling must be between -20.0 and 0.0".to_string());
                }
            }
        }
    }
    if session.tracks.is_empty() {
        return Err("No tracks found. Nothing to export.".to_string());
    }
    Ok(())
}

pub fn default_export_base_path(session_dir: &Path) -> PathBuf {
    session_dir.join("export")
}

pub async fn export_session<F>(
    session: &ExportSessionData,
    session_root: &Path,
    export_base_path: &Path,
    settings: &ExportSettings,
    mut progress_callback: F,
) -> io::Result<Vec<PathBuf>>
where
    F: FnMut(f32, Option<String>),
{
    let mut tracks = session.tracks.clone();
    let connections = session.connections.clone();
    let total_length = tracks
        .iter()
        .flat_map(|track| track.audio_clips.iter())
        .map(audio_clip_end)
        .max()
        .unwrap_or(0);
    if total_length == 0 {
        return Err(io::Error::other("No audio clips found. Nothing to export."));
    }

    let export_formats = settings.selected_formats();
    let codec = ExportCodecSettings {
        mp3_mode: settings.mp3_mode,
        mp3_bitrate_kbps: settings.mp3_bitrate_kbps,
        ogg_quality: settings.ogg_quality,
    };
    let has_solo = tracks.iter().any(|track| track.soloed);
    let metadata = session.metadata.clone();

    progress_callback(0.0, Some("Analyzing tracks".to_string()));
    tokio::task::yield_now().await;

    if matches!(settings.render_mode, ExportRenderMode::Mixdown) {
        let output_ports: Vec<usize> = settings.hw_out_ports.iter().copied().collect();
        let output_channels = output_ports.len().max(1);
        let hw_out_channel_map: HashMap<usize, usize> = output_ports
            .iter()
            .enumerate()
            .map(|(channel_idx, port)| (*port, channel_idx))
            .collect();
        let mut mixed_buffer = vec![0.0_f32; total_length * output_channels];
        let track_count = tracks.len().max(1);
        for (track_idx, track) in tracks.iter_mut().enumerate() {
            if track.muted || (has_solo && !track.soloed) {
                continue;
            }
            let progress_start = 0.1 + (track_idx as f32 / track_count as f32) * 0.7;
            let progress_span = 0.7 / track_count as f32;
            progress_callback(
                progress_start,
                Some(format!("Processing track: {}", track.name)),
            );
            tokio::task::yield_now().await;

            let routed_ports: Vec<(usize, usize)> = connections
                .iter()
                .filter(|conn| {
                    conn.kind == Kind::Audio
                        && conn.from_track == track.name
                        && conn.to_track == "hw:out"
                })
                .filter_map(|conn| {
                    hw_out_channel_map
                        .get(&conn.to_port)
                        .map(|dest_idx| (conn.from_port, *dest_idx))
                })
                .collect();
            if routed_ports.is_empty() {
                continue;
            }
            let track_buffer = mix_track_clips_to_channels(
                &track.audio_clips,
                session_root,
                total_length,
                track.output_ports,
                track.level,
                track.balance,
                true,
            )?;
            for frame in 0..total_length {
                let track_base = frame * track.output_ports.max(1);
                let mixed_base = frame * output_channels;
                for (source_port, dest_channel) in &routed_ports {
                    if *source_port >= track.output_ports.max(1) {
                        continue;
                    }
                    mixed_buffer[mixed_base + *dest_channel] +=
                        track_buffer[track_base + *source_port];
                }
            }
            progress_callback(
                progress_start + progress_span,
                Some(format!("Finished: {}", track.name)),
            );
        }

        if settings.realtime_fallback {
            progress_callback(0.82, Some("Real-time fallback pacing".to_string()));
            let seconds = (total_length as f64 / settings.sample_rate_hz.max(1) as f64).max(0.0);
            tokio::time::sleep(Duration::from_secs_f64(seconds)).await;
        }

        if settings.normalize {
            apply_export_normalization(
                &mut mixed_buffer,
                ExportNormalizeParams {
                    mode: settings.normalize_mode,
                    target_dbfs: settings.normalize_dbfs,
                    target_lufs: settings.normalize_lufs,
                    true_peak_dbtp: settings.normalize_dbtp,
                    tp_limiter: settings.normalize_tp_limiter,
                    sample_rate: settings.sample_rate_hz as i32,
                    output_channels,
                },
            )?;
        }
        apply_master_limiter(
            &mut mixed_buffer,
            settings.master_limiter,
            settings.master_limiter_ceiling_dbtp,
        );

        let base_path = export_base_path.to_path_buf();
        let write_span = 0.1 / export_formats.len().max(1) as f32;
        let mut written = Vec::new();
        for (format_idx, format) in export_formats.iter().enumerate() {
            progress_callback(
                (0.9 + write_span * format_idx as f32).clamp(0.0, 0.99),
                Some(format!("Writing {} ({})", format, settings.bit_depth)),
            );
            let out_path = base_path.with_extension(export_format_extension(*format));
            write_export_audio(ExportWriteRequest {
                export_path: &out_path,
                mixed_buffer: &mixed_buffer,
                sample_rate: settings.sample_rate_hz as i32,
                output_channels,
                bit_depth: settings.bit_depth,
                format: *format,
                codec,
                metadata: &metadata,
            })?;
            written.push(out_path);
        }
        progress_callback(1.0, Some("Complete".to_string()));
        return Ok(written);
    }

    let stem_mode_label = if matches!(settings.render_mode, ExportRenderMode::StemsPreFader) {
        "pre"
    } else {
        "post"
    };
    let export_parent = export_base_path.parent().unwrap_or_else(|| Path::new("."));
    let export_stem = export_base_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("export");
    let stem_dir = export_parent.join(format!("{export_stem}_stems"));
    fs::create_dir_all(&stem_dir)?;

    let selected_tracks: Vec<&ExportTrack> = tracks
        .iter()
        .filter(|track| !track.muted && (!has_solo || track.soloed))
        .collect();
    if selected_tracks.is_empty() {
        return Err(io::Error::other("No tracks are eligible for stem export"));
    }

    let mut written = Vec::new();
    for (idx, track) in selected_tracks.iter().enumerate() {
        progress_callback(
            0.1 + (idx as f32 / selected_tracks.len().max(1) as f32) * 0.75,
            Some(format!("Rendering stem: {}", track.name)),
        );
        let output_channels = track.output_ports.max(1);
        let mut stem_buffer = mix_track_clips_to_channels(
            &track.audio_clips,
            session_root,
            total_length,
            output_channels,
            track.level,
            track.balance,
            matches!(settings.render_mode, ExportRenderMode::StemsPostFader),
        )?;
        if settings.normalize {
            apply_export_normalization(
                &mut stem_buffer,
                ExportNormalizeParams {
                    mode: settings.normalize_mode,
                    target_dbfs: settings.normalize_dbfs,
                    target_lufs: settings.normalize_lufs,
                    true_peak_dbtp: settings.normalize_dbtp,
                    tp_limiter: settings.normalize_tp_limiter,
                    sample_rate: settings.sample_rate_hz as i32,
                    output_channels,
                },
            )?;
        }
        apply_master_limiter(
            &mut stem_buffer,
            settings.master_limiter,
            settings.master_limiter_ceiling_dbtp,
        );
        for format in &export_formats {
            let stem_file = stem_dir.join(format!(
                "{}_{}.{}",
                sanitize_export_component(&track.name),
                stem_mode_label,
                export_format_extension(*format)
            ));
            write_export_audio(ExportWriteRequest {
                export_path: &stem_file,
                mixed_buffer: &stem_buffer,
                sample_rate: settings.sample_rate_hz as i32,
                output_channels,
                bit_depth: settings.bit_depth,
                format: *format,
                codec,
                metadata: &metadata,
            })?;
            written.push(stem_file);
        }
        if settings.realtime_fallback {
            let seconds = (total_length as f64 / settings.sample_rate_hz.max(1) as f64).max(0.0);
            tokio::time::sleep(Duration::from_secs_f64(seconds)).await;
        }
    }
    progress_callback(1.0, Some("Complete".to_string()));
    Ok(written)
}

fn audio_clip_end(clip: &AudioClipData) -> usize {
    if !clip.grouped_clips.is_empty() {
        clip.grouped_clips
            .iter()
            .map(audio_clip_end)
            .max()
            .unwrap_or(0)
    } else {
        clip.start + clip.length
    }
}

fn mix_track_clips_to_channels(
    clips: &[AudioClipData],
    session_root: &Path,
    total_length: usize,
    output_channels: usize,
    level_db: f32,
    balance: f32,
    apply_fader: bool,
) -> io::Result<Vec<f32>> {
    let output_channels = output_channels.max(1);
    let mut mixed = vec![0.0_f32; total_length * output_channels];
    let channel_gains = if apply_fader {
        let level_amp = 10.0_f32.powf(level_db / 20.0);
        if output_channels == 2 {
            vec![
                if balance <= 0.0 {
                    level_amp
                } else {
                    level_amp * (1.0 - balance)
                },
                if balance >= 0.0 {
                    level_amp
                } else {
                    level_amp * (1.0 + balance)
                },
            ]
        } else {
            vec![level_amp; output_channels]
        }
    } else {
        vec![1.0; output_channels]
    };
    for clip in clips {
        mix_clip_into_buffer(
            clip,
            session_root,
            &mut mixed,
            total_length,
            output_channels,
            &channel_gains,
        )?;
    }
    Ok(mixed)
}

fn mix_clip_into_buffer(
    clip: &AudioClipData,
    session_root: &Path,
    mixed: &mut [f32],
    total_length: usize,
    output_channels: usize,
    channel_gains: &[f32],
) -> io::Result<()> {
    if clip.muted {
        return Ok(());
    }
    if !clip.grouped_clips.is_empty() {
        for child in &clip.grouped_clips {
            mix_clip_into_buffer(
                child,
                session_root,
                mixed,
                total_length,
                output_channels,
                channel_gains,
            )?;
        }
        return Ok(());
    }
    let clip_path = resolve_audio_clip_path(clip, session_root);
    let mut wav = Wav::<f32>::from_path(&clip_path).map_err(|e| {
        io::Error::other(format!(
            "Failed to open WAV '{}': {}",
            clip_path.display(),
            e
        ))
    })?;
    let clip_channels = wav.n_channels().max(1) as usize;
    let samples: wavers::Samples<f32> = wav.read().map_err(|e| {
        io::Error::other(format!("WAV read error '{}': {}", clip_path.display(), e))
    })?;
    if samples.is_empty() {
        return Ok(());
    }
    let clip_frames = samples.len() / clip_channels;
    let offset_frame = clip.offset.min(clip_frames);
    let length_frames = clip.length.min(clip_frames.saturating_sub(offset_frame));
    for frame_idx in 0..length_frames {
        let src_frame = offset_frame + frame_idx;
        let dst_frame = clip.start + frame_idx;
        if dst_frame >= total_length {
            break;
        }
        let src_idx = src_frame * clip_channels;
        let dst_idx = dst_frame * output_channels;
        for out_ch in 0..output_channels {
            let source_sample = if clip_channels == 1 {
                samples[src_idx]
            } else {
                samples[src_idx + out_ch.min(clip_channels.saturating_sub(1))]
            };
            mixed[dst_idx + out_ch] += source_sample * channel_gains[out_ch];
        }
    }
    Ok(())
}

fn resolve_audio_clip_path(clip: &AudioClipData, session_root: &Path) -> PathBuf {
    let name = clip
        .preview_name
        .as_ref()
        .or(clip.source_name.as_ref())
        .unwrap_or(&clip.name);
    let path = PathBuf::from(name);
    if path.is_absolute() {
        path
    } else {
        session_root.join(path)
    }
}

fn apply_master_limiter(samples: &mut [f32], enabled: bool, ceiling_dbtp: f32) {
    if !enabled {
        return;
    }
    let ceiling_amp = 10.0_f32.powf(ceiling_dbtp / 20.0).clamp(0.0, 1.0);
    for sample in samples {
        *sample = sample.clamp(-ceiling_amp, ceiling_amp);
    }
}

fn export_format_extension(format: ExportFormat) -> &'static str {
    match format {
        ExportFormat::Wav => "wav",
        ExportFormat::Mp3 => "mp3",
        ExportFormat::Ogg => "ogg",
        ExportFormat::Flac => "flac",
    }
}

fn sanitize_export_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "track".to_string()
    } else {
        out
    }
}

#[derive(Clone, Copy)]
struct ExportCodecSettings {
    mp3_mode: ExportMp3Mode,
    mp3_bitrate_kbps: u16,
    ogg_quality: f32,
}

struct ExportWriteRequest<'a> {
    export_path: &'a Path,
    mixed_buffer: &'a [f32],
    sample_rate: i32,
    output_channels: usize,
    bit_depth: ExportBitDepth,
    format: ExportFormat,
    codec: ExportCodecSettings,
    metadata: &'a ExportMetadata,
}

#[derive(Clone, Copy)]
struct ExportNormalizeParams {
    mode: ExportNormalizeMode,
    target_dbfs: f32,
    target_lufs: f32,
    true_peak_dbtp: f32,
    tp_limiter: bool,
    sample_rate: i32,
    output_channels: usize,
}

fn write_export_audio(req: ExportWriteRequest<'_>) -> io::Result<()> {
    match req.format {
        ExportFormat::Wav => write_wav_with_bit_depth(
            req.export_path,
            req.mixed_buffer,
            req.sample_rate,
            req.output_channels,
            req.bit_depth,
        ),
        ExportFormat::Flac => write_flac_with_bit_depth(
            req.export_path,
            req.mixed_buffer,
            req.sample_rate,
            req.output_channels,
            req.bit_depth,
        ),
        ExportFormat::Mp3 => write_mp3(
            req.export_path,
            req.mixed_buffer,
            req.sample_rate,
            req.output_channels,
            req.codec,
            req.metadata,
        ),
        ExportFormat::Ogg => write_ogg_vorbis(
            req.export_path,
            req.mixed_buffer,
            req.sample_rate,
            req.output_channels,
            req.codec,
            req.metadata,
        ),
    }
}

fn write_wav_with_bit_depth(
    export_path: &Path,
    mixed_buffer: &[f32],
    sample_rate: i32,
    output_channels: usize,
    bit_depth: ExportBitDepth,
) -> io::Result<()> {
    match bit_depth {
        ExportBitDepth::Int16 => {
            let quantized: Vec<i16> = mixed_buffer
                .iter()
                .map(|s| {
                    (s.clamp(-1.0, 1.0) * i16::MAX as f32)
                        .round()
                        .clamp(i16::MIN as f32, i16::MAX as f32) as i16
                })
                .collect();
            wavers::write::<i16, _>(export_path, &quantized, sample_rate, output_channels as u16)
        }
        ExportBitDepth::Int24 => wavers::write::<i24::i24, _>(
            export_path,
            &mixed_buffer
                .iter()
                .map(|s| {
                    i24::i24::from_i32(
                        (s.clamp(-1.0, 1.0) * 8_388_607.0)
                            .round()
                            .clamp(-8_388_608.0, 8_388_607.0) as i32,
                    )
                })
                .collect::<Vec<i24::i24>>(),
            sample_rate,
            output_channels as u16,
        ),
        ExportBitDepth::Int32 => {
            let quantized: Vec<i32> = mixed_buffer
                .iter()
                .map(|s| {
                    (s.clamp(-1.0, 1.0) * i32::MAX as f32)
                        .round()
                        .clamp(i32::MIN as f32, i32::MAX as f32) as i32
                })
                .collect();
            wavers::write::<i32, _>(export_path, &quantized, sample_rate, output_channels as u16)
        }
        ExportBitDepth::Float32 => wavers::write::<f32, _>(
            export_path,
            mixed_buffer,
            sample_rate,
            output_channels as u16,
        ),
    }
    .map_err(|e| {
        io::Error::other(format!(
            "Failed to write '{}': {}",
            export_path.display(),
            e
        ))
    })
}

fn quantize_samples_for_bit_depth(
    mixed_buffer: &[f32],
    bit_depth: ExportBitDepth,
) -> (Vec<i32>, u8) {
    let (scale, min, max, bits_per_sample) = match bit_depth {
        ExportBitDepth::Int16 => (i16::MAX as f32, i16::MIN as f32, i16::MAX as f32, 16),
        ExportBitDepth::Int24 => (8_388_607.0, -8_388_608.0, 8_388_607.0, 24),
        ExportBitDepth::Int32 => (i32::MAX as f32, i32::MIN as f32, i32::MAX as f32, 32),
        ExportBitDepth::Float32 => (8_388_607.0, -8_388_608.0, 8_388_607.0, 24),
    };
    (
        mixed_buffer
            .iter()
            .map(|s| (s.clamp(-1.0, 1.0) * scale).round().clamp(min, max) as i32)
            .collect(),
        bits_per_sample,
    )
}

fn write_flac_with_bit_depth(
    export_path: &Path,
    mixed_buffer: &[f32],
    sample_rate: i32,
    output_channels: usize,
    bit_depth: ExportBitDepth,
) -> io::Result<()> {
    let (quantized, bits_per_sample) = quantize_samples_for_bit_depth(mixed_buffer, bit_depth);
    let config = flacenc::config::Encoder::default()
        .into_verified()
        .map_err(|e| io::Error::other(format!("Invalid FLAC encoder config: {e:?}")))?;
    let source = flacenc::source::MemSource::from_samples(
        &quantized,
        output_channels,
        bits_per_sample as usize,
        sample_rate.max(1) as usize,
    );
    let stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size)
        .map_err(|e| io::Error::other(format!("FLAC encode failed: {e}")))?;
    let mut sink = ByteSink::new();
    stream
        .write(&mut sink)
        .map_err(|e| io::Error::other(format!("FLAC bitstream write failed: {e}")))?;
    fs::write(export_path, sink.as_slice()).map_err(|e| {
        io::Error::other(format!(
            "Failed to write '{}': {}",
            export_path.display(),
            e
        ))
    })
}

fn write_mp3(
    export_path: &Path,
    mixed_buffer: &[f32],
    sample_rate: i32,
    output_channels: usize,
    codec: ExportCodecSettings,
    metadata: &ExportMetadata,
) -> io::Result<()> {
    if output_channels != 1 && output_channels != 2 {
        return Err(io::Error::other(format!(
            "MP3 export supports only mono/stereo, got {} channels",
            output_channels
        )));
    }
    let mut builder = Mp3Builder::new()
        .ok_or_else(|| io::Error::other("Failed to initialize MP3 encoder builder"))?;
    builder
        .set_num_channels(output_channels as u8)
        .map_err(|e| io::Error::other(format!("MP3 set channels failed: {e}")))?;
    builder
        .set_sample_rate(sample_rate.max(1) as u32)
        .map_err(|e| io::Error::other(format!("MP3 set sample rate failed: {e}")))?;
    let bitrate = match codec.mp3_bitrate_kbps {
        96 => Mp3Bitrate::Kbps96,
        128 => Mp3Bitrate::Kbps128,
        160 => Mp3Bitrate::Kbps160,
        192 => Mp3Bitrate::Kbps192,
        224 => Mp3Bitrate::Kbps224,
        256 => Mp3Bitrate::Kbps256,
        _ => Mp3Bitrate::Kbps320,
    };
    builder
        .set_brate(bitrate)
        .map_err(|e| io::Error::other(format!("MP3 set bitrate failed: {e}")))?;
    if matches!(codec.mp3_mode, ExportMp3Mode::Vbr) {
        builder
            .set_vbr_mode(mp3lame_encoder::VbrMode::Mtrh)
            .map_err(|e| io::Error::other(format!("MP3 set VBR mode failed: {e}")))?;
        builder
            .set_vbr_quality(Mp3Quality::NearBest)
            .map_err(|e| io::Error::other(format!("MP3 set VBR quality failed: {e}")))?;
    } else {
        builder
            .set_vbr_mode(mp3lame_encoder::VbrMode::Off)
            .map_err(|e| io::Error::other(format!("MP3 set CBR mode failed: {e}")))?;
    }
    builder
        .set_quality(Mp3Quality::Best)
        .map_err(|e| io::Error::other(format!("MP3 set quality failed: {e}")))?;
    let id3_year = metadata
        .year
        .map(|value| value.to_string())
        .unwrap_or_default();
    let mut id3_comment = String::new();
    if let Some(track_number) = metadata.track_number {
        id3_comment.push_str(&format!("TRACKNUMBER={track_number};"));
    }
    if !metadata.genre.is_empty() {
        id3_comment.push_str(&format!("GENRE={};", metadata.genre));
    }
    builder
        .set_id3_tag(mp3lame_encoder::Id3Tag {
            title: b"",
            artist: metadata.author.as_bytes(),
            album: metadata.album.as_bytes(),
            album_art: &[],
            year: id3_year.as_bytes(),
            comment: id3_comment.as_bytes(),
        })
        .map_err(|e| io::Error::other(format!("Failed to set MP3 ID3 tag: {e:?}")))?;
    let mut encoder = builder
        .build()
        .map_err(|e| io::Error::other(format!("MP3 encoder build failed: {e}")))?;
    let mut out = Vec::with_capacity(mp3lame_encoder::max_required_buffer_size(
        mixed_buffer.len(),
    ));
    for chunk in mixed_buffer.chunks(4096 * output_channels.max(1)) {
        encoder
            .encode_to_vec(InterleavedPcm(chunk), &mut out)
            .map_err(|e| io::Error::other(format!("MP3 encode failed: {e}")))?;
    }
    encoder
        .flush_to_vec::<FlushNoGap>(&mut out)
        .map_err(|e| io::Error::other(format!("MP3 finalization failed: {e}")))?;
    fs::write(export_path, out).map_err(|e| {
        io::Error::other(format!(
            "Failed to write '{}': {}",
            export_path.display(),
            e
        ))
    })
}

fn write_ogg_vorbis(
    export_path: &Path,
    mixed_buffer: &[f32],
    sample_rate: i32,
    output_channels: usize,
    codec: ExportCodecSettings,
    metadata: &ExportMetadata,
) -> io::Result<()> {
    let mut out = Vec::new();
    let mut builder = VorbisEncoderBuilder::new(
        NonZeroU32::new(sample_rate.max(1) as u32)
            .ok_or_else(|| io::Error::other("Invalid sample rate for OGG"))?,
        NonZeroU8::new(output_channels as u8)
            .ok_or_else(|| io::Error::other("Invalid channel count for OGG"))?,
        &mut out,
    )
    .map_err(|e| io::Error::other(format!("OGG encoder init failed: {e}")))?;
    builder.bitrate_management_strategy(VorbisBitrateManagementStrategy::QualityVbr {
        target_quality: codec.ogg_quality.clamp(-0.1, 1.0),
    });
    if !metadata.author.is_empty() {
        builder
            .comment_tag("ARTIST", metadata.author.as_str())
            .map_err(|e| io::Error::other(format!("Failed to set OGG ARTIST tag: {e}")))?;
    }
    if !metadata.album.is_empty() {
        builder
            .comment_tag("ALBUM", metadata.album.as_str())
            .map_err(|e| io::Error::other(format!("Failed to set OGG ALBUM tag: {e}")))?;
    }
    if let Some(year) = metadata.year {
        builder
            .comment_tag("DATE", year.to_string())
            .map_err(|e| io::Error::other(format!("Failed to set OGG DATE tag: {e}")))?;
    }
    if let Some(track_number) = metadata.track_number {
        builder
            .comment_tag("TRACKNUMBER", track_number.to_string())
            .map_err(|e| io::Error::other(format!("Failed to set OGG TRACKNUMBER tag: {e}")))?;
    }
    if !metadata.genre.is_empty() {
        builder
            .comment_tag("GENRE", metadata.genre.as_str())
            .map_err(|e| io::Error::other(format!("Failed to set OGG GENRE tag: {e}")))?;
    }
    let mut encoder = builder
        .build()
        .map_err(|e| io::Error::other(format!("OGG encoder build failed: {e}")))?;
    for chunk in mixed_buffer.chunks(2048 * output_channels.max(1)) {
        let frames = chunk.len() / output_channels.max(1);
        if frames == 0 {
            continue;
        }
        let mut planar = vec![vec![0.0_f32; frames]; output_channels];
        for frame in 0..frames {
            for ch in 0..output_channels {
                planar[ch][frame] = chunk[frame * output_channels + ch];
            }
        }
        let block = planar.iter().map(Vec::as_slice).collect::<Vec<_>>();
        encoder
            .encode_audio_block(&block)
            .map_err(|e| io::Error::other(format!("OGG encode failed: {e}")))?;
    }
    encoder
        .finish()
        .map_err(|e| io::Error::other(format!("OGG finalization failed: {e}")))?;
    fs::write(export_path, out).map_err(|e| {
        io::Error::other(format!(
            "Failed to write '{}': {}",
            export_path.display(),
            e
        ))
    })
}

fn measure_lufs_and_true_peak(
    samples: &[f32],
    channels: usize,
    sample_rate: i32,
) -> io::Result<(f32, f32)> {
    let mut meter = EbuR128::new(
        channels as u32,
        sample_rate as u32,
        LoudnessMode::I | LoudnessMode::TRUE_PEAK,
    )
    .map_err(|e| io::Error::other(format!("Failed to initialize loudness meter: {e}")))?;
    meter
        .add_frames_f32(samples)
        .map_err(|e| io::Error::other(format!("Loudness analysis failed: {e}")))?;
    let lufs = meter
        .loudness_global()
        .map_err(|e| io::Error::other(format!("Failed to get integrated loudness: {e}")))?
        as f32;
    if !lufs.is_finite() {
        return Err(io::Error::other("Integrated loudness is not finite"));
    }
    let mut tp = 0.0_f32;
    for ch in 0..channels as u32 {
        tp = tp.max(
            meter
                .true_peak(ch)
                .map_err(|e| io::Error::other(format!("Failed to get true peak: {e}")))?
                as f32,
        );
    }
    Ok((lufs, tp))
}

fn apply_export_normalization(
    samples: &mut [f32],
    params: ExportNormalizeParams,
) -> io::Result<()> {
    match params.mode {
        ExportNormalizeMode::Peak => {
            let peak = samples
                .iter()
                .fold(0.0_f32, |acc, sample| acc.max(sample.abs()));
            if peak > 0.0 {
                let target_amp = 10.0_f32.powf(params.target_dbfs / 20.0).clamp(0.0, 1.0);
                let gain = target_amp / peak;
                for sample in samples {
                    *sample *= gain;
                }
            }
        }
        ExportNormalizeMode::Loudness => {
            let (measured_lufs, measured_tp_amp) =
                measure_lufs_and_true_peak(samples, params.output_channels, params.sample_rate)?;
            let gain_loudness_db = params.target_lufs - measured_lufs;
            let gain_loudness = 10.0_f32.powf(gain_loudness_db / 20.0);
            let ceiling_amp = 10.0_f32.powf(params.true_peak_dbtp / 20.0).clamp(0.0, 1.0);
            let gain_tp = if measured_tp_amp > 0.0 {
                ceiling_amp / measured_tp_amp
            } else {
                gain_loudness
            };
            let applied_gain = if params.tp_limiter {
                gain_loudness
            } else {
                gain_loudness.min(gain_tp)
            };
            for sample in samples.iter_mut() {
                *sample *= applied_gain;
            }
            if params.tp_limiter {
                let predicted_tp = measured_tp_amp * applied_gain;
                if predicted_tp > ceiling_amp && ceiling_amp > 0.0 {
                    for sample in samples.iter_mut() {
                        *sample = sample.clamp(-ceiling_amp, ceiling_amp);
                    }
                }
            }
        }
    }
    Ok(())
}
