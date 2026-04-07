use iced::Color;
use maolan_engine as engine;
use std::{sync::LazyLock, time::Duration};

pub const APP_BACKGROUND_COLOR: Color = Color::from_rgb8(23, 31, 48);

pub mod gui {
    use super::Duration;

    pub const MIN_CLIP_WIDTH_PX: f32 = 12.0;
    pub const PREF_DEVICE_AUTO_ID: &str = "__auto__";
    pub const METER_DIRTY_EPSILON_DB: f32 = 0.5;
    pub const METER_QUANTIZE_STEP_DB: f32 = 1.0;
    pub const AUTOSAVE_SNAPSHOT_INTERVAL: Duration = Duration::from_secs(60);
}

pub mod audio_defaults {
    pub use crate::audio_defaults::*;
}

pub mod workspace {
    use super::Color;

    pub const MIN_TIMELINE_BARS: f32 = 256.0;
    pub const PLAYHEAD_WIDTH_PX: f32 = 1.0;
    pub const TIMELINE_LEFT_INSET_PX: f32 = 0.0;

    pub const RULER_HEIGHT: f32 = 28.0;
    pub const BEATS_PER_BAR: usize = 4;
    pub const MIN_TICK_SPACING_PX: f32 = 8.0;
    pub const MIN_LABEL_SPACING_PX: f32 = 64.0;

    pub const TEMPO_HEIGHT: f32 = 28.0;
    pub const TEMPO_HIT_HEIGHT: f32 = 14.0;
    pub const TIME_SIG_HIT_X_SPLIT: f32 = 36.0;
    pub const LEFT_HIT_WIDTH: f32 = 84.0;
    pub const CONTEXT_MENU_WIDTH: f32 = 132.0;
    pub const CONTEXT_MENU_ITEM_HEIGHT: f32 = 16.0;

    pub const AUDIO_CLIP_SELECTED_BASE: Color = Color::from_rgb8(96, 126, 186);
    pub const MIDI_CLIP_SELECTED_BASE: Color = Color::from_rgb8(84, 133, 72);
    pub const MIDI_CLIP_BORDER: Color = Color::from_rgb8(148, 215, 118);
}

pub mod ui_timing {
    use super::Duration;

    pub const DOUBLE_CLICK: Duration = Duration::from_millis(350);
    pub const PLAYHEAD_UPDATE_INTERVAL: Duration = Duration::from_millis(50);
    pub const RECORDING_PREVIEW_UPDATE_INTERVAL: Duration = Duration::from_secs(1);
    pub const RECORDING_PREVIEW_PEAKS_UPDATE_INTERVAL: Duration = Duration::from_secs(2);
}

pub mod platform_caps {
    pub const HAS_SEPARATE_AUDIO_INPUT_DEVICE: bool = cfg!(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "openbsd"
    ));
    pub const REQUIRE_SAMPLE_RATES_FOR_HW_READY: bool = cfg!(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "openbsd"
    ));
    pub const REQUIRE_VST3_STATE_FOR_SAVE: bool = cfg!(target_os = "macos");
    pub const SUPPORTS_LV2: bool = cfg!(all(unix, not(target_os = "macos")));
    pub const SUPPORTS_PLUGIN_GRAPH: bool = cfg!(all(unix, not(target_os = "macos")));
}

pub mod main {
    pub const ICON_BYTES: &[u8] = include_bytes!("../images/icon.png");
}

pub mod workspace_ids {
    pub const EDITOR_SCROLL_ID: &str = "workspace.editor.scroll";
    pub const EDITOR_TIMELINE_SCROLL_ID: &str = "workspace.editor.timeline.scroll";
    pub const TRACKS_SCROLL_ID: &str = "workspace.tracks.scroll";
    pub const WORKSPACE_TEMPO_SCROLL_ID: &str = "workspace.tempo.scroll";
    pub const WORKSPACE_RULER_SCROLL_ID: &str = "workspace.ruler.scroll";
    pub const PIANO_TEMPO_SCROLL_ID: &str = "workspace.piano.tempo.scroll";
    pub const PIANO_RULER_SCROLL_ID: &str = "workspace.piano.ruler.scroll";
}

pub mod state_ids {
    pub const HW_IN_ID: &str = "hw:in";
    pub const HW_OUT_ID: &str = "hw:out";
    pub const METRONOME_TRACK_ID: &str = "metronome";
    pub const MIDI_HW_IN_ID: &str = "midi:hw:in";
    pub const MIDI_HW_OUT_ID: &str = "midi:hw:out";
}

