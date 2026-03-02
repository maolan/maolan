// VST3 COM interface wrappers using vst3 crate
//
// This module provides safe Rust wrappers around VST3 COM interfaces
// using the vst3 crate's trait-based API.

use std::ffi::c_void;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use vst3::Steinberg::Vst::ProcessModes_::kRealtime;
use vst3::Steinberg::Vst::SymbolicSampleSizes_::kSample32;
use vst3::Steinberg::Vst::*;
use vst3::Steinberg::*;
use vst3::{ComPtr, Interface};

static HOST_RUN_LOOP_STATE: OnceLock<Mutex<HostRunLoopState>> = OnceLock::new();

struct HostRunLoopState {
    event_handlers: Vec<RunLoopEventHandler>,
    timer_handlers: Vec<RunLoopTimerHandler>,
}

struct RunLoopEventHandler {
    handler: usize,
    fd: i32,
}

struct RunLoopTimerHandler {
    handler: usize,
    interval: Duration,
    next_tick: Instant,
}

fn run_loop_state() -> &'static Mutex<HostRunLoopState> {
    HOST_RUN_LOOP_STATE.get_or_init(|| {
        Mutex::new(HostRunLoopState {
            event_handlers: Vec::new(),
            timer_handlers: Vec::new(),
        })
    })
}

pub fn pump_host_run_loop() {
    // Linux VST3 UIs can rely on host-owned timers/fd callbacks.
    // Drive those callbacks from the GUI-side UI loop.
    let (event_calls, timer_calls): (Vec<(usize, i32)>, Vec<usize>) = {
        let mut state = run_loop_state().lock().expect("run loop mutex poisoned");
        let now = Instant::now();
        let event_calls = state
            .event_handlers
            .iter()
            .map(|h| (h.handler, h.fd))
            .collect::<Vec<_>>();
        let mut timer_calls = Vec::new();
        for timer in &mut state.timer_handlers {
            if now >= timer.next_tick {
                timer_calls.push(timer.handler);
                timer.next_tick = now + timer.interval;
            }
        }
        (event_calls, timer_calls)
    };

    for (handler, fd) in event_calls {
        let handler_ptr = handler as *mut Linux::IEventHandler;
        if handler_ptr.is_null() {
            continue;
        }
        unsafe {
            ((*(*handler_ptr).vtbl).onFDIsSet)(handler_ptr, fd);
        }
    }
    for handler in timer_calls {
        let handler_ptr = handler as *mut Linux::ITimerHandler;
        if handler_ptr.is_null() {
            continue;
        }
        unsafe {
            ((*(*handler_ptr).vtbl).onTimer)(handler_ptr);
        }
    }
}

/// Safe wrapper around VST3 plugin factory
pub struct PluginFactory {
    // Keep COM objects before the module so they are released before dlclose.
    factory: ComPtr<IPluginFactory>,
    module: libloading::Library,
    module_inited: bool,
}

impl std::fmt::Debug for PluginFactory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginFactory")
            .field("factory", &"<COM ptr>")
            .field("module", &"<library>")
            .finish()
    }
}

impl PluginFactory {
    /// Load a VST3 plugin bundle and create a factory
    pub fn from_module(bundle_path: &Path) -> Result<Self, String> {
        // Determine the actual module path based on platform
        let module_path = get_module_path(bundle_path)?;

        // Load the shared library
        let library = unsafe {
            libloading::Library::new(&module_path)
                .map_err(|e| format!("Failed to load VST3 module {:?}: {}", module_path, e))?
        };

        // Many Windows plugins (including iPlug2-based VST3) rely on InitDll to
        // initialize module globals used for resource lookup and UI setup.
        let module_inited = unsafe {
            match library.get::<unsafe extern "system" fn() -> bool>(b"InitDll") {
                Ok(init_dll) => init_dll(),
                Err(_) => false,
            }
        };

        // Get the factory function
        // VST3 plugins export: extern "system" fn GetPluginFactory() -> *mut c_void
        let get_factory: libloading::Symbol<unsafe extern "system" fn() -> *mut c_void> = unsafe {
            library
                .get(b"GetPluginFactory")
                .map_err(|e| format!("Failed to find GetPluginFactory: {}", e))?
        };

        // Call it to get the factory
        let factory_ptr = unsafe { get_factory() };
        if factory_ptr.is_null() {
            return Err("GetPluginFactory returned null".to_string());
        }

        // Wrap in ComPtr - the vst3 crate provides this smart pointer
        let factory = unsafe { ComPtr::from_raw(factory_ptr as *mut IPluginFactory) }
            .ok_or("Failed to create ComPtr for IPluginFactory")?;

        Ok(Self {
            factory,
            module: library,
            module_inited,
        })
    }

    /// Get information about a plugin class using the trait
    pub fn get_class_info(&self, index: i32) -> Option<ClassInfo> {
        use vst3::Steinberg::IPluginFactoryTrait;

        let mut info = PClassInfo {
            cid: [0; 16],
            cardinality: 0,
            category: [0; 32],
            name: [0; 64],
        };

        let result = unsafe { self.factory.getClassInfo(index, &mut info) };

        if result == kResultOk {
            Some(ClassInfo {
                name: extract_cstring(&info.name),
                category: extract_cstring(&info.category),
                cid: info.cid,
            })
        } else {
            None
        }
    }

