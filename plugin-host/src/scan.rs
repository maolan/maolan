//! Plugin scanner: loads a plugin library, validates it, and dumps metadata to JSON.

use crate::clap::{
    CLAP_EXT_AUDIO_PORTS, CLAP_EXT_PARAMS, CLAP_VERSION, ClapAudioPortInfo, ClapHost,
    ClapParamInfo, ClapPluginAudioPorts, ClapPluginEntry, ClapPluginFactory, ClapPluginParams,
};
use serde::{Deserialize, Serialize};
use std::ffi::{CStr, CString, c_char, c_void};
use std::path::{Path, PathBuf};
use std::ptr;

/// Metadata for a single parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamMetadata {
    pub id: u32,
    pub name: String,
    pub module: String,
    pub min_value: f64,
    pub max_value: f64,
    pub default_value: f64,
    pub flags: u32,
}

/// Metadata for an audio port.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioPortMetadata {
    pub id: u32,
    pub name: String,
    pub channel_count: u32,
    pub flags: u32,
}

/// Metadata for a single plugin inside a library.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    pub id: String,
    pub name: String,
    pub vendor: String,
    pub version: String,
    pub description: String,
    pub params: Vec<ParamMetadata>,
    pub audio_inputs: Vec<AudioPortMetadata>,
    pub audio_outputs: Vec<AudioPortMetadata>,
}

/// Full scan result for a plugin library.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub format: String,
    pub path: String,
    pub plugins: Vec<PluginMetadata>,
    pub error: Option<String>,
}

