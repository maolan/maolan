//! AUv3 `AuUnit` implementation over `AUAudioUnit` (Objective-C API),
//! hand-rolled FFI including the Apple Block ABI for completion handlers,
//! parameter observers and the pull-input render block (see `objc.rs`).
//!
//! Unlike v2, AUv3 has no MIDI-injection or transport API: MIDI input and
//! `AuTransport` are accepted but ignored here. Component selection between
//! `V2Unit` and `V3Unit` happens in `au::run_au_loop` from the
//! `kAudioComponentFlag_IsV3AudioUnit` registry flag.

use super::objc;
use super::v2::ffi;
use super::{AuComponentDesc, AuInitConfig, AuParamInfo, AuTransport, AuUnit};
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::Mutex;

use ffi::{
    AudioBuffer, AudioComponentDescription, AudioTimeStamp, CFDataGetBytePtr, CFDataGetLength,
    CFPropertyListCreateWithData, CFPropertyListCreateXMLData, CFRelease, NO_ERR,
    PLIST_MUTABLE_CONTAINERS_AND_LEAVES,
};

type Id = objc::Id;

const INSTANTIATE_LOAD_OUT_OF_PROCESS: usize = 1;
const AV_AUDIO_COMMON_FORMAT_PCM_FLOAT32: isize = 1;
const MAX_AU_RENDER_CHANNELS: usize = 32;

fn ns_error_to_string(err: Id) -> String {
    if err.is_null() {
        return String::new();
    }
    objc::ns_string_to_string(unsafe { objc::send0(err, objc::sel("localizedDescription")) })
}

fn status_error(context: &str, status: i32) -> String {
    format!("{context} failed with status {status}")
}

/// One-shot completion context for `instantiateWithComponentDescription:`.
struct InstantiateResult {
    tx: Option<std::sync::mpsc::Sender<(Id, Id)>>,
}

extern "C" fn instantiate_completion_invoke(block: *mut c_void, unit: Id, error: Id) {
    // SAFETY: block was created by block_copy_with_context with a
    // Box<InstantiateResult> context; ownership of the box is consumed here.
    // The unit/error arguments are only valid for the duration of the
    // callback (the caller releases them when it returns), so the unit is
    // retained here for V3Unit's lifetime.
    unsafe {
        if !unit.is_null() {
            objc::send0_unit(unit, objc::sel("retain"));
        }
        let ctx = objc::block_context::<InstantiateResult>(block);
        let result = &mut *ctx;
        if let Some(tx) = result.tx.take() {
            let _ = tx.send((unit, error));
        }
        drop(Box::from_raw(ctx));
    }
}

extern "C" fn view_controller_completion_invoke(block: *mut c_void, vc: Id) {
    // SAFETY: same one-shot contract as instantiate_completion_invoke,
    // including retaining the view controller beyond the callback.
    unsafe {
        if !vc.is_null() {
            objc::send0_unit(vc, objc::sel("retain"));
        }
        let ctx = objc::block_context::<std::sync::mpsc::Sender<Id>>(block);
        let result = &mut *ctx;
        let _ = result.send(vc);
        drop(Box::from_raw(ctx));
    }
}

/// Plugin-side parameter change queue fed by the parameter observer block.
struct ParamObserver {
    queue: Mutex<Vec<(u64, f32)>>,
}

extern "C" fn parameter_observer_invoke(block: *mut c_void, address: u64, value: f32) {
    // SAFETY: the context is a Box<ParamObserver> owned by V3Unit for the
    // unit's lifetime; the observer is removed before the box is dropped.
    unsafe {
        let ctx = objc::block_context::<ParamObserver>(block);
        if let Ok(mut q) = (*ctx).queue.lock() {
            q.push((address, value));
        }
    }
}

/// Render-pull-input context: one stable pointer per input channel plane.
struct PullInput {
    ptrs: Vec<*mut f32>,
}