    /// Count the number of classes using the trait
    pub fn count_classes(&self) -> i32 {
        use vst3::Steinberg::IPluginFactoryTrait;
        unsafe { self.factory.countClasses() }
    }

    /// Create an instance of a plugin using the trait
    pub fn create_instance(&self, class_id: &[i8; 16]) -> Result<PluginInstance, String> {
        use vst3::Steinberg::IPluginFactoryTrait;

        let mut instance_ptr: *mut c_void = std::ptr::null_mut();

        let result = unsafe {
            self.factory.createInstance(
                class_id.as_ptr(),
                IComponent::IID.as_ptr() as *const i8,
                &mut instance_ptr,
            )
        };

        if result != kResultOk || instance_ptr.is_null() {
            return Err(format!(
                "Failed to create plugin instance (result: {})",
                result
            ));
        }

        let component = unsafe { ComPtr::from_raw(instance_ptr as *mut IComponent) }
            .ok_or("Failed to create ComPtr for IComponent")?;

        Ok(PluginInstance::new(component))
    }

    pub fn create_edit_controller(
        &self,
        class_id: &[i8; 16],
    ) -> Result<ComPtr<IEditController>, String> {
        use vst3::Steinberg::IPluginFactoryTrait;

        let mut instance_ptr: *mut c_void = std::ptr::null_mut();

        let result = unsafe {
            self.factory.createInstance(
                class_id.as_ptr(),
                IEditController::IID.as_ptr() as *const i8,
                &mut instance_ptr,
            )
        };

        if result != kResultOk || instance_ptr.is_null() {
            return Err(format!(
                "Failed to create edit controller instance (result: {})",
                result
            ));
        }

        unsafe { ComPtr::from_raw(instance_ptr as *mut IEditController) }
            .ok_or("Failed to create ComPtr for IEditController".to_string())
    }
}

impl Drop for PluginFactory {
    fn drop(&mut self) {
        if !self.module_inited {
            return;
        }

        unsafe {
            if let Ok(exit_dll) = self.module.get::<unsafe extern "system" fn() -> bool>(b"ExitDll")
            {
                let _ = exit_dll();
            }
        }
    }
}

/// Information about a plugin class
pub struct ClassInfo {
    pub name: String,
    pub category: String,
    pub cid: [i8; 16],
}

/// Safe wrapper around a VST3 plugin instance
pub struct PluginInstance {
    pub component: ComPtr<IComponent>,
    pub audio_processor: Option<ComPtr<IAudioProcessor>>,
    pub edit_controller: Option<ComPtr<IEditController>>,
    host_context: Box<HostApplicationContext>,
    component_handler: Box<HostComponentHandlerContext>,
}

impl std::fmt::Debug for PluginInstance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginInstance")
            .field("component", &"<COM ptr>")
            .field("audio_processor", &self.audio_processor.is_some())
            .field("edit_controller", &self.edit_controller.is_some())
            .finish()
    }
}

impl PluginInstance {
    fn new(component: ComPtr<IComponent>) -> Self {
        Self {
            component,
            audio_processor: None,
            edit_controller: None,
            host_context: Box::new(HostApplicationContext::new()),
            component_handler: Box::new(HostComponentHandlerContext::new()),
        }
    }

    pub fn audio_bus_counts(&self) -> (usize, usize) {
        use vst3::Steinberg::Vst::{BusDirections_, IComponentTrait, MediaTypes_};

        let input_count = unsafe {
            self.component
                .getBusCount(MediaTypes_::kAudio as i32, BusDirections_::kInput as i32)
        }
        .max(0) as usize;
        let output_count = unsafe {
            self.component
                .getBusCount(MediaTypes_::kAudio as i32, BusDirections_::kOutput as i32)
        }
        .max(0) as usize;
        (input_count, output_count)
    }

    pub fn event_bus_counts(&self) -> (usize, usize) {
        use vst3::Steinberg::Vst::{BusDirections_, IComponentTrait, MediaTypes_};

        let input_count = unsafe {
            self.component
                .getBusCount(MediaTypes_::kEvent as i32, BusDirections_::kInput as i32)
        }
        .max(0) as usize;
        let output_count = unsafe {
            self.component
                .getBusCount(MediaTypes_::kEvent as i32, BusDirections_::kOutput as i32)
        }
        .max(0) as usize;
        (input_count, output_count)
    }

