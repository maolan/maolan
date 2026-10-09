//! AudioUnit (AUv2) plugin hosting. The `AuUnit` trait is the seam where a
//! future AUv3 backend (`au/v3.rs` over `AUAudioUnit`) will plug in; the SHM
//! run loop below is written against the trait only.

pub mod gui_cocoa;
pub mod v2;

use maolan_plugin_protocol::events::EventPair;
use maolan_plugin_protocol::protocol::*;
use maolan_plugin_protocol::ringbuf::RingBuffer;
use maolan_plugin_protocol::shm::ShmMapping;
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::Ordering;
use std::time::Duration;

/// AU component described by its three registry fourccs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuComponentDesc {
    pub comp_type: u32,
    pub subtype: u32,
    pub manufacturer: u32,
}

/// Initialization-time audio configuration.
#[derive(Clone, Copy, Debug)]
pub struct AuInitConfig {
    pub sample_rate: f64,
    pub max_block_size: usize,
    pub num_inputs: usize,
    pub num_outputs: usize,
}

/// One enumerable AudioUnit parameter. `index` is a dense table index built
/// at enumeration time; the protocol's u32 `ParameterEvent::param_index`
/// carries exactly this index. AU parameter addresses are (scope, element,
/// paramID) tuples that do not fit the u32, so the unit maps them through the
/// dense table instead.
#[derive(Clone, Debug, PartialEq)]
pub struct AuParamInfo {
    pub index: u32,
    pub scope: u32,
    pub element: u32,
    pub param_id: u32,
    pub name: String,
    pub min: f64,
    pub max: f64,
    pub default: f64,
    pub flags: u64,
}

/// Transport snapshot handed to `render`. AUv2 has no standard transport
/// property; `V2Unit` ignores it, AUv3 will map it onto `AUHostMusicalContext`.
#[derive(Clone, Copy, Debug, Default)]
pub struct AuTransport {
    pub playing: bool,
    pub playhead_sample: i64,
    pub tempo: f64,
    pub numerator: u32,
    pub denominator: u32,
}

/// Format-agnostic AudioUnit instance. All pointer-returning methods only
/// borrow; buffers stay owned by the unit.
pub trait AuUnit: Send {
    /// Locate the component in the registry and instantiate it (not yet
    /// initialized).
    fn open(desc: &AuComponentDesc) -> Result<Self, String>
    where
        Self: Sized;
    /// Set stream formats, render callbacks and initialize the instance.
    fn initialize(&mut self, config: &AuInitConfig) -> Result<(), String>;
    fn name(&self) -> &str;
    /// True for `aumu`/`aumf` components that accept MIDI via MusicDeviceMIDIEvent.
    fn is_music_device(&self) -> bool;
    fn input_buffer_mut(&mut self, channel: usize) -> Option<&mut [f32]>;
    fn output_buffer(&self, channel: usize) -> Option<&[f32]>;
    fn render(
        &mut self,
        frames: usize,
        midi_in: &[crate::util::MidiEvent],
        transport: &AuTransport,
    ) -> Result<(), String>;
    fn parameters(&self) -> &[AuParamInfo];
    /// `index` is the dense `AuParamInfo::index`.
    fn set_parameter(&mut self, index: u32, value: f64) -> Result<(), String>;
    fn get_parameter(&self, index: u32) -> Option<f64>;
    /// Parameter changes initiated from the plugin side (GUI), as
    /// (dense index, value), drained by the host loop for the echo ring.
    fn take_param_updates(&mut self) -> Vec<(u32, f64)>;
    fn save_state(&mut self) -> Result<Vec<u8>, String>;
    fn restore_state(&mut self, data: &[u8]) -> Result<(), String>;
    fn latency_samples(&self) -> u32;
    /// `(bundle_path, view_class_name)` when the component publishes a Cocoa UI.
    fn cocoa_ui(&self) -> Option<(&str, &str)>;
    /// Raw `AudioUnit`/`AUAudioUnit` pointer, used by the view factory.
    fn raw_unit(&self) -> *mut c_void;
    /// `parent` is an `NSView *` for embedded mode, null for floating.
    fn show_ui(&mut self, parent: *mut c_void) -> Result<(), String>;
    fn hide_ui(&mut self) -> Result<(), String>;
    fn close_ui(&mut self);
}

const SHM_LATENCY_SAMPLES_OFFSET: usize = 84;