pub mod state_track {
    pub const TRACK_FOLDER_HEADER_HEIGHT: f32 = 24.0;
    pub const TRACK_SUBTRACK_GAP: f32 = 2.0;
    pub const TRACK_SUBTRACK_MIN_HEIGHT: f32 = 40.0;
}

pub mod connections_plugins {
    pub const PLUGIN_W: f32 = 170.0;
    pub const MIN_PLUGIN_H: f32 = 96.0;
    pub const AUDIO_PORT_RADIUS: f32 = 4.5;
    pub const MIDI_PORT_RADIUS: f32 = 3.5;
    pub const MIN_PORT_GAP: f32 = 2.0;
    pub const PORT_HIT_RADIUS: f32 = AUDIO_PORT_RADIUS + 2.0;
    pub const MIDI_HIT_RADIUS: f32 = MIDI_PORT_RADIUS + 2.0;
    pub const TRACK_IO_W: f32 = 86.0;
    pub const TRACK_IO_H: f32 = 200.0;
    pub const TRACK_IO_MARGIN_X: f32 = 24.0;
}

pub mod plugins_x11 {
    use std::ffi::c_long;

    pub const CLIENT_MESSAGE: i32 = 33;
    pub const DESTROY_NOTIFY: i32 = 17;
    pub const STRUCTURE_NOTIFY_MASK: i64 = 1 << 17;
    pub const EXPOSURE_MASK: i64 = 1 << 15;
    pub const XEMBED_EMBEDDED_NOTIFY: c_long = 0;
    pub const XEMBED_WINDOW_ACTIVATE: c_long = 1;
    pub const XEMBED_FOCUS_IN: c_long = 4;
    pub const XEMBED_FOCUS_CURRENT: c_long = 0;
}

pub mod plugins_clap {
    use std::ffi::{c_int, c_long};

    #[cfg(all(unix, not(target_os = "macos")))]
    pub const DESTROY_NOTIFY: c_int = 17;
    #[cfg(all(unix, not(target_os = "macos")))]
    pub const CLIENT_MESSAGE: c_int = 33;
    #[cfg(all(unix, not(target_os = "macos")))]
    pub const STRUCTURE_NOTIFY_MASK: c_long = 1 << 17;
}

pub mod plugins_lv2 {
    pub const GTK_WINDOW_TOPLEVEL: i32 = 0;
    pub const LV2_URID_MAP: &str = "http://lv2plug.in/ns/ext/urid#map";
    pub const LV2_URID_MAP_TYPO_COMPAT: &str = "http://lv2plug.in/ns//ext/urid#map";
    pub const LV2_URID_UNMAP: &str = "http://lv2plug.in/ns/ext/urid#unmap";
    pub const LV2_UI_GTK3: &str = "http://lv2plug.in/ns/extensions/ui#Gtk3UI";
    pub const LV2_UI_GTK: &str = "http://lv2plug.in/ns/extensions/ui#GtkUI";
    pub const LV2_UI_X11: &str = "http://lv2plug.in/ns/extensions/ui#X11UI";
    pub const LV2_UI_QT4: &str = "http://lv2plug.in/ns/extensions/ui#Qt4UI";
    pub const LV2_UI_QT5: &str = "http://lv2plug.in/ns/extensions/ui#Qt5UI";
    pub const LV2_UI_QT6: &str = "http://lv2plug.in/ns/extensions/ui#Qt6UI";
    pub const LV2_UI_PARENT: &str = "http://lv2plug.in/ns/extensions/ui#parent";
    pub const LV2_UI_RESIZE: &str = "http://lv2plug.in/ns/extensions/ui#resize";
    pub const LV2_UI_IDLE_INTERFACE: &str = "http://lv2plug.in/ns/extensions/ui#idleInterface";
    pub const LV2_UI_SHOW_INTERFACE: &str = "http://lv2plug.in/ns/extensions/ui#showInterface";
    pub const LV2_UI_HIDE_INTERFACE: &str = "http://lv2plug.in/ns/extensions/ui#hideInterface";
    pub const LV2_INSTANCE_ACCESS: &str = "http://lv2plug.in/ns/ext/instance-access";
}