extern "C" fn pull_input_invoke(
    block: *mut c_void,
    _flags: *mut u32,
    _timestamp: *const AudioTimeStamp,
    frames: u32,
    _bus: isize,
    input_data: *mut ffi::AudioBufferList,
) -> i32 {
    // SAFETY: same contract as v2's input_render_callback; the PullInput box
    // is owned by V3Unit and outlives the render resources. The AU allocates
    // `n` contiguous AudioBuffer descriptors after the count.
    unsafe {
        if block.is_null() || input_data.is_null() {
            return -1;
        }
        let pull = &*objc::block_context::<PullInput>(block);
        let list = &mut *input_data;
        let n = (list.number_buffers as usize).min(pull.ptrs.len());
        let buffers = list.buffers.as_mut_ptr();
        for (i, ptr) in pull.ptrs.iter().enumerate().take(n) {
            let buf = &mut *buffers.add(i);
            buf.data = ptr.cast();
            buf.number_channels = 1;
            buf.data_byte_size = frames * 4;
        }
        NO_ERR
    }
}

/// Ask an `AUAudioUnit` for its view controller; null when the unit has no
/// custom UI. Safe to call from any thread; blocks until the completion
/// handler fires. Requires a running NSApplication: the ViewBridge raises an
/// ObjC exception (uncatchable from Rust) when the app is not running.
/// # Safety
/// `unit` must be a valid `AUAudioUnit *`.
pub unsafe fn request_view_controller(unit: Id) -> Result<Id, String> {
    if unit.is_null() {
        return Err("null AUAudioUnit".to_string());
    }
    let app = unsafe { objc::send0(objc::class("NSApplication"), objc::sel("sharedApplication")) };
    if app.is_null() || !unsafe { objc::send0_running(app) } {
        return Err(
            "NSApplication is not running; cannot request an AUv3 view controller".to_string(),
        );
    }
    let (tx, rx) = std::sync::mpsc::channel::<Id>();
    let block = unsafe {
        objc::block_copy_with_context(
            view_controller_completion_invoke as *mut c_void,
            Box::into_raw(Box::new(tx)) as usize,
        )
    };
    unsafe {
        objc::send1_unit(
            unit,
            objc::sel("requestViewControllerWithCompletionHandler:"),
            block as usize,
        );
        objc::block_release(block);
    }
    rx.recv()
        .map_err(|e| format!("view controller request lost: {e}"))
}

/// Locate the component and report whether the registry flags mark it as an
/// AUv3 (`kAudioComponentFlag_IsV3AudioUnit`).
pub fn component_is_v3(desc: &AuComponentDesc) -> bool {
    let cd = AudioComponentDescription {
        component_type: desc.comp_type,
        component_subtype: desc.subtype,
        component_manufacturer: desc.manufacturer,
        component_flags: 0,
        component_flags_mask: 0,
    };
    let component = unsafe { ffi::AudioComponentFindNext(std::ptr::null_mut(), &cd) };
    if component.is_null() {
        return false;
    }
    let mut full = AudioComponentDescription {
        component_type: 0,
        component_subtype: 0,
        component_manufacturer: 0,
        component_flags: 0,
        component_flags_mask: 0,
    };
    let status = unsafe { ffi::AudioComponentGetDescription(component, &mut full) };
    status == NO_ERR && (full.component_flags & ffi::COMPONENT_FLAG_IS_V3) != 0
}