unsafe fn latency_samples_atomic(ptr: *mut u8) -> &'static std::sync::atomic::AtomicU32 {
    unsafe { &*(ptr.add(SHM_LATENCY_SAMPLES_OFFSET) as *const std::sync::atomic::AtomicU32) }
}

fn fourcc(s: &str) -> Option<u32> {
    let b = s.as_bytes();
    if b.len() != 4 {
        return None;
    }
    Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

/// Plugin spec: `au:<type>:<subtype>:<manufacturer>` with 4-char fourccs,
/// e.g. `au:aufx:pass:appl`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuSpec {
    pub desc: AuComponentDesc,
}

pub fn parse_au_spec(spec: &str) -> Option<AuSpec> {
    let rest = spec.strip_prefix("au:")?;
    let mut parts = rest.split(':');
    let comp_type = fourcc(parts.next()?)?;
    let subtype = fourcc(parts.next()?)?;
    let manufacturer = fourcc(parts.next()?)?;
    if parts.next().is_some() {
        return None;
    }
    Some(AuSpec {
        desc: AuComponentDesc {
            comp_type,
            subtype,
            manufacturer,
        },
    })
}

pub fn au_spec_string(desc: &AuComponentDesc) -> String {
    fn c(f: u32) -> String {
        String::from_utf8_lossy(&f.to_be_bytes()).into_owned()
    }
    format!(
        "au:{}:{}:{}",
        c(desc.comp_type),
        c(desc.subtype),
        c(desc.manufacturer)
    )
}

/// Magic + record layout for the REQUEST_AU_PARAMETERS scratch payload.
pub const AU_PARAMS_MAGIC: u32 = 0x4155_5052; // "AUPR"
const AU_PARAMS_OFFSET: usize = 3072;
const AU_PARAMS_MAX_SIZE: usize = SCRATCH_SIZE - AU_PARAMS_OFFSET;
/// Fixed part of one serialized param record: index, scope, element, paramID,
/// min, max, default, flags (8 x u32) plus name_len (u32); name bytes follow.
const AU_PARAM_RECORD_FIXED: usize = 36;

/// Serialize the parameter table into scratch at offset 3072.
///
/// # Safety
/// `ptr` must point to a valid SHM allocation.
pub unsafe fn write_au_params_to_scratch(
    ptr: *mut u8,
    params: &[AuParamInfo],
) -> Result<(), String> {
    unsafe {
        let mut dest = scratch_ptr(ptr).add(AU_PARAMS_OFFSET);
        let mut remaining = AU_PARAMS_MAX_SIZE;
        if remaining < 8 {
            return Err("scratch too small for AU parameters".to_string());
        }
        std::ptr::write_unaligned(dest as *mut u32, AU_PARAMS_MAGIC);
        dest = dest.add(4);
        remaining -= 4;
        std::ptr::write_unaligned(dest as *mut u32, params.len() as u32);
        dest = dest.add(4);
        remaining -= 4;
        for p in params {
            if remaining < AU_PARAM_RECORD_FIXED {
                return Err("scratch overflow writing AU parameters".to_string());
            }
            std::ptr::write_unaligned(dest as *mut u32, p.index);
            std::ptr::write_unaligned(dest.add(4) as *mut u32, p.scope);
            std::ptr::write_unaligned(dest.add(8) as *mut u32, p.element);
            std::ptr::write_unaligned(dest.add(12) as *mut u32, p.param_id);
            std::ptr::write_unaligned(dest.add(16) as *mut u32, (p.min as f32).to_bits());
            std::ptr::write_unaligned(dest.add(20) as *mut u32, (p.max as f32).to_bits());
            std::ptr::write_unaligned(dest.add(24) as *mut u32, (p.default as f32).to_bits());
            std::ptr::write_unaligned(dest.add(28) as *mut u32, p.flags as u32);
            dest = dest.add(32);
            remaining -= 32;
            let bytes = p.name.as_bytes();
            let len = bytes.len().min(remaining.saturating_sub(4));
            if len < bytes.len() {
                return Err("scratch overflow writing AU parameters".to_string());
            }
            std::ptr::write_unaligned(dest as *mut u32, len as u32);
            dest = dest.add(4);
            remaining -= 4;
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), dest, len);
            dest = dest.add(len);
            remaining -= len;
        }
        Ok(())
    }
}

