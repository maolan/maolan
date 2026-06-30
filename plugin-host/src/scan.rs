use crate::clap::{
    CLAP_EXT_AUDIO_PORTS, CLAP_EXT_PARAMS, CLAP_VERSION, ClapAudioPortInfo, ClapHost,
    ClapParamInfo, ClapPluginAudioPorts, ClapPluginEntry, ClapPluginFactory, ClapPluginParams,
};
use serde::{Deserialize, Serialize};
use std::ffi::{CStr, CString, c_char, c_void};
use std::path::{Path, PathBuf};
use std::ptr;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioPortMetadata {
    pub id: u32,
    pub name: String,
    pub channel_count: u32,
    pub flags: u32,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub format: String,
    pub path: String,
    pub plugins: Vec<PluginMetadata>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanDiagnostic {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin_uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle_uri: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanOutput<T> {
    pub errors: Vec<ScanDiagnostic>,
    pub warnings: Vec<ScanDiagnostic>,
    pub data: T,
}

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

fn clap_version_is_compatible(plugin_version: &crate::clap::ClapVersion) -> bool {
    plugin_version.major == crate::clap::CLAP_VERSION.major
        && plugin_version.minor <= crate::clap::CLAP_VERSION.minor
}

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

        unsafe {
            let ext = (*plugin)
                .get_extension
                .map(|f| f(plugin, CLAP_EXT_PARAMS.as_ptr()));
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

        unsafe {
            let ext = (*plugin)
                .get_extension
                .map(|f| f(plugin, CLAP_EXT_AUDIO_PORTS.as_ptr()));
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

#[cfg(windows)]
fn default_clap_search_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    crate::paths::push_windows_clap_roots(&mut roots);
    roots
}

#[cfg(not(any(
    target_os = "macos",
    target_os = "linux",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "windows"
)))]
fn default_clap_search_roots() -> Vec<PathBuf> {
    Vec::new()
}

fn is_supported_clap_binary(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("clap"))
}

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
                    .map(|f| f(plugin, c"clap.gui".as_ptr()));
                if let Some(ptr) = ext
                    && !ptr.is_null()
                {
                    let gui = &*(ptr as *const ClapPluginGui);
                    caps.has_gui = gui
                        .is_api_supported
                        .map(|f| f(plugin, c"x11".as_ptr(), true))
                        .unwrap_or(false)
                        || gui
                            .is_api_supported
                            .map(|f| f(plugin, c"win32".as_ptr(), true))
                            .unwrap_or(false)
                        || gui
                            .is_api_supported
                            .map(|f| f(plugin, c"cocoa".as_ptr(), true))
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
                    .map(|f| f(plugin, CLAP_EXT_PARAMS.as_ptr()));
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
                    .map(|f| f(plugin, c"clap.state".as_ptr()));
                if let Some(ptr) = ext
                    && !ptr.is_null()
                {
                    caps.has_state = true;
                }
            }

            unsafe {
                let ext = (*plugin)
                    .get_extension
                    .map(|f| f(plugin, CLAP_EXT_AUDIO_PORTS.as_ptr()));
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
                            caps.audio_inputs += info.channel_count as usize;
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
                            caps.audio_outputs += info.channel_count as usize;
                        }
                    }
                }
            }

            unsafe {
                let ext = (*plugin)
                    .get_extension
                    .map(|f| f(plugin, c"clap.note-ports".as_ptr()));
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
            path: format!("{}::{}", path_str, plugin_id),
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
    out.dedup_by(|a, b| {
        a.name.eq_ignore_ascii_case(&b.name) && a.path.eq_ignore_ascii_case(&b.path)
    });
    out
}

pub fn scan_vst3_plugins() -> Vec<crate::vst3::Vst3PluginInfo> {
    crate::vst3::host::Vst3Host::new().list_plugins()
}

#[cfg(unix)]
pub fn scan_lv2_plugins() -> Vec<crate::lv2::Lv2PluginInfo> {
    crate::lv2::Lv2Host::new(48_000.0).list_plugins()
}