// ─── CLAP system-scan types (match engine message types) ───

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClapPluginInfo {
    pub name: String,
    pub path: String,
    pub capabilities: Option<ClapPluginCapabilities>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClapPluginCapabilities {
    pub has_gui: bool,
    pub gui_apis: Vec<String>,
    pub supports_embedded: bool,
    pub supports_floating: bool,
    pub has_params: bool,
    pub has_state: bool,
    pub audio_inputs: usize,
    pub audio_outputs: usize,
    pub midi_inputs: usize,
    pub midi_outputs: usize,
}

unsafe extern "C" fn dummy_get_extension(_: *const ClapHost, _: *const c_char) -> *const c_void {
    ptr::null()
}
unsafe extern "C" fn dummy_request_restart(_: *const ClapHost) {}
unsafe extern "C" fn dummy_request_process(_: *const ClapHost) {}
unsafe extern "C" fn dummy_request_callback(_: *const ClapHost) {}

fn cstr_to_string(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

/// Check if a plugin's CLAP version is compatible with the host.
fn clap_version_is_compatible(plugin_version: &crate::clap::ClapVersion) -> bool {
    plugin_version.major == crate::clap::CLAP_VERSION.major
        && plugin_version.minor <= crate::clap::CLAP_VERSION.minor
}

/// Scan a CLAP plugin library and return metadata for every plugin it contains.
pub fn scan_clap_plugin(plugin_path: &str) -> ScanResult {
    let path = Path::new(plugin_path);
    if !path.exists() {
        return ScanResult {
            format: "clap".to_string(),
            path: plugin_path.to_string(),
            plugins: Vec::new(),
            error: Some(format!("path does not exist: {plugin_path}")),
        };
    }

    let library = match unsafe { libloading::Library::new(path) } {
        Ok(lib) => lib,
        Err(e) => {
            return ScanResult {
                format: "clap".to_string(),
                path: plugin_path.to_string(),
                plugins: Vec::new(),
                error: Some(format!("failed to load library: {e}")),
            };
        }
    };

    let entry: libloading::Symbol<*const ClapPluginEntry> =
        match unsafe { library.get(b"clap_entry\0") } {
            Ok(sym) => sym,
            Err(e) => {
                return ScanResult {
                    format: "clap".to_string(),
                    path: plugin_path.to_string(),
                    plugins: Vec::new(),
                    error: Some(format!("clap_entry not found: {e}")),
                };
            }
        };

    let entry = unsafe { &**entry };

    if let Some(init) = entry.init {
        let path_c = match CString::new(plugin_path) {
            Ok(s) => s,
            Err(_) => {
                return ScanResult {
                    format: "clap".to_string(),
                    path: plugin_path.to_string(),
                    plugins: Vec::new(),
                    error: Some("plugin path contains null bytes".to_string()),
                };
            }
        };
        if !unsafe { init(path_c.as_ptr()) } {
            return ScanResult {
                format: "clap".to_string(),
                path: plugin_path.to_string(),
                plugins: Vec::new(),
                error: Some("clap_entry.init() failed".to_string()),
            };
        }
    }

    let factory = if let Some(get_factory) = entry.get_factory {
        let factory_id = CString::new("clap.plugin-factory").unwrap();
        let factory_ptr = unsafe { get_factory(factory_id.as_ptr()) };
        if factory_ptr.is_null() {
            return ScanResult {
                format: "clap".to_string(),
                path: plugin_path.to_string(),
                plugins: Vec::new(),
                error: Some("clap.plugin-factory not found".to_string()),
            };
        }
        unsafe { &*(factory_ptr as *const ClapPluginFactory) }
    } else {
        return ScanResult {
            format: "clap".to_string(),
            path: plugin_path.to_string(),
            plugins: Vec::new(),
            error: Some("clap_entry.get_factory is null".to_string()),
        };
    };

    let count = factory
        .get_plugin_count
        .map(|f| unsafe { f(factory) })
        .unwrap_or(0);

    let mut host = ClapHost {
        clap_version: CLAP_VERSION,
        host_data: ptr::null_mut(),
        name: c"maolan-plugin-host".as_ptr(),
        vendor: c"Maolan".as_ptr(),
        url: c"https://maolan.github.io".as_ptr(),
        version: c"0.1.0".as_ptr(),
        get_extension: Some(dummy_get_extension),
        request_restart: Some(dummy_request_restart),
        request_process: Some(dummy_request_process),
        request_callback: Some(dummy_request_callback),
    };
    host.host_data = (&mut host as *mut ClapHost).cast::<c_void>();

    let mut plugins = Vec::with_capacity(count as usize);

    for i in 0..count {
        let desc = factory
            .get_plugin_descriptor
            .map(|f| unsafe { f(factory, i) })
            .unwrap_or(ptr::null());
        if desc.is_null() {
            continue;
        }
        let desc = unsafe { &*desc };

        if !clap_version_is_compatible(&desc.clap_version) {
            continue;
        }

        let plugin_id = cstr_to_string(desc.id);
        let plugin_id_c = match CString::new(&*plugin_id) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let plugin = factory
            .create_plugin
            .map(|f| unsafe { f(factory, &host, plugin_id_c.as_ptr()) })
            .unwrap_or(ptr::null());
        if plugin.is_null() {
            continue;
        }

        let init_ok = unsafe { (*plugin).init }
            .map(|f| unsafe { f(plugin) })
            .unwrap_or(false);
        if !init_ok {
            unsafe {
                if let Some(destroy) = (*plugin).destroy {
                    destroy(plugin);
                }
            }
            continue;
        }

        let mut params = Vec::new();
        let mut audio_inputs = Vec::new();
        let mut audio_outputs = Vec::new();

        // Query params extension.
        unsafe {
            let ext = (*plugin)
                .get_extension
                .map(|f| f(plugin, CLAP_EXT_PARAMS.as_ptr() as *const c_char));
            if let Some(ptr) = ext
                && !ptr.is_null()
            {
                let p = &*(ptr as *const ClapPluginParams);
                let count = p.count.map(|f| f(plugin)).unwrap_or(0);
                for pi in 0..count {
                    let mut info = ClapParamInfo {
                        id: 0,
                        flags: 0,
                        cookie: ptr::null_mut(),
                        name: [0; 256],
                        module: [0; 1024],
                        min_value: 0.0,
                        max_value: 0.0,
                        default_value: 0.0,
                    };
                    if p.get_info
                        .map(|f| f(plugin, pi, &mut info))
                        .unwrap_or(false)
                    {
                        let name = CStr::from_ptr(info.name.as_ptr())
                            .to_string_lossy()
                            .into_owned();
                        let module = CStr::from_ptr(info.module.as_ptr())
                            .to_string_lossy()
                            .into_owned();
                        params.push(ParamMetadata {
                            id: info.id,
                            name,
                            module,
                            min_value: info.min_value,
                            max_value: info.max_value,
                            default_value: info.default_value,
                            flags: info.flags,
                        });
                    }
                }
            }
        }

        // Query audio-ports extension.
        unsafe {
            let ext = (*plugin)
                .get_extension
                .map(|f| f(plugin, CLAP_EXT_AUDIO_PORTS.as_ptr() as *const c_char));
            if let Some(ptr) = ext
                && !ptr.is_null()
            {
                let ap = &*(ptr as *const ClapPluginAudioPorts);
                let in_count = ap.count.map(|f| f(plugin, true)).unwrap_or(0);
                let out_count = ap.count.map(|f| f(plugin, false)).unwrap_or(0);
                for pi in 0..in_count {
                    let mut info = ClapAudioPortInfo {
                        id: 0,
                        name: [0; 256],
                        flags: 0,
                        channel_count: 0,
                        port_type: ptr::null(),
                        in_place_pair: 0,
                    };
                    if ap
                        .get
                        .map(|f| f(plugin, pi, true, &mut info))
                        .unwrap_or(false)
                    {
                        let name = CStr::from_ptr(info.name.as_ptr())
                            .to_string_lossy()
                            .into_owned();
                        audio_inputs.push(AudioPortMetadata {
                            id: info.id,
                            name,
                            channel_count: info.channel_count,
                            flags: info.flags,
                        });
                    }
                }
                for pi in 0..out_count {
                    let mut info = ClapAudioPortInfo {
                        id: 0,
                        name: [0; 256],
                        flags: 0,
                        channel_count: 0,
                        port_type: ptr::null(),
                        in_place_pair: 0,
                    };
                    if ap
                        .get
                        .map(|f| f(plugin, pi, false, &mut info))
                        .unwrap_or(false)
                    {
                        let name = CStr::from_ptr(info.name.as_ptr())
                            .to_string_lossy()
                            .into_owned();
                        audio_outputs.push(AudioPortMetadata {
                            id: info.id,
                            name,
                            channel_count: info.channel_count,
                            flags: info.flags,
                        });
                    }
                }
            }
        }

        plugins.push(PluginMetadata {
            id: plugin_id,
            name: cstr_to_string(desc.name),
            vendor: cstr_to_string(desc.vendor),
            version: cstr_to_string(desc.version),
            description: cstr_to_string(desc.description),
            params,
            audio_inputs,
            audio_outputs,
        });

        unsafe {
            if let Some(destroy) = (*plugin).destroy {
                destroy(plugin);
            }
        }
    }

    if let Some(deinit) = entry.deinit {
        unsafe { deinit() };
    }

    ScanResult {
        format: "clap".to_string(),
        path: plugin_path.to_string(),
        plugins,
        error: None,
    }
}

// ─── CLAP system scanning ───

#[cfg(any(
    target_os = "macos",
    target_os = "linux",
    target_os = "freebsd",
    target_os = "openbsd"
))]
fn default_clap_search_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    #[cfg(target_os = "macos")]
    {
        crate::paths::push_macos_audio_plugin_roots(&mut roots, "CLAP");
    }
    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
    {
        crate::paths::push_unix_plugin_roots(&mut roots, "clap");
    }
    roots
}

