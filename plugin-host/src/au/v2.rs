//! AUv2 `AuUnit` implementation over the pure C AudioUnit API, hand-rolled
//! FFI (no bindgen, no objc crates).

use super::{AuComponentDesc, AuInitConfig, AuParamInfo, AuTransport, AuUnit};
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::Mutex;

pub mod ffi {
    use super::c_void;
    use std::ffi::c_char;

    pub type OSStatus = i32;
    pub type AudioUnit = *mut c_void;
    pub type AudioComponent = *mut c_void;
    pub type CFTypeRef = *mut c_void;
    pub type CFStringRef = *mut c_void;
    pub type CFURLRef = *mut c_void;
    pub type CFDataRef = *mut c_void;

    pub const NO_ERR: OSStatus = 0;

    // Fourccs.
    pub const COMP_TYPE_EFFECT: u32 = u32::from_be_bytes(*b"aufx");
    pub const COMP_TYPE_MUSIC_EFFECT: u32 = u32::from_be_bytes(*b"aumf");
    pub const COMP_TYPE_MUSIC_DEVICE: u32 = u32::from_be_bytes(*b"aumu");
    pub const MANUFACTURER_APPLE: u32 = u32::from_be_bytes(*b"appl");

    // AudioUnit scopes.
    pub const SCOPE_GLOBAL: u32 = 0;
    pub const SCOPE_INPUT: u32 = 1;
    pub const SCOPE_OUTPUT: u32 = 2;

    // AudioUnit property IDs.
    pub const PROP_CLASS_INFO: u32 = 0;
    pub const PROP_PARAMETER_LIST: u32 = 3;
    pub const PROP_PARAMETER_INFO: u32 = 4;
    pub const PROP_STREAM_FORMAT: u32 = 8;
    pub const PROP_ELEMENT_COUNT: u32 = 11;
    pub const PROP_LATENCY: u32 = 12;
    pub const PROP_MAX_FRAMES_PER_SLICE: u32 = 14;
    pub const PROP_SET_RENDER_CALLBACK: u32 = 23;
    pub const PROP_COCOA_UI: u32 = 31;
    pub const PROP_PARAMETER_VALUE: u32 = 47;

    // Stream format constants.
    pub const FORMAT_LINEAR_PCM: u32 = u32::from_be_bytes(*b"lpcm");
    pub const FORMAT_FLAG_IS_FLOAT: u32 = 1;
    pub const FORMAT_FLAG_IS_NON_INTERLEAVED: u32 = 1 << 5;
    pub const FORMAT_FLAG_IS_PACKED: u32 = 1 << 3;

    // Property-list parsing options.
    pub const PLIST_MUTABLE_CONTAINERS_AND_LEAVES: u64 = 2;

    pub const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;

