use crate::clap::{
    CLAP_EXT_AUDIO_PORTS, CLAP_EXT_PARAMS, CLAP_EXT_POSIX_FD_SUPPORT, CLAP_EXT_TIMER_SUPPORT,
    ClapAudioBuffer, ClapEventHeader, ClapEventParamGesture, ClapEventParamMod,
    ClapEventParamValue, ClapPluginParams, ClapProcess, EventBuffer, EventCapture, PluginInstance,
    ThreadType, host_fds, host_timers, set_thread_type,
};
use crate::events::EventPair;
use crate::protocol::*;
use crate::ringbuf::RingBuffer;
use crate::shm::ShmMapping;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Flush flag set by `host_params_request_flush`.
static PARAMS_FLUSH_REQUESTED: AtomicBool = AtomicBool::new(false);

pub fn request_params_flush() {
    PARAMS_FLUSH_REQUESTED.store(true, Ordering::Release);
}

/// Rescan flag set by `host_audio_ports_rescan`.
static AUDIO_PORTS_RESCAN_REQUESTED: AtomicBool = AtomicBool::new(false);

pub fn request_audio_ports_rescan() {
    AUDIO_PORTS_RESCAN_REQUESTED.store(true, Ordering::Release);
}

/// Audio port buffer configuration built from `clap.audio-ports`.
struct PortBuffers {
    inputs: Vec<ClapAudioBuffer>,
    outputs: Vec<ClapAudioBuffer>,
    _input_ptrs: Vec<Vec<*mut f32>>,
    _output_ptrs: Vec<Vec<*mut f32>>,
}

impl PortBuffers {
    /// Query the plugin's `clap.audio-ports` extension and build per-port buffers
    /// that point into the SHM audio planes. Returns `None` if the extension is missing.
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

/// Runtime state for the plugin-host process.
pub struct HostRuntime {
    pub mapping: ShmMapping,
    pub events: EventPair,
    pub format: String,
    pub plugin_path: String,
    pub instance_id: String,
}

impl HostRuntime {
    /// Attach to an existing shared-memory segment and event pipes.
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

    /// Extract the plugin ID for CLAP factories.
    /// `plugin_path` may be encoded as "path::plugin_id" or "path#plugin_id" by the caller.
    fn plugin_id(&self) -> &str {
        if let Some(pos) = self.plugin_path.rfind("::") {
            &self.plugin_path[pos + 2..]
        } else if let Some(pos) = self.plugin_path.rfind('#') {
            &self.plugin_path[pos + 1..]
        } else {
            ""
        }
    }

    /// Extract the real file path (without the optional #plugin_id suffix).
    fn real_plugin_path(&self) -> &str {
        if let Some(pos) = self.plugin_path.rfind("::") {
            &self.plugin_path[..pos]
        } else if let Some(pos) = self.plugin_path.rfind('#') {
            &self.plugin_path[..pos]
        } else {
            &self.plugin_path
        }
    }

    /// Signal readiness to the DAW.
    pub fn signal_ready(&self) {
        let header = unsafe { header_mut(self.mapping.as_ptr()) };
        header.ready.store(1, Ordering::Release);
        tracing::info!(instance_id = %self.instance_id, "Plugin host ready");
    }

    /// Write a test magic number into the scratch area.
    pub fn write_test_magic(&self) {
        let scratch = unsafe { scratch_ptr(self.mapping.as_ptr()) };
        let magic: u32 = 0xDEADBEEF;
        unsafe {
            std::ptr::write_unaligned(scratch as *mut u32, magic);
        }
    }

    /// Blocking wait until the DAW requests shutdown or a fatal signal arrives.
    /// Uses the event pipe to sleep instead of burning CPU.
    pub fn run_until_shutdown(&self) {
        let header = unsafe { header_ref(self.mapping.as_ptr()) };
        let start = Instant::now();
        loop {
            if header.shutdown_request.load(Ordering::Acquire) != 0 {
                tracing::info!(instance_id = %self.instance_id, "Shutdown requested");
                break;
            }
            if start.elapsed() >= Duration::from_millis(100) {
                header.heartbeat.fetch_add(1, Ordering::Relaxed);
            }
            match self.events.wait_daw(Duration::from_millis(100)) {
                Ok(()) => continue,
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
                Err(e) => {
                    tracing::error!("Event pipe error: {e}");
                    break;
                }
            }
        }
    }