/// Read the parameter table written by `write_au_params_to_scratch`.
///
/// # Safety
/// `ptr` must point to a valid SHM allocation.
pub unsafe fn read_au_params_from_scratch(ptr: *mut u8) -> Option<Vec<AuParamInfo>> {
    unsafe {
        let mut src = scratch_ptr(ptr).add(AU_PARAMS_OFFSET);
        let mut remaining = AU_PARAMS_MAX_SIZE;
        if remaining < 8 {
            return None;
        }
        if std::ptr::read_unaligned(src as *mut u32) != AU_PARAMS_MAGIC {
            return None;
        }
        src = src.add(4);
        remaining -= 4;
        let count = std::ptr::read_unaligned(src as *mut u32) as usize;
        src = src.add(4);
        remaining -= 4;
        let mut params = Vec::with_capacity(count);
        for _ in 0..count {
            if remaining < AU_PARAM_RECORD_FIXED {
                return None;
            }
            let index = std::ptr::read_unaligned(src as *mut u32);
            let scope = std::ptr::read_unaligned(src.add(4) as *mut u32);
            let element = std::ptr::read_unaligned(src.add(8) as *const u32);
            let param_id = std::ptr::read_unaligned(src.add(12) as *const u32);
            let min = f32::from_bits(std::ptr::read_unaligned(src.add(16) as *const u32)) as f64;
            let max = f32::from_bits(std::ptr::read_unaligned(src.add(20) as *const u32)) as f64;
            let default =
                f32::from_bits(std::ptr::read_unaligned(src.add(24) as *const u32)) as f64;
            let flags = u64::from(std::ptr::read_unaligned(src.add(28) as *const u32));
            src = src.add(32);
            remaining -= 32;
            if remaining < 4 {
                return None;
            }
            let name_len = std::ptr::read_unaligned(src as *mut u32) as usize;
            src = src.add(4);
            remaining -= 4;
            if name_len > remaining {
                return None;
            }
            let bytes = std::slice::from_raw_parts(src, name_len);
            let name = String::from_utf8(bytes.to_vec()).ok()?;
            src = src.add(name_len);
            remaining -= name_len;
            params.push(AuParamInfo {
                index,
                scope,
                element,
                param_id,
                name,
                min,
                max,
                default,
                flags,
            });
        }
        Some(params)
    }
}

fn apply_au_param_ring(unit: &mut dyn AuUnit, ptr: *mut u8) {
    let ring = unsafe {
        let buf = param_ring_ptr(ptr);
        let (w, r) = param_indices(ptr);
        RingBuffer::new(buf, w, r, RING_CAPACITY)
    };
    while let Some(ev) = ring.pop() {
        if let Err(_e) = unit.set_parameter(ev.param_index, f64::from(ev.value)) {}
    }
}

fn drain_midi_ring(ptr: *mut u8, port_idx: usize) -> Vec<crate::util::MidiEvent> {
    let ring = unsafe {
        let buf = midi_in_ring_ptr(ptr, port_idx);
        let (w, r) = midi_in_indices(ptr, port_idx);
        RingBuffer::new(buf, w, r, RING_CAPACITY)
    };
    let mut events = Vec::new();
    while let Some(ev) = ring.pop() {
        events.push(crate::util::MidiEvent {
            frame: ev.sample_offset,
            data: ev.data.to_vec(),
        });
    }
    events
}

fn read_au_transport(ptr: *mut u8) -> AuTransport {
    let t = unsafe { transport_ref(ptr) };
    AuTransport {
        playing: t.flags & 0x1 != 0,
        playhead_sample: t.playhead_sample as i64,
        tempo: t.tempo,
        numerator: t.numerator,
        denominator: t.denominator,
    }
}

fn write_au_echo_ring(unit: &mut dyn AuUnit, ptr: *mut u8, cache: &mut HashMap<u32, f32>) {
    let ring = unsafe {
        let buf = echo_ring_ptr(ptr);
        let (w, r) = echo_indices(ptr);
        RingBuffer::new(buf, w, r, RING_CAPACITY)
    };
    for (index, value) in unit.take_param_updates() {
        let ev = ParameterEvent {
            param_index: index,
            value: value as f32,
            sample_offset: 0,
            event_kind: PARAM_EVENT_VALUE,
        };
        if !ring.push(ev) {
            break;
        }
        cache.insert(index, value as f32);
    }
    for p in unit.parameters() {
        let current = match unit.get_parameter(p.index) {
            Some(v) => v as f32,
            None => continue,
        };
        if cache.get(&p.index) != Some(&current) {
            let ev = ParameterEvent {
                param_index: p.index,
                value: current,
                sample_offset: 0,
                event_kind: PARAM_EVENT_VALUE,
            };
            if !ring.push(ev) {
                break;
            }
            cache.insert(p.index, current);
        }
    }
}