#[cfg(not(any(
    target_os = "macos",
    target_os = "linux",
    target_os = "freebsd",
    target_os = "openbsd"
)))]
fn default_clap_search_roots() -> Vec<PathBuf> {
    Vec::new()
}

fn is_supported_clap_binary(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("clap"))
}

/// Scan a single CLAP bundle and return plugin info entries.
fn scan_clap_bundle(path: &Path, scan_capabilities: bool) -> Vec<ClapPluginInfo> {
    use crate::clap::{
        ClapPluginAudioPorts, ClapPluginEntry, ClapPluginFactory, ClapPluginGui,
        ClapPluginNotePorts, ClapPluginParams,
    };

    let path_str = path.to_string_lossy().to_string();
    let factory_id = c"clap.plugin-factory";
    let mut host = ClapHost {
        clap_version: CLAP_VERSION,
        host_data: ptr::null_mut(),
        name: c"Maolan".as_ptr(),
        vendor: c"Maolan".as_ptr(),
        url: c"https://example.invalid".as_ptr(),
        version: c"0.0.1".as_ptr(),
        get_extension: Some(dummy_get_extension),
        request_restart: Some(dummy_request_restart),
        request_process: Some(dummy_request_process),
        request_callback: Some(dummy_request_callback),
    };
    host.host_data = (&mut host as *mut ClapHost).cast::<c_void>();

    let lib = match unsafe { libloading::Library::new(path) } {
        Ok(l) => l,
        Err(_) => {
            return vec![ClapPluginInfo {
                name: path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| path_str.clone()),
                path: path_str,
                capabilities: None,
            }];
        }
    };

    let entry: libloading::Symbol<*const ClapPluginEntry> =
        match unsafe { lib.get(b"clap_entry\0") } {
            Ok(sym) => sym,
            Err(_) => {
                return vec![ClapPluginInfo {
                    name: path
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| path_str.clone()),
                    path: path_str,
                    capabilities: None,
                }];
            }
        };
    let entry = unsafe { &**entry };

    if let Some(init) = entry.init {
        let path_c = match CString::new(&*path_str) {
            Ok(s) => s,
            Err(_) => {
                return vec![ClapPluginInfo {
                    name: path_str.clone(),
                    path: path_str,
                    capabilities: None,
                }];
            }
        };
        if !unsafe { init(path_c.as_ptr()) } {
            return vec![ClapPluginInfo {
                name: path_str.clone(),
                path: path_str,
                capabilities: None,
            }];
        }
    }

    let factory = if let Some(get_factory) = entry.get_factory {
        let factory_ptr = unsafe { get_factory(factory_id.as_ptr()) };
        if factory_ptr.is_null() {
            return vec![ClapPluginInfo {
                name: path_str.clone(),
                path: path_str,
                capabilities: None,
            }];
        }
        unsafe { &*(factory_ptr as *const ClapPluginFactory) }
    } else {
        return vec![ClapPluginInfo {
            name: path_str.clone(),
            path: path_str,
            capabilities: None,
        }];
    };

    let count = factory
        .get_plugin_count
        .map(|f| unsafe { f(factory) })
        .unwrap_or(0);

    let mut out = Vec::with_capacity(count as usize);

    for i in 0..count {
        let desc = factory
            .get_plugin_descriptor
            .map(|f| unsafe { f(factory, i) })
            .unwrap_or(ptr::null());
        if desc.is_null() {
            continue;
        }
        let desc = unsafe { &*desc };
        if !clap_version_is_compatible(&desc.clap_version) {
            continue;
        }
        let name = cstr_to_string(desc.name);
        let plugin_id = cstr_to_string(desc.id);
        let plugin_id_c = match CString::new(&*plugin_id) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let plugin = factory
            .create_plugin
            .map(|f| unsafe { f(factory, &host, plugin_id_c.as_ptr()) })
            .unwrap_or(ptr::null());
        if plugin.is_null() {
            continue;
        }
        let init_ok = unsafe { (*plugin).init }
            .map(|f| unsafe { f(plugin) })
            .unwrap_or(false);
        if !init_ok {
            unsafe {
                if let Some(destroy) = (*plugin).destroy {
                    destroy(plugin);
                }
            }
            continue;
        }

        let mut capabilities = None;
        if scan_capabilities {
            let mut caps = ClapPluginCapabilities {
                has_gui: false,
                gui_apis: Vec::new(),
                supports_embedded: false,
                supports_floating: false,
                has_params: false,
                has_state: false,
                audio_inputs: 0,
                audio_outputs: 0,
                midi_inputs: 0,
                midi_outputs: 0,
            };

            unsafe {
                let ext = (*plugin)
                    .get_extension
                    .map(|f| f(plugin, c"clap.gui".as_ptr() as *const c_char));
                if let Some(ptr) = ext
                    && !ptr.is_null()
                {
                    let gui = &*(ptr as *const ClapPluginGui);
                    caps.has_gui = gui
                        .is_api_supported
                        .map(|f| f(plugin, c"x11".as_ptr() as *const c_char, true))
                        .unwrap_or(false)
                        || gui
                            .is_api_supported
                            .map(|f| f(plugin, c"win32".as_ptr() as *const c_char, true))
                            .unwrap_or(false)
                        || gui
                            .is_api_supported
                            .map(|f| f(plugin, c"cocoa".as_ptr() as *const c_char, true))
                            .unwrap_or(false);
                    if caps.has_gui {
                        caps.gui_apis = vec!["x11".to_string()];
                        caps.supports_embedded = true;
                        caps.supports_floating = gui
                            .is_api_supported
                            .map(|f| f(plugin, ptr::null(), false))
                            .unwrap_or(false);
                    }
                }
            }

            unsafe {
                let ext = (*plugin)
                    .get_extension
                    .map(|f| f(plugin, CLAP_EXT_PARAMS.as_ptr() as *const c_char));
                if let Some(ptr) = ext
                    && !ptr.is_null()
                {
                    let p = &*(ptr as *const ClapPluginParams);
                    caps.has_params = p.count.map(|f| f(plugin)).unwrap_or(0) > 0;
                }
            }

            unsafe {
                let ext = (*plugin)
                    .get_extension
                    .map(|f| f(plugin, c"clap.state".as_ptr() as *const c_char));
                if let Some(ptr) = ext
                    && !ptr.is_null()
                {
                    caps.has_state = true;
                }
            }

            unsafe {
                let ext = (*plugin)
                    .get_extension
                    .map(|f| f(plugin, CLAP_EXT_AUDIO_PORTS.as_ptr() as *const c_char));
                if let Some(ptr) = ext
                    && !ptr.is_null()
                {
                    let ap = &*(ptr as *const ClapPluginAudioPorts);
                    caps.audio_inputs = ap.count.map(|f| f(plugin, true)).unwrap_or(0) as usize;
                    caps.audio_outputs = ap.count.map(|f| f(plugin, false)).unwrap_or(0) as usize;
                }
            }

            unsafe {
                let ext = (*plugin)
                    .get_extension
                    .map(|f| f(plugin, c"clap.note-ports".as_ptr() as *const c_char));
                if let Some(ptr) = ext
                    && !ptr.is_null()
                {
                    let np = &*(ptr as *const ClapPluginNotePorts);
                    caps.midi_inputs = np.count.map(|f| f(plugin, true)).unwrap_or(0) as usize;
                    caps.midi_outputs = np.count.map(|f| f(plugin, false)).unwrap_or(0) as usize;
                }
            }

            capabilities = Some(caps);
        }

        unsafe {
            if let Some(destroy) = (*plugin).destroy {
                destroy(plugin);
            }
        }

        out.push(ClapPluginInfo {
            name,
            path: path_str.clone(),
            capabilities,
        });
    }

    if let Some(deinit) = entry.deinit {
        unsafe { deinit() };
    }

    if out.is_empty() {
        out.push(ClapPluginInfo {
            name: path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| path_str.clone()),
            path: path_str,
            capabilities: None,
        });
    }

    out
}