    /// Initialize the component
    pub fn initialize(&mut self, factory: &PluginFactory) -> Result<(), String> {
        use vst3::Steinberg::IPluginBaseTrait;
        use vst3::Steinberg::Vst::{IComponentTrait, IEditControllerTrait};

        // Pass a stable host application context for plugins that require IHostApplication.
        let context = &mut self.host_context.host as *mut IHostApplication as *mut FUnknown;
        let result = unsafe { self.component.initialize(context) };

        if result != kResultOk {
            return Err(format!(
                "Failed to initialize component (result: {})",
                result
            ));
        }

        // Query for IAudioProcessor
        let mut processor_ptr: *mut c_void = std::ptr::null_mut();
        let result = unsafe {
            // Access the vtable through the raw pointer
            let component_raw = self.component.as_ptr();
            let vtbl = (*component_raw).vtbl;
            let query_interface = (*vtbl).base.base.queryInterface;
            // Cast IID from [u8; 16] to [i8; 16]
            let iid = std::mem::transmute::<&[u8; 16], &[i8; 16]>(&IAudioProcessor::IID);
            query_interface(component_raw as *mut _, iid, &mut processor_ptr)
        };

        if result == kResultOk && !processor_ptr.is_null() {
            self.audio_processor =
                unsafe { ComPtr::from_raw(processor_ptr as *mut IAudioProcessor) };
        }

        // Query for IEditController directly from component first.
        let mut controller_ptr: *mut c_void = std::ptr::null_mut();
        let query_result = unsafe {
            let component_raw = self.component.as_ptr();
            let vtbl = (*component_raw).vtbl;
            let query_interface = (*vtbl).base.base.queryInterface;
            let iid = std::mem::transmute::<&[u8; 16], &[i8; 16]>(&IEditController::IID);
            query_interface(component_raw as *mut _, iid, &mut controller_ptr)
        };
        if query_result == kResultOk && !controller_ptr.is_null() {
            self.edit_controller =
                unsafe { ComPtr::from_raw(controller_ptr as *mut IEditController) };
        }

        // If not available directly, instantiate the dedicated controller class.
        if self.edit_controller.is_none() {
            let mut controller_cid: TUID = [0; 16];
            let cid_result = unsafe { self.component.getControllerClassId(&mut controller_cid) };
            if cid_result == kResultOk {
                let mut maybe_controller = factory.create_edit_controller(&controller_cid).ok();
                if let Some(controller) = maybe_controller.as_mut() {
                    let controller_context =
                        &mut self.host_context.host as *mut IHostApplication as *mut FUnknown;
                    let init_result = unsafe { controller.initialize(controller_context) };
                    if init_result != kResultOk {
                        maybe_controller = None;
                    }
                }
                self.edit_controller = maybe_controller;
            }
        }

        if let Some(controller) = self.edit_controller.as_ref() {
            let handler =
                &mut self.component_handler.handler as *mut IComponentHandler as *mut FUnknown;
            let _ = unsafe { controller.setComponentHandler(handler as *mut IComponentHandler) };
        }

        Ok(())
    }

    /// Set the component active/inactive
    pub fn set_active(&mut self, active: bool) -> Result<(), String> {
        let result = unsafe { self.component.setActive(if active { 1 } else { 0 }) };

        if result != kResultOk {
            return Err(format!("Failed to set active state (result: {})", result));
        }

        Ok(())
    }

    /// Setup processing parameters
    pub fn setup_processing(
        &mut self,
        sample_rate: f64,
        max_samples: i32,
        input_channels: i32,
        output_channels: i32,
    ) -> Result<(), String> {
        use vst3::Steinberg::Vst::{IAudioProcessorTrait, SpeakerArr};

        let processor = self
            .audio_processor
            .as_ref()
            .ok_or("No audio processor available")?;

        let sample_size_result = unsafe { processor.canProcessSampleSize(kSample32 as i32) };
        if sample_size_result != kResultOk {
            return Err(format!(
                "Plugin does not support 32-bit sample size (result: {})",
                sample_size_result
            ));
        }

        let (input_bus_count, output_bus_count) = self.audio_bus_counts();
        if input_bus_count > 0 || output_bus_count > 0 {
            let mut input_arrangements = vec![SpeakerArr::kEmpty; input_bus_count];
            let mut output_arrangements = vec![SpeakerArr::kEmpty; output_bus_count];

            for (idx, arr) in input_arrangements.iter_mut().enumerate() {
                let r = unsafe {
                    processor.getBusArrangement(BusDirections_::kInput as i32, idx as i32, arr)
                };
                if r != kResultOk {
                    *arr = if input_channels > 1 {
                        SpeakerArr::kStereo
                    } else {
                        SpeakerArr::kMono
                    };
                }
            }
            for (idx, arr) in output_arrangements.iter_mut().enumerate() {
                let r = unsafe {
                    processor.getBusArrangement(BusDirections_::kOutput as i32, idx as i32, arr)
                };
                if r != kResultOk {
                    *arr = if output_channels > 1 {
                        SpeakerArr::kStereo
                    } else {
                        SpeakerArr::kMono
                    };
                }
            }

            let set_arrangements = unsafe {
                processor.setBusArrangements(
                    input_arrangements.as_mut_ptr(),
                    input_arrangements.len() as i32,
                    output_arrangements.as_mut_ptr(),
                    output_arrangements.len() as i32,
                )
            };
            let _ = set_arrangements;
        }

        let mut setup = ProcessSetup {
            processMode: kRealtime as i32,
            symbolicSampleSize: kSample32 as i32,
            maxSamplesPerBlock: max_samples,
            sampleRate: sample_rate,
        };

        let result = unsafe { processor.setupProcessing(&mut setup) };

        if result != kResultOk {
            return Err(format!("Failed to setup processing (result: {})", result));
        }

        Ok(())
    }

