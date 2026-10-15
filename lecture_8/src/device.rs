use crate::AppResult;
use crate::memory::find_memory_type;

use ash::ext::debug_utils;
use ash::khr::{surface, swapchain};
use ash::vk::{self, Handle};
use ash::{Device, Entry, Instance};
use sdl3::video::{VkInstance, Window};
use std::ffi::{CStr, CString, c_char, c_void};

const APP_NAME: &CStr = c"VKE001";
pub const WINDOW_NAME: &str = "VKE001";
pub const DEPTH_FORMAT: vk::Format = vk::Format::D32_SFLOAT;

// The validation layer's messages

// Called by the validation layer, from inside whichever Vulkan call it's checking.
unsafe extern "system" fn debug_callback(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    kind: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    _: *mut c_void,
) -> vk::Bool32 {
    // SAFETY: the layer passes valid callback data, whose message is a '\0'-terminated string.
    let message = unsafe { CStr::from_ptr((*data).p_message) };
    println!("[{severity:?}] [{kind:?}]: {}", message.to_string_lossy());

    // FALSE: let the Vulkan call go ahead.
    vk::FALSE
}

// Swapchain-sized images

// Everything whose size follows the window's: the swapchain, a view of each of its images, and the depth buffer.
struct Targets {
    swapchain: vk::SwapchainKHR,
    images: Vec<vk::Image>,
    views: Vec<vk::ImageView>,
    extent: vk::Extent2D,
    depth_image: vk::Image,
    depth_memory: vk::DeviceMemory,
    depth_view: vk::ImageView,
}

// A 2D view of a whole single-layer, single-mip image.
fn create_view(device: &Device, image: vk::Image, format: vk::Format, aspect: vk::ImageAspectFlags) -> AppResult<vk::ImageView> {
    let range = vk::ImageSubresourceRange { aspect_mask: aspect, base_mip_level: 0, level_count: 1, base_array_layer: 0, layer_count: 1 };
    let info =
        vk::ImageViewCreateInfo::default().image(image).view_type(vk::ImageViewType::TYPE_2D).format(format).subresource_range(range);

    // SAFETY: device and image are live, and the view matches the image's format and layout.
    Ok(unsafe { device.create_image_view(&info, None)? })
}

// The device context

// The objects every frame needs: instance, GPU, device, queue, surface, swapchain and depth buffer.
pub struct DeviceContext {
    _entry: Entry,
    pub instance: Instance,
    pub device: Device,
    pub physical_device: vk::PhysicalDevice,
    pub queue: vk::Queue,
    pub surface: vk::SurfaceKHR,
    pub surface_loader: surface::Instance,
    pub surface_format: vk::SurfaceFormatKHR,
    pub swapchain_loader: swapchain::Device,
    pub swapchain: vk::SwapchainKHR,
    pub swapchain_images: Vec<vk::Image>,
    pub swapchain_image_views: Vec<vk::ImageView>,
    pub swapchain_extent: vk::Extent2D,
    pub command_pool: vk::CommandPool,
    pub depth_image: vk::Image,
    pub depth_image_view: vk::ImageView,
    pub depth_image_memory: vk::DeviceMemory,
    pub memory_properties: vk::PhysicalDeviceMemoryProperties,
    pub min_ubo_alignment: u64,
    debug: Option<(debug_utils::Instance, vk::DebugUtilsMessengerEXT)>,
}