#[cfg(target_os = "freebsd")]
pub mod state_platform_freebsd {
    pub const AFMT_S16_LE: u64 = 0x00000010;
    pub const AFMT_S16_BE: u64 = 0x00000020;
    pub const AFMT_S8: u64 = 0x00000040;
    pub const AFMT_S32_LE: u64 = 0x00001000;
    pub const AFMT_S32_BE: u64 = 0x00002000;
    pub const AFMT_S24_LE: u64 = 0x00010000;
    pub const AFMT_S24_BE: u64 = 0x00020000;
}

pub mod widget_piano {
    pub use maolan_widgets::midi::{
        CTRL_SCROLL_ID, H_ZOOM_MAX, H_ZOOM_MIN, KEYBOARD_WIDTH, KEYS_SCROLL_ID, MAIN_SPLIT_SPACING,
        MAX_RPN_NRPN_POINTS, MIDI_DIN_BYTES_PER_SEC, MIDI_NOTE_COUNT, NOTES_PER_OCTAVE,
        NOTES_SCROLL_ID, OCTAVES, PITCH_MAX, RIGHT_SCROLL_GUTTER_WIDTH, SYSEX_SCROLL_ID,
        TOOLS_STRIP_WIDTH, WHITE_KEY_HEIGHT, WHITE_KEYS_PER_OCTAVE,
    };
}

pub mod workspace_mixer {
    use super::LazyLock;

    pub const FADER_MIN_DB: f32 = -90.0;
    pub const FADER_MAX_DB: f32 = 20.0;
    pub const STRIP_WIDTH: f32 = 96.0;
    pub const PAN_SLIDER_WIDTH: f32 = 52.0;
    pub const READOUT_WIDTH: f32 = 72.0;
    pub const FADER_WIDTH: f32 = 14.0;
    pub const SCALE_WIDTH: f32 = 22.0;
    pub const PAN_ROW_HEIGHT: f32 = 12.0;
    pub const STRIP_NAME_CHAR_PX: f32 = 6.3;
    pub const STRIP_NAME_SIDE_PADDING: f32 = 4.0;
    pub const METER_BAR_WIDTH: f32 = 3.0;
    pub const METER_BAR_GAP: f32 = 2.0;
    pub const METER_PAD_X: f32 = 3.0;
    pub const BAY_INSET: f32 = 1.0;

    pub static LEVEL_LABELS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
        let mut labels = Vec::with_capacity(1101);
        for i in 0..=1100 {
            let level = -90.0 + (i as f32) * 0.1;
            let s: &'static str = Box::leak(format!("{:+.1} dB", level).into_boxed_str());
            labels.push(s);
        }
        labels
    });

    pub static BALANCE_LABELS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
        let mut labels = Vec::with_capacity(201);
        for i in -100..=100 {
            let s: &'static str = if i == 0 {
                "C"
            } else if i < 0 {
                Box::leak(format!("L{}", -i).into_boxed_str())
            } else {
                Box::leak(format!("R{}", i).into_boxed_str())
            };
            labels.push(s);
        }
        labels
    });
}

pub mod workspace_editor {
    pub const CHECKPOINTS: usize = 16;
}

pub mod gui_mod {
    use super::{LazyLock, engine};

    pub const MAX_RECENT_SESSIONS: usize = 12;
    pub const AUDIO_BIT_DEPTH_OPTIONS: [usize; 4] = [32, 24, 16, 8];
    pub const MAX_PEAK_BINS: usize = 32_768;
    pub const BINS_PER_UPDATE: usize = 2048;
    pub const CHUNK_FRAMES: usize = 16_384;
    pub static CLIENT: LazyLock<engine::client::Client> =
        LazyLock::new(engine::client::Client::default);
    pub const STANDARD_EXPORT_SAMPLE_RATES: [u32; 12] = [
        8000, 11025, 16000, 22050, 32000, 44100, 48000, 88200, 96000, 176400, 192000, 384000,
    ];
}

pub mod gui_update_mod {
    pub const ATTACK_ALPHA: f32 = 0.60;
    pub const RELEASE_ALPHA: f32 = 0.22;
}

pub mod gui_update_dispatch_transport {
    pub const AUTOSAVE_KEEP_COUNT: usize = 10;
}

#[cfg(target_os = "linux")]
pub mod state_platform_linux {
    pub const SAMPLE_RATE_CANDIDATES: [u32; 12] = [
        8_000, 11_025, 16_000, 22_050, 32_000, 44_100, 48_000, 88_200, 96_000, 176_400, 192_000,
        384_000,
    ];
}

