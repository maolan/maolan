pub const SUPPORTS_METER_POLL: bool = cfg!(any(
    target_os = "windows",
    target_os = "freebsd",
    target_os = "linux"
));
pub const HAS_SEPARATE_AUDIO_INPUT_DEVICE: bool = cfg!(any(
    target_os = "windows",
    target_os = "linux",
    target_os = "freebsd"
));
pub const REQUIRE_SAMPLE_RATES_FOR_HW_READY: bool =
    cfg!(any(target_os = "linux", target_os = "freebsd"));
pub const REQUIRE_VST3_STATE_FOR_SAVE: bool = cfg!(any(target_os = "windows", target_os = "macos"));
pub const SUPPORTS_LV2: bool = cfg!(all(unix, not(target_os = "macos")));
pub const SUPPORTS_PLUGIN_GRAPH: bool = cfg!(any(
    target_os = "windows",
    all(unix, not(target_os = "macos"))
));