#[cfg(unix)]
fn capture_stderr_during<F, R>(f: F) -> (R, String)
where
    F: FnOnce() -> R,
{
    use std::os::fd::RawFd;
    use std::thread;

    let mut pipe_fds: [RawFd; 2] = [-1; 2];
    unsafe {
        if libc::pipe(pipe_fds.as_mut_ptr()) != 0 {
            return (f(), String::new());
        }
    }
    let read_fd = pipe_fds[0];
    let write_fd = pipe_fds[1];

    let saved_stderr = unsafe { libc::dup(libc::STDERR_FILENO) };
    if saved_stderr < 0 {
        unsafe {
            libc::close(read_fd);
            libc::close(write_fd);
        }
        return (f(), String::new());
    }

    unsafe {
        if libc::dup2(write_fd, libc::STDERR_FILENO) < 0 {
            libc::close(saved_stderr);
            libc::close(read_fd);
            libc::close(write_fd);
            return (f(), String::new());
        }
    }

    let reader = thread::spawn(move || {
        let mut captured = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            let n = unsafe { libc::read(read_fd, buf.as_mut_ptr().cast::<c_void>(), buf.len()) };
            if n <= 0 {
                break;
            }
            captured.extend_from_slice(&buf[..n as usize]);
        }
        unsafe {
            libc::close(read_fd);
        }
        String::from_utf8_lossy(&captured).into_owned()
    });

    let result = f();

    unsafe {
        libc::dup2(saved_stderr, libc::STDERR_FILENO);
        libc::close(saved_stderr);
        libc::close(write_fd);
    }

    let captured = reader.join().unwrap_or_default();
    (result, captured)
}

#[cfg(not(unix))]
fn capture_stderr_during<F, R>(f: F) -> (R, String)
where
    F: FnOnce() -> R,
{
    (f(), String::new())
}

fn file_uri_to_bundle_uri(uri: &str) -> Option<String> {
    let path = uri.strip_prefix("file://")?;
    path_to_bundle_uri(path)
}

fn path_to_bundle_uri(path: &str) -> Option<String> {
    if path.ends_with(".lv2/manifest.ttl") {
        let bundle = &path[..path.len() - "manifest.ttl".len()];
        Some(format!("file://{bundle}"))
    } else if path.ends_with(".lv2/") {
        Some(format!("file://{path}"))
    } else if path.ends_with(".lv2") {
        Some(format!("file://{path}/"))
    } else {
        None
    }
}

fn extract_bundle_from_raw_path(message: &str) -> Option<String> {
    for suffix in [".lv2/manifest.ttl", ".lv2/"] {
        if let Some(pos) = message.find(suffix) {
            let before = &message[..pos + suffix.len()];
            let start = before.rfind(' ').map(|i| i + 1).unwrap_or(0);
            let path = &before[start..];
            return path_to_bundle_uri(path);
        }
    }
    None
}

fn extract_uris_from_message(message: &str) -> (Option<String>, Option<String>) {
    let mut plugin_uri = None;
    let mut bundle_uri = None;

    let mut rest = message;
    while let Some(start) = rest.find('<') {
        let after = &rest[start + 1..];
        if let Some(end) = after.find('>') {
            let uri = &after[..end];
            if uri.starts_with("http://") || uri.starts_with("https://") || uri.starts_with("urn:")
            {
                plugin_uri = Some(uri.to_string());
            } else if let Some(bundle) = file_uri_to_bundle_uri(uri) {
                bundle_uri = Some(bundle);
            }
            rest = &after[end + 1..];
        } else {
            break;
        }
    }

    if bundle_uri.is_none() {
        bundle_uri = extract_bundle_from_raw_path(message);
    }

    (plugin_uri, bundle_uri)
}

fn build_diagnostic(line: &str) -> ScanDiagnostic {
    let (plugin_uri, bundle_uri) = extract_uris_from_message(line);
    ScanDiagnostic {
        message: line.to_string(),
        plugin_uri,
        plugin_name: None,
        bundle_uri,
    }
}

fn classify_stderr(output: &str) -> (Vec<ScanDiagnostic>, Vec<ScanDiagnostic>) {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let lower = line.to_lowercase();
        if lower.contains("error:") {
            errors.push(build_diagnostic(line));
        } else if lower.contains("warning:") || lower.contains("note:") {
            warnings.push(build_diagnostic(line));
        }
    }
    (errors, warnings)
}

fn enrich_plugin_names(diagnostics: &mut [ScanDiagnostic], data: &serde_json::Value) {
    let items = data
        .as_array()
        .or_else(|| data.get("plugins").and_then(|p| p.as_array()));
    let Some(items) = items else {
        return;
    };

    for diag in diagnostics {
        if diag.plugin_name.is_some() {
            continue;
        }
        let search_uri = diag.plugin_uri.as_deref().or(diag.bundle_uri.as_deref());
        let Some(search) = search_uri else {
            continue;
        };

        for item in items {
            for key in ["uri", "bundle_uri", "path", "id"] {
                if let Some(value) = item.get(key).and_then(|v| v.as_str()) {
                    if value == search {
                        diag.plugin_name =
                            item.get("name").and_then(|v| v.as_str()).map(String::from);
                        break;
                    }
                }
            }
            if diag.plugin_name.is_some() {
                break;
            }
        }
    }
}

