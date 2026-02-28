#![cfg(all(unix, not(target_os = "macos")))]

use maolan_engine::lv2::{Lv2Processor, Lv2TransportInfo};

#[test]
fn lv2_load_and_process_smoke() {
    let host = lilv::World::new();
    host.load_all();
    let Some(plugin) = host.plugins().iter().find(|p| p.verify()) else {
        eprintln!("No LV2 plugin found; skipping");
        return;
    };
    let plugin_uri_node = plugin.uri();
    let Some(uri) = plugin_uri_node.as_uri() else {
        eprintln!("Plugin URI invalid; skipping");
        return;
    };
    let uri = uri.to_string();

    let mut processor = Lv2Processor::new(48_000.0, 256, &uri).expect("failed to load LV2");
    let _ = processor.process_with_audio_io(256, &[], Lv2TransportInfo::default());
}
