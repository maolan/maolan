mod x11;

pub struct GuiVst3UiHost;

impl GuiVst3UiHost {
    pub fn new() -> Self {
        Self
    }

    pub fn open_editor(
        &mut self,
        plugin_path: &str,
        plugin_name: &str,
        plugin_id: &str,
    ) -> Result<(), String> {
        let plugin_path = plugin_path.to_string();
        let plugin_name = plugin_name.to_string();
        let plugin_id = plugin_id.to_string();
        std::thread::Builder::new()
            .name("vst3-ui".to_string())
            .spawn(move || {
                if let Err(err) = x11::open_editor_blocking(&plugin_path, &plugin_name, &plugin_id)
                {
                    eprintln!("VST3 UI error: {err}");
                }
            })
            .map_err(|e| format!("Failed to spawn VST3 UI thread: {e}"))?;
        Ok(())
    }
}