    /// Run the dummy null plugin: copy input audio channels to output channels.
    /// Blocks on the event pipe for each block, then signals completion.
    pub fn run_null_plugin(&self) {
        let header = unsafe { header_ref(self.mapping.as_ptr()) };
        let ptr = self.mapping.as_ptr();
        tracing::info!(instance_id = %self.instance_id, "Null plugin running");

        loop {
            if header.shutdown_request.load(Ordering::Acquire) != 0 {
                tracing::info!(instance_id = %self.instance_id, "Null plugin shutdown requested");
                break;
            }

            // Wait for DAW to wake us with a new block.
            match self.events.wait_daw(Duration::from_millis(100)) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
                Err(e) => {
                    tracing::error!("Null plugin event error: {e}");
                    break;
                }
            }

            let block_size = header.block_size.load(Ordering::Acquire) as usize;
            let num_in = header.num_input_channels.load(Ordering::Acquire) as usize;
            let num_out = header.num_output_channels.load(Ordering::Acquire) as usize;

            if block_size == 0 || block_size > MAX_BLOCK_SIZE {
                tracing::warn!("Invalid block size {block_size}, skipping");
                let _ = self.events.signal_daw();
                continue;
            }

            // Copy each input channel to the corresponding output channel.
            // Input uses bus 0 (main), output uses bus 1 (sidechain) so we don't overwrite input.
            let max_ch = num_in.min(num_out).min(MAX_CHANNELS);
            for ch in 0..max_ch {
                let in_ptr = unsafe { audio_channel_ptr(ptr, ch, 0) };
                let out_ptr = unsafe { audio_channel_ptr(ptr, ch, 1) };
                unsafe {
                    std::ptr::copy_nonoverlapping(in_ptr, out_ptr, block_size);
                }
            }

            // Signal completion.
            if let Err(e) = self.events.signal_daw() {
                tracing::error!("Failed to signal DAW: {e}");
                break;
            }
        }
    }