pub struct V3Unit {
    unit: Id,
    desc: AuComponentDesc,
    name: String,
    params: Vec<AuParamInfo>,
    /// Dense index -> AUParameterAddress.
    addresses: Vec<u64>,
    /// Dense index -> retained AUParameter *.
    param_objs: Vec<Id>,
    address_map: HashMap<u64, u32>,
    input_planes: Vec<Vec<f32>>,
    output_planes: Vec<Vec<f32>>,
    max_block_size: usize,
    sample_rate: f64,
    sample_time: f64,
    /// Retained `AURenderBlock` object returned by allocateRenderBlockAndReturnError:.
    render_block: Id,
    /// Heap-copied pull-input block passed to the render block.
    pull_block: *mut c_void,
    observer: Option<Box<ParamObserver>>,
    observer_token: Id,
    /// Set once the unit rejects the pull-input block (v2-wrapped units).
    pull_rejected: bool,
    pull_input: Option<Box<PullInput>>,
    ui_shown: bool,
}

// SAFETY: the AUAudioUnit handle and all block contexts are only touched from
// the thread owning the V3Unit (the host block loop); GUI work is dispatched
// to the GUI thread, which only receives the unit pointer while it is alive.
unsafe impl Send for V3Unit {}

impl V3Unit {
    fn instantiate(desc: &AuComponentDesc) -> Result<Id, String> {
        let au_audio_unit = objc::class("AUAudioUnit");
        if au_audio_unit.is_null() {
            return Err("AUAudioUnit class not found".to_string());
        }
        let cd = AudioComponentDescription {
            component_type: desc.comp_type,
            component_subtype: desc.subtype,
            component_manufacturer: desc.manufacturer,
            component_flags: 0,
            component_flags_mask: 0,
        };
        let (tx, rx) = std::sync::mpsc::channel::<(Id, Id)>();
        let block = unsafe {
            objc::block_copy_with_context(
                instantiate_completion_invoke as *mut c_void,
                Box::into_raw(Box::new(InstantiateResult { tx: Some(tx) })) as usize,
            )
        };
        unsafe {
            let f: unsafe extern "C" fn(Id, objc::Sel, AudioComponentDescription, usize, usize) =
                std::mem::transmute(objc::msg_send_addr());
            f(
                au_audio_unit,
                objc::sel("instantiateWithComponentDescription:options:completionHandler:"),
                cd,
                INSTANTIATE_LOAD_OUT_OF_PROCESS,
                block as usize,
            );
            objc::block_release(block);
        }
        let (unit, error) = rx
            .recv_timeout(std::time::Duration::from_secs(60))
            .map_err(|e| format!("AUAudioUnit instantiation timed out: {e}"))?;
        if unit.is_null() {
            let detail = ns_error_to_string(error);
            return Err(if detail.is_empty() {
                "AUAudioUnit instantiation returned nil".to_string()
            } else {
                format!("AUAudioUnit instantiation failed: {detail}")
            });
        }
        Ok(unit)
    }

    fn make_format(&self, channels: usize) -> Result<Id, String> {
        unsafe {
            let alloc = objc::send0(objc::class("AVAudioFormat"), objc::sel("alloc"));
            let f: unsafe extern "C" fn(Id, objc::Sel, isize, f64, usize, u8) -> Id =
                std::mem::transmute(objc::msg_send_addr());
            let format = f(
                alloc,
                objc::sel("initWithCommonFormat:sampleRate:channels:interleaved:"),
                AV_AUDIO_COMMON_FORMAT_PCM_FLOAT32,
                self.sample_rate,
                channels,
                0,
            );
            if format.is_null() {
                return Err("AVAudioFormat creation failed".to_string());
            }
            Ok(format)
        }
    }

    fn configure_buses(&self, selector: &str, count: usize) -> Result<(), String> {
        let buses = unsafe { objc::send0(self.unit, objc::sel(selector)) };
        if buses.is_null() {
            return Ok(());
        }
        let available = unsafe { objc::send0_isize(buses, objc::sel("count")) };
        if available <= 0 || count == 0 {
            return Ok(());
        }
        let format = self.make_format(count)?;
        let result = (|| -> Result<(), String> {
            let bus = unsafe { objc::send1(buses, objc::sel("objectAtIndexedSubscript:"), 0usize) };
            if bus.is_null() {
                return Err(format!("{selector}: bus 0 is nil"));
            }
            let mut err: Id = std::ptr::null_mut();
            let ok = unsafe {
                objc::send2_bool_ret(
                    bus,
                    objc::sel("setFormat:error:"),
                    format as usize,
                    &raw mut err as usize,
                ) != 0
            };
            if !ok {
                let detail = ns_error_to_string(err);
                return Err(format!("{selector}: setFormat failed: {detail}"));
            }
            Ok(())
        })();
        unsafe { objc::send0_unit(format, objc::sel("release")) };
        result
    }