fn collect_clap_plugins(root: &Path, out: &mut Vec<ClapPluginInfo>, scan_capabilities: bool) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else {
            continue;
        };
        if ft.is_dir() {
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    matches!(
                        name,
                        "deps" | "build" | "incremental" | ".fingerprint" | "examples"
                    )
                })
            {
                continue;
            }
            collect_clap_plugins(&path, out, scan_capabilities);
            continue;
        }

        if is_supported_clap_binary(&path) {
            let infos = scan_clap_bundle(&path, scan_capabilities);
            if infos.is_empty() {
                let name = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.to_string_lossy().to_string());
                out.push(ClapPluginInfo {
                    name,
                    path: path.to_string_lossy().to_string(),
                    capabilities: None,
                });
            } else {
                out.extend(infos);
            }
        }
    }
}

/// Scan all CLAP plugins on the system.
pub fn scan_clap_plugins(scan_capabilities: bool) -> Vec<ClapPluginInfo> {
    let mut roots = default_clap_search_roots();

    if let Ok(extra) = std::env::var("CLAP_PATH") {
        for p in std::env::split_paths(&extra) {
            if !p.as_os_str().is_empty() {
                roots.push(p);
            }
        }
    }

    let mut out = Vec::new();
    for root in roots {
        collect_clap_plugins(&root, &mut out, scan_capabilities);
    }

    out.sort_by_key(|a| a.name.to_lowercase());
    out.dedup_by(|a, b| a.path.eq_ignore_ascii_case(&b.path));
    out
}

