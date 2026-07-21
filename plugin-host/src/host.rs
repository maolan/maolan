use crate::clap::{
    CLAP_EVENT_MIDI, CLAP_EVENT_NOTE_OFF, CLAP_EVENT_NOTE_ON, CLAP_EXT_AUDIO_PORTS,
    CLAP_EXT_PARAMS, CLAP_EXT_TIMER_SUPPORT, ClapAudioBuffer, ClapEventHeader, ClapEventMidi,
    ClapEventNote, ClapEventParamGesture, ClapEventParamMod, ClapEventParamValue, ClapPluginParams,
    ClapProcess, EventBuffer, EventCapture, PluginInstance, ThreadType, host_timers_snapshot,
    set_thread_type,
};
#[cfg(unix)]
use crate::clap::{CLAP_EXT_POSIX_FD_SUPPORT, host_fds_snapshot};
use crate::events::EventPair;
#[cfg(windows)]
use crate::gui_win32::win32::{ContainerWindow, create_container_window};
use crate::protocol::*;
use crate::ringbuf::RingBuffer;
use crate::shm::ShmMapping;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
#[cfg(windows)]
use windows_sys::Win32::UI::WindowsAndMessaging::{
    SW_HIDE, SW_SHOW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER, SetWindowPos, ShowWindow,
};

static PARAMS_FLUSH_REQUESTED: AtomicBool = AtomicBool::new(false);

pub fn request_params_flush() {
    PARAMS_FLUSH_REQUESTED.store(true, Ordering::Release);
}

static AUDIO_PORTS_RESCAN_REQUESTED: AtomicBool = AtomicBool::new(false);

pub fn request_audio_ports_rescan() {
    AUDIO_PORTS_RESCAN_REQUESTED.store(true, Ordering::Release);
}

const SHM_LATENCY_SAMPLES_OFFSET: usize = 84;