    fn enumerate_parameters(&mut self) -> Result<(), String> {
        let tree = unsafe { objc::send0(self.unit, objc::sel("parameterTree")) };
        if tree.is_null() {
            return Ok(());
        }
        let all = unsafe { objc::send0(tree, objc::sel("allParameters")) };
        if all.is_null() {
            return Ok(());
        }
        let count = unsafe { objc::send0_isize(all, objc::sel("count")) };
        for i in 0..count.max(0) as usize {
            let node = unsafe { objc::send1(all, objc::sel("objectAtIndex:"), i) };
            if node.is_null() {
                continue;
            }
            let address = unsafe { objc::send0_usize(node, objc::sel("address")) } as u64;
            let min = unsafe { objc::send0_f32(node, objc::sel("minValue")) };
            let max = unsafe { objc::send0_f32(node, objc::sel("maxValue")) };
            let value = unsafe { objc::send0_f32(node, objc::sel("value")) };
            let display_name = unsafe { objc::send0(node, objc::sel("displayName")) };
            let name = objc::ns_string_to_string(display_name);
            let dense = self.params.len() as u32;
            self.addresses.push(address);
            unsafe { objc::send0_unit(node, objc::sel("retain")) };
            self.param_objs.push(node);
            self.address_map.insert(address, dense);
            self.params.push(AuParamInfo {
                index: dense,
                scope: 0,
                element: (address >> 32) as u32,
                param_id: address as u32,
                name,
                min: f64::from(min),
                max: f64::from(max),
                default: f64::from(value),
                flags: 0,
            });
        }
        if !self.params.is_empty() {
            let observer = Box::new(ParamObserver {
                queue: Mutex::new(Vec::new()),
            });
            let block = unsafe {
                objc::block_copy_with_context(
                    parameter_observer_invoke as *mut c_void,
                    (&*observer as *const ParamObserver) as usize,
                )
            };
            let token = unsafe {
                objc::send1(
                    tree,
                    objc::sel("tokenByAddingParameterObserver:"),
                    block as usize,
                )
            };
            unsafe { objc::block_release(block) };
            self.observer = Some(observer);
            self.observer_token = token;
        }
        Ok(())
    }
}

impl AuUnit for V3Unit {
    fn open(desc: &AuComponentDesc) -> Result<Self, String> {
        // Reuse the v2 registry lookup for the display name.
        let cd = AudioComponentDescription {
            component_type: desc.comp_type,
            component_subtype: desc.subtype,
            component_manufacturer: desc.manufacturer,
            component_flags: 0,
            component_flags_mask: 0,
        };
        let component = unsafe { ffi::AudioComponentFindNext(std::ptr::null_mut(), &cd) };
        let name = unsafe {
            let mut name_ref: ffi::CFStringRef = std::ptr::null_mut();
            if !component.is_null()
                && ffi::AudioComponentCopyName(component, &mut name_ref) == NO_ERR
                && !name_ref.is_null()
            {
                let len = ffi::CFStringGetLength(name_ref);
                let cap =
                    ffi::CFStringGetMaximumSizeForEncoding(len, ffi::K_CF_STRING_ENCODING_UTF8) + 1;
                let mut buf = vec![0u8; cap.max(1) as usize];
                let s = if ffi::CFStringGetCString(
                    name_ref,
                    buf.as_mut_ptr().cast(),
                    buf.len() as isize,
                    ffi::K_CF_STRING_ENCODING_UTF8,
                ) {
                    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
                    buf.truncate(end);
                    String::from_utf8_lossy(&buf).into_owned()
                } else {
                    String::new()
                };
                ffi::CFRelease(name_ref.cast());
                s
            } else {
                super::au_spec_string(desc)
            }
        };
        let unit = Self::instantiate(desc)?;
        Ok(V3Unit {
            unit,
            desc: *desc,
            name,
            params: Vec::new(),
            addresses: Vec::new(),
            param_objs: Vec::new(),
            address_map: HashMap::new(),
            input_planes: Vec::new(),
            output_planes: Vec::new(),
            max_block_size: 0,
            sample_rate: 0.0,
            sample_time: 0.0,
            render_block: std::ptr::null_mut(),
            pull_block: std::ptr::null_mut(),
            observer: None,
            observer_token: std::ptr::null_mut(),
            pull_rejected: false,
            pull_input: None,
            ui_shown: false,
        })
    }

