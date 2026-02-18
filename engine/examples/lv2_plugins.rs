use maolan_engine::lv2::Lv2Host;

fn main() {
    let mut host = Lv2Host::new(48_000.0);
    let plugins = host.list_plugins();
    println!("Found {} LV2 plugins", plugins.len());

    for plugin in plugins {
        println!(
            "- {} [{}] | audio in/out: {}/{} | midi in/out: {}/{}",
            plugin.name,
            plugin.uri,
            plugin.audio_inputs,
            plugin.audio_outputs,
            plugin.midi_inputs,
            plugin.midi_outputs
        );
    }

    let Some(uri) = std::env::args().nth(1) else {
        println!();
        println!("Pass a plugin URI to test instantiate/activate and unload:");
        println!(
            "cargo run --manifest-path engine/Cargo.toml --example lv2_plugins -- <plugin_uri>"
        );
        return;
    };

    println!();
    println!("Loading {uri}");
    if let Err(error) = host.load_plugin(&uri) {
        eprintln!("Load failed: {error}");
        std::process::exit(1);
    }
    println!("Loaded plugins: {}", host.loaded_count());

    println!("Unloading {uri}");
    if let Err(error) = host.unload_plugin(&uri) {
        eprintln!("Unload failed: {error}");
        std::process::exit(1);
    }
    println!("Loaded plugins: {}", host.loaded_count());
}