impl DeviceContext {
    pub fn new(window: &Window, enable_validation: bool) -> AppResult<DeviceContext> {
        // SAFETY: each Vulkan call gets valid create infos and live parent objects, created just above it. Everything made here is destroyed in Drop.
        unsafe {
            // "linked": the Vulkan loader was linked at build time, so this can't fail.
            let entry = Entry::linked();
            let app_info = vk::ApplicationInfo::default().application_name(APP_NAME).engine_name(APP_NAME).api_version(vk::API_VERSION_1_3);

            // The instance extensions SDL needs to make a surface for this window, plus debug utils for validation messages.
            let sdl_extensions = window.vulkan_instance_extensions().map_err(|error| format!("SDL3 Vulkan extensions: {error}"))?;
            let extension_names: Vec<CString> = sdl_extensions.iter().map(|name| CString::new(name.as_str())).collect::<Result<_, _>>()?;
            let mut extensions: Vec<*const c_char> = extension_names.iter().map(|name| name.as_ptr()).collect();
            let mut layers: Vec<*const c_char> = Vec::new();
            if enable_validation {
                extensions.push(debug_utils::NAME.as_ptr());
                layers.push(c"VK_LAYER_KHRONOS_validation".as_ptr());
            }

            let instance_info = vk::InstanceCreateInfo::default()
                .application_info(&app_info)
                .enabled_extension_names(&extensions)
                .enabled_layer_names(&layers);
            let instance = entry.create_instance(&instance_info, None)?;

            // Warnings and errors from the validation layer go to debug_callback.
            let debug = if enable_validation {
                let loader = debug_utils::Instance::new(&entry, &instance);
                let info = vk::DebugUtilsMessengerCreateInfoEXT::default()
                    .message_severity(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING)
                    .message_type(
                        vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                            | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                            | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
                    )
                    .pfn_user_callback(Some(debug_callback));
                let messenger = loader.create_debug_utils_messenger(&info, None)?;
                Some((loader, messenger))
            } else {
                None
            };

            // SDL creates the surface; its handle types are SDL's, so the handles cross over as raw integers.
            let surface_raw = window
                .vulkan_create_surface(instance.handle().as_raw() as usize as VkInstance)
                .map_err(|error| format!("Vulkan surface: {error}"))?;
            let surface = vk::SurfaceKHR::from_raw(surface_raw as u64);
            let surface_loader = surface::Instance::new(&entry, &instance);

            // Every GPU with Vulkan 1.3 (the shaders are compiled for it) and a queue family that can draw and present to our surface; a graphics queue can always copy too. Discrete GPUs first, then integrated, then anything else (such as llvmpipe, which runs on the CPU).
            let rank = |kind: vk::PhysicalDeviceType| match kind {
                vk::PhysicalDeviceType::DISCRETE_GPU => 0,
                vk::PhysicalDeviceType::INTEGRATED_GPU => 1,
                _ => 2,
            };
            let (physical_device, queue_family_index) = instance
                .enumerate_physical_devices()?
                .into_iter()
                .filter(|&physical_device| instance.get_physical_device_properties(physical_device).api_version >= vk::API_VERSION_1_3)
                .filter_map(|physical_device| {
                    let families = instance.get_physical_device_queue_family_properties(physical_device);
                    let index = (0..families.len() as u32).find(|&index| {
                        families[index as usize].queue_flags.contains(vk::QueueFlags::GRAPHICS)
                            && surface_loader.get_physical_device_surface_support(physical_device, index, surface).unwrap_or(false)
                    })?;
                    Some((physical_device, index))
                })
                .min_by_key(|&(physical_device, _)| rank(instance.get_physical_device_properties(physical_device).device_type))
                .ok_or("no GPU can draw to this window")?;

            let priorities = [1.0];
            let queue_infos = [vk::DeviceQueueCreateInfo::default().queue_family_index(queue_family_index).queue_priorities(&priorities)];
            let device_extensions = [swapchain::NAME.as_ptr()];
            let device_info = vk::DeviceCreateInfo::default().queue_create_infos(&queue_infos).enabled_extension_names(&device_extensions);
            let device = instance.create_device(physical_device, &device_info, None)?;

            let queue = device.get_device_queue(queue_family_index, 0);
            let memory_properties = instance.get_physical_device_memory_properties(physical_device);
            let properties = instance.get_physical_device_properties(physical_device);
            let min_ubo_alignment = properties.limits.min_uniform_buffer_offset_alignment;
            println!("✓ Using GPU: {}", properties.device_name_as_c_str().unwrap_or(c"<unknown>").to_string_lossy());

            // Whichever format the surface lists first. The order carries no promise; every surface offers at least one format.
            let surface_format = *surface_loader
                .get_physical_device_surface_formats(physical_device, surface)?
                .first()
                .ok_or("the surface has no formats")?;

            let swapchain_loader = swapchain::Device::new(&instance, &device);

            let pool_info = vk::CommandPoolCreateInfo::default()
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
                .queue_family_index(queue_family_index);
            let command_pool = device.create_command_pool(&pool_info, None)?;

            let mut context = DeviceContext {
                _entry: entry,
                instance,
                device,
                physical_device,
                queue,
                surface,
                surface_loader,
                surface_format,
                swapchain_loader,
                swapchain: vk::SwapchainKHR::null(),
                swapchain_images: Vec::new(),
                swapchain_image_views: Vec::new(),
                swapchain_extent: vk::Extent2D::default(),
                command_pool,
                depth_image: vk::Image::null(),
                depth_image_view: vk::ImageView::null(),
                depth_image_memory: vk::DeviceMemory::null(),
                memory_properties,
                min_ubo_alignment,
                debug,
            };

            let (width, height) = window.size_in_pixels();
            context.recreate_swapchain(width, height)?;
            Ok(context)
        }
    }

    // Builds the swapchain and depth buffer at the window's new size, replacing the old ones. The first call has nothing to replace.
    pub fn recreate_swapchain(&mut self, width: u32, height: u32) -> AppResult<()> {
        // SAFETY: after device_wait_idle the GPU uses none of the old objects, so they can be destroyed; the new ones are made from valid infos.
        unsafe {
            self.device.device_wait_idle()?;
            let old_swapchain = self.swapchain;
            self.destroy_targets(false);

            let targets = self.create_targets(width, height, old_swapchain)?;
            if old_swapchain != vk::SwapchainKHR::null() {
                self.swapchain_loader.destroy_swapchain(old_swapchain, None);
            }

            self.swapchain = targets.swapchain;
            self.swapchain_images = targets.images;
            self.swapchain_image_views = targets.views;
            self.swapchain_extent = targets.extent;
            self.depth_image = targets.depth_image;
            self.depth_image_memory = targets.depth_memory;
            self.depth_image_view = targets.depth_view;
        }
        Ok(())
    }

