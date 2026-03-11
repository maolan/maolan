use iced::Color;
use std::time::Duration;

pub mod gui {
    use super::Duration;

    pub const MIN_CLIP_WIDTH_PX: f32 = 12.0;
    pub const PREF_DEVICE_AUTO_ID: &str = "__auto__";
    pub const METER_DIRTY_EPSILON_DB: f32 = 0.5;
    pub const AUTOSAVE_SNAPSHOT_INTERVAL: Duration = Duration::from_secs(60);
}

pub mod workspace {
    use super::Color;

    pub const MIN_TIMELINE_BARS: f32 = 256.0;
    pub const PLAYHEAD_WIDTH_PX: f32 = 1.0;
    pub const TIMELINE_LEFT_INSET_PX: f32 = 0.0;

    pub const RULER_HEIGHT: f32 = 28.0;
    pub const BEATS_PER_BAR: usize = 4;
    pub const BARS_TO_DRAW: usize = 256;
    pub const MIN_TICK_SPACING_PX: f32 = 8.0;
    pub const MIN_LABEL_SPACING_PX: f32 = 64.0;

    pub const TEMPO_HEIGHT: f32 = 28.0;
    pub const TEMPO_HIT_HEIGHT: f32 = 14.0;
    pub const TIME_SIG_HIT_X_SPLIT: f32 = 36.0;
    pub const LEFT_HIT_WIDTH: f32 = 84.0;
    pub const CONTEXT_MENU_WIDTH: f32 = 132.0;
    pub const CONTEXT_MENU_ITEM_HEIGHT: f32 = 16.0;

    pub const CLIP_RESIZE_HANDLE_WIDTH: f32 = 5.0;
    pub const AUDIO_CLIP_BASE: Color = Color::from_rgb8(68, 88, 132);
    pub const AUDIO_CLIP_SELECTED_BASE: Color = Color::from_rgb8(96, 126, 186);
    pub const AUDIO_CLIP_BORDER: Color = Color::from_rgb8(78, 93, 130);
    pub const AUDIO_CLIP_SELECTED_BORDER: Color = Color::from_rgb8(176, 218, 255);
    pub const MIDI_CLIP_BASE: Color = Color::from_rgb8(55, 90, 50);
    pub const MIDI_CLIP_SELECTED_BASE: Color = Color::from_rgb8(84, 133, 72);
    pub const MIDI_CLIP_BORDER: Color = Color::from_rgb8(148, 215, 118);
    pub const MIDI_CLIP_SELECTED_BORDER: Color = Color::from_rgb8(196, 255, 151);

    pub const TICK_VALUES: [f32; 13] = [
        20.0, 12.0, 6.0, 0.0, -6.0, -12.0, -18.0, -24.0, -36.0, -48.0, -60.0, -72.0, -90.0,
    ];
    pub const TICK_LABELS: [&str; 13] = [
        "+20", "+12", "+6", "0", "-6", "-12", "-18", "-24", "-36", "-48", "-60", "-72", "-90",
    ];
}
