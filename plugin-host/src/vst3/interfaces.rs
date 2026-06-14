#![allow(clippy::unnecessary_cast)]

use std::ffi::c_void;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use vst3::Steinberg::Vst::ProcessModes_::kRealtime;
use vst3::Steinberg::Vst::SymbolicSampleSizes_::kSample32;
use vst3::Steinberg::Vst::*;
use vst3::Steinberg::*;
use vst3::{Class, ComPtr, ComWrapper, Interface};

pub fn protected_call<T, F>(op: F) -> Result<T, String>
where
    F: FnOnce() -> T + std::panic::UnwindSafe,
{
    match std::panic::catch_unwind(op) {
        Ok(result) => Ok(result),
        Err(_) => Err("Plugin call panicked".to_string()),
    }
}

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

pub struct PluginFactory {
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
    pub fn from_module(bundle_path: &Path) -> Result<Self, String> {
        let module_path = get_module_path(bundle_path)?;

        let library = unsafe {
            libloading::Library::new(&module_path)
                .map_err(|e| format!("Failed to load VST3 module {:?}: {}", module_path, e))?
        };

        let module_inited = unsafe {
            match library.get::<unsafe extern "system" fn() -> bool>(b"InitDll") {
                Ok(init_dll) => init_dll(),
                Err(_) => false,
            }
        };

        let get_factory: libloading::Symbol<unsafe extern "system" fn() -> *mut c_void> = unsafe {
            library
                .get(b"GetPluginFactory")
                .map_err(|e| format!("Failed to find GetPluginFactory: {}", e))?
        };

        let factory_ptr = unsafe { get_factory() };
        if factory_ptr.is_null() {
            return Err("GetPluginFactory returned null".to_string());
        }

        let factory = unsafe { ComPtr::from_raw(factory_ptr as *mut IPluginFactory) }
            .ok_or("Failed to create ComPtr for IPluginFactory")?;

        Ok(Self {
            factory,
            module: library,
            module_inited,
        })
    }

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

    pub fn count_classes(&self) -> i32 {
        use vst3::Steinberg::IPluginFactoryTrait;
        unsafe { self.factory.countClasses() }
    }

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

    pub fn get_factory_info(&self) -> Option<FactoryInfo> {
        use vst3::Steinberg::IPluginFactoryTrait;

        let mut info = PFactoryInfo {
            vendor: [0; 64],
            url: [0; 256],
            email: [0; 128],
            flags: 0,
        };

        let result = unsafe { self.factory.getFactoryInfo(&mut info) };

        if result == kResultOk {
            Some(FactoryInfo {
                vendor: extract_cstring(&info.vendor),
                url: extract_cstring(&info.url),
                email: extract_cstring(&info.email),
                flags: info.flags,
            })
        } else {
            None
        }
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
            if let Ok(exit_dll) = self
                .module
                .get::<unsafe extern "system" fn() -> bool>(b"ExitDll")
            {
                let _ = exit_dll();
            }
        }
    }
}

pub struct ClassInfo {
    pub name: String,
    pub category: String,
    pub cid: [i8; 16],
}

#[derive(Debug, Clone)]
pub struct FactoryInfo {
    pub vendor: String,
    pub url: String,
    pub email: String,
    pub flags: i32,
}

#[derive(Debug, Clone)]
pub struct Vst3GuiInfo {
    pub has_gui: bool,
    pub size: Option<(i32, i32)>,
}

pub struct HostPlugFrame {
    pub resize_requested: AtomicBool,
    pub requested_size: Mutex<Option<(i32, i32)>>,
}

impl Default for HostPlugFrame {
    fn default() -> Self {
        Self {
            resize_requested: AtomicBool::new(false),
            requested_size: Mutex::new(None),
        }
    }
}

impl HostPlugFrame {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Class for HostPlugFrame {
    type Interfaces = (IPlugFrame,);
}

impl IPlugFrameTrait for HostPlugFrame {
    unsafe fn resizeView(&self, _view: *mut IPlugView, new_size: *mut ViewRect) -> tresult {
        if !new_size.is_null() {
            let rect = unsafe { *new_size };
            let width = rect.right - rect.left;
            let height = rect.bottom - rect.top;
            if let Ok(mut size) = self.requested_size.lock() {
                *size = Some((width, height));
            }
            self.resize_requested.store(true, Ordering::Relaxed);
        }
        kResultOk
    }
}

pub struct ComponentHandler {
    pub parameter_changes: Arc<Mutex<Vec<(u32, f64)>>>,
}

impl ComponentHandler {
    pub fn new(parameter_changes: Arc<Mutex<Vec<(u32, f64)>>>) -> Self {
        Self { parameter_changes }
    }
}

impl Class for ComponentHandler {
    type Interfaces = (IComponentHandler,);
}

impl IComponentHandlerTrait for ComponentHandler {
    unsafe fn beginEdit(&self, _id: ParamID) -> tresult {
        kResultOk
    }

