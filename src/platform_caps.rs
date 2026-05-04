pub use crate::consts::platform_caps::{
    HAS_SEPARATE_AUDIO_INPUT_DEVICE, REQUIRE_SAMPLE_RATES_FOR_HW_READY,
    REQUIRE_VST3_STATE_FOR_SAVE, SUPPORTS_LV2,
};
#[cfg(all(unix, not(target_os = "macos")))]
pub use crate::consts::platform_caps::SUPPORTS_PLUGIN_GRAPH;