unsafe fn latency_samples_atomic(ptr: *mut u8) -> &'static std::sync::atomic::AtomicU32 {
    unsafe { &*(ptr.add(SHM_LATENCY_SAMPLES_OFFSET) as *const std::sync::atomic::AtomicU32) }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn close_x11_gui_window(window: &mut Option<crate::gui_x11::x11::ContainerWindow>) {
    if let Some(window) = window.take() {
        drop(window);
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn abandon_x11_gui_window(window: &mut Option<crate::gui_x11::x11::ContainerWindow>) {
    if let Some(window) = window.take() {
        std::mem::forget(window);
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn floating_gui_parent_api() -> GuiParentApi {
    let forced_x11 = ["WINIT_UNIX_BACKEND", "GDK_BACKEND", "QT_QPA_PLATFORM"]
        .iter()
        .filter_map(std::env::var_os)
        .filter_map(|value| value.into_string().ok())
        .any(|value| {
            let value = value.to_ascii_lowercase();
            value.contains("x11") || value.contains("xcb")
        });
    if forced_x11 {
        GuiParentApi::X11
    } else if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        GuiParentApi::Wayland
    } else if std::env::var_os("DISPLAY").is_some() {
        GuiParentApi::X11
    } else {
        GuiParentApi::None
    }
}

struct PortBuffers {
    inputs: Vec<ClapAudioBuffer>,
    outputs: Vec<ClapAudioBuffer>,
    _input_ptrs: Vec<Vec<*mut f32>>,
    _output_ptrs: Vec<Vec<*mut f32>>,
}

impl PortBuffers {
    fn from_plugin(
        plugin: *const crate::clap::ClapPlugin,
        ptr: *mut u8,
        num_in: usize,
        num_out: usize,
    ) -> Option<Self> {
        let ext = unsafe {
            (*plugin)
                .get_extension
                .map(|f| f(plugin, CLAP_EXT_AUDIO_PORTS.as_ptr()))
        }?;
        if ext.is_null() {
            return None;
        }
        let ap = unsafe { &*(ext as *const crate::clap::ClapPluginAudioPorts) };
        let in_count = ap.count.map(|f| unsafe { f(plugin, true) }).unwrap_or(0) as usize;
        let out_count = ap.count.map(|f| unsafe { f(plugin, false) }).unwrap_or(0) as usize;

        let mut inputs = Vec::with_capacity(in_count);
        let mut input_ptrs = Vec::with_capacity(in_count);
        let mut global_ch: usize = 0;
        for i in 0..in_count {
            let mut info = crate::clap::ClapAudioPortInfo {
                id: 0,
                name: [0; 256],
                flags: 0,
                channel_count: 1,
                port_type: ptr::null(),
                in_place_pair: 0,
            };
            let ch_count = if ap
                .get
                .map(|f| unsafe { f(plugin, i as u32, true, &mut info) })
                .unwrap_or(false)
            {
                info.channel_count.max(1) as usize
            } else {
                1
            };
            let mut port_channels = Vec::with_capacity(ch_count);
            for _ in 0..ch_count {
                let shm_ptr = if global_ch < num_in {
                    unsafe { audio_channel_ptr(ptr, global_ch, 0) }
                } else {
                    ptr::null_mut()
                };
                port_channels.push(shm_ptr);
                global_ch += 1;
            }
            inputs.push(ClapAudioBuffer {
                data32: port_channels.as_mut_ptr(),
                data64: ptr::null_mut(),
                channel_count: port_channels.len() as u32,
                latency: 0,
                constant_mask: 0,
            });
            input_ptrs.push(port_channels);
        }

        let mut outputs = Vec::with_capacity(out_count);
        let mut output_ptrs = Vec::with_capacity(out_count);
        global_ch = 0;
        for i in 0..out_count {
            let mut info = crate::clap::ClapAudioPortInfo {
                id: 0,
                name: [0; 256],
                flags: 0,
                channel_count: 1,
                port_type: ptr::null(),
                in_place_pair: 0,
            };
            let ch_count = if ap
                .get
                .map(|f| unsafe { f(plugin, i as u32, false, &mut info) })
                .unwrap_or(false)
            {
                info.channel_count.max(1) as usize
            } else {
                1
            };
            let mut port_channels = Vec::with_capacity(ch_count);
            for _ in 0..ch_count {
                let shm_ptr = if global_ch < num_out {
                    unsafe { audio_channel_ptr(ptr, global_ch, 1) }
                } else {
                    ptr::null_mut()
                };
                port_channels.push(shm_ptr);
                global_ch += 1;
            }
            outputs.push(ClapAudioBuffer {
                data32: port_channels.as_mut_ptr(),
                data64: ptr::null_mut(),
                channel_count: port_channels.len() as u32,
                latency: 0,
                constant_mask: 0,
            });
            output_ptrs.push(port_channels);
        }

        Some(Self {
            inputs,
            outputs,
            _input_ptrs: input_ptrs,
            _output_ptrs: output_ptrs,
        })
    }
}

pub struct HostRuntime {
    pub mapping: ShmMapping,
    pub events: EventPair,
    pub format: String,
    pub plugin_path: String,
    pub instance_id: String,
}

impl HostRuntime {
    pub fn attach(
        shm_name: &str,
        events: EventPair,
        format: String,
        plugin_path: String,
        instance_id: String,
    ) -> Result<Self, String> {
        let mapping = ShmMapping::open_existing(shm_name, SHM_SIZE)?;
        Ok(Self {
            mapping,
            events,
            format,
            plugin_path,
            instance_id,
        })
    }

    fn plugin_id(&self) -> &str {
        if let Some(pos) = self.plugin_path.rfind("::") {
            &self.plugin_path[pos + 2..]
        } else if let Some(pos) = self.plugin_path.rfind('#') {
            &self.plugin_path[pos + 1..]
        } else {
            ""
        }
    }

    fn real_plugin_path(&self) -> &str {
        if let Some(pos) = self.plugin_path.rfind("::") {
            &self.plugin_path[..pos]
        } else if let Some(pos) = self.plugin_path.rfind('#') {
            &self.plugin_path[..pos]
        } else {
            &self.plugin_path
        }
    }

    pub fn signal_ready(&self) {
        let header = unsafe { header_mut(self.mapping.as_ptr()) };
        header.ready.store(1, Ordering::Release);
    }

    pub fn write_test_magic(&self) {
        let scratch = unsafe { scratch_ptr(self.mapping.as_ptr()) };
        let magic: u32 = 0xDEADBEEF;
        unsafe {
            std::ptr::write_unaligned(scratch as *mut u32, magic);
        }
    }

    pub fn run_until_shutdown(&self) {
        let header = unsafe { header_ref(self.mapping.as_ptr()) };
        let start = Instant::now();
        loop {
            if header.shutdown_request.load(Ordering::Acquire) != 0 {
                break;
            }
            if start.elapsed() >= Duration::from_millis(100) {
                header.heartbeat.fetch_add(1, Ordering::Relaxed);
            }
            match self.events.wait_daw(Duration::from_millis(100)) {
                Ok(()) => continue,
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
                Err(_e) => {
                    break;
                }
            }
        }
    }

    pub fn run_null_plugin(&self) {
        let header = unsafe { header_ref(self.mapping.as_ptr()) };
        let ptr = self.mapping.as_ptr();

        loop {
            if header.shutdown_request.load(Ordering::Acquire) != 0 {
                break;
            }

            match self.events.wait_daw(Duration::from_millis(100)) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
                Err(_e) => {
                    break;
                }
            }

            let block_size = header.block_size.load(Ordering::Acquire) as usize;
            let num_in = header.num_input_channels.load(Ordering::Acquire) as usize;
            let num_out = header.num_output_channels.load(Ordering::Acquire) as usize;

            if block_size == 0 || block_size > MAX_BLOCK_SIZE {
                let _ = self.events.signal_daw();
                continue;
            }

            let max_ch = num_in.min(num_out).min(MAX_CHANNELS);
            for ch in 0..max_ch {
                let in_ptr = unsafe { audio_channel_ptr(ptr, ch, 0) };
                let out_ptr = unsafe { audio_channel_ptr(ptr, ch, 1) };
                unsafe {
                    std::ptr::copy_nonoverlapping(in_ptr, out_ptr, block_size);
                }
            }

            if let Err(_e) = self.events.signal_daw() {
                break;
            }
        }
    }

    fn try_clap_native_floating(plugin: &mut PluginInstance) -> Result<(), String> {
        if plugin.gui_created() {
            plugin.gui_destroy();
        }

        #[cfg(all(unix, not(target_os = "macos")))]
        {
            let backend = floating_gui_parent_api();
            let apis: &[&str] = match backend {
                GuiParentApi::Wayland => &["wayland", "x11"],
                GuiParentApi::X11 => &["x11"],
                GuiParentApi::None => &["x11", "wayland"],
            };

            if let Some((ref preferred, true)) = plugin.gui_preferred_api()
                && apis.iter().any(|api| api == preferred)
                && plugin.gui_create(preferred, true).is_ok()
            {
                return plugin.gui_show();
            }

            for api in apis {
                if plugin.gui_is_api_supported(api, true) && plugin.gui_create(api, true).is_ok() {
                    return plugin.gui_show();
                }
            }
        }

        #[cfg(windows)]
        {
            if let Some((ref api, true)) = plugin.gui_preferred_api()
                && api == "win32"
            {
                plugin
                    .gui_create(api, true)
                    .and_then(|_| plugin.gui_show())?;
                return Ok(());
            }
            if plugin.gui_is_api_supported("win32", true)
                && plugin.gui_create("win32", true).is_ok()
            {
                return plugin.gui_show();
            }
        }

        #[cfg(target_os = "macos")]
        {
            if let Some((ref api, true)) = plugin.gui_preferred_api()
                && api == "cocoa"
            {
                plugin
                    .gui_create(api, true)
                    .and_then(|_| plugin.gui_show())?;
                return Ok(());
            }
            if plugin.gui_is_api_supported("cocoa", true)
                && plugin.gui_create("cocoa", true).is_ok()
            {
                return plugin.gui_show();
            }
        }

        Err("Plugin does not support native floating GUI for the selected backend".to_string())
    }

    fn serialize_clap_parameters(
        scratch: *mut u8,
        params: &[crate::clap::ParamInfo],
    ) -> Result<usize, String> {
        let max_len = SCRATCH_SIZE;
        let mut offset = 0usize;

        if offset + 4 > max_len {
            return Err("scratch overflow".to_string());
        }
        unsafe {
            std::ptr::write_unaligned(scratch.add(offset) as *mut u32, params.len() as u32);
        }
        offset += 4;

        for param in params {
            if offset + 4 > max_len {
                return Err("scratch overflow".to_string());
            }
            unsafe {
                std::ptr::write_unaligned(scratch.add(offset) as *mut u32, param.id);
            }
            offset += 4;

            let name_bytes = param.name.as_bytes();
            if offset + 4 > max_len {
                return Err("scratch overflow".to_string());
            }
            unsafe {
                std::ptr::write_unaligned(scratch.add(offset) as *mut u32, name_bytes.len() as u32);
            }
            offset += 4;
            if offset + name_bytes.len() > max_len {
                return Err("scratch overflow".to_string());
            }
            unsafe {
                std::ptr::copy_nonoverlapping(
                    name_bytes.as_ptr(),
                    scratch.add(offset),
                    name_bytes.len(),
                );
            }
            offset += name_bytes.len();

            let module_bytes = param.module.as_bytes();
            if offset + 4 > max_len {
                return Err("scratch overflow".to_string());
            }
            unsafe {
                std::ptr::write_unaligned(
                    scratch.add(offset) as *mut u32,
                    module_bytes.len() as u32,
                );
            }
            offset += 4;
            if offset + module_bytes.len() > max_len {
                return Err("scratch overflow".to_string());
            }
            unsafe {
                std::ptr::copy_nonoverlapping(
                    module_bytes.as_ptr(),
                    scratch.add(offset),
                    module_bytes.len(),
                );
            }
            offset += module_bytes.len();

            if offset + 24 > max_len {
                return Err("scratch overflow".to_string());
            }
            unsafe {
                std::ptr::write_unaligned(
                    scratch.add(offset) as *mut u64,
                    param.min_value.to_bits(),
                );
                std::ptr::write_unaligned(
                    scratch.add(offset + 8) as *mut u64,
                    param.max_value.to_bits(),
                );
                std::ptr::write_unaligned(
                    scratch.add(offset + 16) as *mut u64,
                    param.default_value.to_bits(),
                );
            }
            offset += 24;
        }

        Ok(offset)
    }

    fn serialize_clap_note_names(
        scratch: *mut u8,
        note_names: &std::collections::HashMap<u8, String>,
    ) -> Result<usize, String> {
        let max_len = SCRATCH_SIZE;
        let mut offset = 0usize;

        if offset + 4 > max_len {
            return Err("scratch overflow".to_string());
        }
        unsafe {
            std::ptr::write_unaligned(scratch.add(offset) as *mut u32, note_names.len() as u32);
        }
        offset += 4;

        for (note, name) in note_names {
            if offset + 4 > max_len {
                return Err("scratch overflow".to_string());
            }
            unsafe {
                std::ptr::write_unaligned(scratch.add(offset) as *mut u32, u32::from(*note));
            }
            offset += 4;

            let name_bytes = name.as_bytes();
            if offset + 4 > max_len {
                return Err("scratch overflow".to_string());
            }
            unsafe {
                std::ptr::write_unaligned(scratch.add(offset) as *mut u32, name_bytes.len() as u32);
            }
            offset += 4;
            if offset + name_bytes.len() > max_len {
                return Err("scratch overflow".to_string());
            }
            unsafe {
                std::ptr::copy_nonoverlapping(
                    name_bytes.as_ptr(),
                    scratch.add(offset),
                    name_bytes.len(),
                );
            }
            offset += name_bytes.len();
        }

        Ok(offset)
    }

    pub fn run_clap_plugin(&self) {
        let mut plugin = match PluginInstance::new(self.real_plugin_path(), self.plugin_id()) {
            Ok(p) => p,
            Err(_e) => {
                return;
            }
        };

        let ptr = self.mapping.as_ptr();
        let header = unsafe { header_ref(self.mapping.as_ptr()) };

        unsafe {
            let host_ptr = plugin.host_ptr();
            if !host_ptr.is_null() {
                let host_data = (*host_ptr).host_data as *mut crate::clap::HostData;
                if !host_data.is_null() {
                    (*host_data).header = header as *const _ as *mut _;
                }
            }
        }

        unsafe {
            maolan_plugin_protocol::protocol::write_plugin_name_to_scratch(ptr, &plugin.name());
        }

        let sample_rate = unsafe {
            let ts = transport_ref(ptr);
            if ts.sample_rate_hz > 0.0 {
                ts.sample_rate_hz
            } else {
                48000.0
            }
        };

        if let Err(_e) = plugin.activate(sample_rate, 1, MAX_BLOCK_SIZE as u32) {
            return;
        }

        let mut port_buffers = PortBuffers::from_plugin(plugin.plugin_ptr(), ptr, 0, 0);

        let has_note_ports = unsafe {
            (*plugin.plugin_ptr())
                .get_extension
                .map(|f| {
                    f(
                        plugin.plugin_ptr(),
                        crate::clap::CLAP_EXT_NOTE_PORTS.as_ptr(),
                    )
                })
                .filter(|p| !p.is_null())
                .is_some()
        };

        let audio_in_channels = port_buffers
            .as_ref()
            .map(|pb| pb.inputs.iter().map(|p| p.channel_count).sum::<u32>())
            .unwrap_or(0);
        let audio_out_channels = port_buffers
            .as_ref()
            .map(|pb| pb.outputs.iter().map(|p| p.channel_count).sum::<u32>())
            .unwrap_or(0);
        let (midi_in_ports, midi_out_ports) = unsafe {
            let ext = (*plugin.plugin_ptr()).get_extension.map(|f| {
                f(
                    plugin.plugin_ptr(),
                    crate::clap::CLAP_EXT_NOTE_PORTS.as_ptr(),
                )
            });
            if let Some(ptr) = ext
                && !ptr.is_null()
            {
                let np = &*(ptr as *const crate::clap::ClapPluginNotePorts);
                let in_count = np.count.map(|f| f(plugin.plugin_ptr(), true)).unwrap_or(0);
                let out_count = np.count.map(|f| f(plugin.plugin_ptr(), false)).unwrap_or(0);
                (in_count, out_count)
            } else {
                (0, 0)
            }
        };
        unsafe {
            let header = maolan_plugin_protocol::protocol::header_mut(ptr);
            header
                .midi_in_port_count
                .store(midi_in_ports, Ordering::Release);
            header
                .midi_out_port_count
                .store(midi_out_ports, Ordering::Release);
            maolan_plugin_protocol::protocol::write_port_counts_to_scratch(
                ptr,
                audio_in_channels,
                audio_out_channels,
                midi_in_ports,
                midi_out_ports,
            );
            latency_samples_atomic(ptr).store(plugin.latency_samples(), Ordering::Release);
        }

        self.signal_ready();

        let param_ring = unsafe {
            let buf = param_ring_ptr(ptr);
            let (w, r) = param_indices(ptr);
            RingBuffer::new(buf, w, r, RING_CAPACITY)
        };

        let echo_ring = unsafe {
            let buf = echo_ring_ptr(ptr);
            let (w, r) = echo_indices(ptr);
            RingBuffer::new(buf, w, r, RING_CAPACITY)
        };

        let mut midi_in_rings: Vec<RingBuffer<maolan_plugin_protocol::protocol::MidiEvent>> = unsafe {
            (0..midi_in_ports as usize)
                .map(|port| {
                    let buf = midi_in_ring_ptr(ptr, port);
                    let (w, r) = midi_in_indices(ptr, port);
                    RingBuffer::new(buf, w, r, RING_CAPACITY)
                })
                .collect()
        };
        let midi_out_rings: Vec<RingBuffer<maolan_plugin_protocol::protocol::MidiEvent>> = unsafe {
            (0..midi_out_ports as usize)
                .map(|port| {
                    let buf = midi_out_ring_ptr(ptr, port);
                    let (w, r) = midi_out_indices(ptr, port);
                    RingBuffer::new(buf, w, r, RING_CAPACITY)
                })
                .collect()
        };

        let params_ext = unsafe {
            (*plugin.plugin_ptr())
                .get_extension
                .map(|f| f(plugin.plugin_ptr(), CLAP_EXT_PARAMS.as_ptr()))
                .filter(|p| !p.is_null())
                .map(|p| p as *const ClapPluginParams)
        };

        let timer_ext = unsafe {
            (*plugin.plugin_ptr())
                .get_extension
                .map(|f| f(plugin.plugin_ptr(), CLAP_EXT_TIMER_SUPPORT.as_ptr()))
                .filter(|p| !p.is_null())
                .map(|p| p as *const crate::clap::ClapPluginTimerSupport)
        };

        #[cfg(unix)]
        let fd_ext = unsafe {
            (*plugin.plugin_ptr())
                .get_extension
                .map(|f| f(plugin.plugin_ptr(), CLAP_EXT_POSIX_FD_SUPPORT.as_ptr()))
                .filter(|p| !p.is_null())
                .map(|p| p as *const crate::clap::ClapPluginPosixFdSupport)
        };
        let mut steady_time: i64 = 0;
        #[cfg(unix)]
        let daw_read_fd = self.events.host_read_fd();
        let mut started_processing = false;
        #[cfg(windows)]
        let mut clap_gui_window: Option<ContainerWindow> = None;
        #[cfg(all(unix, not(target_os = "macos")))]
        let mut clap_gui_window_x11: Option<crate::gui_x11::x11::ContainerWindow> = None;
        let mut _resource_directory: Option<String> = None;

        loop {
            if header.shutdown_request.load(Ordering::Acquire) != 0 {
                break;
            }

            if crate::clap::take_host_gui_closed_requested() {
                plugin.gui_destroy();
                #[cfg(windows)]
                {
                    clap_gui_window = None;
                }
                #[cfg(all(unix, not(target_os = "macos")))]
                {
                    close_x11_gui_window(&mut clap_gui_window_x11);
                }
            }

            let req = header.request_type.load(Ordering::Acquire);
            if req != 0 {
                let scratch = unsafe { scratch_ptr(ptr) };
                let result = match req {
                    1 => match plugin.save_state() {
                        Ok(bytes) if bytes.len() <= SCRATCH_SIZE => {
                            unsafe {
                                std::ptr::copy_nonoverlapping(bytes.as_ptr(), scratch, bytes.len());
                            }
                            header
                                .scratch_size
                                .store(bytes.len() as u32, Ordering::Release);
                            Ok(())
                        }
                        Ok(bytes) => Err(format!(
                            "CLAP state is too large for scratch buffer: {} bytes",
                            bytes.len()
                        )),
                        Err(e) => Err(e),
                    },
                    2 => {
                        let size = header.scratch_size.load(Ordering::Acquire) as usize;
                        if size > SCRATCH_SIZE {
                            Err(format!("Invalid CLAP state size: {size} bytes"))
                        } else {
                            let bytes = unsafe { std::slice::from_raw_parts(scratch, size) };
                            plugin.load_state(bytes)
                        }
                    }
                    3 => {
                        let gui_supported = plugin.gui_is_supported();
                        if !gui_supported {
                            Err("Plugin does not support GUI".to_string())
                        } else {
                            #[cfg(windows)]
                            {
                                let gui_mode = header.gui_mode();

                                let native_floating = if gui_mode == GuiMode::Floating {
                                    let native = Self::try_clap_native_floating(&mut plugin);
                                    if native.is_ok() {
                                        clap_gui_window = None;
                                    }
                                    native
                                } else {
                                    Err("GUI mode is embedded".to_string())
                                };

                                if native_floating.is_ok() {
                                    native_floating
                                } else {
                                    if plugin.gui_created() {
                                        plugin.gui_destroy();
                                    }
                                    clap_gui_window = None;

                                    let title = std::path::Path::new(self.real_plugin_path())
                                        .file_stem()
                                        .and_then(|s| s.to_str())
                                        .unwrap_or("Plugin");
                                    let parent = if gui_mode == GuiMode::Embedded {
                                        let p = header.parent_window_usize();
                                        if p != 0 {
                                            Some(p as windows_sys::Win32::Foundation::HWND)
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    };
                                    match unsafe {
                                        create_container_window(
                                            parent.unwrap_or(std::ptr::null_mut()),
                                            title,
                                            800,
                                            600,
                                        )
                                    } {
                                        Ok(window) => {
                                            let hwnd = window.hwnd;
                                            let result = plugin
                                                .gui_create("win32", false)
                                                .inspect(|_| ())
                                                .inspect_err(|_e| ())
                                                .and_then(|_| {
                                                    plugin
                                                        .gui_get_size()
                                                        .inspect(|(_w, _h)| ())
                                                        .inspect_err(|_e| ())
                                                })
                                                .and_then(|(w, h)| {
                                                    if w > 0 && h > 0 {
                                                        unsafe {
                                                            SetWindowPos(
                                                                hwnd,
                                                                std::ptr::null_mut(),
                                                                0,
                                                                0,
                                                                w as i32,
                                                                h as i32,
                                                                SWP_NOMOVE
                                                                    | SWP_NOZORDER
                                                                    | SWP_NOACTIVATE,
                                                            );
                                                        }
                                                    }
                                                    plugin
                                                        .gui_set_parent(hwnd as u64)
                                                        .inspect(|_| ())
                                                        .inspect_err(|_e| ())
                                                })
                                                .and_then(|_| {
                                                    unsafe {
                                                        ShowWindow(hwnd, SW_SHOW);
                                                    }
                                                    plugin
                                                        .gui_show()
                                                        .inspect(|_| ())
                                                        .inspect_err(|_e| ())
                                                });
                                            if result.is_ok() {
                                                clap_gui_window = Some(window);
                                            }
                                            result
                                        }
                                        Err(e) => Err(e),
                                    }
                                }
                            }
                            #[cfg(all(unix, not(target_os = "macos")))]
                            {
                                let gui_mode = header.gui_mode();

                                // If the DAW wants a floating UI and the plugin supports native
                                // floating, let the plugin create its own top-level window.
                                let native_floating = if gui_mode == GuiMode::Floating {
                                    let native = Self::try_clap_native_floating(&mut plugin);
                                    if native.is_ok() {
                                        close_x11_gui_window(&mut clap_gui_window_x11);
                                    }
                                    native
                                } else {
                                    Err("GUI mode is embedded".to_string())
                                };

                                if native_floating.is_ok() {
                                    native_floating
                                } else {
                                    let embedded_wayland = if gui_mode == GuiMode::Embedded
                                        && header.gui_parent_api() == GuiParentApi::Wayland
                                    {
                                        let parent = header.parent_window_usize() as u64;
                                        if parent != 0
                                            && plugin.gui_is_api_supported("wayland", false)
                                        {
                                            if plugin.gui_created() {
                                                plugin.gui_destroy();
                                            }
                                            close_x11_gui_window(&mut clap_gui_window_x11);
                                            plugin
                                                .gui_create("wayland", false)
                                                .and_then(|_| {
                                                    plugin
                                                        .gui_set_parent_with_api(parent, "wayland")
                                                })
                                                .and_then(|_| plugin.gui_show())
                                        } else {
                                            Err("Plugin does not support embedded Wayland GUI"
                                                .to_string())
                                        }
                                    } else {
                                        Err("GUI mode is not embedded Wayland".to_string())
                                    };

                                    if embedded_wayland.is_ok()
                                        || (gui_mode == GuiMode::Embedded
                                            && header.gui_parent_api() == GuiParentApi::Wayland)
                                    {
                                        embedded_wayland
                                    } else {
                                        let already_created = plugin.gui_created();
                                        if plugin.gui_created() {
                                            abandon_x11_gui_window(&mut clap_gui_window_x11);
                                        } else {
                                            close_x11_gui_window(&mut clap_gui_window_x11);
                                        }

                                        let title = std::path::Path::new(self.real_plugin_path())
                                            .file_stem()
                                            .and_then(|s| s.to_str())
                                            .unwrap_or("Plugin");
                                        let parent = if gui_mode == GuiMode::Embedded {
                                            let p = header.parent_window_usize();
                                            if p != 0
                                                && header.gui_parent_api() == GuiParentApi::X11
                                            {
                                                Some(p as crate::gui_x11::x11::Window)
                                            } else {
                                                None
                                            }
                                        } else {
                                            None
                                        };
                                        match crate::gui_x11::x11::create_container_window(
                                            None, parent, title, 800, 600,
                                        ) {
                                            Ok(window) => {
                                                let window_id = window.window();
                                                window.map();
                                                let result = if already_created {
                                                    Ok(())
                                                } else {
                                                    plugin
                                                        .gui_create("x11", false)
                                                        .inspect(|_| ())
                                                        .inspect_err(|_e| ())
                                                }
                                                .and_then(|_| {
                                                    plugin
                                                        .gui_get_size()
                                                        .inspect(|(_w, _h)| ())
                                                        .inspect_err(|_e| ())
                                                })
                                                .and_then(|(w, h)| {
                                                    if w > 0 && h > 0 {
                                                        window.resize(w, h);
                                                    }
                                                    plugin
                                                        .gui_set_parent(window_id)
                                                        .inspect(|_| ())
                                                        .inspect_err(|_e| ())
                                                })
                                                .and_then(|_| {
                                                    plugin
                                                        .gui_show()
                                                        .inspect(|_| ())
                                                        .inspect_err(|_e| ())
                                                });
                                                if result.is_ok() {
                                                    clap_gui_window_x11 = Some(window);
                                                }
                                                result
                                            }
                                            Err(e) => Err(e),
                                        }
                                    }
                                }
                            }
                            #[cfg(target_os = "macos")]
                            {
                                let gui_mode = header.gui_mode();
                                let is_floating = gui_mode == GuiMode::Floating;
                                let window_id = if is_floating {
                                    0
                                } else {
                                    header.parent_window_usize() as u64
                                };

                                if plugin.gui_created() {
                                    plugin.gui_destroy();
                                }
                                let create_result = plugin.gui_create("cocoa", is_floating);
                                create_result
                                    .and_then(|_| {
                                        if window_id != 0 {
                                            plugin.gui_set_parent(window_id)
                                        } else {
                                            Ok(())
                                        }
                                    })
                                    .and_then(|_| plugin.gui_show())
                            }
                        }
                    }
                    4 => {
                        let hide_result = if plugin.gui_created() {
                            plugin.gui_hide()
                        } else {
                            Ok(())
                        };
                        plugin.gui_destroy();
                        #[cfg(windows)]
                        {
                            if let Some(ref window) = clap_gui_window {
                                unsafe {
                                    ShowWindow(window.hwnd, SW_HIDE);
                                }
                            }
                            clap_gui_window = None;
                        }
                        #[cfg(all(unix, not(target_os = "macos")))]
                        {
                            close_x11_gui_window(&mut clap_gui_window_x11);
                        }
                        hide_result
                    }
                    5 => {
                        std::sync::atomic::fence(Ordering::SeqCst);
                        let dir = unsafe { read_resource_directory_from_scratch(ptr) };
                        tracing::info!(?dir, "CLAP host request 5 read resource directory");
                        match dir {
                            Some(dir) => {
                                _resource_directory = Some(dir.clone());
                                Ok(())
                            }
                            None => Err("Invalid resource directory in scratch".to_string()),
                        }
                    }
                    6 => match plugin.file_references() {
                        Ok(refs) => unsafe {
                            write_file_references_to_scratch(ptr, &refs).map_err(|e| {
                                format!("Failed to write file references to scratch: {e}")
                            })
                        },
                        Err(e) => Err(e),
                    },
                    7 => {
                        if let Some((index, path)) =
                            unsafe { read_file_reference_update_from_scratch(ptr) }
                        {
                            plugin.update_file_reference_path(index, &path)
                        } else {
                            Err("Invalid file-reference update in scratch".to_string())
                        }
                    }
                    maolan_plugin_protocol::protocol::REQUEST_CLAP_PARAMETERS => {
                        tracing::info!("CLAP host: received parameter request");
                        let params = plugin.parameter_infos();
                        tracing::info!(count = params.len(), "CLAP host: got parameter infos");
                        match Self::serialize_clap_parameters(scratch, &params) {
                            Ok(size) => {
                                header.scratch_size.store(size as u32, Ordering::Release);
                                Ok(())
                            }
                            Err(e) => Err(e),
                        }
                    }
                    maolan_plugin_protocol::protocol::REQUEST_CLAP_NOTE_NAMES => {
                        tracing::info!("CLAP host: received note-name request");
                        let note_names = plugin.note_names();
                        tracing::info!(count = note_names.len(), "CLAP host: got note names");
                        match Self::serialize_clap_note_names(scratch, &note_names) {
                            Ok(size) => {
                                header.scratch_size.store(size as u32, Ordering::Release);
                                Ok(())
                            }
                            Err(e) => Err(e),
                        }
                    }
                    _ => Err(format!("Unknown request type: {req}")),
                };
                header
                    .request_status
                    .store(if result.is_ok() { 1 } else { 2 }, Ordering::Release);
                if matches!(
                    req,
                    1 | 2
                        | 5
                        | 6
                        | 7
                        | maolan_plugin_protocol::protocol::REQUEST_CLAP_PARAMETERS
                        | maolan_plugin_protocol::protocol::REQUEST_CLAP_NOTE_NAMES
                ) {
                    let _ = self.events.signal_daw();
                }
                header.request_type.store(0, Ordering::Release);
                continue;
            }

            set_thread_type(ThreadType::MainThread);

            self.handle_idle_work(&plugin, params_ext, timer_ext);

            let timeout_ms = self.next_timer_ms().min(100);

            #[cfg(unix)]
            {
                let (daw_ready, ready_fds) = match timeout_ms {
                    0 => (true, Vec::new()),
                    ms => self.poll_daw_and_fds(daw_read_fd, Duration::from_millis(ms)),
                };

                if let Some(ext) = fd_ext {
                    for (fd, flags) in ready_fds {
                        unsafe {
                            if let Some(cb) = (*ext).on_fd {
                                cb(plugin.plugin_ptr(), fd, flags);
                            }
                        }
                    }
                }

                if !daw_ready {
                    continue;
                }
            }

            #[cfg(windows)]
            {
                match self
                    .events
                    .wait_daw_with_message_pump(Duration::from_millis(timeout_ms.max(1)))
                {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
                    Err(_e) => {
                        break;
                    }
                }
            }

            let block_size = header.block_size.load(Ordering::Acquire) as usize;
            let num_in = header.num_input_channels.load(Ordering::Acquire) as usize;
            let num_out = header.num_output_channels.load(Ordering::Acquire) as usize;

            if block_size == 0 || block_size > MAX_BLOCK_SIZE {
                let _ = self.events.signal_daw();
                continue;
            }

            if AUDIO_PORTS_RESCAN_REQUESTED.swap(false, Ordering::Acquire) {
                port_buffers = PortBuffers::from_plugin(plugin.plugin_ptr(), ptr, num_in, num_out);
            }

            if let Some(ref mut pb) = port_buffers {
                let mut global_ch: usize = 0;
                for port in &mut pb._input_ptrs {
                    for ch in port.iter_mut() {
                        *ch = if global_ch < num_in {
                            unsafe { audio_channel_ptr(ptr, global_ch, 0) }
                        } else {
                            ptr::null_mut()
                        };
                        global_ch += 1;
                    }
                }
                global_ch = 0;
                for port in &mut pb._output_ptrs {
                    for ch in port.iter_mut() {
                        *ch = if global_ch < num_out {
                            unsafe { audio_channel_ptr(ptr, global_ch, 1) }
                        } else {
                            ptr::null_mut()
                        };
                        global_ch += 1;
                    }
                }
            } else {
                let mut in_ptrs: [*mut f32; MAX_CHANNELS] = [ptr::null_mut(); MAX_CHANNELS];
                let mut out_ptrs: [*mut f32; MAX_CHANNELS] = [ptr::null_mut(); MAX_CHANNELS];
                for (ch, in_ptr) in in_ptrs
                    .iter_mut()
                    .enumerate()
                    .take(num_in.min(MAX_CHANNELS))
                {
                    *in_ptr = unsafe { audio_channel_ptr(ptr, ch, 0) };
                }
                for (ch, out_ptr) in out_ptrs
                    .iter_mut()
                    .enumerate()
                    .take(num_out.min(MAX_CHANNELS))
                {
                    *out_ptr = unsafe { audio_channel_ptr(ptr, ch, 1) };
                }
            }

            let mut event_buf = EventBuffer::new();
            while let Some(ev) = param_ring.pop() {
                match ev.event_kind {
                    PARAM_EVENT_MOD => {
                        event_buf.push_param_mod(ev.param_index, ev.value as f64, ev.sample_offset);
                    }
                    PARAM_EVENT_GESTURE_BEGIN => {
                        event_buf.push_param_gesture_begin(ev.param_index, ev.sample_offset);
                    }
                    PARAM_EVENT_GESTURE_END => {
                        event_buf.push_param_gesture_end(ev.param_index, ev.sample_offset);
                    }
                    _ => {
                        event_buf.push_param_value(
                            ev.param_index,
                            ev.value as f64,
                            ev.sample_offset,
                        );
                    }
                }
            }
            for (port_idx, ring) in midi_in_rings.iter_mut().enumerate() {
                while let Some(ev) = ring.pop() {
                    if has_note_ports {
                        self.push_midi_as_clap_events(
                            &mut event_buf,
                            ev.data,
                            port_idx as u16,
                            ev.sample_offset,
                        );
                    } else {
                        event_buf.push_midi(ev.data, port_idx as u16, ev.sample_offset);
                    }
                }
            }

            let in_events = event_buf.as_input_events();

            let mut event_capture = EventCapture::new();
            let out_events = event_capture.as_output_events();

            if PARAMS_FLUSH_REQUESTED.swap(false, Ordering::Acquire)
                && let Some(params_ptr) = params_ext
            {
                unsafe {
                    let flush = (*params_ptr).flush;
                    if let Some(f) = flush {
                        let empty_in = crate::clap::empty_input_events();
                        let mut flush_capture = EventCapture::new();
                        let flush_out = flush_capture.as_output_events();
                        f(plugin.plugin_ptr(), &empty_in, &flush_out);

                        for bytes in flush_capture.drain() {
                            if bytes.len() >= std::mem::size_of::<ClapEventHeader>() {
                                let h = &*(bytes.as_ptr() as *const ClapEventHeader);
                                self.echo_event_to_daw(h, &bytes, &echo_ring);
                            }
                        }
                    }
                }
            }

            let transport =
                unsafe { transport_ref(ptr) as *const TransportState as *const std::ffi::c_void };

            if !started_processing {
                set_thread_type(ThreadType::AudioThread);
                if let Err(_e) = plugin.start_processing() {
                    break;
                }
                started_processing = true;
            }

            set_thread_type(ThreadType::AudioThread);

            let process_result = if let Some(ref mut pb) = port_buffers {
                let process = ClapProcess {
                    steady_time,
                    frames_count: block_size as u32,
                    transport,
                    audio_inputs: pb.inputs.as_ptr(),
                    audio_outputs: pb.outputs.as_mut_ptr(),
                    audio_inputs_count: pb.inputs.len() as u32,
                    audio_outputs_count: pb.outputs.len() as u32,
                    in_events: &in_events,
                    out_events: &out_events,
                };
                plugin.process(&process)
            } else {
                let mut fallback_in_ptrs: Vec<*mut f32> = Vec::new();
                let mut fallback_out_ptrs: Vec<*mut f32> = Vec::new();
                fallback_in_ptrs.resize(num_in.min(MAX_CHANNELS), ptr::null_mut());
                fallback_out_ptrs.resize(num_out.min(MAX_CHANNELS), ptr::null_mut());
                for (ch, in_ptr) in fallback_in_ptrs.iter_mut().enumerate() {
                    *in_ptr = unsafe { audio_channel_ptr(ptr, ch, 0) };
                }
                for (ch, out_ptr) in fallback_out_ptrs.iter_mut().enumerate() {
                    *out_ptr = unsafe { audio_channel_ptr(ptr, ch, 1) };
                }
                let fallback_audio_in = ClapAudioBuffer {
                    data32: fallback_in_ptrs.as_mut_ptr(),
                    data64: ptr::null_mut(),
                    channel_count: num_in as u32,
                    latency: 0,
                    constant_mask: 0,
                };
                let mut fallback_audio_out = ClapAudioBuffer {
                    data32: fallback_out_ptrs.as_mut_ptr(),
                    data64: ptr::null_mut(),
                    channel_count: num_out as u32,
                    latency: 0,
                    constant_mask: 0,
                };
                let process = ClapProcess {
                    steady_time,
                    frames_count: block_size as u32,
                    transport,
                    audio_inputs: &fallback_audio_in,
                    audio_outputs: &mut fallback_audio_out,
                    audio_inputs_count: 1,
                    audio_outputs_count: 1,
                    in_events: &in_events,
                    out_events: &out_events,
                };
                plugin.process(&process)
            };

            set_thread_type(ThreadType::MainThread);

            if let Err(_e) = process_result {
                break;
            }

            unsafe {
                latency_samples_atomic(ptr).store(plugin.latency_samples(), Ordering::Release);
            }

            steady_time += block_size as i64;

            for bytes in event_capture.drain() {
                if bytes.len() >= std::mem::size_of::<ClapEventHeader>() {
                    let h = unsafe { &*(bytes.as_ptr() as *const ClapEventHeader) };
                    self.echo_event_to_daw(h, &bytes, &echo_ring);
                    self.write_midi_output_event_to_rings(h, &bytes, &midi_out_rings);
                }
            }

            if let Err(_e) = self.events.signal_daw() {
                break;
            }
        }

        if started_processing {
            set_thread_type(ThreadType::AudioThread);
            plugin.stop_processing();
            set_thread_type(ThreadType::MainThread);
        }
        plugin.deactivate();
    }

    pub fn shutdown(self) {}

    fn push_midi_as_clap_events(
        &self,
        event_buf: &mut EventBuffer,
        data: [u8; 3],
        port_index: u16,
        sample_offset: u32,
    ) {
        let status = data[0] & 0xF0;
        let channel = (data[0] & 0x0F) as i16;
        let note_id = -1i32;
        match status {
            0x90 => {
                let velocity = data[2] as f64 / 127.0;
                if velocity > 0.0 {
                    event_buf.push_note_on(
                        note_id,
                        port_index as i16,
                        channel,
                        data[1] as i16,
                        velocity,
                        sample_offset,
                    );
                } else {
                    event_buf.push_note_off(
                        note_id,
                        port_index as i16,
                        channel,
                        data[1] as i16,
                        0.0,
                        sample_offset,
                    );
                }
            }
            0x80 => {
                let velocity = data[2] as f64 / 127.0;
                event_buf.push_note_off(
                    note_id,
                    port_index as i16,
                    channel,
                    data[1] as i16,
                    velocity,
                    sample_offset,
                );
            }
            _ => {}
        }

        event_buf.push_midi(data, port_index, sample_offset);
    }

    fn echo_event_to_daw(
        &self,
        header: &ClapEventHeader,
        bytes: &[u8],
        echo_ring: &RingBuffer<ParameterEvent>,
    ) {
        match header.type_ {
            crate::clap::CLAP_EVENT_PARAM_VALUE
                if bytes.len() >= std::mem::size_of::<ClapEventParamValue>() =>
            {
                let ev = unsafe { &*(bytes.as_ptr() as *const ClapEventParamValue) };
                let echo = ParameterEvent {
                    param_index: ev.param_id,
                    value: ev.value as f32,
                    sample_offset: ev.header.time,
                    event_kind: PARAM_EVENT_VALUE,
                };
                if !echo_ring.push(echo) {}
            }
            crate::clap::CLAP_EVENT_PARAM_MOD
                if bytes.len() >= std::mem::size_of::<ClapEventParamMod>() =>
            {
                let ev = unsafe { &*(bytes.as_ptr() as *const ClapEventParamMod) };
                let echo = ParameterEvent {
                    param_index: ev.param_id,
                    value: ev.amount as f32,
                    sample_offset: ev.header.time,
                    event_kind: PARAM_EVENT_MOD,
                };
                if !echo_ring.push(echo) {}
            }
            crate::clap::CLAP_EVENT_PARAM_GESTURE_BEGIN
                if bytes.len() >= std::mem::size_of::<ClapEventParamGesture>() =>
            {
                let ev = unsafe { &*(bytes.as_ptr() as *const ClapEventParamGesture) };
                let echo = ParameterEvent {
                    param_index: ev.param_id,
                    value: 0.0,
                    sample_offset: ev.header.time,
                    event_kind: PARAM_EVENT_GESTURE_BEGIN,
                };
                if !echo_ring.push(echo) {}
            }
            crate::clap::CLAP_EVENT_PARAM_GESTURE_END
                if bytes.len() >= std::mem::size_of::<ClapEventParamGesture>() =>
            {
                let ev = unsafe { &*(bytes.as_ptr() as *const ClapEventParamGesture) };
                let echo = ParameterEvent {
                    param_index: ev.param_id,
                    value: 0.0,
                    sample_offset: ev.header.time,
                    event_kind: PARAM_EVENT_GESTURE_END,
                };
                if !echo_ring.push(echo) {}
            }
            _ => {}
        }
    }

    fn write_midi_output_event_to_rings(
        &self,
        header: &ClapEventHeader,
        bytes: &[u8],
        rings: &[RingBuffer<maolan_plugin_protocol::protocol::MidiEvent>],
    ) {
        match header.type_ {
            CLAP_EVENT_NOTE_ON | CLAP_EVENT_NOTE_OFF
                if bytes.len() >= std::mem::size_of::<ClapEventNote>() =>
            {
                let ev = unsafe { &*(bytes.as_ptr() as *const ClapEventNote) };
                let port = ev.port_index.max(0) as usize;
                if let Some(ring) = rings.get(port) {
                    let status = if header.type_ == CLAP_EVENT_NOTE_ON {
                        0x90
                    } else {
                        0x80
                    };
                    let midi_ev = maolan_plugin_protocol::protocol::MidiEvent {
                        sample_offset: header.time,
                        data: [
                            status | (ev.channel.max(0) as u8 & 0x0F),
                            ev.key.max(0) as u8,
                            (ev.velocity * 127.0).clamp(0.0, 127.0) as u8,
                        ],
                        channel: ev.channel.max(0) as u8 & 0x0F,
                        flags: 0,
                        _pad: 0,
                    };
                    let _ = ring.push(midi_ev);
                }
            }
            CLAP_EVENT_MIDI if bytes.len() >= std::mem::size_of::<ClapEventMidi>() => {
                let ev = unsafe { &*(bytes.as_ptr() as *const ClapEventMidi) };
                let port = ev.port_index as usize;
                if let Some(ring) = rings.get(port) {
                    let midi_ev = maolan_plugin_protocol::protocol::MidiEvent {
                        sample_offset: header.time,
                        data: ev.data,
                        channel: ev.data.first().copied().unwrap_or(0) & 0x0F,
                        flags: 0,
                        _pad: 0,
                    };
                    let _ = ring.push(midi_ev);
                }
            }
            _ => {}
        }
    }

    #[cfg(unix)]
    fn poll_daw_and_fds(&self, daw_fd: i32, timeout: Duration) -> (bool, Vec<(i32, u32)>) {
        let fds = host_fds_snapshot();
        if fds.is_empty() {
            return (self.events.wait_daw(timeout).is_ok(), Vec::new());
        }
        let mut poll_fds: Vec<libc::pollfd> = Vec::with_capacity(fds.len() + 1);
        poll_fds.push(libc::pollfd {
            fd: daw_fd,
            events: libc::POLLIN,
            revents: 0,
        });
        for f in fds.iter() {
            let mut events = 0;
            if f.flags & 1 != 0 {
                events |= libc::POLLIN;
            }
            if f.flags & 2 != 0 {
                events |= libc::POLLOUT;
            }
            if f.flags & 4 != 0 {
                events |= libc::POLLERR;
            }
            poll_fds.push(libc::pollfd {
                fd: f.fd,
                events,
                revents: 0,
            });
        }
        let ms = timeout.as_millis().clamp(0, i32::MAX as u128) as i32;
        let rc = unsafe { libc::poll(poll_fds.as_mut_ptr(), poll_fds.len() as libc::nfds_t, ms) };
        if rc < 0 {
            return (false, Vec::new());
        }
        let mut ready_fds = Vec::new();
        for (i, f) in fds.iter().enumerate() {
            let pfd = &poll_fds[i + 1];
            if pfd.revents != 0 {
                let mut flags = 0;
                if pfd.revents & libc::POLLIN != 0 {
                    flags |= 1;
                }
                if pfd.revents & libc::POLLOUT != 0 {
                    flags |= 2;
                }
                if pfd.revents & libc::POLLERR != 0 {
                    flags |= 4;
                }
                ready_fds.push((f.fd, flags));
            }
        }
        (poll_fds[0].revents & libc::POLLIN != 0, ready_fds)
    }

    fn next_timer_ms(&self) -> u64 {
        let timers = host_timers_snapshot();
        let now = Instant::now();
        timers
            .iter()
            .map(|t| {
                if t.deadline <= now {
                    0
                } else {
                    (t.deadline - now).as_millis() as u64
                }
            })
            .min()
            .unwrap_or(100)
    }

    fn handle_idle_work(
        &self,
        plugin: &PluginInstance,
        _params_ext: Option<*const ClapPluginParams>,
        timer_ext: Option<*const crate::clap::ClapPluginTimerSupport>,
    ) {
        let now = Instant::now();
        let mut fired_timers = Vec::new();
        {
            let mut timers = host_timers_snapshot().as_ref().clone();
            let mut changed = false;
            for t in timers.iter_mut() {
                if t.deadline <= now {
                    fired_timers.push(t.id);
                    t.deadline = now + Duration::from_millis(t.period_ms as u64);
                    changed = true;
                }
            }
            if changed {
                crate::clap::replace_host_timers(timers);
            }
        }
        if let Some(ext) = timer_ext {
            for id in fired_timers {
                unsafe {
                    if let Some(f) = (*ext).on_timer {
                        f(plugin.plugin_ptr(), id);
                    }
                }
            }
        }
    }
}