/// Serialize a state blob as u32 length + bytes into scratch start.
fn serialize_au_state(scratch: *mut u8, state: &[u8]) -> Result<usize, String> {
    if state.len() + 4 > SCRATCH_SIZE {
        return Err("AU state too large for scratch".to_string());
    }
    unsafe {
        std::ptr::write_unaligned(scratch as *mut u32, state.len() as u32);
        std::ptr::copy_nonoverlapping(state.as_ptr(), scratch.add(4), state.len());
    }
    Ok(state.len() + 4)
}

fn deserialize_au_state(scratch: *const u8, size: usize) -> Result<Vec<u8>, String> {
    if size < 4 {
        return Err("scratch too small for AU state".to_string());
    }
    let len = unsafe { std::ptr::read_unaligned(scratch as *const u32) } as usize;
    if len + 4 > size {
        return Err("scratch underflow reading AU state".to_string());
    }
    let mut data = vec![0u8; len];
    unsafe {
        std::ptr::copy_nonoverlapping(scratch.add(4), data.as_mut_ptr(), len);
    }
    Ok(data)
}

struct AuGui {
    thread: gui_cocoa::GuiThread,
    window: Option<gui_cocoa::CocoaWindow>,
}

pub struct AuRunArgs {
    pub spec: String,
    pub mapping: ShmMapping,
    pub events: EventPair,
    pub instance_id: String,
    pub sample_rate: f64,
    pub buffer_size: usize,
    pub num_inputs: usize,
    pub num_outputs: usize,
}

pub fn run_au(args: AuRunArgs) {
    // Handle the synthetic test plugins inline (matching run_vst3) so they
    // never touch the GUI startup path.
    match args.spec.as_str() {
        "__test__" => {
            let mapping_ptr = args.mapping.as_ptr();
            let header = unsafe { header_ref(mapping_ptr) };
            let scratch = unsafe { scratch_ptr(mapping_ptr) };
            unsafe {
                std::ptr::write_unaligned(scratch as *mut u32, 0xDEADBEEF);
            }
            header.ready.store(1, Ordering::Release);
            return;
        }
        "__crash__" => {
            let header = unsafe { header_ref(args.mapping.as_ptr()) };
            header.ready.store(1, Ordering::Release);
            std::process::exit(1);
        }
        "__hang__" => {
            let header = unsafe { header_ref(args.mapping.as_ptr()) };
            header.ready.store(1, Ordering::Release);
            loop {
                std::thread::sleep(Duration::from_secs(60));
            }
        }
        _ => {}
    }

    // AU Cocoa views dispatch to the process main thread during view
    // creation, so the NSApplication run loop must run on the main thread;
    // the SHM block loop moves to a spawned thread. The block loop ends the
    // process on shutdown since the main thread never leaves -[NSApp run].
    std::thread::Builder::new()
        .name("maolan-au-loop".to_string())
        .spawn(move || run_au_loop(args))
        .expect("spawn AU loop thread");
    gui_cocoa::run_gui_main();
}

