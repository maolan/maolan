#![cfg(all(unix, not(target_os = "macos")))]

use maolan_engine::lv2::Lv2Processor;

#[test]
fn lv2_load_first_plugin_smoke() {
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
    eprintln!("Loading LV2 plugin: {uri}");
    let _processor = Lv2Processor::new(48_000.0, 256, &uri).expect("LV2 load failed");
    eprintln!("LV2 processor instantiated");
}