    /// Run a CLAP plugin inside the host process, marshalling audio via shared memory.
    pub fn run_clap_plugin(&self) {
        let mut plugin = match PluginInstance::new(self.real_plugin_path(), self.plugin_id()) {
            Ok(p) => p,
            Err(e) => {
                tracing::error!(
                    "Failed to load CLAP plugin '{}': {e}",
                    self.real_plugin_path()
                );
                return;
            }
        };

        tracing::info!(
            instance_id = %self.instance_id,
            name = %plugin.name(),
            "CLAP plugin loaded"
        );

        let ptr = self.mapping.as_ptr();
        let header = unsafe { header_ref(self.mapping.as_ptr()) };

        unsafe {
            maolan_plugin_protocol::protocol::write_plugin_name_to_scratch(ptr, &plugin.name());
        }

        // Read sample rate from transport, fallback to 48 kHz for backward compat.
        let sample_rate = unsafe {
            let ts = transport_ref(ptr);
            if ts.sample_rate_hz > 0.0 {
                ts.sample_rate_hz
            } else {
                48000.0
            }
        };

        if let Err(e) = plugin.activate(sample_rate, 1, MAX_BLOCK_SIZE as u32) {
            tracing::error!("Failed to activate plugin: {e}");
            return;
        }

        // Build per-port audio buffers from the plugin's audio-ports extension.
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

        // Set up ring buffers.
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
        let midi_ring = unsafe {
            let buf = midi_ring_ptr(ptr);
            let (w, r) = midi_indices(ptr);
            RingBuffer::new(buf, w, r, RING_CAPACITY)
        };

        // Cache plugin extension pointers for idle callbacks.
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
        let fd_ext = unsafe {
            (*plugin.plugin_ptr())
                .get_extension
                .map(|f| f(plugin.plugin_ptr(), CLAP_EXT_POSIX_FD_SUPPORT.as_ptr()))
                .filter(|p| !p.is_null())
                .map(|p| p as *const crate::clap::ClapPluginPosixFdSupport)
        };

        let mut steady_time: i64 = 0;
        let daw_read_fd = self.events.host_read_fd();
        let mut started_processing = false;

        loop {
            if header.shutdown_request.load(Ordering::Acquire) != 0 {
                tracing::info!(instance_id = %self.instance_id, "CLAP plugin shutdown requested");
                break;
            }

            // Handle non-audio requests (GUI, state).
            let req = header.request_type.load(Ordering::Acquire);
            if req != 0 {
                let result = match req {
                    3 => {
                        tracing::info!(instance_id = %self.instance_id, "GUI show requested");
                        if !plugin.gui_is_supported() {
                            Err("Plugin does not support GUI".to_string())
                        } else {
                            let window_id = header.parent_window_usize() as u64;
                            let is_floating = window_id == 0;
                            // Always recreate GUI if already created, to handle parent/floating changes.
                            if plugin.gui_created() {
                                plugin.gui_destroy();
                            }
                            let create_result = plugin.gui_create("x11", is_floating);
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
                    4 => {
                        tracing::info!(instance_id = %self.instance_id, "GUI hide requested");
                        plugin.gui_hide()
                    }
                    _ => Err(format!("Unknown request type: {req}")),
                };
                header
                    .request_status
                    .store(if result.is_ok() { 1 } else { 2 }, Ordering::Release);
                if req == 1 || req == 2 {
                    let _ = self.events.signal_daw();
                }
                header.request_type.store(0, Ordering::Release);
                continue;
            }

            // Idle work: timers, FDs, parameter flush, on_main_thread callback.
            set_thread_type(ThreadType::MainThread);
            self.handle_idle_work(&plugin, params_ext, timer_ext);

            // Wait for DAW signal or timeout (max 100 ms to service timers/FDs).
            let timeout_ms = self.next_timer_ms().min(100);
            let (daw_ready, ready_fds) = match timeout_ms {
                0 => (true, Vec::new()), // timers expired immediately
                ms => self.poll_daw_and_fds(daw_read_fd, Duration::from_millis(ms as u64)),
            };

            // Fire FD callbacks only for FDs that actually signaled readiness.
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
                // Timeout only — loop around to handle timers/FDs.
                continue;
            }

            let block_size = header.block_size.load(Ordering::Acquire) as usize;
            let num_in = header.num_input_channels.load(Ordering::Acquire) as usize;
            let num_out = header.num_output_channels.load(Ordering::Acquire) as usize;

            if block_size == 0 || block_size > MAX_BLOCK_SIZE {
                tracing::warn!("Invalid block size {block_size}, skipping");
                let _ = self.events.signal_daw();
                continue;
            }

            // Rebuild port buffers if the plugin requested an audio-ports rescan.
            if AUDIO_PORTS_RESCAN_REQUESTED.swap(false, Ordering::Acquire) {
                tracing::info!(instance_id = %self.instance_id, "Rebuilding audio port buffers after rescan");
                port_buffers = PortBuffers::from_plugin(plugin.plugin_ptr(), ptr, num_in, num_out);
            }

            // Update SHM pointers each block in case the DAW remapped channels.
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
                // Fallback: single bus with all channels (old behavior).
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

            // Build input event list from parameter and MIDI ring buffers.
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
            while let Some(ev) = midi_ring.pop() {
                if has_note_ports {
                    self.push_midi_as_clap_events(
                        &mut event_buf,
                        ev.data,
                        ev.channel as u16,
                        ev.sample_offset,
                    );
                } else {
                    event_buf.push_midi(ev.data, ev.channel as u16, ev.sample_offset);
                }
            }
            let in_events = event_buf.as_input_events();

            // Capture events the plugin emits back to the DAW.
            let mut event_capture = EventCapture::new();
            let out_events = event_capture.as_output_events();

            // Flush parameters on main thread if requested.
            if PARAMS_FLUSH_REQUESTED.swap(false, Ordering::Acquire) {
                if let Some(params_ptr) = params_ext {
                    unsafe {
                        let flush = (*params_ptr).flush;
                        if let Some(f) = flush {
                            let empty_in = crate::clap::empty_input_events();
                            let mut flush_capture = EventCapture::new();
                            let flush_out = flush_capture.as_output_events();
                            f(plugin.plugin_ptr(), &empty_in, &flush_out);
                            // Echo flushed events immediately.
                            for bytes in flush_capture.drain() {
                                if bytes.len() >= std::mem::size_of::<ClapEventHeader>() {
                                    let h = &*(bytes.as_ptr() as *const ClapEventHeader);
                                    self.echo_event_to_daw(h, &bytes, &echo_ring);
                                }
                            }
                        }
                    }
                }
            }

            let transport =
                unsafe { transport_ref(ptr) as *const TransportState as *const std::ffi::c_void };

            tracing::debug!(
                instance_id = %self.instance_id,
                block_size,
                num_in,
                num_out,
                events_in = event_buf.len(),
                "Processing block"
            );

            if !started_processing {
                set_thread_type(ThreadType::AudioThread);
                if let Err(e) = plugin.start_processing() {
                    tracing::error!("Failed to start processing: {e}");
                    break;
                }
                started_processing = true;
            }

            set_thread_type(ThreadType::AudioThread);

            let process = if let Some(ref mut pb) = port_buffers {
                ClapProcess {
                    steady_time,
                    frames_count: block_size as u32,
                    transport,
                    audio_inputs: pb.inputs.as_ptr(),
                    audio_outputs: pb.outputs.as_mut_ptr(),
                    audio_inputs_count: pb.inputs.len() as u32,
                    audio_outputs_count: pb.outputs.len() as u32,
                    in_events: &in_events,
                    out_events: &out_events,
                }
            } else {
                // Fallback single-bus (must reconstruct temporaries here).
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
                let audio_in = ClapAudioBuffer {
                    data32: in_ptrs.as_mut_ptr(),
                    data64: ptr::null_mut(),
                    channel_count: num_in as u32,
                    latency: 0,
                    constant_mask: 0,
                };
                let mut audio_out = ClapAudioBuffer {
                    data32: out_ptrs.as_mut_ptr(),
                    data64: ptr::null_mut(),
                    channel_count: num_out as u32,
                    latency: 0,
                    constant_mask: 0,
                };
                ClapProcess {
                    steady_time,
                    frames_count: block_size as u32,
                    transport,
                    audio_inputs: &audio_in,
                    audio_outputs: &mut audio_out,
                    audio_inputs_count: 1,
                    audio_outputs_count: 1,
                    in_events: &in_events,
                    out_events: &out_events,
                }
            };

            let process_result = plugin.process(&process);

            set_thread_type(ThreadType::MainThread);

            if let Err(e) = process_result {
                tracing::error!("Plugin process error: {e}");
                break;
            }

            steady_time += block_size as i64;

            // Forward captured events to the DAW via echo ring.
            for bytes in event_capture.drain() {
                if bytes.len() >= std::mem::size_of::<ClapEventHeader>() {
                    let h = unsafe { &*(bytes.as_ptr() as *const ClapEventHeader) };
                    self.echo_event_to_daw(h, &bytes, &echo_ring);
                }
            }

            tracing::debug!(instance_id = %self.instance_id, "Block processed, signalling DAW");

            if let Err(e) = self.events.signal_daw() {
                tracing::error!("Failed to signal DAW: {e}");
                break;
            }
        }

        if started_processing {
            set_thread_type(ThreadType::AudioThread);
            plugin.stop_processing();
            set_thread_type(ThreadType::MainThread);
        }
        plugin.deactivate();
        tracing::info!(instance_id = %self.instance_id, "CLAP plugin stopped");
    }

    /// Clean up and exit.
    pub fn shutdown(self) {
        tracing::info!(instance_id = %self.instance_id, "Plugin host shutting down");
    }

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    /// Convert a raw MIDI event to CLAP note events if applicable, pushing into `event_buf`.
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
        // Always emit the raw MIDI event as well.
        event_buf.push_midi(data, port_index, sample_offset);
    }