fn serialize_scan_output<T: Serialize>(data: T, stderr: &str) -> Result<String, serde_json::Error> {
    let (mut errors, mut warnings) = classify_stderr(stderr);
    let data_value = serde_json::to_value(&data)?;
    enrich_plugin_names(&mut errors, &data_value);
    enrich_plugin_names(&mut warnings, &data_value);
    serde_json::to_string_pretty(&ScanOutput {
        errors,
        warnings,
        data: data_value,
    })
}

pub fn run_scan(format: &str, plugin_path: &str, output_path: Option<&str>) -> i32 {
    let json = match format {
        "clap" => {
            if plugin_path == "--system" {
                let (data, stderr) = capture_stderr_during(|| scan_clap_plugins(false));
                match serialize_scan_output(data, &stderr) {
                    Ok(j) => j,
                    Err(_e) => {
                        return 1;
                    }
                }
            } else {
                let (data, stderr) = capture_stderr_during(|| scan_clap_plugin(plugin_path));
                match serialize_scan_output(data, &stderr) {
                    Ok(j) => j,
                    Err(_e) => {
                        return 1;
                    }
                }
            }
        }
        "vst3" => {
            if plugin_path != "--system" {
                return 1;
            }
            let (data, stderr) = capture_stderr_during(scan_vst3_plugins);
            match serialize_scan_output(data, &stderr) {
                Ok(j) => j,
                Err(_e) => {
                    return 1;
                }
            }
        }
        #[cfg(unix)]
        "lv2" => {
            if plugin_path != "--system" {
                return 1;
            }
            let (data, stderr) = capture_stderr_during(scan_lv2_plugins);
            match serialize_scan_output(data, &stderr) {
                Ok(j) => j,
                Err(_e) => {
                    return 1;
                }
            }
        }
        _ => {
            return 1;
        }
    };

    if let Some(path) = output_path {
        match std::fs::write(path, &json) {
            Ok(()) => 0,
            Err(_e) => 1,
        }
    } else {
        println!("{json}");
        0
    }
}

#[cfg(test)]
mod tests {
    use super::{build_diagnostic, classify_stderr, enrich_plugin_names};

    #[test]
    fn classify_stderr_separates_errors_and_warnings() {
        let stderr = "error: failed to open file\n\
            lilv_world_load_file(): error: Error loading file\n\
            load_dir_entry(): warning: Skipping non-directory\n\
            lilv_world_compare_versions(): note: Previously loaded\n\
            \n\
            some untagged line";
        let (errors, warnings) = classify_stderr(stderr);
        assert_eq!(errors.len(), 2);
        assert!(errors.iter().all(|e| e.message.contains("error:")));
        assert_eq!(warnings.len(), 2);
        assert!(warnings[0].message.contains("warning:"));
        assert!(warnings[1].message.contains("note:"));
    }

    #[test]
    fn classify_stderr_extracts_bundle_and_plugin_uris() {
        let stderr = "error: failed to open file /usr/lib/lv2/bg-midi-pattern.lv2/manifest.ttl (No such file or directory)\n\
            lilv_world_load_file(): error: Error loading file <file:///usr/lib/lv2/bg-midi-pattern.lv2/manifest.ttl> (Unknown error)\n\
            lilv_world_compare_versions(): warning: Ignoring duplicate version 0.1 of <http://polyeffects.com/lv2/cv_to_note> from <file:///usr/lib/lv2/cv_to_note.lv2/>";
        let (errors, warnings) = classify_stderr(stderr);
        assert_eq!(errors.len(), 2);
        assert_eq!(
            errors[0].bundle_uri,
            Some("file:///usr/lib/lv2/bg-midi-pattern.lv2/".to_string())
        );
        assert_eq!(
            errors[1].bundle_uri,
            Some("file:///usr/lib/lv2/bg-midi-pattern.lv2/".to_string())
        );
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings[0].plugin_uri,
            Some("http://polyeffects.com/lv2/cv_to_note".to_string())
        );
        assert_eq!(
            warnings[0].bundle_uri,
            Some("file:///usr/lib/lv2/cv_to_note.lv2/".to_string())
        );
    }

    #[test]
    fn enrich_plugin_names_finds_name_from_data() {
        let mut diag = build_diagnostic(
            "lilv_world_compare_versions(): warning: Ignoring duplicate version 0.1 of <http://example.com/plugin> from <file:///usr/lib/lv2/example.lv2/>",
        );
        let data = serde_json::json!([
            {
                "uri": "http://example.com/plugin",
                "name": "Example Plugin",
                "bundle_uri": "file:///usr/lib/lv2/example.lv2/"
            }
        ]);
        enrich_plugin_names(std::slice::from_mut(&mut diag), &data);
        assert_eq!(diag.plugin_name, Some("Example Plugin".to_string()));
    }
}