    unsafe fn performEdit(&self, id: ParamID, value_normalized: ParamValue) -> tresult {
        if let Ok(mut changes) = self.parameter_changes.lock() {
            changes.push((id, value_normalized));
        }
        kResultOk
    }

    unsafe fn endEdit(&self, _id: ParamID) -> tresult {
        kResultOk
    }

    unsafe fn restartComponent(&self, _flags: i32) -> tresult {
        kResultOk
    }
}

pub struct PluginInstance {
    pub component: ComPtr<IComponent>,
    pub audio_processor: Option<ComPtr<IAudioProcessor>>,
    pub edit_controller: Option<ComPtr<IEditController>>,
    host_context: Box<HostApplicationContext>,
    component_handler: Option<ComWrapper<ComponentHandler>>,
    pub parameter_changes: Arc<Mutex<Vec<(u32, f64)>>>,
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
            component_handler: None,
            parameter_changes: Arc::new(Mutex::new(Vec::new())),
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

    pub fn main_audio_channel_counts(&self) -> (usize, usize) {
        use vst3::Steinberg::Vst::{BusDirections_, BusTypes_, IComponentTrait, MediaTypes_};

        let main_channels_for_direction = |direction: i32| -> usize {
            let bus_count = unsafe {
                self.component
                    .getBusCount(MediaTypes_::kAudio as i32, direction)
            }
            .max(0) as usize;
            if bus_count == 0 {
                return 0;
            }

            let mut first_nonzero = 0usize;
            for idx in 0..bus_count {
                let mut info: vst3::Steinberg::Vst::BusInfo = unsafe { std::mem::zeroed() };
                let result = unsafe {
                    self.component.getBusInfo(
                        MediaTypes_::kAudio as i32,
                        direction,
                        idx as i32,
                        &mut info,
                    )
                };
                if result != kResultOk {
                    continue;
                }
                let channels = info.channelCount.max(0) as usize;
                if channels > 0 && first_nonzero == 0 {
                    first_nonzero = channels;
                }
                if info.busType == BusTypes_::kMain as i32 {
                    return channels.max(1);
                }
            }

            first_nonzero.max(1)
        };

        (
            main_channels_for_direction(BusDirections_::kInput as i32),
            main_channels_for_direction(BusDirections_::kOutput as i32),
        )
    }

    #[allow(clippy::unnecessary_cast)]
    pub fn initialize(&mut self, factory: &PluginFactory) -> Result<(), String> {
        use vst3::Steinberg::IPluginBaseTrait;
        use vst3::Steinberg::Vst::{IComponentTrait, IEditControllerTrait};

        let context = &mut self.host_context.host as *mut IHostApplication as *mut FUnknown;
        let result = unsafe { self.component.initialize(context) };

        if result != kResultOk {
            return Err(format!(
                "Failed to initialize component (result: {})",
                result
            ));
        }

        let mut processor_ptr: *mut c_void = std::ptr::null_mut();
        let result = unsafe {
            let component_raw = self.component.as_ptr();
            let vtbl = (*component_raw).vtbl;
            let query_interface = (*vtbl).base.base.queryInterface;

            let iid = std::mem::transmute::<&[u8; 16], &[i8; 16]>(&IAudioProcessor::IID);
            query_interface(component_raw as *mut _, iid, &mut processor_ptr)
        };

        if result == kResultOk && !processor_ptr.is_null() {
            self.audio_processor =
                unsafe { ComPtr::from_raw(processor_ptr as *mut IAudioProcessor) };
        }

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
            let handler = ComWrapper::new(ComponentHandler::new(self.parameter_changes.clone()));
            if let Some(handler_ptr) = handler.to_com_ptr::<IComponentHandler>() {
                let _ = unsafe { controller.setComponentHandler(handler_ptr.into_raw()) };
            }
            self.component_handler = Some(handler);
        }

        if let Some(ref controller) = self.edit_controller {
            let _ = connect_component_and_controller(&self.component, controller);
        }

        Ok(())
    }