#[cfg(target_os = "freebsd")]
pub mod state_platform_freebsd_lists {
    pub const DIRECT_KEYS: [&str; 7] = [
        "formats",
        "iformats",
        "oformats",
        "pformats",
        "rformats",
        "playformats",
        "recformats",
    ];
    pub const RATE_KEYS: [&str; 8] = [
        "rates",
        "rate",
        "irates",
        "orates",
        "playrates",
        "recrates",
        "playback_rates",
        "capture_rates",
    ];
    pub const SAMPLE_RATE_CANDIDATES: [i32; 12] = [
        8_000, 11_025, 16_000, 22_050, 32_000, 44_100, 48_000, 88_200, 96_000, 176_400, 192_000,
        384_000,
    ];
}

pub mod plugins_clap_version {
    pub const MAJOR: u32 = 1;
    pub const MINOR: u32 = 2;
    pub const REVISION: u32 = 0;
}

pub mod message_lists {
    use crate::message::{
        ExportBitDepth, ExportMp3Mode, ExportNormalizeMode, ExportRenderMode, PianoChordKind,
        PianoNrpnKind, PianoRpnKind, PianoScaleRoot, PianoVelocityKind, SnapMode,
    };

    pub const SNAP_MODE_ALL: [SnapMode; 8] = [
        SnapMode::NoSnap,
        SnapMode::Clips,
        SnapMode::Bar,
        SnapMode::Beat,
        SnapMode::Eighth,
        SnapMode::Sixteenth,
        SnapMode::ThirtySecond,
        SnapMode::SixtyFourth,
    ];
    pub const PIANO_VELOCITY_KIND_ALL: [PianoVelocityKind; 2] = [
        PianoVelocityKind::NoteVelocity,
        PianoVelocityKind::ReleaseVelocity,
    ];
    pub const PIANO_RPN_KIND_ALL: [PianoRpnKind; 3] = [
        PianoRpnKind::PitchBendSensitivity,
        PianoRpnKind::FineTuning,
        PianoRpnKind::CoarseTuning,
    ];
    pub const PIANO_SCALE_ROOT_ALL: [PianoScaleRoot; 12] = [
        PianoScaleRoot::C,
        PianoScaleRoot::CSharp,
        PianoScaleRoot::D,
        PianoScaleRoot::DSharp,
        PianoScaleRoot::E,
        PianoScaleRoot::F,
        PianoScaleRoot::FSharp,
        PianoScaleRoot::G,
        PianoScaleRoot::GSharp,
        PianoScaleRoot::A,
        PianoScaleRoot::ASharp,
        PianoScaleRoot::B,
    ];
    pub const PIANO_CHORD_KIND_ALL: [PianoChordKind; 5] = [
        PianoChordKind::MajorTriad,
        PianoChordKind::MinorTriad,
        PianoChordKind::Dominant7,
        PianoChordKind::Major7,
        PianoChordKind::Minor7,
    ];
    pub const PIANO_NRPN_KIND_ALL: [PianoNrpnKind; 3] = [
        PianoNrpnKind::Brightness,
        PianoNrpnKind::VibratoRate,
        PianoNrpnKind::VibratoDepth,
    ];
    pub const EXPORT_NORMALIZE_MODE_ALL: [ExportNormalizeMode; 2] =
        [ExportNormalizeMode::Peak, ExportNormalizeMode::Loudness];
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_background_color_has_expected_values() {
        // Verify the RGB components
        assert!((APP_BACKGROUND_COLOR.r - 0.09).abs() < 0.01); // 23/255
        assert!((APP_BACKGROUND_COLOR.g - 0.12).abs() < 0.01); // 31/255
        assert!((APP_BACKGROUND_COLOR.b - 0.19).abs() < 0.01); // 48/255
    }

    #[test]
    fn gui_constants_are_expected_values() {
        assert_eq!(gui::PREF_DEVICE_AUTO_ID, "__auto__");
        assert_eq!(gui::AUTOSAVE_SNAPSHOT_INTERVAL.as_secs(), 60);
    }

    #[test]
    fn workspace_constants_are_expected_values() {
        assert_eq!(workspace::BEATS_PER_BAR, 4);
        assert_eq!(workspace::MIN_LABEL_SPACING_PX, 64.0);
    }

    #[test]
    fn ui_timing_constants_are_expected_values() {
        assert_eq!(ui_timing::DOUBLE_CLICK.as_millis(), 350);
        assert_eq!(ui_timing::PLAYHEAD_UPDATE_INTERVAL.as_millis(), 50);
        assert_eq!(ui_timing::RECORDING_PREVIEW_UPDATE_INTERVAL.as_secs(), 1);
    }

