use anyhow::{Ok, Result, anyhow};
use std::collections::HashSet;
use std::ffi::CStr;
use std::os::raw::c_void;
use thiserror::Error;
use vulkanalia::Version;
use vulkanalia::loader::{LIBRARY, LibloadingLoader};
use vulkanalia::prelude::v1_0::*;
use vulkanalia::vk;
use vulkanalia::window as vk_window;
use winit::window::Window;

use vulkanalia::vk::ExtDebugUtilsExtension;

pub struct Context {
    pub entry: Entry,
    pub instance: Instance,
    pub data: AppData,
    pub device: Device,
}

pub struct AppData {
    pub messenger: vk::DebugUtilsMessengerEXT,
    physical_device: vk::PhysicalDevice,
    graphics_queue: vk::Queue,
}

const VALIDATION_ENABLED: bool = cfg!(debug_assertions);

const VALIDATION_LAYER: vk::ExtensionName =
    vk::ExtensionName::from_bytes(b"VK_LAYER_KHRONOS_validation");

const PORTABILITY_MACOS_VERSION: Version = Version::new(1, 3, 216);

impl Context {
    pub unsafe fn create(window: &Window) -> Result<Self> {
        let loader = LibloadingLoader::new(LIBRARY)?;
        let mut data = AppData {
            messenger: vk::DebugUtilsMessengerEXT::null(),
            physical_device: vk::PhysicalDevice::null(),
            graphics_queue: vk::Queue::null(),
        };
        let entry = Entry::new(loader).map_err(|e| anyhow!("{}", e))?;
        let instance = Self::create_instance(window, &entry, &mut data)?;
        Self::pick_physical_device(&instance, &mut data)?;

        let device = Self::create_logical_device(&entry, &instance, &mut data)?;
        Ok(Self {
            entry,
            instance,
            data,
            device,
        })
    }

    unsafe fn create_instance(
        window: &Window,
        entry: &Entry,
        data: &mut AppData,
    ) -> Result<Instance> {
        let app_info = vk::ApplicationInfo::builder()
            .application_name(b"Vulkan App\0")
            .application_version(vk::make_version(1, 0, 0))
            .engine_name(b"No Engine\0")
            .engine_version(vk::make_version(1, 0, 0))
            .api_version(vk::make_version(1, 3, 0)); // Vulkan 1.3

        let available_layers = entry
            .enumerate_instance_layer_properties()?
            .iter()
            .map(|l| l.layer_name)
            .collect::<HashSet<_>>();

        tracing::info!("Available layers:");
        for layer in &available_layers {
            let name = unsafe { CStr::from_ptr(layer.as_ptr()) };
            tracing::info!("  {}", name.to_str().unwrap());
        }

        if VALIDATION_ENABLED && !available_layers.contains(&VALIDATION_LAYER) {
            return Err(anyhow!("Validation layer requested but not supported."));
        }

        let layers = if VALIDATION_ENABLED {
            vec![VALIDATION_LAYER.as_ptr()]
        } else {
            Vec::new()
        };

        // Extensions
        let mut extensions = vk_window::get_required_instance_extensions(window)
            .iter()
            .map(|e| e.as_ptr())
            .collect::<Vec<_>>();

        if VALIDATION_ENABLED {
            extensions.push(vk::EXT_DEBUG_UTILS_EXTENSION.name.as_ptr());
        }

        // Enable portability extensions for MoltenVK on macOS
        let flags = if cfg!(target_os = "macos") && entry.version()? >= Version::new(1, 3, 216) {
            extensions.push(
                vk::KHR_GET_PHYSICAL_DEVICE_PROPERTIES2_EXTENSION
                    .name
                    .as_ptr(),
            );
            extensions.push(vk::KHR_PORTABILITY_ENUMERATION_EXTENSION.name.as_ptr());
            vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR
        } else {
            vk::InstanceCreateFlags::empty()
        };

        let create_info = vk::InstanceCreateInfo::builder()
            .application_info(&app_info)
            .enabled_extension_names(&extensions)
            .flags(flags);

        let mut info = vk::InstanceCreateInfo::builder()
            .application_info(&app_info)
            .enabled_layer_names(&layers)
            .enabled_extension_names(&extensions)
            .flags(flags);

        let mut debug_info = vk::DebugUtilsMessengerCreateInfoEXT::builder()
            .message_severity(vk::DebugUtilsMessageSeverityFlagsEXT::all())
            .message_type(
                vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                    | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                    | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
            )
            .user_callback(Some(Self::debug_callback));

        if VALIDATION_ENABLED {
            info = info.push_next(&mut debug_info);
        }

        let instance = entry.create_instance(&info, None)?;

        if VALIDATION_ENABLED {
            let debug_info = vk::DebugUtilsMessengerCreateInfoEXT::builder()
                .message_severity(vk::DebugUtilsMessageSeverityFlagsEXT::all())
                .message_type(
                    vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                        | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                        | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
                )
                .user_callback(Some(Self::debug_callback));

            data.messenger = instance.create_debug_utils_messenger_ext(&debug_info, None)?;
        }

        Ok(instance)
    }