fn run_au_loop(args: AuRunArgs) {
    let AuRunArgs {
        spec,
        mapping,
        events,
        instance_id: _,
        sample_rate,
        buffer_size,
        num_inputs,
        num_outputs,
    } = args;

    let header = unsafe { header_ref(mapping.as_ptr()) };
    let ptr = mapping.as_ptr();

    let parsed = match parse_au_spec(&spec) {
        Some(p) => p,
        None => {
            tracing::error!(%spec, "AU host: invalid plugin spec");
            return;
        }
    };

    let mut unit = match v2::V2Unit::open(&parsed.desc) {
        Ok(u) => u,
        Err(e) => {
            tracing::error!(%e, "AU host: failed to open component");
            return;
        }
    };

    if let Err(e) = unit.initialize(&AuInitConfig {
        sample_rate,
        max_block_size: buffer_size.max(1),
        num_inputs,
        num_outputs,
    }) {
        tracing::error!(%e, "AU host: failed to initialize");
        return;
    }

    unsafe {
        maolan_plugin_protocol::protocol::write_plugin_name_to_scratch(
            mapping.as_ptr(),
            unit.name(),
        );
    }
    header.midi_in_port_count.store(
        if unit.is_music_device() { 1 } else { 0 },
        Ordering::Release,
    );
    header.midi_out_port_count.store(0, Ordering::Release);
    unsafe {
        latency_samples_atomic(ptr).store(unit.latency_samples(), Ordering::Release);
    }
    header.ready.store(1, Ordering::Release);

    let mut au_param_cache = HashMap::new();
    for p in unit.parameters() {
        if let Some(v) = unit.get_parameter(p.index) {
            au_param_cache.insert(p.index, v as f32);
        }
    }

    let mut gui: Option<AuGui> = None;

    loop {
        if header.shutdown_request.load(Ordering::Acquire) != 0 {
            break;
        }

        let req = header.request_type.load(Ordering::Acquire);
        if req != 0 {
            let scratch = unsafe { scratch_ptr(ptr) };
            let result = match req {
                1 => match unit.save_state() {
                    Ok(state) => match serialize_au_state(scratch, &state) {
                        Ok(size) => {
                            header.scratch_size.store(size as u32, Ordering::Release);
                            Ok(())
                        }
                        Err(e) => Err(e),
                    },
                    Err(e) => Err(e),
                },
                2 => {
                    let size = header.scratch_size.load(Ordering::Acquire) as usize;
                    match deserialize_au_state(scratch, size) {
                        Ok(state) => {
                            let result = unit.restore_state(&state);
                            if result.is_ok() {
                                au_param_cache.clear();
                            }
                            result
                        }
                        Err(e) => Err(e),
                    }
                }
                3 => (|| -> Result<(), String> {
                    if gui.is_none() {
                        gui = Some(AuGui {
                            thread: gui_cocoa::GuiThread::start()?,
                            window: None,
                        });
                    }
                    let g = gui.as_mut().expect("gui thread exists");
                    if g.window.is_none() {
                        let parent = {
                            let h = unsafe { header_ref(ptr) };
                            if h.gui_mode() == GuiMode::Embedded
                                && h.gui_parent_api() == GuiParentApi::Cocoa
                            {
                                h.parent_window_usize() as *mut c_void
                            } else {
                                std::ptr::null_mut()
                            }
                        };
                        g.window = Some(gui_cocoa::CocoaWindow::create(
                            &g.thread,
                            unit.name(),
                            &unit as &dyn AuUnit,
                            parent,
                        )?);
                    }
                    if let Some(w) = g.window.as_ref() {
                        w.show();
                    }
                    unit.show_ui(std::ptr::null_mut())
                })(),
                4 => {
                    if let Some(g) = gui.as_ref()
                        && let Some(w) = g.window.as_ref()
                    {
                        w.hide();
                    }
                    unit.hide_ui()
                }
                5 => Ok(()),
                maolan_plugin_protocol::protocol::REQUEST_AU_PARAMETERS => {
                    match unsafe { write_au_params_to_scratch(ptr, unit.parameters()) } {
                        Ok(()) => {
                            let size = 8 + unit
                                .parameters()
                                .iter()
                                .map(|p| AU_PARAM_RECORD_FIXED + p.name.len())
                                .sum::<usize>();
                            header
                                .scratch_size
                                .store(size.min(SCRATCH_SIZE) as u32, Ordering::Release);
                            Ok(())
                        }
                        Err(e) => Err(e),
                    }
                }
                _ => Err(format!("Unknown request type: {req}")),
            };
            if let Err(e) = &result {
                tracing::error!(request = req, error = %e, "AU host: request failed");
            }
            header
                .request_status
                .store(if result.is_ok() { 1 } else { 2 }, Ordering::Release);

            if matches!(
                req,
                1 | 2 | 3 | maolan_plugin_protocol::protocol::REQUEST_AU_PARAMETERS
            ) {
                let _ = events.signal_daw();
            }
            header.request_type.store(0, Ordering::Release);
            continue;
        }

        let wait_result = events.wait_daw(Duration::from_millis(100));
        match wait_result {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
            Err(_e) => break,
        }

        let block_size = header.block_size.load(Ordering::Acquire) as usize;
        let num_in = header.num_input_channels.load(Ordering::Acquire) as usize;
        let num_out = header.num_output_channels.load(Ordering::Acquire) as usize;

        if block_size == 0 || block_size > MAX_BLOCK_SIZE {
            header.mark_block_response();
            if !header.block_response_eventless() {
                let _ = events.signal_daw();
            }
            continue;
        }

        apply_au_param_ring(&mut unit, ptr);

        let transport = read_au_transport(ptr);

        for ch in 0..num_in {
            let src = unsafe { audio_channel_ptr(ptr, ch, 0) };
            if let Some(dst) = unit.input_buffer_mut(ch) {
                let len = block_size.min(dst.len());
                unsafe {
                    std::ptr::copy_nonoverlapping(src, dst.as_mut_ptr(), len);
                }
            }
        }

        let midi_input = if unit.is_music_device() {
            let mut evts = drain_midi_ring(ptr, 0);
            evts.sort_by_key(|ev| ev.frame);
            evts
        } else {
            Vec::new()
        };

        if let Err(_e) = unit.render(block_size, &midi_input, &transport) {
            header.mark_block_response();
            if !header.block_response_eventless() {
                let _ = events.signal_daw();
            }
            continue;
        }
        unsafe {
            latency_samples_atomic(ptr).store(unit.latency_samples(), Ordering::Release);
        }

        write_au_echo_ring(&mut unit, ptr, &mut au_param_cache);

        for ch in 0..num_out {
            let dst = unsafe { audio_channel_ptr(ptr, ch, 1) };
            if let Some(src) = unit.output_buffer(ch) {
                let len = block_size.min(src.len());
                unsafe {
                    std::ptr::copy_nonoverlapping(src.as_ptr(), dst, len);
                }
            }
        }

        header.mark_block_response();
        if !header.block_response_eventless() && events.signal_daw().is_err() {
            break;
        }
    }

    if let Some(g) = gui.as_ref()
        && let Some(w) = g.window.as_ref()
    {
        w.close();
    }
    unit.close_ui();
    // The main thread is inside -[NSApplication run] and never returns, so
    // the process must exit from here once the block loop is done.
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_params() -> Vec<AuParamInfo> {
        vec![
            AuParamInfo {
                index: 0,
                scope: 0,
                element: 0,
                param_id: 1,
                name: "Gain".to_string(),
                min: 0.0,
                max: 1.0,
                default: 0.5,
                flags: 0,
            },
            AuParamInfo {
                index: 1,
                scope: 0,
                element: 0,
                param_id: 0x1234_5678,
                name: "päram \"2\"".to_string(),
                min: -60.0,
                max: 12.0,
                default: 0.0,
                flags: 3,
            },
        ]
    }

    #[test]
    fn parse_au_spec_roundtrip() {
        let spec = au_spec_string(&AuComponentDesc {
            comp_type: u32::from_be_bytes(*b"aufx"),
            subtype: u32::from_be_bytes(*b"pass"),
            manufacturer: u32::from_be_bytes(*b"appl"),
        });
        assert_eq!(spec, "au:aufx:pass:appl");
        let parsed = parse_au_spec(&spec).expect("spec parses");
        assert_eq!(parsed.desc.comp_type, u32::from_be_bytes(*b"aufx"));
        assert_eq!(parsed.desc.subtype, u32::from_be_bytes(*b"pass"));
        assert_eq!(parsed.desc.manufacturer, u32::from_be_bytes(*b"appl"));
    }

    #[test]
    fn parse_au_spec_rejects_malformed() {
        assert!(parse_au_spec("aufx:pass:appl").is_none());
        assert!(parse_au_spec("au:aufx:pass").is_none());
        assert!(parse_au_spec("au:aufx:pass:appl:extra").is_none());
        assert!(parse_au_spec("au:aufx:p:appl").is_none());
        assert!(parse_au_spec("au:aufx:pass:appl ").is_none());
    }

    #[test]
    fn au_params_scratch_roundtrip() {
        let params = sample_params();
        let mut shm = vec![0u8; LAYOUT_SIZE];
        let ptr = shm.as_mut_ptr();
        unsafe {
            init_shm_layout(ptr, LAYOUT_SIZE);
            write_au_params_to_scratch(ptr, &params).expect("write");
            let read = read_au_params_from_scratch(ptr).expect("read");
            assert_eq!(read, params);
        }
    }

    #[test]
    fn au_state_roundtrip() {
        let state = vec![0xAA; 1000];
        let mut shm = vec![0u8; LAYOUT_SIZE];
        let ptr = shm.as_mut_ptr();
        unsafe {
            init_shm_layout(ptr, LAYOUT_SIZE);
            let scratch = scratch_ptr(ptr);
            let size = serialize_au_state(scratch, &state).expect("serialize");
            let read = deserialize_au_state(scratch, size).expect("deserialize");
            assert_eq!(read, state);
        }
    }
}