    /// Convert a captured CLAP event into a `ParameterEvent` and push it to the echo ring.
    fn echo_event_to_daw(
        &self,
        header: &ClapEventHeader,
        bytes: &[u8],
        echo_ring: &RingBuffer<ParameterEvent>,
    ) {
        match header.type_ {
            crate::clap::CLAP_EVENT_PARAM_VALUE => {
                if bytes.len() >= std::mem::size_of::<ClapEventParamValue>() {
                    let ev = unsafe { &*(bytes.as_ptr() as *const ClapEventParamValue) };
                    let echo = ParameterEvent {
                        param_index: ev.param_id,
                        value: ev.value as f32,
                        sample_offset: ev.header.time,
                        event_kind: PARAM_EVENT_VALUE,
                    };
                    if !echo_ring.push(echo) {
                        tracing::warn!("Echo ring full, dropping parameter value event");
                    }
                }
            }
            crate::clap::CLAP_EVENT_PARAM_MOD => {
                if bytes.len() >= std::mem::size_of::<ClapEventParamMod>() {
                    let ev = unsafe { &*(bytes.as_ptr() as *const ClapEventParamMod) };
                    let echo = ParameterEvent {
                        param_index: ev.param_id,
                        value: ev.amount as f32,
                        sample_offset: ev.header.time,
                        event_kind: PARAM_EVENT_MOD,
                    };
                    if !echo_ring.push(echo) {
                        tracing::warn!("Echo ring full, dropping parameter mod event");
                    }
                }
            }
            crate::clap::CLAP_EVENT_PARAM_GESTURE_BEGIN => {
                if bytes.len() >= std::mem::size_of::<ClapEventParamGesture>() {
                    let ev = unsafe { &*(bytes.as_ptr() as *const ClapEventParamGesture) };
                    let echo = ParameterEvent {
                        param_index: ev.param_id,
                        value: 0.0,
                        sample_offset: ev.header.time,
                        event_kind: PARAM_EVENT_GESTURE_BEGIN,
                    };
                    if !echo_ring.push(echo) {
                        tracing::warn!("Echo ring full, dropping gesture begin event");
                    }
                }
            }
            crate::clap::CLAP_EVENT_PARAM_GESTURE_END => {
                if bytes.len() >= std::mem::size_of::<ClapEventParamGesture>() {
                    let ev = unsafe { &*(bytes.as_ptr() as *const ClapEventParamGesture) };
                    let echo = ParameterEvent {
                        param_index: ev.param_id,
                        value: 0.0,
                        sample_offset: ev.header.time,
                        event_kind: PARAM_EVENT_GESTURE_END,
                    };
                    if !echo_ring.push(echo) {
                        tracing::warn!("Echo ring full, dropping gesture end event");
                    }
                }
            }
            _ => {
                // Other event types are not echoed via the parameter ring.
            }
        }
    }

