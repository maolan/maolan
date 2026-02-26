// VST3 COM interface wrappers using vst3 crate
//
// This module provides safe Rust wrappers around VST3 COM interfaces
// using the vst3 crate's trait-based API.

use std::ffi::c_void;
use std::path::Path;
use vst3::Steinberg::Vst::*;
use vst3::Steinberg::Vst::ProcessModes_::kRealtime;
use vst3::Steinberg::Vst::SymbolicSampleSizes_::kSample32;
use vst3::Steinberg::*;
use vst3::{ComPtr, Interface};

/// Safe wrapper around VST3 plugin factory
pub struct PluginFactory {
    _module: libloading::Library,
    factory: ComPtr<IPluginFactory>,
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
            _module: library,
            factory,
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
        }
    }

    /// Initialize the component
    pub fn initialize(&mut self) -> Result<(), String> {
        use vst3::Steinberg::IPluginBaseTrait;

        // TODO: Create proper host context (FUnknown implementation)
        // For now, pass null
        let result = unsafe { self.component.initialize(std::ptr::null_mut()) };

        if result != kResultOk {
            return Err(format!("Failed to initialize component (result: {})", result));
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
            query_interface(
                component_raw as *mut _,
                iid,
                &mut processor_ptr,
            )
        };

        if result == kResultOk && !processor_ptr.is_null() {
            self.audio_processor =
                unsafe { ComPtr::from_raw(processor_ptr as *mut IAudioProcessor) };
        }

        // Query for IEditController
        let mut controller_ptr: *mut c_void = std::ptr::null_mut();
        let result = unsafe {
            // Access the vtable through the raw pointer
            let component_raw = self.component.as_ptr();
            let vtbl = (*component_raw).vtbl;
            let query_interface = (*vtbl).base.base.queryInterface;
            // Cast IID from [u8; 16] to [i8; 16]
            let iid = std::mem::transmute::<&[u8; 16], &[i8; 16]>(&IEditController::IID);
            query_interface(
                component_raw as *mut _,
                iid,
                &mut controller_ptr,
            )
        };

        if result == kResultOk && !controller_ptr.is_null() {
            self.edit_controller =
                unsafe { ComPtr::from_raw(controller_ptr as *mut IEditController) };
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
    pub fn setup_processing(&mut self, sample_rate: f64, max_samples: i32) -> Result<(), String> {
        use vst3::Steinberg::Vst::IAudioProcessorTrait;

        let processor = self
            .audio_processor
            .as_ref()
            .ok_or("No audio processor available")?;

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

    /// Terminate the component
    pub fn terminate(&mut self) -> Result<(), String> {
        use vst3::Steinberg::IPluginBaseTrait;

        let result = unsafe { self.component.terminate() };

        if result != kResultOk {
            return Err(format!("Failed to terminate component (result: {})", result));
        }

        Ok(())
    }
}

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