    #[repr(C)]
    pub struct AudioComponentDescription {
        pub component_type: u32,
        pub component_subtype: u32,
        pub component_manufacturer: u32,
        pub component_flags: u32,
        pub component_flags_mask: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct AudioStreamBasicDescription {
        pub sample_rate: f64,
        pub format_id: u32,
        pub format_flags: u32,
        pub bytes_per_packet: u32,
        pub frames_per_packet: u32,
        pub bytes_per_frame: u32,
        pub channels_per_frame: u32,
        pub bits_per_channel: u32,
        pub reserved: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct AudioBuffer {
        pub number_channels: u32,
        pub data_byte_size: u32,
        pub data: *mut c_void,
    }

    /// Variable-length; `buffers` extends past the struct.
    #[repr(C)]
    pub struct AudioBufferList {
        pub number_buffers: u32,
        pub buffers: [AudioBuffer; 1],
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct AudioTimeStamp {
        pub sample_time: f64,
        pub host_time: u64,
        pub rate_scalar: f64,
        pub word_clock_time: u64,
        pub smpte_time: [u8; 32],
        pub flags: u32,
        pub reserved: u32,
    }

    #[repr(C)]
    pub struct AURenderCallbackStruct {
        pub input_proc: Option<
            extern "C" fn(
                *mut c_void,
                *mut u32,
                *const AudioTimeStamp,
                u32,
                u32,
                *mut AudioBufferList,
            ) -> OSStatus,
        >,
        pub input_proc_ref_con: *mut c_void,
    }

    /// Modern (macOS 10.x+) `AudioUnitParameterInfo` layout from
    /// <AudioToolbox/AudioUnitProperties.h>: 104 bytes. `name` is a C string
    /// when `name[0] != 0`, otherwise `cf_name_string` is used (and must be
    /// released by the caller).
    #[repr(C)]
    pub struct AudioUnitParameterInfo {
        pub name: [c_char; 52],
        pub unit_name: CFStringRef,
        pub clump_id: u32,
        pub cf_name_string: CFStringRef,
        pub unit: u32,
        pub min_value: f32,
        pub max_value: f32,
        pub default_value: f32,
        pub flags: u32,
    }

    const _: () = assert!(std::mem::size_of::<AudioUnitParameterInfo>() == 104);

    #[repr(C)]
    pub struct AudioUnitCocoaViewInfo {
        pub bundle_location: CFURLRef,
        pub cocoa_au_view_class: [CFStringRef; 1],
    }

    #[link(name = "AudioUnit", kind = "framework")]
    unsafe extern "C" {
        pub fn AudioComponentFindNext(
            in_component: AudioComponent,
            in_desc: *const AudioComponentDescription,
        ) -> AudioComponent;
        pub fn AudioComponentInstanceNew(
            in_component: AudioComponent,
            out_instance: *mut AudioUnit,
        ) -> OSStatus;
        pub fn AudioComponentInstanceDispose(instance: AudioUnit) -> OSStatus;
        pub fn AudioComponentCopyName(
            in_component: AudioComponent,
            out_name: *mut CFStringRef,
        ) -> OSStatus;
        pub fn AudioComponentGetVersion(
            in_component: AudioComponent,
            out_version: *mut u32,
        ) -> OSStatus;
        pub fn AudioComponentGetDescription(
            in_component: AudioComponent,
            out_desc: *mut AudioComponentDescription,
        ) -> OSStatus;
        pub fn AudioUnitSetProperty(
            unit: AudioUnit,
            id: u32,
            scope: u32,
            element: u32,
            data: *const c_void,
            size: u32,
        ) -> OSStatus;
        pub fn AudioUnitGetProperty(
            unit: AudioUnit,
            id: u32,
            scope: u32,
            element: u32,
            out_data: *mut c_void,
            io_size: *mut u32,
        ) -> OSStatus;
        pub fn AudioUnitInitialize(unit: AudioUnit) -> OSStatus;
        pub fn AudioUnitUninitialize(unit: AudioUnit) -> OSStatus;
        pub fn AudioUnitRender(
            unit: AudioUnit,
            io_action_flags: *mut u32,
            in_time_stamp: *const AudioTimeStamp,
            in_output_bus_number: u32,
            in_number_frames: u32,
            io_data: *mut AudioBufferList,
        ) -> OSStatus;
        pub fn AudioUnitAddPropertyListener(
            unit: AudioUnit,
            id: u32,
            listener: Option<extern "C" fn(*mut c_void, AudioUnit, u32, u32, u32)>,
            ref_con: *mut c_void,
        ) -> OSStatus;
        pub fn AudioUnitRemovePropertyListenerWithUserData(
            unit: AudioUnit,
            id: u32,
            listener: extern "C" fn(*mut c_void, AudioUnit, u32, u32, u32),
            ref_con: *mut c_void,
        ) -> OSStatus;
        pub fn AudioUnitGetParameter(
            unit: AudioUnit,
            param_id: u32,
            scope: u32,
            element: u32,
            out_value: *mut f32,
        ) -> OSStatus;
        pub fn AudioUnitSetParameter(
            unit: AudioUnit,
            param_id: u32,
            scope: u32,
            element: u32,
            value: f32,
            offset: u32,
        ) -> OSStatus;
    }

    #[link(name = "AudioToolbox", kind = "framework")]
    unsafe extern "C" {
        pub fn MusicDeviceMIDIEvent(
            unit: AudioUnit,
            status: u32,
            data1: u32,
            data2: u32,
            offset: u32,
        ) -> OSStatus;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        pub static kCFRunLoopDefaultMode: CFStringRef;
        pub fn CFRelease(cf: CFTypeRef);
        pub fn CFRetain(cf: CFTypeRef) -> CFTypeRef;
        pub fn CFStringGetCString(
            the_string: CFStringRef,
            buffer: *mut c_char,
            buffer_size: isize,
            encoding: u32,
        ) -> bool;
        pub fn CFStringGetLength(the_string: CFStringRef) -> isize;
        pub fn CFStringGetMaximumSizeForEncoding(length: isize, encoding: u32) -> isize;
        pub fn CFURLCopyFileSystemPath(url: CFURLRef, path_style: u32) -> CFStringRef;
        pub fn CFPropertyListCreateXMLData(
            allocator: *mut c_void,
            property_list: CFTypeRef,
        ) -> CFDataRef;
        pub fn CFPropertyListCreateWithData(
            allocator: *mut c_void,
            data: CFDataRef,
            options: u64,
            format: *mut u32,
            error: *mut c_void,
        ) -> CFTypeRef;
        pub fn CFDataCreate(allocator: *mut c_void, bytes: *const u8, length: isize) -> CFDataRef;
        pub fn CFDataGetBytePtr(data: CFDataRef) -> *const u8;
        pub fn CFDataGetLength(data: CFDataRef) -> isize;
    }
}

use ffi::*;

fn cf_string_to_string(s: CFStringRef) -> String {
    if s.is_null() {
        return String::new();
    }
    unsafe {
        let len = CFStringGetLength(s);
        let cap = CFStringGetMaximumSizeForEncoding(len, K_CF_STRING_ENCODING_UTF8) + 1;
        let mut buf = vec![0u8; cap.max(1) as usize];
        if CFStringGetCString(
            s,
            buf.as_mut_ptr().cast(),
            buf.len() as isize,
            K_CF_STRING_ENCODING_UTF8,
        ) {
            let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
            buf.truncate(end);
            String::from_utf8_lossy(&buf).into_owned()
        } else {
            String::new()
        }
    }
}

fn cf_url_to_path(url: CFURLRef) -> Option<String> {
    if url.is_null() {
        return None;
    }
    unsafe {
        let path = CFURLCopyFileSystemPath(url, 0);
        if path.is_null() {
            return None;
        }
        let s = cf_string_to_string(path);
        CFRelease(path.cast());
        Some(s)
    }
}

fn status_error(context: &str, status: OSStatus) -> String {
    format!("{context} failed with OSStatus {status}")
}

/// Shared state passed as the render-callback refcon. Lives in a Box that is
/// leaked into the AU for the unit's lifetime and recovered in `Drop` after
/// the listener is removed and the unit uninitialized.
struct InputRef {
    /// One stable pointer per input channel plane.
    ptrs: Vec<*mut f32>,
}

extern "C" fn input_render_callback(
    ref_con: *mut c_void,
    _action_flags: *mut u32,
    _time_stamp: *const AudioTimeStamp,
    _bus: u32,
    frames: u32,
    io_data: *mut AudioBufferList,
) -> OSStatus {
    // SAFETY: `ref_con` is the `InputRef` Box owned by `V2Unit`; it outlives
    // the callback registration and the AU never calls the callback
    // concurrently with buffer reallocation (the Vec of pointers is fixed
    // after `initialize`). `io_data` is provided by the AU and has one
    // buffer per non-interleaved channel.
    unsafe {
        if ref_con.is_null() || io_data.is_null() {
            return -1;
        }
        let input_ref = &*(ref_con as *const InputRef);
        let list = &mut *io_data;
        let n = (list.number_buffers as usize).min(input_ref.ptrs.len());
        // `AudioBufferList` is variable-length; the AU allocates `n` buffer
        // descriptors contiguously after the count, so index past the
        // declared `[AudioBuffer; 1]` via raw pointer arithmetic.
        let buffers = list.buffers.as_mut_ptr();
        for i in 0..n {
            let buf = &mut *buffers.add(i);
            buf.data = input_ref.ptrs[i].cast();
            buf.number_channels = 1;
            buf.data_byte_size = frames * 4;
        }
        NO_ERR
    }
}

/// Queue of (scope, element) parameter notifications from the AU.
struct ListenerState {
    queue: Mutex<Vec<(u32, u32)>>,
}

extern "C" fn parameter_listener(
    ref_con: *mut c_void,
    _unit: AudioUnit,
    _id: u32,
    scope: u32,
    element: u32,
) {
    // SAFETY: ref_con points to the ListenerState Box owned by V2Unit.
    unsafe {
        if ref_con.is_null() {
            return;
        }
        let state = &*(ref_con as *const ListenerState);
        if let Ok(mut q) = state.queue.lock() {
            q.push((scope, element));
        }
    }
}

pub struct V2Unit {
    unit: AudioUnit,
    component: AudioComponent,
    desc: AuComponentDesc,
    name: String,
    params: Vec<AuParamInfo>,
    /// (scope, element, paramID) -> dense index into `params`.
    address_map: HashMap<(u32, u32, u32), u32>,
    input_planes: Vec<Vec<f32>>,
    output_planes: Vec<Vec<f32>>,
    max_block_size: usize,
    sample_rate: f64,
    sample_time: f64,
    input_ref: Option<Box<InputRef>>,
    listener: Option<Box<ListenerState>>,
    ui: Option<UiInfo>,
    ui_shown: bool,
}

// SAFETY: the raw AudioUnit handle and the `InputRef` plane pointers are only
// touched from the thread that owns the V2Unit (the host block loop); Cocoa
// GUI work is dispatched to the GUI thread, which only receives the unit
// pointer for view creation while the unit is alive and unchanged.
unsafe impl Send for V2Unit {}

struct UiInfo {
    bundle_path: String,
    class_name: String,
}

impl V2Unit {
    fn get_property_size(&self, id: u32, scope: u32, element: u32) -> Result<u32, String> {
        let mut size = 0u32;
        let status = unsafe {
            AudioUnitGetProperty(
                self.unit,
                id,
                scope,
                element,
                std::ptr::null_mut(),
                &mut size,
            )
        };
        if status == NO_ERR {
            Ok(size)
        } else {
            Err(status_error("AudioUnitGetProperty(size)", status))
        }
    }

    fn enumerate_parameters(&mut self) -> Result<(), String> {
        let size = self.get_property_size(PROP_PARAMETER_LIST, SCOPE_GLOBAL, 0)?;
        let count = size as usize / 4;
        if count == 0 {
            return Ok(());
        }
        let mut ids = vec![0u32; count];
        let mut io_size = size;
        let status = unsafe {
            AudioUnitGetProperty(
                self.unit,
                PROP_PARAMETER_LIST,
                SCOPE_GLOBAL,
                0,
                ids.as_mut_ptr().cast(),
                &mut io_size,
            )
        };
        if status != NO_ERR {
            return Err(status_error("AudioUnitGetProperty(ParameterList)", status));
        }
        for (index, param_id) in ids.into_iter().enumerate() {
            let mut info = AudioUnitParameterInfo {
                name: [0; 52],
                unit_name: std::ptr::null_mut(),
                clump_id: 0,
                cf_name_string: std::ptr::null_mut(),
                unit: 0,
                min_value: 0.0,
                max_value: 0.0,
                default_value: 0.0,
                flags: 0,
            };
            let mut io_size = std::mem::size_of::<AudioUnitParameterInfo>() as u32;
            let status = unsafe {
                AudioUnitGetProperty(
                    self.unit,
                    PROP_PARAMETER_INFO,
                    SCOPE_GLOBAL,
                    param_id,
                    (&mut info as *mut AudioUnitParameterInfo).cast(),
                    &mut io_size,
                )
            };
            if status != NO_ERR {
                continue;
            }
            let name = if info.name[0] != 0 {
                unsafe { std::ffi::CStr::from_ptr(info.name.as_ptr()) }
                    .to_string_lossy()
                    .into_owned()
            } else {
                let s = cf_string_to_string(info.cf_name_string);
                if !info.cf_name_string.is_null() {
                    unsafe { CFRelease(info.cf_name_string.cast()) };
                }
                s
            };
            let dense = index as u32;
            self.address_map
                .insert((SCOPE_GLOBAL, param_id, param_id), dense);
            self.params.push(AuParamInfo {
                index: dense,
                scope: SCOPE_GLOBAL,
                element: param_id,
                param_id,
                name,
                min: f64::from(info.min_value),
                max: f64::from(info.max_value),
                default: f64::from(info.default_value),
                flags: u64::from(info.flags),
            });
        }
        Ok(())
    }

    fn set_stream_format(&self, scope: u32, channels: usize) -> Result<(), String> {
        if channels == 0 {
            return Ok(());
        }
        let format = AudioStreamBasicDescription {
            sample_rate: self.sample_rate(),
            format_id: FORMAT_LINEAR_PCM,
            format_flags: FORMAT_FLAG_IS_FLOAT
                | FORMAT_FLAG_IS_NON_INTERLEAVED
                | FORMAT_FLAG_IS_PACKED,
            bytes_per_packet: 4,
            frames_per_packet: 1,
            bytes_per_frame: 4,
            channels_per_frame: channels as u32,
            bits_per_channel: 32,
            reserved: 0,
        };
        let status = unsafe {
            AudioUnitSetProperty(
                self.unit,
                PROP_STREAM_FORMAT,
                scope,
                0,
                (&format as *const AudioStreamBasicDescription).cast(),
                std::mem::size_of::<AudioStreamBasicDescription>() as u32,
            )
        };
        if status != NO_ERR {
            return Err(status_error("AudioUnitSetProperty(StreamFormat)", status));
        }
        Ok(())
    }

    fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    fn query_cocoa_ui(&self) -> Option<UiInfo> {
        let prop_size = self
            .get_property_size(PROP_COCOA_UI, SCOPE_GLOBAL, 0)
            .unwrap_or(0) as usize;
        if prop_size < std::mem::size_of::<AudioUnitCocoaViewInfo>() {
            return None;
        }
        let count = (prop_size - 8) / 8 + 1;
        let mut buf = vec![0u8; prop_size + 8];
        let mut size = (buf.len() - 8) as u32;
        let status = unsafe {
            AudioUnitGetProperty(
                self.unit,
                PROP_COCOA_UI,
                SCOPE_GLOBAL,
                0,
                buf.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if status != NO_ERR {
            return None;
        }
        let bundle_location = unsafe { *(buf.as_ptr() as *const CFURLRef) };
        let class_refs =
            unsafe { std::slice::from_raw_parts(buf.as_ptr().add(8) as *const CFStringRef, count) };
        let info = AudioUnitCocoaViewInfo {
            bundle_location,
            cocoa_au_view_class: [class_refs[0]],
        };
        if info.cocoa_au_view_class[0].is_null() {
            return None;
        }
        let class_name = cf_string_to_string(info.cocoa_au_view_class[0]);
        let bundle_path = cf_url_to_path(info.bundle_location);
        unsafe {
            CFRelease(info.cocoa_au_view_class[0].cast());
            if !info.bundle_location.is_null() {
                CFRelease(info.bundle_location.cast());
            }
        }
        bundle_path.map(|bundle_path| UiInfo {
            bundle_path,
            class_name,
        })
    }
}

impl AuUnit for V2Unit {
    fn open(desc: &AuComponentDesc) -> Result<Self, String> {
        let cd = AudioComponentDescription {
            component_type: desc.comp_type,
            component_subtype: desc.subtype,
            component_manufacturer: desc.manufacturer,
            component_flags: 0,
            component_flags_mask: 0,
        };
        let component = unsafe { AudioComponentFindNext(std::ptr::null_mut(), &cd) };
        if component.is_null() {
            return Err(format!(
                "AudioComponent not found: {}",
                super::au_spec_string(desc)
            ));
        }
        let mut unit: AudioUnit = std::ptr::null_mut();
        let status = unsafe { AudioComponentInstanceNew(component, &mut unit) };
        if status != NO_ERR || unit.is_null() {
            return Err(status_error("AudioComponentInstanceNew", status));
        }
        let mut name_ref: CFStringRef = std::ptr::null_mut();
        let name = unsafe {
            if AudioComponentCopyName(component, &mut name_ref) == NO_ERR && !name_ref.is_null() {
                let s = cf_string_to_string(name_ref);
                CFRelease(name_ref.cast());
                s
            } else {
                super::au_spec_string(desc)
            }
        };
        Ok(V2Unit {
            unit,
            component,
            desc: *desc,
            name,
            params: Vec::new(),
            address_map: HashMap::new(),
            input_planes: Vec::new(),
            output_planes: Vec::new(),
            max_block_size: 0,
            sample_rate: 0.0,
            sample_time: 0.0,
            input_ref: None,
            listener: None,
            ui: None,
            ui_shown: false,
        })
    }

    fn initialize(&mut self, config: &AuInitConfig) -> Result<(), String> {
        self.max_block_size = config.max_block_size;
        self.sample_rate = config.sample_rate;
        self.input_planes = vec![vec![0.0f32; config.max_block_size]; config.num_inputs];
        self.output_planes = vec![vec![0.0f32; config.max_block_size]; config.num_outputs];

        let music_device = self.is_music_device();
        if !music_device {
            self.set_stream_format(SCOPE_INPUT, config.num_inputs)?;
        }
        self.set_stream_format(SCOPE_OUTPUT, config.num_outputs)?;

        let max_frames = config.max_block_size as u32;
        unsafe {
            AudioUnitSetProperty(
                self.unit,
                PROP_MAX_FRAMES_PER_SLICE,
                SCOPE_GLOBAL,
                0,
                (&max_frames as *const u32).cast(),
                4,
            );
        }

        if !music_device && config.num_inputs > 0 {
            let input_ref = Box::new(InputRef {
                ptrs: self
                    .input_planes
                    .iter_mut()
                    .map(|p| p.as_mut_ptr())
                    .collect(),
            });
            let callback = AURenderCallbackStruct {
                input_proc: Some(input_render_callback),
                input_proc_ref_con: (&*input_ref as *const InputRef).cast::<c_void>()
                    as *mut c_void,
            };
            let status = unsafe {
                AudioUnitSetProperty(
                    self.unit,
                    PROP_SET_RENDER_CALLBACK,
                    SCOPE_INPUT,
                    0,
                    (&callback as *const AURenderCallbackStruct).cast(),
                    std::mem::size_of::<AURenderCallbackStruct>() as u32,
                )
            };
            if status != NO_ERR {
                return Err(status_error(
                    "AudioUnitSetProperty(SetRenderCallback)",
                    status,
                ));
            }
            self.input_ref = Some(input_ref);
        }

        let status = unsafe { AudioUnitInitialize(self.unit) };
        if status != NO_ERR {
            return Err(status_error("AudioUnitInitialize", status));
        }

        self.enumerate_parameters()?;

        if !self.params.is_empty() {
            let listener = Box::new(ListenerState {
                queue: Mutex::new(Vec::new()),
            });
            let status = unsafe {
                AudioUnitAddPropertyListener(
                    self.unit,
                    PROP_PARAMETER_VALUE,
                    Some(parameter_listener),
                    (&*listener as *const ListenerState).cast::<c_void>() as *mut c_void,
                )
            };
            if status == NO_ERR {
                self.listener = Some(listener);
            }
        }

        self.ui = self.query_cocoa_ui();
        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn is_music_device(&self) -> bool {
        self.desc.comp_type == COMP_TYPE_MUSIC_DEVICE
            || self.desc.comp_type == COMP_TYPE_MUSIC_EFFECT
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
        midi_in: &[crate::util::MidiEvent],
        _transport: &AuTransport,
    ) -> Result<(), String> {
        for ev in midi_in {
            let status = unsafe {
                MusicDeviceMIDIEvent(
                    self.unit,
                    u32::from(ev.data.first().copied().unwrap_or(0)),
                    u32::from(ev.data.get(1).copied().unwrap_or(0)),
                    u32::from(ev.data.get(2).copied().unwrap_or(0)),
                    ev.frame,
                )
            };
            if status != NO_ERR {
                return Err(status_error("MusicDeviceMIDIEvent", status));
            }
        }

        let num_out = self.output_planes.len();
        if num_out == 0 {
            return Ok(());
        }
        // Fixed-size mirror of `AudioBufferList`: the buffers array starts
        // after the count *and* its 4 bytes of alignment padding, which the
        // previous flexible-array hack got wrong.
        #[repr(C)]
        struct BufferList {
            number_buffers: u32,
            _pad: u32,
            buffers: [AudioBuffer; MAX_AU_RENDER_CHANNELS],
        }
        const MAX_AU_RENDER_CHANNELS: usize = 32;
        let mut list = BufferList {
            number_buffers: num_out as u32,
            _pad: 0,
            buffers: [AudioBuffer {
                number_channels: 1,
                data_byte_size: 0,
                data: std::ptr::null_mut(),
            }; MAX_AU_RENDER_CHANNELS],
        };
        for (i, plane) in self.output_planes.iter_mut().enumerate() {
            let buf = &mut list.buffers[i];
            buf.data = plane.as_mut_ptr().cast();
            buf.number_channels = 1;
            buf.data_byte_size = (frames * 4) as u32;
        }
        unsafe {
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
            let status = AudioUnitRender(
                self.unit,
                &mut flags,
                &timestamp,
                0,
                frames as u32,
                (&mut list as *mut BufferList).cast(),
            );
            self.sample_time += frames as f64;
            if status != NO_ERR {
                return Err(status_error("AudioUnitRender", status));
            }
        }
        Ok(())
    }

    fn parameters(&self) -> &[AuParamInfo] {
        &self.params
    }

    fn set_parameter(&mut self, index: u32, value: f64) -> Result<(), String> {
        let Some(p) = self.params.get(index as usize) else {
            return Err(format!("unknown AU parameter index {index}"));
        };
        let status = unsafe {
            AudioUnitSetParameter(self.unit, p.param_id, p.scope, p.element, value as f32, 0)
        };
        if status != NO_ERR {
            return Err(status_error("AudioUnitSetParameter", status));
        }
        Ok(())
    }

    fn get_parameter(&self, index: u32) -> Option<f64> {
        let p = self.params.get(index as usize)?;
        let mut value = 0.0f32;
        let status =
            unsafe { AudioUnitGetParameter(self.unit, p.param_id, p.scope, p.element, &mut value) };
        (status == NO_ERR).then_some(f64::from(value))
    }

    fn take_param_updates(&mut self) -> Vec<(u32, f64)> {
        let Some(listener) = self.listener.as_ref() else {
            return Vec::new();
        };
        let notifications = {
            let Ok(mut q) = listener.queue.lock() else {
                return Vec::new();
            };
            std::mem::take(&mut *q)
        };
        let mut out = Vec::new();
        for (scope, element) in notifications {
            if let Some(&dense) = self.address_map.get(&(scope, element, element)) {
                let mut value = 0.0f32;
                let status = unsafe {
                    AudioUnitGetParameter(self.unit, element, scope, element, &mut value)
                };
                if status == NO_ERR {
                    out.push((dense, f64::from(value)));
                }
            }
        }
        out
    }

    fn save_state(&mut self) -> Result<Vec<u8>, String> {
        let mut size = std::mem::size_of::<CFTypeRef>() as u32;
        let mut plist: CFTypeRef = std::ptr::null_mut();
        let status = unsafe {
            AudioUnitGetProperty(
                self.unit,
                PROP_CLASS_INFO,
                SCOPE_GLOBAL,
                0,
                (&mut plist as *mut CFTypeRef).cast(),
                &mut size,
            )
        };
        if status != NO_ERR || plist.is_null() {
            return Err(status_error("AudioUnitGetProperty(ClassInfo)", status));
        }
        unsafe {
            let data = CFPropertyListCreateXMLData(std::ptr::null_mut(), plist);
            CFRelease(plist);
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
            let cf_data = CFDataCreate(std::ptr::null_mut(), data.as_ptr(), data.len() as isize);
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
            let status = AudioUnitSetProperty(
                self.unit,
                PROP_CLASS_INFO,
                SCOPE_GLOBAL,
                0,
                (&plist as *const CFTypeRef).cast(),
                std::mem::size_of::<CFTypeRef>() as u32,
            );
            CFRelease(plist);
            if status != NO_ERR {
                return Err(status_error("AudioUnitSetProperty(ClassInfo)", status));
            }
            Ok(())
        }
    }

    fn latency_samples(&self) -> u32 {
        let mut latency = 0.0f32;
        let mut size = 4u32;
        let status = unsafe {
            AudioUnitGetProperty(
                self.unit,
                PROP_LATENCY,
                SCOPE_GLOBAL,
                0,
                (&mut latency as *mut f32).cast(),
                &mut size,
            )
        };
        if status == NO_ERR {
            let samples = (f64::from(latency) * self.sample_rate).round();
            samples.max(0.0) as u32
        } else {
            0
        }
    }

    fn cocoa_ui(&self) -> Option<(&str, &str)> {
        self.ui
            .as_ref()
            .map(|ui| (ui.bundle_path.as_str(), ui.class_name.as_str()))
    }

    fn raw_unit(&self) -> *mut c_void {
        self.unit.cast()
    }

    fn show_ui(&mut self, _parent: *mut c_void) -> Result<(), String> {
        // Window management happens on the GUI thread in the run loop
        // (`au::run_au`); the unit only validates that a UI exists.
        if self.ui.is_none() {
            return Err("AU component does not publish a Cocoa UI".to_string());
        }
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

impl Drop for V2Unit {
    fn drop(&mut self) {
        if !self.unit.is_null() {
            unsafe {
                if let Some(listener) = self.listener.take() {
                    AudioUnitRemovePropertyListenerWithUserData(
                        self.unit,
                        PROP_PARAMETER_VALUE,
                        parameter_listener,
                        (&*listener as *const ListenerState).cast::<c_void>() as *mut c_void,
                    );
                }
                // Drop the input callback refcon Box after uninitializing so
                // the AU can never call back into freed memory.
                AudioUnitUninitialize(self.unit);
                self.input_ref.take();
                AudioComponentInstanceDispose(self.unit);
                self.unit = std::ptr::null_mut();
            }
        }
        let _ = self.component;
    }
}

#[cfg(test)]
mod tests {
    use super::ffi::*;

    #[test]
    fn parameter_info_is_expected_size() {
        assert_eq!(std::mem::size_of::<AudioUnitParameterInfo>(), 104);
        assert_eq!(std::mem::align_of::<AudioUnitParameterInfo>(), 8);
    }

    #[test]
    fn effect_fourccs() {
        assert_eq!(COMP_TYPE_EFFECT, u32::from_be_bytes(*b"aufx"));
        assert_eq!(COMP_TYPE_MUSIC_EFFECT, u32::from_be_bytes(*b"aumf"));
        assert_eq!(COMP_TYPE_MUSIC_DEVICE, u32::from_be_bytes(*b"aumu"));
        assert_eq!(MANUFACTURER_APPLE, u32::from_be_bytes(*b"appl"));
    }
}