// ─── VST3 system scanning ───

pub fn scan_vst3_plugins() -> Vec<crate::vst3::Vst3PluginInfo> {
    crate::vst3::host::Vst3Host::new().list_plugins()
}

// ─── LV2 system scanning ───

#[cfg(unix)]
pub fn scan_lv2_plugins() -> Vec<crate::lv2::Lv2PluginInfo> {
    crate::lv2::Lv2Host::new(48_000.0).list_plugins()
}

#[cfg(not(unix))]
pub fn scan_lv2_plugins() -> Vec<crate::lv2::Lv2PluginInfo> {
    Vec::new()
}

// ─── Unified scan runner ───

/// Run a scan and print JSON to stdout or write to `output_path`.
///
/// * `format`: `"clap"`, `"vst3"`, or `"lv2"`
/// * `plugin_path`: specific file/directory to scan, or `"--system"` for system-wide scan
/// * `output_path`: optional file to write JSON to
pub fn run_scan(format: &str, plugin_path: &str, output_path: Option<&str>) -> i32 {
    let json = match format {
        "clap" => {
            if plugin_path == "--system" {
                match serde_json::to_string_pretty(&scan_clap_plugins(true)) {
                    Ok(j) => j,
                    Err(e) => {
                        eprintln!("Failed to serialize scan result: {e}");
                        return 1;
                    }
                }
            } else {
                match serde_json::to_string_pretty(&scan_clap_plugin(plugin_path)) {
                    Ok(j) => j,
                    Err(e) => {
                        eprintln!("Failed to serialize scan result: {e}");
                        return 1;
                    }
                }
            }
        }
        "vst3" => {
            if plugin_path != "--system" {
                eprintln!("VST3 single-file scan not yet supported; use --system");
                return 1;
            }
            match serde_json::to_string_pretty(&scan_vst3_plugins()) {
                Ok(j) => j,
                Err(e) => {
                    eprintln!("Failed to serialize scan result: {e}");
                    return 1;
                }
            }
        }
        "lv2" => {
            if plugin_path != "--system" {
                eprintln!("LV2 single-file scan not yet supported; use --system");
                return 1;
            }
            match serde_json::to_string_pretty(&scan_lv2_plugins()) {
                Ok(j) => j,
                Err(e) => {
                    eprintln!("Failed to serialize scan result: {e}");
                    return 1;
                }
            }
        }
        _ => {
            eprintln!("Scan format '{}' not supported", format);
            return 1;
        }
    };

    if let Some(path) = output_path {
        match std::fs::write(path, &json) {
            Ok(()) => {
                println!("{path}");
                0
            }
            Err(e) => {
                eprintln!("Failed to write {path}: {e}");
                1
            }
        }
    } else {
        println!("{json}");
        0
    }
}