    // SAFETY (for callers): the device must be idle.
    unsafe fn create_targets(&self, width: u32, height: u32, old_swapchain: vk::SwapchainKHR) -> AppResult<Targets> {
        unsafe {
            let caps = self.surface_loader.get_physical_device_surface_capabilities(self.physical_device, self.surface)?;

            // u32::MAX means the swapchain decides the surface's size (as on Wayland): use the window's pixel size, within the surface's limits.
            let extent = if caps.current_extent.width != u32::MAX {
                caps.current_extent
            } else {
                vk::Extent2D {
                    width: width.clamp(caps.min_image_extent.width, caps.max_image_extent.width),
                    height: height.clamp(caps.min_image_extent.height, caps.max_image_extent.height),
                }
            };

            // One image more than the minimum, so we're less likely to wait for the presentation engine to release one. A max_image_count of 0 means no limit.
            let mut image_count = caps.min_image_count + 1;
            if caps.max_image_count > 0 {
                image_count = image_count.min(caps.max_image_count);
            }

            // FIFO: wait for the display's vertical blank, so the frame rate is the refresh rate. Every Vulkan driver supports it.
            let swapchain_info = vk::SwapchainCreateInfoKHR::default()
                .surface(self.surface)
                .min_image_count(image_count)
                .image_format(self.surface_format.format)
                .image_color_space(self.surface_format.color_space)
                .image_extent(extent)
                .image_array_layers(1)
                .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
                .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
                .pre_transform(caps.current_transform)
                .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
                .present_mode(vk::PresentModeKHR::FIFO)
                .clipped(true)
                .old_swapchain(old_swapchain);
            let swapchain = self.swapchain_loader.create_swapchain(&swapchain_info, None)?;

            let images = self.swapchain_loader.get_swapchain_images(swapchain)?;
            let views = images
                .iter()
                .map(|&image| create_view(&self.device, image, self.surface_format.format, vk::ImageAspectFlags::COLOR))
                .collect::<AppResult<Vec<_>>>()?;

            // The depth buffer: one image, in device-local memory, the size of the swapchain.
            let depth_info = vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .format(DEPTH_FORMAT)
                .extent(vk::Extent3D { width: extent.width, height: extent.height, depth: 1 })
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT)
                .sharing_mode(vk::SharingMode::EXCLUSIVE);
            let depth_image = self.device.create_image(&depth_info, None)?;

            let requirements = self.device.get_image_memory_requirements(depth_image);
            let memory_type_index =
                find_memory_type(&self.memory_properties, requirements.memory_type_bits, vk::MemoryPropertyFlags::DEVICE_LOCAL)
                    .ok_or("no device-local memory type for the depth buffer")?;
            let allocate_info = vk::MemoryAllocateInfo::default().allocation_size(requirements.size).memory_type_index(memory_type_index);
            let depth_memory = self.device.allocate_memory(&allocate_info, None)?;
            self.device.bind_image_memory(depth_image, depth_memory, 0)?;
            let depth_view = create_view(&self.device, depth_image, DEPTH_FORMAT, vk::ImageAspectFlags::DEPTH)?;

            Ok(Targets { swapchain, images, views, extent, depth_image, depth_memory, depth_view })
        }
    }

    // Destroys the views and the depth buffer, and the swapchain too if `swapchain` is true. Destroying a null handle does nothing, so this is safe before the first create.
    // SAFETY (for callers): the device must be idle.
    unsafe fn destroy_targets(&mut self, swapchain: bool) {
        unsafe {
            for &view in &self.swapchain_image_views {
                self.device.destroy_image_view(view, None);
            }
            self.swapchain_image_views.clear();

            // Null handles left behind can't be destroyed twice, even if making the new ones fails.
            self.device.destroy_image_view(std::mem::take(&mut self.depth_image_view), None);
            self.device.destroy_image(std::mem::take(&mut self.depth_image), None);
            self.device.free_memory(std::mem::take(&mut self.depth_image_memory), None);

            if swapchain {
                self.swapchain_loader.destroy_swapchain(std::mem::take(&mut self.swapchain), None);
            }
        }
    }
}

// Children before parents: everything made from the device goes before the device, and the device and surface before the instance.
impl Drop for DeviceContext {
    fn drop(&mut self) {
        // SAFETY: after device_wait_idle the GPU uses nothing, and the renderer, which holds objects made from this device, was dropped first.
        unsafe {
            let _ = self.device.device_wait_idle();
            self.destroy_targets(true);
            self.device.destroy_command_pool(self.command_pool, None);
            self.device.destroy_device(None);
            self.surface_loader.destroy_surface(self.surface, None);
            if let Some((loader, messenger)) = &self.debug {
                loader.destroy_debug_utils_messenger(*messenger, None);
            }
            self.instance.destroy_instance(None);
        }
    }
}