    fn initialize(&mut self, config: &AuInitConfig) -> Result<(), String> {
        self.max_block_size = config.max_block_size;
        self.sample_rate = config.sample_rate;
        self.input_planes = vec![vec![0.0f32; config.max_block_size]; config.num_inputs];
        self.output_planes = vec![vec![0.0f32; config.max_block_size]; config.num_outputs];

        self.configure_buses("inputBusses", config.num_inputs)?;
        self.configure_buses("outputBusses", config.num_outputs)?;

        unsafe {
            objc::send1_usize(
                self.unit,
                objc::sel("setMaximumFramesToRender:"),
                config.max_block_size,
            );
        }

        let mut err: Id = std::ptr::null_mut();
        let ok = unsafe {
            objc::send1_bool(
                self.unit,
                objc::sel("allocateRenderResourcesAndReturnError:"),
                &raw mut err as usize,
            )
        };
        if !ok {
            let detail = ns_error_to_string(err);
            return Err(format!("allocateRenderResources failed: {detail}"));
        }

        let render_block = unsafe { objc::send0(self.unit, objc::sel("renderBlock")) };
        if render_block.is_null() {
            return Err("AUAudioUnit renderBlock is nil".to_string());
        }
        // Property getters return +0; retain for the unit's lifetime.
        unsafe { objc::send0_unit(render_block, objc::sel("retain")) };
        self.render_block = render_block;

        if !self.input_planes.is_empty() {
            let pull = Box::new(PullInput {
                ptrs: self
                    .input_planes
                    .iter_mut()
                    .map(|p| p.as_mut_ptr())
                    .collect(),
            });
            self.pull_block = unsafe {
                objc::block_copy_with_context(
                    pull_input_invoke as *mut c_void,
                    (&*pull as *const PullInput) as usize,
                )
            };
            self.pull_input = Some(pull);
        }

        self.enumerate_parameters()?;
        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn is_music_device(&self) -> bool {
        self.desc.comp_type == ffi::COMP_TYPE_MUSIC_DEVICE
            || self.desc.comp_type == ffi::COMP_TYPE_MUSIC_EFFECT
    }

    fn input_buffer_mut(&mut self, channel: usize) -> Option<&mut [f32]> {
        self.input_planes.get_mut(channel).map(Vec::as_mut_slice)
    }

    fn output_buffer(&self, channel: usize) -> Option<&[f32]> {
        self.output_planes.get(channel).map(Vec::as_slice)
    }

    fn render(
        &mut self,
        frames: usize,
        _midi_in: &[crate::util::MidiEvent],
        _transport: &AuTransport,
    ) -> Result<(), String> {
        if self.render_block.is_null() || self.output_planes.is_empty() {
            return Ok(());
        }
        // Fixed-size mirror of AudioBufferList, same layout as v2: the buffer
        // array starts after the count plus its 4 bytes of alignment padding.
        #[repr(C)]
        struct BufferList {
            number_buffers: u32,
            _pad: u32,
            buffers: [AudioBuffer; MAX_AU_RENDER_CHANNELS],
        }
        let num_out = self.output_planes.len().min(MAX_AU_RENDER_CHANNELS);
        let mut list = BufferList {
            number_buffers: num_out as u32,
            _pad: 0,
            buffers: [AudioBuffer {
                number_channels: 1,
                data_byte_size: 0,
                data: std::ptr::null_mut(),
            }; MAX_AU_RENDER_CHANNELS],
        };
        for (i, plane) in self.output_planes.iter_mut().enumerate().take(num_out) {
            let buf = &mut list.buffers[i];
            buf.data = plane.as_mut_ptr().cast();
            buf.number_channels = 1;
            buf.data_byte_size = (frames * 4) as u32;
        }
        unsafe {
            let invoke = *(self.render_block as *mut *mut c_void).add(2);
            let f: extern "C" fn(
                *mut c_void,
                *mut u32,
                *const AudioTimeStamp,
                u32,
                isize,
                *mut ffi::AudioBufferList,
                *mut c_void,
            ) -> i32 = std::mem::transmute(invoke);
            let mut flags = 0u32;
            let timestamp = AudioTimeStamp {
                sample_time: self.sample_time,
                host_time: 0,
                rate_scalar: 0.0,
                word_clock_time: 0,
                smpte_time: [0; 32],
                flags: 1, // kAudioTimeStampSampleTimeValid
                reserved: 0,
            };
            let mut call = |pull: *mut c_void| {
                f(
                    self.render_block,
                    &raw mut flags,
                    &timestamp,
                    frames as u32,
                    0,
                    (&raw mut list).cast(),
                    pull,
                )
            };
            // AUv2 components wrapped by the v3 API (AUAudioUnit_RemoteV2)
            // reject a non-nil pull block; retry once with nil and remember
            // the outcome. Input then renders as silence for such units.
            let pull = if self.pull_rejected || self.pull_block.is_null() {
                std::ptr::null_mut()
            } else {
                self.pull_block
            };
            let mut status = call(pull);
            if status != NO_ERR && !pull.is_null() {
                status = call(std::ptr::null_mut());
                if status == NO_ERR {
                    self.pull_rejected = true;
                }
            }
            self.sample_time += frames as f64;
            if status != NO_ERR {
                return Err(status_error("AUAudioUnit render block", status));
            }
        }
        Ok(())
    }

    fn parameters(&self) -> &[AuParamInfo] {
        &self.params
    }

    fn set_parameter(&mut self, index: u32, value: f64) -> Result<(), String> {
        let Some(&param) = self.param_objs.get(index as usize) else {
            return Err(format!("unknown AU parameter index {index}"));
        };
        // Passing our observer token as originator suppresses the redundant
        // echo notification for host-originated changes.
        unsafe {
            objc::send2_f32_unit(
                param,
                objc::sel("setValue:originator:"),
                value as f32,
                self.observer_token as usize,
            );
        }
        Ok(())
    }

    fn get_parameter(&self, index: u32) -> Option<f64> {
        let &param = self.param_objs.get(index as usize)?;
        Some(f64::from(unsafe {
            objc::send0_f32(param, objc::sel("value"))
        }))
    }

    fn take_param_updates(&mut self) -> Vec<(u32, f64)> {
        let Some(observer) = self.observer.as_ref() else {
            return Vec::new();
        };
        let updates = {
            let Ok(mut q) = observer.queue.lock() else {
                return Vec::new();
            };
            std::mem::take(&mut *q)
        };
        updates
            .into_iter()
            .filter_map(|(address, value)| {
                self.address_map
                    .get(&address)
                    .map(|&dense| (dense, f64::from(value)))
            })
            .collect()
    }

    fn save_state(&mut self) -> Result<Vec<u8>, String> {
        unsafe {
            let state = objc::send0(self.unit, objc::sel("fullStateForDocument"));
            let state = if state.is_null() {
                objc::send0(self.unit, objc::sel("fullState"))
            } else {
                state
            };
            if state.is_null() {
                return Err("AUAudioUnit exposes no fullState".to_string());
            }
            // +0 getter result; no release (see renderBlock above).
            let data = CFPropertyListCreateXMLData(std::ptr::null_mut(), state);
            if data.is_null() {
                return Err("CFPropertyListCreateXMLData returned null".to_string());
            }
            let len = CFDataGetLength(data);
            let bytes = std::slice::from_raw_parts(CFDataGetBytePtr(data), len as usize);
            let out = bytes.to_vec();
            CFRelease(data.cast());
            Ok(out)
        }
    }

    fn restore_state(&mut self, data: &[u8]) -> Result<(), String> {
        if data.is_empty() {
            return Err("empty AU state blob".to_string());
        }
        unsafe {
            let cf_data =
                ffi::CFDataCreate(std::ptr::null_mut(), data.as_ptr(), data.len() as isize);
            if cf_data.is_null() {
                return Err("CFDataCreate failed".to_string());
            }
            let plist = CFPropertyListCreateWithData(
                std::ptr::null_mut(),
                cf_data,
                PLIST_MUTABLE_CONTAINERS_AND_LEAVES,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            CFRelease(cf_data.cast());
            if plist.is_null() {
                return Err("state blob is not a valid property list".to_string());
            }
            objc::send1_unit(
                self.unit,
                objc::sel("setFullStateForDocument:"),
                plist as usize,
            );
            CFRelease(plist);
            Ok(())
        }
    }

    fn latency_samples(&self) -> u32 {
        let seconds = unsafe { objc::send0_f64(self.unit, objc::sel("latency")) };
        (seconds * self.sample_rate).round().max(0.0) as u32
    }

    fn cocoa_ui(&self) -> Option<(&str, &str)> {
        None
    }

    fn raw_unit(&self) -> *mut c_void {
        self.unit.cast()
    }

    fn show_ui(&mut self, _parent: *mut c_void) -> Result<(), String> {
        // Window and view-controller management happen on the GUI thread in
        // the run loop (`au::run_au_loop`); the unit only tracks visibility.
        self.ui_shown = true;
        Ok(())
    }

    fn hide_ui(&mut self) -> Result<(), String> {
        self.ui_shown = false;
        Ok(())
    }

    fn close_ui(&mut self) {
        self.ui_shown = false;
    }
}

impl Drop for V3Unit {
    fn drop(&mut self) {
        unsafe {
            if !self.unit.is_null() {
                if !self.observer_token.is_null() {
                    let tree = objc::send0(self.unit, objc::sel("parameterTree"));
                    if !tree.is_null() {
                        objc::send1_unit(
                            tree,
                            objc::sel("removeParameterObserver:"),
                            self.observer_token as usize,
                        );
                    }
                    self.observer_token = std::ptr::null_mut();
                }
                self.observer = None;
                objc::send0_unit(self.unit, objc::sel("deallocateRenderResources"));
                if !self.render_block.is_null() {
                    objc::send0_unit(self.render_block, objc::sel("release"));
                    self.render_block = std::ptr::null_mut();
                }
                if !self.pull_block.is_null() {
                    objc::block_release(self.pull_block);
                    self.pull_block = std::ptr::null_mut();
                }
                // Dropping pull_input after deallocateRenderResources ensures
                // the AU can never call the pull block into freed planes.
                self.pull_input = None;
                for &param in &self.param_objs {
                    objc::send0_unit(param, objc::sel("release"));
                }
                self.param_objs.clear();
                objc::send0_unit(self.unit, objc::sel("release"));
                self.unit = std::ptr::null_mut();
            }
        }
    }
}