    pub fn start_processing(&mut self) -> Result<(), String> {
        use vst3::Steinberg::Vst::IAudioProcessorTrait;

        let Some(processor) = &self.audio_processor else {
            return Ok(());
        };
        let result = unsafe { processor.setProcessing(1) };
        if result != kResultOk {
            return Err(format!(
                "Failed to enable processing state (result: {})",
                result
            ));
        }
        Ok(())
    }

    pub fn stop_processing(&mut self) {
        use vst3::Steinberg::Vst::IAudioProcessorTrait;

        if let Some(processor) = &self.audio_processor {
            unsafe {
                let _ = processor.setProcessing(0);
            }
        }
    }

    /// Terminate the component
    pub fn terminate(&mut self) -> Result<(), String> {
        use vst3::Steinberg::IPluginBaseTrait;

        let result = unsafe { self.component.terminate() };

        if result != kResultOk {
            return Err(format!(
                "Failed to terminate component (result: {})",
                result
            ));
        }

        Ok(())
    }
}

#[repr(C)]
struct HostApplicationContext {
    host: IHostApplication,
    run_loop: HostRunLoopContext,
    ref_count: AtomicU32,
}

impl HostApplicationContext {
    fn new() -> Self {
        Self {
            host: IHostApplication {
                vtbl: &HOST_APPLICATION_VTBL,
            },
            run_loop: HostRunLoopContext::new(),
            ref_count: AtomicU32::new(1),
        }
    }
}

#[repr(C)]
struct HostRunLoopContext {
    iface: Linux::IRunLoop,
    ref_count: AtomicU32,
}

impl HostRunLoopContext {
    fn new() -> Self {
        Self {
            iface: Linux::IRunLoop {
                vtbl: &HOST_RUN_LOOP_VTBL,
            },
            ref_count: AtomicU32::new(1),
        }
    }
}

#[repr(C)]
struct HostComponentHandlerContext {
    handler: IComponentHandler,
    ref_count: AtomicU32,
}

impl HostComponentHandlerContext {
    fn new() -> Self {
        Self {
            handler: IComponentHandler {
                vtbl: &HOST_COMPONENT_HANDLER_VTBL,
            },
            ref_count: AtomicU32::new(1),
        }
    }
}

unsafe extern "system" fn component_handler_query_interface(
    this: *mut FUnknown,
    iid: *const TUID,
    obj: *mut *mut c_void,
) -> tresult {
    if this.is_null() || iid.is_null() {
        if !obj.is_null() {
            unsafe { *obj = std::ptr::null_mut() };
        }
        return kNoInterface;
    }

    let iid_bytes = unsafe { &*iid };
    let requested_handler = iid_bytes
        .iter()
        .zip(IComponentHandler::IID.iter())
        .all(|(a, b)| (*a as u8) == *b);
    let requested_unknown = iid_bytes
        .iter()
        .zip(FUnknown::IID.iter())
        .all(|(a, b)| (*a as u8) == *b);
    if !(requested_handler || requested_unknown) {
        if !obj.is_null() {
            unsafe { *obj = std::ptr::null_mut() };
        }
        return kNoInterface;
    }

    let ctx = this as *mut HostComponentHandlerContext;
    unsafe {
        (*ctx).ref_count.fetch_add(1, Ordering::Relaxed);
        if !obj.is_null() {
            *obj = this.cast::<c_void>();
        }
    }
    kResultOk
}

unsafe extern "system" fn component_handler_add_ref(this: *mut FUnknown) -> uint32 {
    if this.is_null() {
        return 0;
    }
    let ctx = this as *mut HostComponentHandlerContext;
    unsafe { (*ctx).ref_count.fetch_add(1, Ordering::Relaxed) + 1 }
}

unsafe extern "system" fn component_handler_release(this: *mut FUnknown) -> uint32 {
    if this.is_null() {
        return 0;
    }
    let ctx = this as *mut HostComponentHandlerContext;
    unsafe { (*ctx).ref_count.fetch_sub(1, Ordering::Relaxed) - 1 }
}

unsafe extern "system" fn component_handler_begin_edit(
    _this: *mut IComponentHandler,
    _id: ParamID,
) -> tresult {
    kResultOk
}

unsafe extern "system" fn component_handler_perform_edit(
    _this: *mut IComponentHandler,
    _id: ParamID,
    _value_normalized: ParamValue,
) -> tresult {
    kResultOk
}

unsafe extern "system" fn component_handler_end_edit(
    _this: *mut IComponentHandler,
    _id: ParamID,
) -> tresult {
    kResultOk
}

unsafe extern "system" fn component_handler_restart_component(
    _this: *mut IComponentHandler,
    _flags: i32,
) -> tresult {
    kResultOk
}

static HOST_COMPONENT_HANDLER_VTBL: IComponentHandlerVtbl = IComponentHandlerVtbl {
    base: FUnknownVtbl {
        queryInterface: component_handler_query_interface,
        addRef: component_handler_add_ref,
        release: component_handler_release,
    },
    beginEdit: component_handler_begin_edit,
    performEdit: component_handler_perform_edit,
    endEdit: component_handler_end_edit,
    restartComponent: component_handler_restart_component,
};