    pub fn query_parameters(&self) -> Vec<super::port::ParameterInfo> {
        let Some(controller) = self.edit_controller.as_ref() else {
            return Vec::new();
        };

        let result = protected_call(|| unsafe {
            use vst3::Steinberg::Vst::IEditControllerTrait;
            let mut params = Vec::new();
            let count = controller.getParameterCount();
            for i in 0..count {
                let mut info: ParameterInfo = std::mem::zeroed();
                if controller.getParameterInfo(i, &mut info) != kResultOk {
                    continue;
                }
                let title = extract_string128(&info.title);
                let short_title = extract_string128(&info.shortTitle);
                let units = extract_string128(&info.units);
                let default_value = controller.getParamNormalized(info.id);
                params.push(super::port::ParameterInfo {
                    id: info.id,
                    title,
                    short_title,
                    units,
                    step_count: info.stepCount,
                    default_value,
                    flags: info.flags,
                });
            }
            params
        });

        result.unwrap_or_default()
    }

    pub fn set_active(&mut self, active: bool) -> Result<(), String> {
        let result = unsafe { self.component.setActive(if active { 1 } else { 0 }) };

        if result != kResultOk {
            return Err(format!("Failed to set active state (result: {})", result));
        }

        Ok(())
    }

    pub fn setup_processing(
        &mut self,
        sample_rate: f64,
        max_samples: i32,
        input_channels: i32,
        output_channels: i32,
    ) -> Result<(), String> {
        use vst3::Steinberg::Vst::{
            BusDirections_, BusInfo, BusInfo_::BusFlags_ as BusFlags, BusTypes_,
            IAudioProcessorTrait, IComponentTrait, MediaTypes_, SpeakerArr,
        };

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

        let configure_audio_buses = |direction: i32, requested_channels: i32| {
            let bus_count = unsafe {
                self.component
                    .getBusCount(MediaTypes_::kAudio as i32, direction)
            }
            .max(0) as usize;
            if bus_count == 0 {
                return Vec::new();
            }

            let mut infos: Vec<BusInfo> = Vec::with_capacity(bus_count);
            for idx in 0..bus_count {
                let mut info: BusInfo = unsafe { std::mem::zeroed() };
                let r = unsafe {
                    self.component.getBusInfo(
                        MediaTypes_::kAudio as i32,
                        direction,
                        idx as i32,
                        &mut info,
                    )
                };
                if r != kResultOk {
                    info.channelCount = if idx == 0 { 2 } else { 0 };
                    info.busType = if idx == 0 {
                        BusTypes_::kMain as i32
                    } else {
                        BusTypes_::kAux as i32
                    };
                    #[allow(clippy::unnecessary_cast)]
                    {
                        info.flags = if idx == 0 {
                            BusFlags::kDefaultActive as u32
                        } else {
                            0
                        };
                    }
                }
                infos.push(info);
            }

            let mut remaining = requested_channels.max(0);
            let mut active = vec![false; bus_count];
            let mut arrangements = vec![SpeakerArr::kEmpty; bus_count];

            let mut ordered: Vec<usize> = (0..bus_count)
                .filter(|&idx| infos[idx].busType == BusTypes_::kMain as i32)
                .collect();
            ordered.extend(
                (0..bus_count).filter(|&idx| infos[idx].busType != BusTypes_::kMain as i32),
            );

            for idx in ordered {
                if remaining <= 0 {
                    break;
                }
                let bus_channels = infos[idx].channelCount.max(1);
                let allocate = remaining.min(bus_channels);
                if allocate > 0 {
                    active[idx] = true;
                    arrangements[idx] = if allocate > 1 {
                        SpeakerArr::kStereo
                    } else {
                        SpeakerArr::kMono
                    };
                    remaining -= allocate;
                }
            }

            if requested_channels > 0 && !active.iter().any(|v| *v) {
                active[0] = true;
                arrangements[0] = if requested_channels > 1 {
                    SpeakerArr::kStereo
                } else {
                    SpeakerArr::kMono
                };
            }

            for (idx, is_active) in active.iter().enumerate().take(bus_count) {
                let _ = unsafe {
                    self.component.activateBus(
                        MediaTypes_::kAudio as i32,
                        direction,
                        idx as i32,
                        if *is_active { 1 } else { 0 },
                    )
                };
            }

            arrangements
        };

        let mut input_arrangements =
            configure_audio_buses(BusDirections_::kInput as i32, input_channels);
        let mut output_arrangements =
            configure_audio_buses(BusDirections_::kOutput as i32, output_channels);
        if !input_arrangements.is_empty() || !output_arrangements.is_empty() {
            let _ = unsafe {
                processor.setBusArrangements(
                    if input_arrangements.is_empty() {
                        std::ptr::null_mut()
                    } else {
                        input_arrangements.as_mut_ptr()
                    },
                    input_arrangements.len() as i32,
                    if output_arrangements.is_empty() {
                        std::ptr::null_mut()
                    } else {
                        output_arrangements.as_mut_ptr()
                    },
                    output_arrangements.len() as i32,
                )
            };
        }

        let event_in_buses = unsafe {
            self.component
                .getBusCount(MediaTypes_::kEvent as i32, BusDirections_::kInput as i32)
        }
        .max(0) as usize;
        for idx in 0..event_in_buses {
            let _ = unsafe {
                self.component.activateBus(
                    MediaTypes_::kEvent as i32,
                    BusDirections_::kInput as i32,
                    idx as i32,
                    1,
                )
            };
        }
        let event_out_buses = unsafe {
            self.component
                .getBusCount(MediaTypes_::kEvent as i32, BusDirections_::kOutput as i32)
        }
        .max(0) as usize;
        for idx in 0..event_out_buses {
            let _ = unsafe {
                self.component.activateBus(
                    MediaTypes_::kEvent as i32,
                    BusDirections_::kOutput as i32,
                    idx as i32,
                    1,
                )
            };
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

fn connect_component_and_controller(
    component: &ComPtr<IComponent>,
    controller: &ComPtr<IEditController>,
) -> Result<(), String> {
    let comp_cp = component.cast::<IConnectionPoint>();
    let ctrl_cp = controller.cast::<IConnectionPoint>();

    if let (Some(comp_cp), Some(ctrl_cp)) = (comp_cp, ctrl_cp) {
        unsafe {
            let result1 = comp_cp.connect(ctrl_cp.as_ptr());
            let result2 = ctrl_cp.connect(comp_cp.as_ptr());
            if result1 == kResultOk && result2 == kResultOk {
                Ok(())
            } else {
                Err(format!(
                    "Connection failed: comp->ctrl={:#x}, ctrl->comp={:#x}",
                    result1, result2
                ))
            }
        }
    } else {
        Ok(())
    }
}

unsafe extern "system" fn host_query_interface(
    this: *mut FUnknown,
    iid: *const TUID,
    obj: *mut *mut c_void,
) -> tresult {
    if this.is_null() || iid.is_null() {
        if !obj.is_null() {
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
                (*ctx).ref_count.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    if !obj.is_null() {
        unsafe {
            if requested_run_loop {
                *obj = (&mut (*ctx).run_loop.iface as *mut Linux::IRunLoop).cast::<c_void>();
            } else {
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

    unsafe { (*ctx).ref_count.fetch_add(1, Ordering::Relaxed) + 1 }
}

unsafe extern "system" fn host_release(this: *mut FUnknown) -> uint32 {
    if this.is_null() {
        return 0;
    }
    let ctx = this as *mut HostApplicationContext;

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

        unsafe {
            *obj = (&mut (*raw).iface as *mut IMessage).cast::<c_void>();
        }
        return kResultOk;
    }

    if wants_attributes {
        let attrs = Box::new(HostAttributeList::new());
        let raw = Box::into_raw(attrs);

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
        unsafe { *obj = std::ptr::null_mut() };
        return kNoInterface;
    }
    let msg = this as *mut HostMessage;

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

    unsafe { (*msg).ref_count.fetch_add(1, Ordering::Relaxed) + 1 }
}

unsafe extern "system" fn host_message_release(this: *mut FUnknown) -> uint32 {
    if this.is_null() {
        return 0;
    }
    let msg = this as *mut HostMessage;

    let remaining = unsafe { (*msg).ref_count.fetch_sub(1, Ordering::AcqRel) - 1 };
    if remaining == 0 {
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

    unsafe { (*msg).message_id }
}

unsafe extern "system" fn host_message_set_id(this: *mut IMessage, id: FIDString) {
    if this.is_null() {
        return;
    }
    let msg = this as *mut HostMessage;

    unsafe {
        (*msg).message_id = if id.is_null() { c"".as_ptr() } else { id };
    }
}

unsafe extern "system" fn host_message_get_attributes(this: *mut IMessage) -> *mut IAttributeList {
    if this.is_null() {
        return std::ptr::null_mut();
    }
    let msg = this as *mut HostMessage;

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
        unsafe { *obj = std::ptr::null_mut() };
        return kNoInterface;
    }
    let attrs = this as *mut HostAttributeList;

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

    unsafe { (*attrs).ref_count.fetch_add(1, Ordering::Relaxed) + 1 }
}

unsafe extern "system" fn host_attr_release(this: *mut FUnknown) -> uint32 {
    if this.is_null() {
        return 0;
    }
    let attrs = this as *mut HostAttributeList;

    let remaining = unsafe { (*attrs).ref_count.fetch_sub(1, Ordering::AcqRel) - 1 };
    if remaining == 0 {
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

fn get_module_path(bundle_path: &Path) -> Result<std::path::PathBuf, String> {
    #[cfg(target_os = "macos")]
    {
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

    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
    {
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

    #[cfg(target_os = "windows")]
    {
        let contents = bundle_path.join("Contents");
        let arch_dir = if cfg!(target_arch = "x86_64") {
            contents.join("x86_64-win")
        } else {
            contents.join("x86-win")
        };

        if let Ok(entries) = std::fs::read_dir(&arch_dir) {
            for entry in entries.flatten() {
                let file_path = entry.path();
                if file_path.is_file()
                    && file_path
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("vst3"))
                {
                    return Ok(file_path);
                }
            }
        }

        let fallback_dir = if cfg!(target_arch = "x86_64") {
            contents.join("x86-win")
        } else {
            contents.join("x86_64-win")
        };
        if let Ok(entries) = std::fs::read_dir(&fallback_dir) {
            for entry in entries.flatten() {
                let file_path = entry.path();
                if file_path.is_file()
                    && file_path
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("vst3"))
                {
                    return Ok(file_path);
                }
            }
        }

        Err(format!(
            "VST3 module not found under {:?}",
            bundle_path.join("Contents")
        ))
    }

    #[cfg(not(any(
        target_os = "macos",
        target_os = "linux",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "windows"
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

fn extract_string128(s: &String128) -> String {
    let len = s.iter().position(|&c| c == 0).unwrap_or(s.len());
    String::from_utf16_lossy(&s[..len])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "maolan-engine-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn iid_ptr_matches_rejects_null_and_non_matching_guids() {
        let iid: TUID = [1; 16];
        let matching_guid = [1_u8; 16];
        let different_guid = [2_u8; 16];

        assert!(!iid_ptr_matches(std::ptr::null(), &matching_guid));
        assert!(iid_ptr_matches(&iid, &matching_guid));
        assert!(!iid_ptr_matches(&iid, &different_guid));
    }

    #[test]
    fn extract_cstring_stops_at_nul_and_uses_lossy_utf8() {
        let bytes = [b'A' as i8, b'B' as i8, -1, 0, b'Z' as i8];

        assert_eq!(extract_cstring(&bytes), "AB\u{FFFD}");
    }

    #[test]
    fn extract_cstring_uses_full_slice_when_not_nul_terminated() {
        let bytes = [b'X' as i8, b'Y' as i8, b'Z' as i8];

        assert_eq!(extract_cstring(&bytes), "XYZ");
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
    #[test]
    fn get_module_path_returns_unix_shared_object_path() {
        let bundle_path = unique_temp_dir("vst3-module").join("Example.vst3");
        let module_path = bundle_path
            .join("Contents")
            .join("x86_64-linux")
            .join("Example.so");
        fs::create_dir_all(module_path.parent().expect("module parent"))
            .expect("create module directory");
        File::create(&module_path).expect("create module file");

        let resolved = get_module_path(&bundle_path).expect("resolve module path");

        assert_eq!(resolved, module_path);

        let _ = fs::remove_dir_all(bundle_path.parent().expect("bundle parent"));
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
    #[test]
    fn get_module_path_errors_when_unix_module_is_missing() {
        let bundle_path = unique_temp_dir("missing-vst3").join("Missing.vst3");
        fs::create_dir_all(&bundle_path).expect("create bundle directory");

        let err = get_module_path(&bundle_path).expect_err("missing module should error");

        assert!(err.contains("Missing.so"));

        let _ = fs::remove_dir_all(bundle_path.parent().expect("bundle parent"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn get_module_path_returns_windows_vst3_binary_path() {
        let bundle_path = unique_temp_dir("vst3-module").join("Example.vst3");
        let module_path = bundle_path
            .join("Contents")
            .join("x86_64-win")
            .join("Example.vst3");
        fs::create_dir_all(module_path.parent().expect("module parent"))
            .expect("create module directory");
        File::create(&module_path).expect("create module file");

        let resolved = get_module_path(&bundle_path).expect("resolve module path");

        assert_eq!(resolved, module_path);

        let _ = fs::remove_dir_all(bundle_path.parent().expect("bundle parent"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn get_module_path_errors_when_windows_module_is_missing() {
        let bundle_path = unique_temp_dir("missing-vst3").join("Missing.vst3");
        fs::create_dir_all(&bundle_path).expect("create bundle directory");

        let err = get_module_path(&bundle_path).expect_err("missing module should error");

        assert!(err.contains("Contents"));

        let _ = fs::remove_dir_all(bundle_path.parent().expect("bundle parent"));
    }
}