    #[test]
    fn main_icon_bytes_is_not_empty() {
        assert!(!main::ICON_BYTES.is_empty());
    }

    #[test]
    fn workspace_ids_are_unique() {
        let ids = [
            workspace_ids::EDITOR_SCROLL_ID,
            workspace_ids::EDITOR_TIMELINE_SCROLL_ID,
            workspace_ids::TRACKS_SCROLL_ID,
            workspace_ids::WORKSPACE_TEMPO_SCROLL_ID,
            workspace_ids::WORKSPACE_RULER_SCROLL_ID,
            workspace_ids::PIANO_TEMPO_SCROLL_ID,
            workspace_ids::PIANO_RULER_SCROLL_ID,
        ];
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len());
    }

    #[test]
    fn state_ids_are_expected_values() {
        assert_eq!(state_ids::HW_IN_ID, "hw:in");
        assert_eq!(state_ids::HW_OUT_ID, "hw:out");
        assert_eq!(state_ids::METRONOME_TRACK_ID, "metronome");
        assert_eq!(state_ids::MIDI_HW_IN_ID, "midi:hw:in");
        assert_eq!(state_ids::MIDI_HW_OUT_ID, "midi:hw:out");
    }

    #[test]
    fn state_track_constants_are_expected_values() {
        assert_eq!(state_track::TRACK_FOLDER_HEADER_HEIGHT, 24.0);
        assert_eq!(state_track::TRACK_SUBTRACK_GAP, 2.0);
        assert_eq!(state_track::TRACK_SUBTRACK_MIN_HEIGHT, 40.0);
    }

    #[test]
    fn connections_plugins_constants_are_expected_values() {
        assert_eq!(connections_plugins::PLUGIN_W, 170.0);
        assert_eq!(connections_plugins::MIN_PLUGIN_H, 96.0);
        assert_eq!(connections_plugins::AUDIO_PORT_RADIUS, 4.5);
        assert_eq!(connections_plugins::MIDI_PORT_RADIUS, 3.5);
    }

    #[test]
    fn plugins_x11_constants_exist() {
        let _ = plugins_x11::CLIENT_MESSAGE;
        let _ = plugins_x11::DESTROY_NOTIFY;
        let _ = plugins_x11::STRUCTURE_NOTIFY_MASK;
        let _ = plugins_x11::EXPOSURE_MASK;
    }

    #[test]
    fn plugins_lv2_constants_exist() {
        assert!(!plugins_lv2::LV2_URID_MAP.is_empty());
        assert!(!plugins_lv2::LV2_URID_UNMAP.is_empty());
        assert!(!plugins_lv2::LV2_UI_X11.is_empty());
    }

    #[test]
    fn workspace_mixer_level_labels_is_populated() {
        assert!(!workspace_mixer::LEVEL_LABELS.is_empty());
        assert!(workspace_mixer::LEVEL_LABELS.len() > 1000);
    }

    #[test]
    fn workspace_mixer_balance_labels_is_populated() {
        assert!(!workspace_mixer::BALANCE_LABELS.is_empty());
        assert_eq!(workspace_mixer::BALANCE_LABELS.len(), 201); // -100 to 100
    }

    #[test]
    fn gui_mod_constants_are_valid() {
        assert_eq!(gui_mod::MAX_RECENT_SESSIONS, 12);
        assert_eq!(gui_mod::AUDIO_BIT_DEPTH_OPTIONS, [32, 24, 16, 8]);
        assert_eq!(gui_mod::MAX_PEAK_BINS, 32768);
        assert_eq!(gui_mod::BINS_PER_UPDATE, 2048);
        assert_eq!(gui_mod::CHUNK_FRAMES, 16384);
        assert!(!gui_mod::STANDARD_EXPORT_SAMPLE_RATES.is_empty());
    }

    #[test]
    fn message_lists_are_populated() {
        assert_eq!(message_lists::SNAP_MODE_ALL.len(), 8);
        assert_eq!(message_lists::PIANO_SCALE_ROOT_ALL.len(), 12);
        assert_eq!(message_lists::PIANO_CHORD_KIND_ALL.len(), 5);
        assert_eq!(message_lists::EXPORT_BIT_DEPTH_ALL.len(), 4);
        assert_eq!(message_lists::EXPORT_RENDER_MODE_ALL.len(), 3);
    }
}