unsafe extern "system" fn host_query_interface(
    this: *mut FUnknown,
    iid: *const TUID,
    obj: *mut *mut c_void,
) -> tresult {
    if this.is_null() || iid.is_null() {
        if !obj.is_null() {
            // SAFETY: Caller provides output storage.
            unsafe {
                *obj = std::ptr::null_mut();
            }
        }
        return kNoInterface;
    }

    let iid_bytes = unsafe { &*iid };
    let requested_host = iid_bytes
        .iter()
        .zip(IHostApplication::IID.iter())
        .all(|(a, b)| (*a as u8) == *b);
    let requested_unknown = iid_bytes
        .iter()
        .zip(FUnknown::IID.iter())
        .all(|(a, b)| (*a as u8) == *b);
    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
    let requested_run_loop = iid_bytes
        .iter()
        .zip(Linux::IRunLoop::IID.iter())
        .all(|(a, b)| (*a as u8) == *b);
    #[cfg(not(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd")))]
    let requested_run_loop = false;
    if !(requested_host || requested_unknown || requested_run_loop) {
        if !obj.is_null() {
            // SAFETY: Caller provides output storage.
            unsafe {
                *obj = std::ptr::null_mut();
            }
        }
        return kNoInterface;
    }

    let ctx = this as *mut HostApplicationContext;
    if !ctx.is_null() {
        unsafe {
            if requested_run_loop {
                (*ctx).run_loop.ref_count.fetch_add(1, Ordering::Relaxed);
            } else {
                // SAFETY: `this` is the first field in HostApplicationContext.
                (*ctx).ref_count.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    if !obj.is_null() {
        unsafe {
            if requested_run_loop {
                *obj = (&mut (*ctx).run_loop.iface as *mut Linux::IRunLoop).cast::<c_void>();
            } else {
                // SAFETY: Caller supplies storage for out-pointer.
                *obj = this.cast::<c_void>();
            }
        }
    }
    kResultOk
}

unsafe extern "system" fn host_add_ref(this: *mut FUnknown) -> uint32 {
    if this.is_null() {
        return 0;
    }
    let ctx = this as *mut HostApplicationContext;
    // SAFETY: `this` points to embedded host interface at offset 0.
    unsafe { (*ctx).ref_count.fetch_add(1, Ordering::Relaxed) + 1 }
}

unsafe extern "system" fn host_release(this: *mut FUnknown) -> uint32 {
    if this.is_null() {
        return 0;
    }
    let ctx = this as *mut HostApplicationContext;
    // SAFETY: `this` points to embedded host interface at offset 0.
    unsafe { (*ctx).ref_count.fetch_sub(1, Ordering::Relaxed) - 1 }
}

unsafe extern "system" fn host_get_name(
    _this: *mut IHostApplication,
    name: *mut String128,
) -> tresult {
    if name.is_null() {
        return kNoInterface;
    }
    let encoded: Vec<u16> = "Maolan".encode_utf16().collect();
    // SAFETY: `name` points to writable `String128`.
    unsafe {
        (*name).fill(0);
        for (idx, ch) in encoded
            .iter()
            .take((*name).len().saturating_sub(1))
            .enumerate()
        {
            (*name)[idx] = *ch;
        }
    }
    kResultOk
}

unsafe extern "system" fn host_create_instance(
    _this: *mut IHostApplication,
    cid: *mut TUID,
    iid: *mut TUID,
    obj: *mut *mut c_void,
) -> tresult {
    if obj.is_null() {
        return kInvalidArgument;
    }
    // SAFETY: caller provided out pointer.
    unsafe {
        *obj = std::ptr::null_mut();
    }

    let wants_message =
        iid_ptr_matches(cid, &IMessage::IID) || iid_ptr_matches(iid, &IMessage::IID);
    let wants_attributes =
        iid_ptr_matches(cid, &IAttributeList::IID) || iid_ptr_matches(iid, &IAttributeList::IID);
    if wants_message {
        let message = Box::new(HostMessage::new());
        let raw = Box::into_raw(message);
        // SAFETY: `iface` is first field and valid for COM client usage.
        unsafe {
            *obj = (&mut (*raw).iface as *mut IMessage).cast::<c_void>();
        }
        return kResultOk;
    }

    if wants_attributes {
        let attrs = Box::new(HostAttributeList::new());
        let raw = Box::into_raw(attrs);
        // SAFETY: `iface` is first field and valid for COM client usage.
        unsafe {
            *obj = (&mut (*raw).iface as *mut IAttributeList).cast::<c_void>();
        }
        return kResultOk;
    }

    kNotImplemented
}

static HOST_APPLICATION_VTBL: IHostApplicationVtbl = IHostApplicationVtbl {
    base: FUnknownVtbl {
        queryInterface: host_query_interface,
        addRef: host_add_ref,
        release: host_release,
    },
    getName: host_get_name,
    createInstance: host_create_instance,
};

unsafe extern "system" fn run_loop_query_interface(
    this: *mut FUnknown,
    iid: *const TUID,
    obj: *mut *mut c_void,
) -> tresult {
    if this.is_null() || iid.is_null() || obj.is_null() {
        return kInvalidArgument;
    }
    let requested_run_loop = iid_ptr_matches(iid, &Linux::IRunLoop::IID);
    let requested_unknown = iid_ptr_matches(iid, &FUnknown::IID);
    if !(requested_run_loop || requested_unknown) {
        unsafe { *obj = std::ptr::null_mut() };
        return kNoInterface;
    }
    let ctx = this as *mut HostRunLoopContext;
    unsafe {
        (*ctx).ref_count.fetch_add(1, Ordering::Relaxed);
        *obj = this.cast::<c_void>();
    }
    kResultOk
}

unsafe extern "system" fn run_loop_add_ref(this: *mut FUnknown) -> uint32 {
    if this.is_null() {
        return 0;
    }
    let ctx = this as *mut HostRunLoopContext;
    unsafe { (*ctx).ref_count.fetch_add(1, Ordering::Relaxed) + 1 }
}

unsafe extern "system" fn run_loop_release(this: *mut FUnknown) -> uint32 {
    if this.is_null() {
        return 0;
    }
    let ctx = this as *mut HostRunLoopContext;
    unsafe { (*ctx).ref_count.fetch_sub(1, Ordering::Relaxed) - 1 }
}

unsafe extern "system" fn run_loop_register_event_handler(
    _this: *mut Linux::IRunLoop,
    handler: *mut Linux::IEventHandler,
    fd: i32,
) -> tresult {
    if handler.is_null() {
        return kInvalidArgument;
    }
    let unknown = handler as *mut FUnknown;
    unsafe {
        let _ = ((*(*unknown).vtbl).addRef)(unknown);
    }
    let mut state = run_loop_state().lock().expect("run loop mutex poisoned");
    state.event_handlers.push(RunLoopEventHandler {
        handler: handler as usize,
        fd,
    });
    kResultOk
}

unsafe extern "system" fn run_loop_unregister_event_handler(
    _this: *mut Linux::IRunLoop,
    handler: *mut Linux::IEventHandler,
) -> tresult {
    if handler.is_null() {
        return kInvalidArgument;
    }
    let mut state = run_loop_state().lock().expect("run loop mutex poisoned");
    state.event_handlers.retain(|h| {
        if h.handler == handler as usize {
            let unknown = handler as *mut FUnknown;
            unsafe {
                let _ = ((*(*unknown).vtbl).release)(unknown);
            }
            false
        } else {
            true
        }
    });
    kResultOk
}

unsafe extern "system" fn run_loop_register_timer(
    _this: *mut Linux::IRunLoop,
    handler: *mut Linux::ITimerHandler,
    milliseconds: u64,
) -> tresult {
    if handler.is_null() {
        return kInvalidArgument;
    }
    let unknown = handler as *mut FUnknown;
    unsafe {
        let _ = ((*(*unknown).vtbl).addRef)(unknown);
    }
    let interval = Duration::from_millis(milliseconds.max(1));
    let mut state = run_loop_state().lock().expect("run loop mutex poisoned");
    state.timer_handlers.push(RunLoopTimerHandler {
        handler: handler as usize,
        interval,
        next_tick: Instant::now() + interval,
    });
    unsafe {
        // Kick timer once so plugins that defer first paint to timer callbacks can initialize.
        ((*(*handler).vtbl).onTimer)(handler);
    }
    kResultOk
}

unsafe extern "system" fn run_loop_unregister_timer(
    _this: *mut Linux::IRunLoop,
    handler: *mut Linux::ITimerHandler,
) -> tresult {
    if handler.is_null() {
        return kInvalidArgument;
    }
    let mut state = run_loop_state().lock().expect("run loop mutex poisoned");
    state.timer_handlers.retain(|t| {
        if t.handler == handler as usize {
            let unknown = handler as *mut FUnknown;
            unsafe {
                let _ = ((*(*unknown).vtbl).release)(unknown);
            }
            false
        } else {
            true
        }
    });
    kResultOk
}

static HOST_RUN_LOOP_VTBL: Linux::IRunLoopVtbl = Linux::IRunLoopVtbl {
    base: FUnknownVtbl {
        queryInterface: run_loop_query_interface,
        addRef: run_loop_add_ref,
        release: run_loop_release,
    },
    registerEventHandler: run_loop_register_event_handler,
    unregisterEventHandler: run_loop_unregister_event_handler,
    registerTimer: run_loop_register_timer,
    unregisterTimer: run_loop_unregister_timer,
};

#[repr(C)]
struct HostMessage {
    iface: IMessage,
    ref_count: AtomicU32,
    message_id: FIDString,
    attributes: *mut IAttributeList,
}

impl HostMessage {
    fn new() -> Self {
        let attrs = Box::new(HostAttributeList::new());
        let attrs_raw = Box::into_raw(attrs);
        Self {
            iface: IMessage {
                vtbl: &HOST_MESSAGE_VTBL,
            },
            ref_count: AtomicU32::new(1),
            message_id: c"".as_ptr(),
            // SAFETY: `iface` is first field.
            attributes: unsafe { &mut (*attrs_raw).iface as *mut IAttributeList },
        }
    }
}

#[repr(C)]
struct HostAttributeList {
    iface: IAttributeList,
    ref_count: AtomicU32,
}

impl HostAttributeList {
    fn new() -> Self {
        Self {
            iface: IAttributeList {
                vtbl: &HOST_ATTRIBUTE_LIST_VTBL,
            },
            ref_count: AtomicU32::new(1),
        }
    }
}

fn iid_ptr_matches(iid_ptr: *const TUID, guid: &[u8; 16]) -> bool {
    if iid_ptr.is_null() {
        return false;
    }
    // SAFETY: caller provides valid IID pointer for the duration of call.
    let iid = unsafe { &*iid_ptr };
    iid.iter()
        .zip(guid.iter())
        .all(|(lhs, rhs)| (*lhs as u8) == *rhs)
}

unsafe extern "system" fn host_message_query_interface(
    this: *mut FUnknown,
    iid: *const TUID,
    obj: *mut *mut c_void,
) -> tresult {
    if this.is_null() || iid.is_null() || obj.is_null() {
        return kInvalidArgument;
    }
    let requested_message = iid_ptr_matches(iid, &IMessage::IID);
    let requested_unknown = iid_ptr_matches(iid, &FUnknown::IID);
    if !(requested_message || requested_unknown) {
        // SAFETY: out pointer valid.
        unsafe { *obj = std::ptr::null_mut() };
        return kNoInterface;
    }
    let msg = this as *mut HostMessage;
    // SAFETY: message context pointer is valid.
    unsafe {
        (*msg).ref_count.fetch_add(1, Ordering::Relaxed);
        *obj = this.cast::<c_void>();
    }
    kResultOk
}

unsafe extern "system" fn host_message_add_ref(this: *mut FUnknown) -> uint32 {
    if this.is_null() {
        return 0;
    }
    let msg = this as *mut HostMessage;
    // SAFETY: pointer valid for atomic update.
    unsafe { (*msg).ref_count.fetch_add(1, Ordering::Relaxed) + 1 }
}

unsafe extern "system" fn host_message_release(this: *mut FUnknown) -> uint32 {
    if this.is_null() {
        return 0;
    }
    let msg = this as *mut HostMessage;
    // SAFETY: pointer valid for atomic update.
    let remaining = unsafe { (*msg).ref_count.fetch_sub(1, Ordering::AcqRel) - 1 };
    if remaining == 0 {
        // SAFETY: Release the message-owned reference to attributes before freeing message.
        unsafe {
            if !(*msg).attributes.is_null() {
                let attrs_unknown = (*msg).attributes.cast::<FUnknown>();
                let _ = host_attr_release(attrs_unknown);
                (*msg).attributes = std::ptr::null_mut();
            }
            let _ = Box::from_raw(msg);
        }
    }
    remaining
}

unsafe extern "system" fn host_message_get_id(this: *mut IMessage) -> FIDString {
    if this.is_null() {
        return c"".as_ptr();
    }
    let msg = this as *mut HostMessage;
    // SAFETY: valid pointer for message context.
    unsafe { (*msg).message_id }
}

unsafe extern "system" fn host_message_set_id(this: *mut IMessage, id: FIDString) {
    if this.is_null() {
        return;
    }
    let msg = this as *mut HostMessage;
    // SAFETY: we only keep borrowed pointer; plugin controls lifetime for call scope.
    unsafe {
        (*msg).message_id = if id.is_null() { c"".as_ptr() } else { id };
    }
}

unsafe extern "system" fn host_message_get_attributes(this: *mut IMessage) -> *mut IAttributeList {
    if this.is_null() {
        return std::ptr::null_mut();
    }
    let msg = this as *mut HostMessage;
    // SAFETY: return a referenced COM pointer to caller.
    unsafe {
        if !(*msg).attributes.is_null() {
            let attrs_unknown = (*msg).attributes.cast::<FUnknown>();
            let _ = host_attr_add_ref(attrs_unknown);
        }
        (*msg).attributes
    }
}

static HOST_MESSAGE_VTBL: IMessageVtbl = IMessageVtbl {
    base: FUnknownVtbl {
        queryInterface: host_message_query_interface,
        addRef: host_message_add_ref,
        release: host_message_release,
    },
    getMessageID: host_message_get_id,
    setMessageID: host_message_set_id,
    getAttributes: host_message_get_attributes,
};

unsafe extern "system" fn host_attr_query_interface(
    this: *mut FUnknown,
    iid: *const TUID,
    obj: *mut *mut c_void,
) -> tresult {
    if this.is_null() || iid.is_null() || obj.is_null() {
        return kInvalidArgument;
    }
    let requested_attr = iid_ptr_matches(iid, &IAttributeList::IID);
    let requested_unknown = iid_ptr_matches(iid, &FUnknown::IID);
    if !(requested_attr || requested_unknown) {
        // SAFETY: out pointer valid.
        unsafe { *obj = std::ptr::null_mut() };
        return kNoInterface;
    }
    let attrs = this as *mut HostAttributeList;
    // SAFETY: pointer valid.
    unsafe {
        (*attrs).ref_count.fetch_add(1, Ordering::Relaxed);
        *obj = this.cast::<c_void>();
    }
    kResultOk
}

unsafe extern "system" fn host_attr_add_ref(this: *mut FUnknown) -> uint32 {
    if this.is_null() {
        return 0;
    }
    let attrs = this as *mut HostAttributeList;
    // SAFETY: pointer valid.
    unsafe { (*attrs).ref_count.fetch_add(1, Ordering::Relaxed) + 1 }
}

unsafe extern "system" fn host_attr_release(this: *mut FUnknown) -> uint32 {
    if this.is_null() {
        return 0;
    }
    let attrs = this as *mut HostAttributeList;
    // SAFETY: pointer valid.
    let remaining = unsafe { (*attrs).ref_count.fetch_sub(1, Ordering::AcqRel) - 1 };
    if remaining == 0 {
        // SAFETY: pointer was allocated with Box::into_raw.
        unsafe {
            let _ = Box::from_raw(attrs);
        }
    }
    remaining
}

unsafe extern "system" fn host_attr_set_int(
    _this: *mut IAttributeList,
    _id: IAttrID,
    _value: i64,
) -> tresult {
    kResultOk
}

unsafe extern "system" fn host_attr_get_int(
    _this: *mut IAttributeList,
    _id: IAttrID,
    value: *mut i64,
) -> tresult {
    if value.is_null() {
        return kInvalidArgument;
    }
    // SAFETY: caller provides writable pointer.
    unsafe { *value = 0 };
    kResultFalse
}

unsafe extern "system" fn host_attr_set_float(
    _this: *mut IAttributeList,
    _id: IAttrID,
    _value: f64,
) -> tresult {
    kResultOk
}

unsafe extern "system" fn host_attr_get_float(
    _this: *mut IAttributeList,
    _id: IAttrID,
    value: *mut f64,
) -> tresult {
    if value.is_null() {
        return kInvalidArgument;
    }
    // SAFETY: caller provides writable pointer.
    unsafe { *value = 0.0 };
    kResultFalse
}

unsafe extern "system" fn host_attr_set_string(
    _this: *mut IAttributeList,
    _id: IAttrID,
    _string: *const TChar,
) -> tresult {
    kResultOk
}

unsafe extern "system" fn host_attr_get_string(
    _this: *mut IAttributeList,
    _id: IAttrID,
    string: *mut TChar,
    size_in_bytes: u32,
) -> tresult {
    if string.is_null() || size_in_bytes < std::mem::size_of::<TChar>() as u32 {
        return kInvalidArgument;
    }
    // SAFETY: buffer has at least one TChar cell.
    unsafe { *string = 0 };
    kResultFalse
}

unsafe extern "system" fn host_attr_set_binary(
    _this: *mut IAttributeList,
    _id: IAttrID,
    _data: *const c_void,
    _size_in_bytes: u32,
) -> tresult {
    kResultOk
}

unsafe extern "system" fn host_attr_get_binary(
    _this: *mut IAttributeList,
    _id: IAttrID,
    data: *mut *const c_void,
    size_in_bytes: *mut u32,
) -> tresult {
    if data.is_null() || size_in_bytes.is_null() {
        return kInvalidArgument;
    }
    // SAFETY: caller provides writable pointers.
    unsafe {
        *data = std::ptr::null();
        *size_in_bytes = 0;
    }
    kResultFalse
}

static HOST_ATTRIBUTE_LIST_VTBL: IAttributeListVtbl = IAttributeListVtbl {
    base: FUnknownVtbl {
        queryInterface: host_attr_query_interface,
        addRef: host_attr_add_ref,
        release: host_attr_release,
    },
    setInt: host_attr_set_int,
    getInt: host_attr_get_int,
    setFloat: host_attr_set_float,
    getFloat: host_attr_get_float,
    setString: host_attr_set_string,
    getString: host_attr_get_string,
    setBinary: host_attr_set_binary,
    getBinary: host_attr_get_binary,
};

/// Get the actual module path from a VST3 bundle path
fn get_module_path(bundle_path: &Path) -> Result<std::path::PathBuf, String> {
    #[cfg(target_os = "windows")]
    {
        // Windows: .vst3/Contents/x86_64-win/plugin.vst3
        let module = bundle_path
            .join("Contents")
            .join("x86_64-win")
            .join(format!(
                "{}.vst3",
                bundle_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("plugin")
            ));
        if module.exists() {
            Ok(module)
        } else {
            Err(format!("VST3 module not found at {:?}", module))
        }
    }

    #[cfg(target_os = "macos")]
    {
        // macOS: .vst3/Contents/MacOS/plugin
        let module = bundle_path.join("Contents").join("MacOS").join(
            bundle_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("plugin"),
        );
        if module.exists() {
            Ok(module)
        } else {
            Err(format!("VST3 module not found at {:?}", module))
        }
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    {
        // Linux/FreeBSD: .vst3/Contents/x86_64-linux/plugin.so
        let module = bundle_path
            .join("Contents")
            .join("x86_64-linux")
            .join(format!(
                "{}.so",
                bundle_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("plugin")
            ));
        if module.exists() {
            Ok(module)
        } else {
            Err(format!("VST3 module not found at {:?}", module))
        }
    }

    #[cfg(not(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "linux",
        target_os = "freebsd"
    )))]
    {
        Err("Unsupported platform".to_string())
    }
}

fn extract_cstring(bytes: &[i8]) -> String {
    let len = bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len());
    let u8_bytes: Vec<u8> = bytes[..len].iter().map(|&b| b as u8).collect();
    String::from_utf8_lossy(&u8_bytes).to_string()
}