    /// Poll the DAW pipe and registered FDs.
    /// Returns `(daw_ready, ready_fds)` where `ready_fds` contains only FDs that signaled.
    fn poll_daw_and_fds(&self, daw_fd: i32, timeout: Duration) -> (bool, Vec<(i32, u32)>) {
        let fds = host_fds().lock().unwrap();
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
                tracing::debug!(fd = f.fd, flags, "FD event");
                ready_fds.push((f.fd, flags));
            }
        }
        (poll_fds[0].revents & libc::POLLIN != 0, ready_fds)
    }

    /// Return the number of milliseconds until the next timer expires (0 if already expired).
    fn next_timer_ms(&self) -> u64 {
        let timers = host_timers().lock().unwrap();
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

    /// Handle timers, FD callbacks, and on_main_thread.
    fn handle_idle_work(
        &self,
        plugin: &PluginInstance,
        _params_ext: Option<*const ClapPluginParams>,
        timer_ext: Option<*const crate::clap::ClapPluginTimerSupport>,
    ) {
        let now = Instant::now();
        let mut fired_timers = Vec::new();
        {
            let mut timers = host_timers().lock().unwrap();
            for t in timers.iter_mut() {
                if t.deadline <= now {
                    fired_timers.push(t.id);
                    t.deadline = now + Duration::from_millis(t.period_ms as u64);
                }
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