    pub extern "system" fn debug_callback(
        severity: vk::DebugUtilsMessageSeverityFlagsEXT,
        type_: vk::DebugUtilsMessageTypeFlagsEXT,
        data: *const vk::DebugUtilsMessengerCallbackDataEXT,
        _: *mut c_void,
    ) -> vk::Bool32 {
        let data = unsafe { *data };
        let message = unsafe { CStr::from_ptr(data.message) }.to_string_lossy();

        if severity >= vk::DebugUtilsMessageSeverityFlagsEXT::ERROR {
            tracing::error!("({:?}) {}", type_, message);
        } else if severity >= vk::DebugUtilsMessageSeverityFlagsEXT::WARNING {
            tracing::warn!("({:?}) {}", type_, message);
        } else if severity >= vk::DebugUtilsMessageSeverityFlagsEXT::INFO {
            tracing::debug!("({:?}) {}", type_, message);
        } else {
            tracing::trace!("({:?}) {}", type_, message);
        }

        vk::FALSE
    }

    unsafe fn destroy(&mut self) {
        if VALIDATION_ENABLED {
            self.instance
                .destroy_debug_utils_messenger_ext(self.data.messenger, None);
        }

        self.instance.destroy_instance(None);
        self.device.destroy_device(None);
    }

    unsafe fn check_physical_device(
        instance: &Instance,
        data: &AppData,
        physical_device: vk::PhysicalDevice,
    ) -> Result<()> {
        QueueFamilyIndices::get(instance, data, physical_device)?;
        Ok(())
    }

    unsafe fn pick_physical_device(instance: &Instance, data: &mut AppData) -> Result<()> {
        for physical_device in instance.enumerate_physical_devices()? {
            let properties = instance.get_physical_device_properties(physical_device);

            if let Err(error) = Self::check_physical_device(instance, data, physical_device) {
                tracing::warn!(
                    "Skipping physical device (`{}`): {}",
                    properties.device_name,
                    error
                );
            } else {
                tracing::info!("Selected physical device (`{}`).", properties.device_name);
                data.physical_device = physical_device;
                return Ok(());
            }
        }

        Err(anyhow!("Failed to find suitable physical device."))
    }

    unsafe fn create_logical_device(
        entry: &Entry,
        instance: &Instance,
        data: &mut AppData,
    ) -> Result<Device> {
        let indices = QueueFamilyIndices::get(instance, data, data.physical_device)?;

        let queue_priorities = &[1.0];
        let queue_info = vk::DeviceQueueCreateInfo::builder()
            .queue_family_index(indices.graphics)
            .queue_priorities(queue_priorities);

        let layers = if VALIDATION_ENABLED {
            vec![VALIDATION_LAYER.as_ptr()]
        } else {
            vec![]
        };

        let mut extensions = vec![];

        // Required by Vulkan SDK on macOS since 1.3.216.
        if cfg!(target_os = "macos") && entry.version()? >= PORTABILITY_MACOS_VERSION {
            extensions.push(vk::KHR_PORTABILITY_SUBSET_EXTENSION.name.as_ptr());
        }
        let features = vk::PhysicalDeviceFeatures::builder();

        let queue_infos = &[queue_info];
        let info = vk::DeviceCreateInfo::builder()
            .queue_create_infos(queue_infos)
            .enabled_layer_names(&layers)
            .enabled_extension_names(&extensions)
            .enabled_features(&features);

        let device = instance.create_device(data.physical_device, &info, None)?;
        data.graphics_queue = device.get_device_queue(indices.graphics, 0);
        Ok(device)
    }
}

#[derive(Debug, Error)]
#[error("Missing {0}.")]
pub struct SuitabilityError(pub &'static str);

#[derive(Copy, Clone, Debug)]
struct QueueFamilyIndices {
    graphics: u32,
}

impl QueueFamilyIndices {
    unsafe fn get(
        instance: &Instance,
        data: &AppData,
        physical_device: vk::PhysicalDevice,
    ) -> Result<Self> {
        let properties = instance.get_physical_device_queue_family_properties(physical_device);

        let graphics = properties
            .iter()
            .position(|p| p.queue_flags.contains(vk::QueueFlags::GRAPHICS))
            .map(|i| i as u32);

        if let Some(graphics) = graphics {
            Ok(Self { graphics })
        } else {
            Err(anyhow!(SuitabilityError(
                "Missing required queue families."
            )))
        }
    }
}
