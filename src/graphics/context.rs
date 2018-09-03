use std::cell::RefCell;
use std::rc::Rc;

use ash::extensions::{Surface, Swapchain};
use ash::version::{DeviceV1_0, EntryV1_0, InstanceV1_0, V1_0};
use ash::vk;
use ash::{Device, Entry, Instance};
use std::ffi::CString;
use std::mem;
use std::ptr;
use winit::{self, dpi};

use conf::{FullscreenType, WindowMode, WindowSetup};
use context::DebugId;
use graphics::*;

use GameResult;

const VERTEX_BUFFER_BINDING_ID: u32 = 0;
const INSTANCE_BUFFER_BINDING_ID: u32 = 1;

macro_rules! offset_of {
    ($base:path, $field:ident) => {{
        #[allow(unused_unsafe)]
        unsafe {
            let b: $base = mem::uninitialized();
            (&b.$field as *const _ as isize) - (&b as *const _ as isize)
        }
    }};
}

pub(crate) struct GraphicsContext {
    globals: Globals,
    globals_buffer: vulkan::Buffer,
    instance_buffer: vulkan::Buffer,
    projection: Matrix4,
    pub(crate) modelview_stack: Vec<Matrix4>,
    white_image: Image,
    pub(crate) screen_rect: Rect,
    color_format: vk::Format,
    depth_format: vk::Format,
    srgb: bool,
    pub(crate) hidpi_factor: f32,
    pub(crate) os_hidpi_factor: f32,
    pub(crate) window: winit::Window,
    multisample_samples: u32,
    entry: Entry<V1_0>,
    instance: Instance<V1_0>,
    pdevice: vk::PhysicalDevice,
    pub(crate) pdevice_memory_props: vk::PhysicalDeviceMemoryProperties,
    pub(crate) graphics_queue: vk::Queue,
    pub(crate) graphics_queue_family_index: u32,
    graphics_pipeline: vk::Pipeline,
    graphics_pipeline_layout: vk::PipelineLayout,
    descriptor_pool: vk::DescriptorPool,
    descriptor_set: vk::DescriptorSet,
    descriptor_set_layout: vk::DescriptorSetLayout,
    render_pass: vk::RenderPass,
    framebuffers: Vec<vk::Framebuffer>,
    pub(crate) device: Device<V1_0>,
    swapchain_images: Vec<vk::Image>,
    swapchain_image_views: Vec<vk::ImageView>,
    // color_images: Vec<vk::Image>,
    // color_image_memories: Vec<vk::DeviceMemory>,
    // color_image_views: Vec<vk::ImageView>,
    // depth_images: Vec<vk::Image>,
    // depth_image_memories: Vec<vk::DeviceMemory>,
    // depth_image_views: Vec<vk::ImageView>,
    pub(crate) command_pool: vk::CommandPool,
    command_buffers: Vec<vk::CommandBuffer>,
    image_available_semaphores: Vec<vk::Semaphore>,
    rendering_complete_semaphores: Vec<vk::Semaphore>,
    frame_fences: Vec<vk::Fence>,
    current_frame: usize,
    image_count: usize,
    swapchain: vk::SwapchainKHR,
    surface: vk::SurfaceKHR,
    default_sampler: vk::Sampler,
    swapchain_loader: Swapchain,
    surface_loader: Surface,
}

impl GraphicsContext {
    pub(crate) fn new(
        events_loop: &winit::EventsLoop,
        window_setup: &WindowSetup,
        window_mode: WindowMode,
        debug_id: DebugId,
    ) -> GameResult<Self> {
        let srgb = window_setup.srgb;

        let mut window_builder = winit::WindowBuilder::new()
            .with_title(window_setup.title.clone())
            .with_transparency(window_setup.transparent)
            .with_resizable(window_mode.resizable);
        window_builder = if !window_setup.icon.is_empty() {
            use winit::Icon;
            window_builder.with_window_icon(Some(Icon::from_path(&window_setup.icon)?))
        } else {
            window_builder
        };
        let window = window_builder.build(events_loop)?;

        let os_hidpi_factor = window.get_hidpi_factor() as f32;
        let hidpi_factor = if window_mode.hidpi {
            os_hidpi_factor
        } else {
            1.0
        };

        let entry: Entry<V1_0> = Entry::new().expect("Failed to load Vulkan entry");
        let instance: Instance<V1_0> = {
            let application_name = CString::new(window_setup.title.clone()).expect("Wrong name");
            let application_info = vk::ApplicationInfo {
                s_type: vk::StructureType::ApplicationInfo,
                p_next: ptr::null(),
                p_application_name: application_name.as_ptr(),
                application_version: 1,
                p_engine_name: CString::new("ggez").expect("Wrong name").as_ptr(),
                engine_version: 1,
                // VK_API_VERSION_1_0
                api_version: vk_make_version!(1, 0, 0),
            };

            let extensions = vulkan::instance_extension_names();
            let create_info = vk::InstanceCreateInfo {
                s_type: vk::StructureType::InstanceCreateInfo,
                p_next: ptr::null(),
                flags: vk::InstanceCreateFlags::empty(),
                p_application_info: &application_info,
                enabled_layer_count: 0,
                pp_enabled_layer_names: ptr::null(),
                enabled_extension_count: extensions.len() as u32,
                pp_enabled_extension_names: extensions.as_ptr(),
            };
            unsafe { entry.create_instance(&create_info, None)? }
        };

        let surface_loader =
            Surface::new(&entry, &instance).expect("Failed to load surface extension");
        let surface = vulkan::create_surface(&entry, &instance, &window)?;

        let pdevices = instance.enumerate_physical_devices()?;
        let pdevice = pdevices.iter().next().unwrap();
        let graphics_queue_family_index = instance
            .get_physical_device_queue_family_properties(*pdevice)
            .iter()
            .enumerate()
            .map(|(index, props)| {
                if props.queue_flags.subset(vk::QUEUE_GRAPHICS_BIT)
                    && surface_loader.get_physical_device_surface_support_khr(
                        *pdevice,
                        index as u32,
                        surface,
                    ) {
                    return index;
                } else {
                    panic!("No matching devices");
                }
            })
            .next()
            .unwrap();

        let device: Device<V1_0> = {
            let queue_priorities = [1.0];
            let queue_create_infos = [vk::DeviceQueueCreateInfo {
                s_type: vk::StructureType::DeviceQueueCreateInfo,
                p_next: ptr::null(),
                flags: vk::DeviceQueueCreateFlags::empty(),
                queue_family_index: graphics_queue_family_index as u32,
                queue_count: queue_priorities.len() as u32,
                p_queue_priorities: queue_priorities.as_ptr(),
            }];
            let features = instance.get_physical_device_features(*pdevice);
            let extensions = [Swapchain::name().as_ptr()];
            let create_info = vk::DeviceCreateInfo {
                s_type: vk::StructureType::DeviceCreateInfo,
                p_next: ptr::null(),
                flags: vk::DeviceCreateFlags::empty(),
                queue_create_info_count: queue_create_infos.len() as u32,
                p_queue_create_infos: queue_create_infos.as_ptr(),
                enabled_layer_count: 0,
                pp_enabled_layer_names: ptr::null(),
                enabled_extension_count: extensions.len() as u32,
                pp_enabled_extension_names: extensions.as_ptr(),
                p_enabled_features: &features,
            };
            unsafe { instance.create_device(*pdevice, &create_info, None)? }
        };

        let graphics_queue =
            unsafe { device.get_device_queue(graphics_queue_family_index as u32, 0) };

        let surface_resolution = vk::Extent2D {
            width: window_mode.width as u32,
            height: window_mode.height as u32,
        };

        let surface_formats =
            surface_loader.get_physical_device_surface_formats_khr(*pdevice, surface)?;
        let surface_format = surface_formats
            .iter()
            .map(|f| match (f.format, f.color_space) {
                (vk::Format::R8g8b8a8Srgb, vk::ColorSpaceKHR::SrgbNonlinear) => f,
                (_, vk::ColorSpaceKHR::SrgbNonlinear) => f,
            })
            .next()
            .unwrap();

        let pdevice_surface_caps =
            surface_loader.get_physical_device_surface_capabilities_khr(*pdevice, surface)?;
        let image_count = if pdevice_surface_caps.max_image_count > 0
            && (pdevice_surface_caps.min_image_count + 1) > pdevice_surface_caps.max_image_count
        {
            pdevice_surface_caps.max_image_count
        } else {
            pdevice_surface_caps.min_image_count + 1
        };

        let swapchain_loader =
            Swapchain::new(&instance, &device).expect("Failed to load swapchain extension");
        let swapchain = {
            let present_modes =
                surface_loader.get_physical_device_surface_present_modes_khr(*pdevice, surface)?;
            let present_mode = if window_setup.vsync {
                present_modes
                    .iter()
                    .cloned()
                    .find(|&pm| pm == vk::PresentModeKHR::Fifo)
                    .unwrap()
            } else {
                present_modes
                    .iter()
                    .cloned()
                    .find(|&pm| pm == vk::PresentModeKHR::Mailbox)
                    .unwrap_or(vk::PresentModeKHR::Immediate)
            };

            let create_info = vk::SwapchainCreateInfoKHR {
                s_type: vk::StructureType::SwapchainCreateInfoKhr,
                p_next: ptr::null(),
                flags: vk::SwapchainCreateFlagsKHR::empty(),
                surface,
                min_image_count: image_count,
                image_format: surface_format.format,
                image_color_space: surface_format.color_space,
                image_extent: surface_resolution,
                image_array_layers: 1,
                image_usage: pdevice_surface_caps.supported_usage_flags,
                image_sharing_mode: vk::SharingMode::Exclusive,
                queue_family_index_count: 1,
                p_queue_family_indices: &(graphics_queue_family_index as u32),
                pre_transform: pdevice_surface_caps.supported_transforms,
                composite_alpha: pdevice_surface_caps.supported_composite_alpha,
                present_mode,
                clipped: 1,
                old_swapchain: vk::SwapchainKHR::null(),
            };
            unsafe { swapchain_loader.create_swapchain_khr(&create_info, None)? }
        };

        let swapchain_image_views = swapchain_loader
            .get_swapchain_images_khr(swapchain)?
            .iter()
            .map(|&image| {
                let create_info = vk::ImageViewCreateInfo {
                    s_type: vk::StructureType::ImageViewCreateInfo,
                    p_next: ptr::null(),
                    flags: vk::ImageViewCreateFlags::empty(),
                    image,
                    view_type: vk::ImageViewType::Type2d,
                    format: surface_format.format,
                    components: vk::ComponentMapping {
                        r: vk::ComponentSwizzle::R,
                        g: vk::ComponentSwizzle::G,
                        b: vk::ComponentSwizzle::B,
                        a: vk::ComponentSwizzle::A,
                    },
                    subresource_range: vk::ImageSubresourceRange {
                        aspect_mask: vk::IMAGE_ASPECT_COLOR_BIT,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    },
                };
                unsafe { device.create_image_view(&create_info, None).unwrap() }
            })
            .collect::<Vec<_>>();

        let command_pool = {
            let create_info = vk::CommandPoolCreateInfo {
                s_type: vk::StructureType::CommandPoolCreateInfo,
                p_next: ptr::null(),
                flags: vk::COMMAND_POOL_CREATE_RESET_COMMAND_BUFFER_BIT,
                queue_family_index: graphics_queue_family_index as u32,
            };
            unsafe { device.create_command_pool(&create_info, None)? }
        };

        let command_buffers = {
            let allocate_info = vk::CommandBufferAllocateInfo {
                s_type: vk::StructureType::CommandBufferAllocateInfo,
                p_next: ptr::null(),
                command_pool,
                level: vk::CommandBufferLevel::Primary,
                command_buffer_count: image_count as u32,
            };
            unsafe { device.allocate_command_buffers(&allocate_info)? }
        };

        let (image_available_semaphores, rendering_complete_semaphores, frame_fences) = {
            let semaphore_create_info = vk::SemaphoreCreateInfo {
                s_type: vk::StructureType::SemaphoreCreateInfo,
                p_next: ptr::null(),
                flags: vk::SemaphoreCreateFlags::empty(),
            };
            let image_available_semaphores = (0..image_count)
                .map(|_| unsafe {
                    device
                        .create_semaphore(&semaphore_create_info, None)
                        .unwrap()
                })
                .collect::<Vec<_>>();
            let rendering_complete_semaphores = (0..image_count)
                .map(|_| unsafe {
                    device
                        .create_semaphore(&semaphore_create_info, None)
                        .unwrap()
                })
                .collect::<Vec<_>>();

            let fence_create_info = vk::FenceCreateInfo {
                s_type: vk::StructureType::FenceCreateInfo,
                p_next: ptr::null(),
                flags: vk::FENCE_CREATE_SIGNALED_BIT,
            };
            let frame_fences = (0..image_count)
                .map(|_| unsafe { device.create_fence(&fence_create_info, None).unwrap() })
                .collect::<Vec<_>>();

            (
                image_available_semaphores,
                rendering_complete_semaphores,
                frame_fences,
            )
        };

        let default_sampler = {
            let create_info = vk::SamplerCreateInfo {
                s_type: vk::StructureType::SamplerCreateInfo,
                p_next: ptr::null(),
                flags: vk::SamplerCreateFlags::empty(),
                mag_filter: vk::Filter::Linear,
                min_filter: vk::Filter::Linear,
                mipmap_mode: vk::SamplerMipmapMode::Linear,
                address_mode_u: vk::SamplerAddressMode::ClampToEdge,
                address_mode_v: vk::SamplerAddressMode::ClampToEdge,
                address_mode_w: vk::SamplerAddressMode::ClampToEdge,
                mip_lod_bias: 0.0,
                anisotropy_enable: 0,
                max_anisotropy: 1.0,
                compare_enable: 0,
                compare_op: vk::CompareOp::Never,
                min_lod: 0.0,
                max_lod: 1.0,
                border_color: vk::BorderColor::FloatOpaqueWhite,
                unnormalized_coordinates: 0,
            };
            unsafe { device.create_sampler(&create_info, None)? }
        };

        let descriptor_pool = {
            let pool_sizes = [
                vk::DescriptorPoolSize {
                    typ: vk::DescriptorType::UniformBuffer,
                    descriptor_count: 1,
                },
                vk::DescriptorPoolSize {
                    typ: vk::DescriptorType::CombinedImageSampler,
                    descriptor_count: 1,
                },
            ];
            let create_info = vk::DescriptorPoolCreateInfo {
                s_type: vk::StructureType::DescriptorPoolCreateInfo,
                p_next: ptr::null(),
                flags: vk::DESCRIPTOR_POOL_CREATE_FREE_DESCRIPTOR_SET_BIT,
                max_sets: 1,
                pool_size_count: pool_sizes.len() as u32,
                p_pool_sizes: pool_sizes.as_ptr(),
            };
            unsafe { device.create_descriptor_pool(&create_info, None)? }
        };

        let descriptor_set_layout = {
            let bindings = [
                vk::DescriptorSetLayoutBinding {
                    binding: 0,
                    descriptor_type: vk::DescriptorType::UniformBuffer,
                    descriptor_count: 1,
                    stage_flags: vk::SHADER_STAGE_VERTEX_BIT,
                    p_immutable_samplers: ptr::null(),
                },
                vk::DescriptorSetLayoutBinding {
                    binding: 1,
                    descriptor_type: vk::DescriptorType::CombinedImageSampler,
                    descriptor_count: 1,
                    stage_flags: vk::SHADER_STAGE_FRAGMENT_BIT,
                    p_immutable_samplers: ptr::null(),
                },
            ];
            let create_info = vk::DescriptorSetLayoutCreateInfo {
                s_type: vk::StructureType::DescriptorSetLayoutCreateInfo,
                p_next: ptr::null(),
                flags: vk::DescriptorSetLayoutCreateFlags::empty(),
                binding_count: bindings.len() as u32,
                p_bindings: bindings.as_ptr(),
            };
            unsafe { device.create_descriptor_set_layout(&create_info, None)? }
        };

        let descriptor_set = {
            let allocate_info = vk::DescriptorSetAllocateInfo {
                s_type: vk::StructureType::DescriptorSetAllocateInfo,
                p_next: ptr::null(),
                descriptor_pool,
                descriptor_set_count: 1,
                p_set_layouts: &descriptor_set_layout,
            };
            unsafe { device.allocate_descriptor_sets(&allocate_info)?[0] }
        };

        let render_pass = {
            let attachments = [vk::AttachmentDescription {
                flags: vk::AttachmentDescriptionFlags::empty(),
                format: surface_format.format,
                samples: vk::SAMPLE_COUNT_1_BIT,
                load_op: vk::AttachmentLoadOp::Clear,
                store_op: vk::AttachmentStoreOp::Store,
                stencil_load_op: vk::AttachmentLoadOp::DontCare,
                stencil_store_op: vk::AttachmentStoreOp::DontCare,
                initial_layout: vk::ImageLayout::Undefined,
                final_layout: vk::ImageLayout::PresentSrcKhr,
            }];
            let color_attachment_ref = vk::AttachmentReference {
                attachment: 0,
                layout: vk::ImageLayout::ColorAttachmentOptimal,
            };
            let subpasses = [vk::SubpassDescription {
                flags: vk::SubpassDescriptionFlags::empty(),
                pipeline_bind_point: vk::PipelineBindPoint::Graphics,
                input_attachment_count: 0,
                p_input_attachments: ptr::null(),
                color_attachment_count: 1,
                p_color_attachments: &color_attachment_ref,
                p_resolve_attachments: ptr::null(),
                p_depth_stencil_attachment: ptr::null(),
                preserve_attachment_count: 0,
                p_preserve_attachments: ptr::null(),
            }];
            let dependencies = [vk::SubpassDependency {
                src_subpass: vk::VK_SUBPASS_EXTERNAL,
                dst_subpass: 0,
                src_stage_mask: vk::PIPELINE_STAGE_COLOR_ATTACHMENT_OUTPUT_BIT,
                dst_stage_mask: vk::PIPELINE_STAGE_COLOR_ATTACHMENT_OUTPUT_BIT,
                src_access_mask: vk::AccessFlags::empty(),
                dst_access_mask: vk::ACCESS_COLOR_ATTACHMENT_READ_BIT
                    | vk::ACCESS_COLOR_ATTACHMENT_WRITE_BIT,
                dependency_flags: vk::DependencyFlags::empty(),
            }];
            let create_info = vk::RenderPassCreateInfo {
                s_type: vk::StructureType::RenderPassCreateInfo,
                p_next: ptr::null(),
                flags: vk::RenderPassCreateFlags::empty(),
                attachment_count: attachments.len() as u32,
                p_attachments: attachments.as_ptr(),
                subpass_count: subpasses.len() as u32,
                p_subpasses: subpasses.as_ptr(),
                dependency_count: dependencies.len() as u32,
                p_dependencies: dependencies.as_ptr(),
            };
            unsafe { device.create_render_pass(&create_info, None)? }
        };

        let framebuffers = swapchain_image_views
            .iter()
            .map(|&image_view| {
                let attachments = [image_view];
                let create_info = vk::FramebufferCreateInfo {
                    s_type: vk::StructureType::FramebufferCreateInfo,
                    p_next: ptr::null(),
                    flags: vk::FramebufferCreateFlags::empty(),
                    render_pass,
                    attachment_count: attachments.len() as u32,
                    p_attachments: attachments.as_ptr(),
                    width: surface_resolution.width,
                    height: surface_resolution.height,
                    layers: 1,
                };
                unsafe { device.create_framebuffer(&create_info, None).unwrap() }
            })
            .collect::<Vec<_>>();

        let graphics_pipeline_layout = {
            let create_info = vk::PipelineLayoutCreateInfo {
                s_type: vk::StructureType::PipelineLayoutCreateInfo,
                p_next: ptr::null(),
                flags: vk::PipelineLayoutCreateFlags::empty(),
                set_layout_count: 1,
                p_set_layouts: &descriptor_set_layout,
                push_constant_range_count: 0,
                p_push_constant_ranges: ptr::null(),
            };
            unsafe { device.create_pipeline_layout(&create_info, None)? }
        };

        let graphics_pipeline = {
            let vertex_module =
                vulkan::create_shader_module(&device, include_bytes!("shader/basic_450.glslv"))?;
            let fragment_module =
                vulkan::create_shader_module(&device, include_bytes!("shader/basic_450.glslf"))?;
            let entrypoint = CString::new("main").expect("Wrong name");
            let stages = [
                vk::PipelineShaderStageCreateInfo {
                    s_type: vk::StructureType::PipelineShaderStageCreateInfo,
                    p_next: ptr::null(),
                    flags: vk::PipelineShaderStageCreateFlags::empty(),
                    stage: vk::SHADER_STAGE_VERTEX_BIT,
                    module: vertex_module,
                    p_name: entrypoint.as_ptr(),
                    p_specialization_info: ptr::null(),
                },
                vk::PipelineShaderStageCreateInfo {
                    s_type: vk::StructureType::PipelineShaderStageCreateInfo,
                    p_next: ptr::null(),
                    flags: vk::PipelineShaderStageCreateFlags::empty(),
                    stage: vk::SHADER_STAGE_FRAGMENT_BIT,
                    module: fragment_module,
                    p_name: entrypoint.as_ptr(),
                    p_specialization_info: ptr::null(),
                },
            ];

            let vertex_binding_descriptions = [
                vk::VertexInputBindingDescription {
                    binding: 0,
                    stride: mem::align_of::<Vertex>() as u32,
                    input_rate: vk::VertexInputRate::Vertex,
                },
                vk::VertexInputBindingDescription {
                    binding: 1,
                    stride: mem::align_of::<InstanceProperties>() as u32,
                    input_rate: vk::VertexInputRate::Instance,
                },
            ];
            let vertex_attribute_descriptions = [
                // Vertex
                vk::VertexInputAttributeDescription {
                    location: 0,
                    binding: VERTEX_BUFFER_BINDING_ID,
                    format: vk::Format::R32g32Sfloat,
                    offset: offset_of!(Vertex, pos) as u32,
                },
                vk::VertexInputAttributeDescription {
                    location: 1,
                    binding: VERTEX_BUFFER_BINDING_ID,
                    format: vk::Format::R32g32Sfloat,
                    offset: offset_of!(Vertex, uv) as u32,
                },
                vk::VertexInputAttributeDescription {
                    location: 2,
                    binding: VERTEX_BUFFER_BINDING_ID,
                    format: vk::Format::R32g32b32a32Sfloat,
                    offset: offset_of!(Vertex, color) as u32,
                },
                // Instance
                vk::VertexInputAttributeDescription {
                    location: 3,
                    binding: INSTANCE_BUFFER_BINDING_ID,
                    format: vk::Format::R32g32b32a32Sfloat,
                    offset: offset_of!(InstanceProperties, src) as u32,
                },
                vk::VertexInputAttributeDescription {
                    location: 4,
                    binding: INSTANCE_BUFFER_BINDING_ID,
                    format: vk::Format::R32g32b32a32Sfloat,
                    offset: offset_of!(InstanceProperties, col1) as u32,
                },
                vk::VertexInputAttributeDescription {
                    location: 5,
                    binding: INSTANCE_BUFFER_BINDING_ID,
                    format: vk::Format::R32g32b32a32Sfloat,
                    offset: offset_of!(InstanceProperties, col2) as u32,
                },
                vk::VertexInputAttributeDescription {
                    location: 6,
                    binding: INSTANCE_BUFFER_BINDING_ID,
                    format: vk::Format::R32g32b32a32Sfloat,
                    offset: offset_of!(InstanceProperties, col3) as u32,
                },
                vk::VertexInputAttributeDescription {
                    location: 7,
                    binding: INSTANCE_BUFFER_BINDING_ID,
                    format: vk::Format::R32g32b32a32Sfloat,
                    offset: offset_of!(InstanceProperties, col4) as u32,
                },
                vk::VertexInputAttributeDescription {
                    location: 8,
                    binding: INSTANCE_BUFFER_BINDING_ID,
                    format: vk::Format::R32g32b32a32Sfloat,
                    offset: offset_of!(InstanceProperties, color) as u32,
                },
            ];
            let vertex_input_state = vk::PipelineVertexInputStateCreateInfo {
                s_type: vk::StructureType::PipelineVertexInputStateCreateInfo,
                p_next: ptr::null(),
                flags: vk::PipelineVertexInputStateCreateFlags::empty(),
                vertex_binding_description_count: vertex_binding_descriptions.len() as u32,
                p_vertex_binding_descriptions: vertex_binding_descriptions.as_ptr(),
                vertex_attribute_description_count: vertex_attribute_descriptions.len() as u32,
                p_vertex_attribute_descriptions: vertex_attribute_descriptions.as_ptr(),
            };

            let input_assembly_state = vk::PipelineInputAssemblyStateCreateInfo {
                s_type: vk::StructureType::PipelineInputAssemblyStateCreateInfo,
                p_next: ptr::null(),
                flags: vk::PipelineInputAssemblyStateCreateFlags::empty(),
                topology: vk::PrimitiveTopology::TriangleList,
                primitive_restart_enable: 0,
            };

            let viewports = [vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: surface_resolution.width as f32,
                height: surface_resolution.height as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            }];
            let scissors = [vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: surface_resolution,
            }];
            let viewport_state = vk::PipelineViewportStateCreateInfo {
                s_type: vk::StructureType::PipelineViewportStateCreateInfo,
                p_next: ptr::null(),
                flags: vk::PipelineViewportStateCreateFlags::empty(),
                viewport_count: viewports.len() as u32,
                p_viewports: viewports.as_ptr(),
                scissor_count: scissors.len() as u32,
                p_scissors: scissors.as_ptr(),
            };

            let rasterization_state = vk::PipelineRasterizationStateCreateInfo {
                s_type: vk::StructureType::PipelineRasterizationStateCreateInfo,
                p_next: ptr::null(),
                flags: vk::PipelineRasterizationStateCreateFlags::empty(),
                depth_clamp_enable: 1,
                rasterizer_discard_enable: 1,
                polygon_mode: vk::PolygonMode::Fill,
                cull_mode: vk::CULL_MODE_BACK_BIT,
                front_face: vk::FrontFace::Clockwise,
                depth_bias_enable: 0,
                depth_bias_constant_factor: 0.0,
                depth_bias_clamp: 0.0,
                depth_bias_slope_factor: 0.0,
                line_width: 1.0,
            };

            let multisample_state = vk::PipelineMultisampleStateCreateInfo {
                s_type: vk::StructureType::PipelineMultisampleStateCreateInfo,
                p_next: ptr::null(),
                flags: vk::PipelineMultisampleStateCreateFlags::empty(),
                rasterization_samples: vk::SAMPLE_COUNT_1_BIT,
                sample_shading_enable: 0,
                min_sample_shading: 0.0,
                p_sample_mask: ptr::null(),
                alpha_to_coverage_enable: 0,
                alpha_to_one_enable: 0,
            };

            let dynamic_states = [vk::DynamicState::Viewport, vk::DynamicState::Scissor];
            let dynamic_state = vk::PipelineDynamicStateCreateInfo {
                s_type: vk::StructureType::PipelineDynamicStateCreateInfo,
                p_next: ptr::null(),
                flags: vk::PipelineDynamicStateCreateFlags::empty(),
                dynamic_state_count: dynamic_states.len() as u32,
                p_dynamic_states: dynamic_states.as_ptr(),
            };

            let attachments = [vk::PipelineColorBlendAttachmentState {
                blend_enable: 0,
                src_color_blend_factor: vk::BlendFactor::ConstantAlpha,
                dst_color_blend_factor: vk::BlendFactor::ConstantAlpha,
                color_blend_op: vk::BlendOp::Add,
                src_alpha_blend_factor: vk::BlendFactor::ConstantAlpha,
                dst_alpha_blend_factor: vk::BlendFactor::ConstantAlpha,
                alpha_blend_op: vk::BlendOp::Add,
                color_write_mask: vk::COLOR_COMPONENT_R_BIT
                    | vk::COLOR_COMPONENT_G_BIT
                    | vk::COLOR_COMPONENT_G_BIT
                    | vk::COLOR_COMPONENT_A_BIT,
            }];
            let color_blend_state = vk::PipelineColorBlendStateCreateInfo {
                s_type: vk::StructureType::PipelineColorBlendStateCreateInfo,
                p_next: ptr::null(),
                flags: vk::PipelineColorBlendStateCreateFlags::empty(),
                logic_op_enable: 0,
                logic_op: vk::LogicOp::Clear,
                attachment_count: attachments.len() as u32,
                p_attachments: attachments.as_ptr(),
                blend_constants: [0.0; 4],
            };

            let create_info = vk::GraphicsPipelineCreateInfo {
                s_type: vk::StructureType::GraphicsPipelineCreateInfo,
                p_next: ptr::null(),
                flags: vk::PipelineCreateFlags::empty(),
                stage_count: stages.len() as u32,
                p_stages: stages.as_ptr(),
                p_vertex_input_state: &vertex_input_state,
                p_input_assembly_state: &input_assembly_state,
                p_tessellation_state: ptr::null(),
                p_viewport_state: &viewport_state,
                p_rasterization_state: &rasterization_state,
                p_multisample_state: &multisample_state,
                p_depth_stencil_state: ptr::null(),
                p_color_blend_state: &color_blend_state,
                p_dynamic_state: &dynamic_state,
                layout: graphics_pipeline_layout,
                render_pass,
                subpass: 0,
                base_pipeline_handle: vk::Pipeline::null(),
                base_pipeline_index: 0,
            };
            let graphics_pipeline = unsafe {
                device
                    .create_graphics_pipelines(vk::PipelineCache::null(), &[create_info], None)
                    .unwrap()[0]
            };
            unsafe {
                device.destroy_shader_module(vertex_module, None);
                device.destroy_shader_module(fragment_module, None);
            }
            graphics_pipeline
        };

        unimplemented!()
    }

    pub(crate) fn update_globals(&mut self) -> GameResult {
        self.globals_buffer.update(&[self.globals])
    }

    pub(crate) fn calculate_transform_matrix(&mut self) {
        let modelview = self
            .modelview_stack
            .last()
            .expect("Transform stack empty; should never happen");
        let mvp = self.projection * modelview;
        self.globals.mvp = mvp.into();
    }

    pub(crate) fn push_transform(&mut self, t: Matrix4) {
        self.modelview_stack.push(t);
    }

    pub(crate) fn pop_transform(&mut self) {
        if self.modelview_stack.len() > 1 {
            let _ = self.modelview_stack.pop();
        }
    }

    pub(crate) fn set_transform(&mut self, t: Matrix4) {
        assert!(
            !self.modelview_stack.is_empty(),
            "Tried to set a transform on an empty transform stack!"
        );
        let last = self
            .modelview_stack
            .last_mut()
            .expect("Transform stack empty; should never happen!");
        *last = t;
    }

    pub(crate) fn transform(&self) -> Matrix4 {
        assert!(
            !self.modelview_stack.is_empty(),
            "Tried to get a transform on an empty transform stack!"
        );
        let last = self
            .modelview_stack
            .last()
            .expect("Transform stack empty; should never happen!");
        *last
    }

    pub(crate) fn update_instance_properties(&mut self, draw_params: DrawTransform) -> GameResult {
        // This clone is cheap since draw_params is Copy
        // TODO: Clean up
        let mut new_draw_params = draw_params;
        new_draw_params.color = draw_params.color;
        let properties = new_draw_params.to_instance_properties(self.srgb);
        self.instance_buffer.update(&[properties])?;
        Ok(())
    }

    pub(crate) fn begin_frame(&mut self) {}

    pub(crate) fn end_frame(&mut self) {}

    pub(crate) fn set_blend_mode(&mut self, mode: BlendMode) -> GameResult {
        unimplemented!()
    }

    pub(crate) fn blend_mode(&self) -> BlendMode {
        unimplemented!()
    }

    /// Shortcut function to set the projection matrix to an
    /// orthographic projection based on the given `Rect`.
    ///
    /// Call `update_globals()` to apply it after calling this.
    pub(crate) fn set_projection_rect(&mut self, rect: Rect) {
        /// Creates an orthographic projection matrix.
        /// Because nalgebra gets frumple when you try to make
        /// one that is upside-down.
        fn ortho(
            left: f32,
            right: f32,
            top: f32,
            bottom: f32,
            far: f32,
            near: f32,
        ) -> [[f32; 4]; 4] {
            let c0r0 = 2.0 / (right - left);
            let c0r1 = 0.0;
            let c0r2 = 0.0;
            let c0r3 = 0.0;

            let c1r0 = 0.0;
            let c1r1 = 2.0 / (top - bottom);
            let c1r2 = 0.0;
            let c1r3 = 0.0;

            let c2r0 = 0.0;
            let c2r1 = 0.0;
            let c2r2 = -2.0 / (far - near);
            let c2r3 = 0.0;

            let c3r0 = -(right + left) / (right - left);
            let c3r1 = -(top + bottom) / (top - bottom);
            let c3r2 = -(far + near) / (far - near);
            let c3r3 = 1.0;

            // our matrices are column-major, so here we are.
            [
                [c0r0, c0r1, c0r2, c0r3],
                [c1r0, c1r1, c1r2, c1r3],
                [c2r0, c2r1, c2r2, c2r3],
                [c3r0, c3r1, c3r2, c3r3],
            ]
        }

        self.screen_rect = rect;
        self.projection = Matrix4::from(ortho(
            rect.x,
            rect.x + rect.w,
            rect.y,
            rect.y + rect.h,
            -1.0,
            1.0,
        ));
    }

    pub(crate) fn set_projection(&mut self, mat: Matrix4) {
        self.projection = mat;
    }

    pub(crate) fn projection(&self) -> Matrix4 {
        self.projection
    }

    /// Sets window mode from a WindowMode object.
    pub(crate) fn set_window_mode(&mut self, mode: WindowMode) -> GameResult {
        let window = &self.window;

        if mode.hidpi {
            self.hidpi_factor = window.get_hidpi_factor() as f32;
        } else {
            self.hidpi_factor = 1.0;
        }

        window.set_maximized(mode.maximized);

        // TODO: find out if single-dimension constraints are possible.
        let min_dimensions = if mode.min_width > 0.0 && mode.min_height > 0.0 {
            Some(dpi::LogicalSize {
                width: mode.min_width.into(),
                height: mode.min_height.into(),
            })
        } else {
            None
        };
        window.set_min_dimensions(min_dimensions);

        let max_dimensions = if mode.max_width > 0.0 && mode.max_height > 0.0 {
            Some(dpi::LogicalSize {
                width: mode.max_width.into(),
                height: mode.max_height.into(),
            })
        } else {
            None
        };
        window.set_max_dimensions(max_dimensions);

        let monitor = window.get_current_monitor();
        match mode.fullscreen_type {
            FullscreenType::Windowed => {
                window.set_fullscreen(None);
                window.set_decorations(!mode.borderless);
                window.set_inner_size(dpi::LogicalSize {
                    width: mode.width.into(),
                    height: mode.height.into(),
                });
            }
            FullscreenType::True => {
                window.set_fullscreen(Some(monitor));
                window.set_inner_size(dpi::LogicalSize {
                    width: mode.width.into(),
                    height: mode.height.into(),
                });
            }
            FullscreenType::Desktop => {
                let position = monitor.get_position();
                let dimensions = monitor.get_dimensions();
                let hidpi_factor = window.get_hidpi_factor();
                self.hidpi_factor = hidpi_factor as f32;
                window.set_fullscreen(None);
                window.set_decorations(false);
                window.set_inner_size(dimensions.to_logical(hidpi_factor));
                window.set_position(position.to_logical(hidpi_factor));
            }
        }
        Ok(())
    }

    /// Communicates changes in the viewport size between glutin and gfx.
    ///
    /// Also replaces gfx.screen_render_target and gfx.depth_view,
    /// so it may cause squirrelliness to
    /// happen with canvases or other things that touch it.
    pub(crate) fn resize_viewport(&mut self) {
        unimplemented!()
    }

    pub(crate) fn color_format(&self) -> vk::Format {
        self.color_format
    }

    pub(crate) fn depth_format(&self) -> vk::Format {
        self.depth_format
    }

    /// This is a filthy hack allow users to override hidpi
    /// scaling if they want to.  Everything that winit touches
    /// is scaled by the hidpi factor that it uses, such as monitor
    /// resolutions and mouse positions.  If you want display-independent
    /// scaling this is Good, if you want pixel-perfect scaling this
    /// is Bad.  We are currently operating on the assumption that you want
    /// pixel-perfect scaling.
    ///
    /// See <https://github.com/tomaka/winit/issues/591#issuecomment-403096230>
    /// and related issues for fuller discussion.
    pub(crate) fn hack_event_hidpi(&self, event: &winit::Event) -> winit::Event {
        event.clone()
    }

    /// Takes a coordinate in winit's Logical scale (aka everything we touch)
    /// and turns it into the equivalent in PhysicalScale, allowing us to
    /// override the DPI if necessary.
    pub(crate) fn to_physical_dpi(&self, x: f32, y: f32) -> (f32, f32) {
        let logical = dpi::LogicalPosition::new(f64::from(x), f64::from(y));
        let physical = dpi::PhysicalPosition::from_logical(logical, self.hidpi_factor.into());
        (physical.x as f32, physical.y as f32)
    }
}

impl Drop for GraphicsContext {
    fn drop(&mut self) {
        self.device.device_wait_idle().unwrap();

        for &image_view in &self.swapchain_image_views {
            unsafe {
                self.device.destroy_image_view(image_view, None);
            }
        }

        // for memory in &self.color_image_memories {
        //     unsafe {
        //         self.device.free_memory(memory, None);
        //     }
        // }
        // for image_view in &self.color_image_views {
        //     unsafe {
        //         self.device.destroy_image_view(image_view, None);
        //     }
        // }
        // for image in &self.color_images {
        //     unsafe {
        //         self.device.destroy_image(image, None);
        //     }
        // }

        // for memory in &self.depth_image_memories {
        //     unsafe {
        //         self.device.free_memory(memory, None);
        //     }
        // }
        // for image_view in &self.depth_image_views {
        //     unsafe {
        //         self.device.destroy_image_view(image_view, None);
        //     }
        // }
        // for image in &self.depth_images {
        //     unsafe {
        //         self.device.destroy_image(image, None);
        //     }
        // }

        for &semaphore in &self.image_available_semaphores {
            unsafe {
                self.device.destroy_semaphore(semaphore, None);
            }
        }
        for &semaphore in &self.rendering_complete_semaphores {
            unsafe {
                self.device.destroy_semaphore(semaphore, None);
            }
        }
        for &fence in &self.frame_fences {
            unsafe {
                self.device.destroy_fence(fence, None);
            }
        }

        for &framebuffer in &self.framebuffers {
            unsafe {
                self.device.destroy_framebuffer(framebuffer, None);
            }
        }

        unsafe {
            self.device.destroy_render_pass(self.render_pass, None);
            self.device
                .destroy_pipeline_layout(self.graphics_pipeline_layout, None);
            self.device.destroy_pipeline(self.graphics_pipeline, None);
            self.device
                .destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            self.device
                .destroy_descriptor_pool(self.descriptor_pool, None);
            self.device.destroy_sampler(self.default_sampler, None);
            self.device.destroy_command_pool(self.command_pool, None);
            self.surface_loader.destroy_surface_khr(self.surface, None);
            self.swapchain_loader
                .destroy_swapchain_khr(self.swapchain, None);
            self.device.destroy_device(None);
            self.instance.destroy_instance(None);
        }
    }
}

// /// A structure that contains graphics state.
// /// For instance,
// /// window info, DPI, rendering pipeline state, etc.
// ///
// /// As an end-user you shouldn't ever have to touch this.
// pub(crate) struct GraphicsContextGeneric<B>
// where
//     B: BackendSpec,
// {
//     // TODO: is this needed?
//     #[allow(unused)]
//     pub(crate) backend_spec: B,
//     pub(crate) window: glutin::GlWindow,
//     pub(crate) multisample_samples: u8,
//     pub(crate) device: Box<B::Device>,
//     pub(crate) factory: Box<B::Factory>,
//     pub(crate) encoder: gfx::Encoder<B::Resources, B::CommandBuffer>,
//     pub(crate) screen_render_target: gfx::handle::RawRenderTargetView<B::Resources>,
//     #[allow(dead_code)]
//     pub(crate) depth_view: gfx::handle::RawDepthStencilView<B::Resources>,

//     pub(crate) data: pipe::Data<B::Resources>,
//     pub(crate) quad_slice: gfx::Slice<B::Resources>,
//     pub(crate) quad_vertex_buffer: gfx::handle::Buffer<B::Resources, Vertex>,

//     pub(crate) default_sampler_info: texture::SamplerInfo,
//     pub(crate) samplers: SamplerCache<B>,

//     default_shader: ShaderId,
//     pub(crate) current_shader: Rc<RefCell<Option<ShaderId>>>,
//     pub(crate) shaders: Vec<Box<dyn ShaderHandle<B>>>,

//     pub(crate) glyph_brush: GlyphBrush<'static, B::Resources, B::Factory>,
// }

// impl<B> fmt::Debug for GraphicsContextGeneric<B>
// where
//     B: BackendSpec,
// {
//     fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
//         write!(formatter, "<GraphicsContext: {:p}>", self)
//     }
// }

// /// A concrete graphics context for GL rendering.
// pub(crate) type GraphicsContext = GraphicsContextGeneric<GlBackendSpec>;

// impl<B> GraphicsContextGeneric<B>
// where
//     B: BackendSpec + 'static,
// {
//     /// TODO: This is sorta redundant with BackendSpec too...?
//     pub(crate) fn new_encoder(&mut self) -> gfx::Encoder<B::Resources, B::CommandBuffer> {
//         let factory = &mut *self.factory;
//         B::encoder(factory)
//     }

//     /// Create a new GraphicsContext
//     pub(crate) fn new(
//         events_loop: &glutin::EventsLoop,
//         window_setup: &WindowSetup,
//         window_mode: WindowMode,
//         backend: B,
//         debug_id: DebugId,
//     ) -> GameResult<Self> {
//         let srgb = window_setup.srgb;
//         let color_format = if srgb {
//             gfx::format::Format(
//                 gfx::format::SurfaceType::R8_G8_B8_A8,
//                 gfx::format::ChannelType::Srgb,
//             )
//         } else {
//             gfx::format::Format(
//                 gfx::format::SurfaceType::R8_G8_B8_A8,
//                 gfx::format::ChannelType::Unorm,
//             )
//         };
//         let depth_format = gfx::format::Format(
//             gfx::format::SurfaceType::D24_S8,
//             gfx::format::ChannelType::Unorm,
//         );

//         // TODO: Alter window size based on hidpi.
//         // Can't get it from window, can we get it from
//         // monitor info...?

//         // WINDOW SETUP
//         let gl_builder = glutin::ContextBuilder::new()
//             //GlRequest::Specific(Api::OpenGl, (backend.major, backend.minor))
//             // TODO: Fix the "Latest" here.
//             .with_gl(glutin::GlRequest::Latest)
//             .with_gl_profile(glutin::GlProfile::Core)
//             .with_multisampling(window_setup.samples as u16)
//             // TODO: Better pixel format here?
//             .with_pixel_format(8, 8)
//             .with_vsync(window_setup.vsync);

//         let mut window_builder = glutin::WindowBuilder::new()
//             .with_title(window_setup.title.clone())
//             .with_transparency(window_setup.transparent)
//             .with_resizable(window_mode.resizable);

//         window_builder = if !window_setup.icon.is_empty() {
//             use winit::Icon;
//             window_builder.with_window_icon(Some(Icon::from_path(&window_setup.icon)?))
//         } else {
//             window_builder
//         };

//         let (window, device, mut factory, screen_render_target, depth_view) = backend.init(
//             window_builder,
//             gl_builder,
//             events_loop,
//             color_format,
//             depth_format,
//         );

//         // See https://docs.rs/winit/0.16.1/winit/dpi/index.html for
//         // an excellent explaination of how this works.
//         let os_hidpi_factor = window.get_hidpi_factor() as f32;
//         let hidpi_factor = if window_mode.hidpi {
//             os_hidpi_factor
//         } else {
//             1.0
//         };

//         // TODO: see winit #548 about DPI.
//         {
//             // TODO: improve.
//             // Log a bunch of OpenGL state info pulled out of winit and gfx
//             let api = window.get_api();
//             let dpi::LogicalSize {
//                 width: w,
//                 height: h,
//             } = window
//                 .get_outer_size()
//                 .ok_or_else(|| GameError::VideoError("Window doesn't exist!".to_owned()))?;
//             let dpi::LogicalSize {
//                 width: dw,
//                 height: dh,
//             } = window
//                 .get_inner_size()
//                 .ok_or_else(|| GameError::VideoError("Window doesn't exist!".to_owned()))?;
//             debug!("Window created.");
//             let (major, minor) = backend.version_tuple();
//             debug!(
//                 "  Asked for     OpenGL {}.{} Core, vsync: {}",
//                 major, minor, window_setup.vsync
//             );
//             debug!("  Actually got: OpenGL ?.? {:?}, vsync: ?", api);
//             debug!("  Window size: {}x{}, drawable size: {}x{}", w, h, dw, dh);
//             let device_info = backend.info(&device);
//             debug!("  {}", device_info);
//         }

//         // GFX SETUP
//         let mut encoder = B::encoder(&mut factory);

//         let blend_modes = [
//             BlendMode::Alpha,
//             BlendMode::Add,
//             BlendMode::Subtract,
//             BlendMode::Invert,
//             BlendMode::Multiply,
//             BlendMode::Replace,
//             BlendMode::Lighten,
//             BlendMode::Darken,
//         ];
//         let multisample_samples = window_setup.samples as u8;
//         let (shader, draw) = create_shader(
//             include_bytes!("shader/basic_150.glslv"),
//             include_bytes!("shader/basic_150.glslf"),
//             EmptyConst,
//             "Empty",
//             &mut encoder,
//             &mut factory,
//             multisample_samples,
//             Some(&blend_modes[..]),
//             color_format,
//             debug_id,
//         )?;

//         let glyph_brush = GlyphBrushBuilder::using_font_bytes(Font::default_font_bytes().to_vec())
//             .build(factory.clone());

//         let rect_inst_props = factory.create_buffer(
//             1,
//             gfx::buffer::Role::Vertex,
//             gfx::memory::Usage::Dynamic,
//             gfx::memory::Bind::SHADER_RESOURCE,
//         )?;

//         let (quad_vertex_buffer, mut quad_slice) =
//             factory.create_vertex_buffer_with_slice(&QUAD_VERTS, &QUAD_INDICES[..]);

//         quad_slice.instances = Some((1, 0));

//         let globals_buffer = factory.create_constant_buffer(1);
//         let mut samplers: SamplerCache<B> = SamplerCache::new();
//         let sampler_info =
//             texture::SamplerInfo::new(texture::FilterMethod::Bilinear, texture::WrapMode::Clamp);
//         let sampler = samplers.get_or_insert(sampler_info, &mut factory);
//         let white_image = ImageGeneric::make_raw(
//             &mut factory,
//             &sampler_info,
//             1,
//             1,
//             &[255, 255, 255, 255],
//             color_format,
//             debug_id,
//         )?;
//         let texture = white_image.texture.clone();
//         let typed_thingy = backend.raw_to_typed_shader_resource(texture);

//         let data = pipe::Data {
//             vbuf: quad_vertex_buffer.clone(),
//             tex: (typed_thingy, sampler),
//             rect_instance_properties: rect_inst_props,
//             globals: globals_buffer,
//             out: screen_render_target.clone(),
//         };

//         // Set initial uniform values
//         let left = 0.0;
//         let right = window_mode.width;
//         let top = 0.0;
//         let bottom = window_mode.height;
//         let initial_projection = Matrix4::identity(); // not the actual initial projection matrix, just placeholder
//         let initial_transform = Matrix4::identity();
//         let globals = Globals {
//             mvp_matrix: initial_projection.into(),
//         };

//         let mut gfx = Self {
//             shader_globals: globals,
//             projection: initial_projection,
//             modelview_stack: vec![initial_transform],
//             white_image,
//             screen_rect: Rect::new(left, top, right - left, bottom - top),
//             color_format,
//             depth_format,
//             srgb,
//             hidpi_factor,
//             os_hidpi_factor,

//             backend_spec: backend,
//             window,
//             multisample_samples,
//             device: Box::new(device as B::Device),
//             factory: Box::new(factory as B::Factory),
//             encoder,
//             screen_render_target,
//             depth_view,

//             data,
//             quad_slice,
//             quad_vertex_buffer,

//             default_sampler_info: sampler_info,
//             samplers,

//             default_shader: shader.shader_id(),
//             current_shader: Rc::new(RefCell::new(None)),
//             shaders: vec![draw],

//             glyph_brush,
//         };
//         gfx.set_window_mode(window_mode)?;

//         // Calculate and apply the actual initial projection matrix
//         let w = window_mode.width;
//         let h = window_mode.height;
//         let rect = Rect {
//             x: 0.0,
//             y: 0.0,
//             w,
//             h,
//         };
//         gfx.set_projection_rect(rect);
//         gfx.calculate_transform_matrix();
//         gfx.update_globals()?;
//         Ok(gfx)
//     }

//     /// Sends the current value of the graphics context's shader globals
//     /// to the graphics card.
//     pub(crate) fn update_globals(&mut self) -> GameResult {
//         self.encoder
//             .update_buffer(&self.data.globals, &[self.shader_globals], 0)?;
//         Ok(())
//     }

//     /// Recalculates the context's Model-View-Projection matrix based on
//     /// the matrices on the top of the respective stacks and the projection
//     /// matrix.
//     pub(crate) fn calculate_transform_matrix(&mut self) {
//         let modelview = self
//             .modelview_stack
//             .last()
//             .expect("Transform stack empty; should never happen");
//         let mvp = self.projection * modelview;
//         self.shader_globals.mvp_matrix = mvp.into();
//     }

//     /// Pushes a homogeneous transform matrix to the top of the transform
//     /// (model) matrix stack.
//     pub(crate) fn push_transform(&mut self, t: Matrix4) {
//         self.modelview_stack.push(t);
//     }

//     /// Pops the current transform matrix off the top of the transform
//     /// (model) matrix stack.
//     pub(crate) fn pop_transform(&mut self) {
//         if self.modelview_stack.len() > 1 {
//             let _ = self.modelview_stack.pop();
//         }
//     }

//     /// Sets the current model-view transform matrix.
//     pub(crate) fn set_transform(&mut self, t: Matrix4) {
//         assert!(
//             !self.modelview_stack.is_empty(),
//             "Tried to set a transform on an empty transform stack!"
//         );
//         let last = self
//             .modelview_stack
//             .last_mut()
//             .expect("Transform stack empty; should never happen!");
//         *last = t;
//     }

//     /// Gets a copy of the current transform matrix.
//     pub(crate) fn transform(&self) -> Matrix4 {
//         assert!(
//             !self.modelview_stack.is_empty(),
//             "Tried to get a transform on an empty transform stack!"
//         );
//         let last = self
//             .modelview_stack
//             .last()
//             .expect("Transform stack empty; should never happen!");
//         *last
//     }

//     /// Converts the given `DrawParam` into an `InstanceProperties` object and
//     /// sends it to the graphics card at the front of the instance buffer.
//     pub(crate) fn update_instance_properties(&mut self, draw_params: DrawTransform) -> GameResult {
//         // This clone is cheap since draw_params is Copy
//         // TODO: Clean up
//         let mut new_draw_params = draw_params;
//         new_draw_params.color = draw_params.color;
//         let properties = new_draw_params.to_instance_properties(self.srgb);
//         self.encoder
//             .update_buffer(&self.data.rect_instance_properties, &[properties], 0)?;
//         Ok(())
//     }

//     /// Draws with the current encoder, slice, and pixel shader. Prefer calling
//     /// this method from `Drawables` so that the pixel shader gets used
//     pub(crate) fn draw(&mut self, slice: Option<&gfx::Slice<B::Resources>>) -> GameResult {
//         let slice = slice.unwrap_or(&self.quad_slice);
//         let id = (*self.current_shader.borrow()).unwrap_or(self.default_shader);
//         let shader_handle = &self.shaders[id];

//         shader_handle.draw(&mut self.encoder, slice, &self.data)?;
//         Ok(())
//     }

//     /// Sets the blend mode of the active shader
//     pub(crate) fn set_blend_mode(&mut self, mode: BlendMode) -> GameResult {
//         let id = (*self.current_shader.borrow()).unwrap_or(self.default_shader);
//         let shader_handle = &mut self.shaders[id];
//         shader_handle.set_blend_mode(mode)
//     }

//     /// Gets the current blend mode of the active shader
//     pub(crate) fn blend_mode(&self) -> BlendMode {
//         let id = (*self.current_shader.borrow()).unwrap_or(self.default_shader);
//         let shader_handle = &self.shaders[id];
//         shader_handle.blend_mode()
//     }

//     /// Shortcut function to set the projection matrix to an
//     /// orthographic projection based on the given `Rect`.
//     ///
//     /// Call `update_globals()` to apply it after calling this.
//     pub(crate) fn set_projection_rect(&mut self, rect: Rect) {
//         /// Creates an orthographic projection matrix.
//         /// Because nalgebra gets frumple when you try to make
//         /// one that is upside-down.
//         fn ortho(
//             left: f32,
//             right: f32,
//             top: f32,
//             bottom: f32,
//             far: f32,
//             near: f32,
//         ) -> [[f32; 4]; 4] {
//             let c0r0 = 2.0 / (right - left);
//             let c0r1 = 0.0;
//             let c0r2 = 0.0;
//             let c0r3 = 0.0;

//             let c1r0 = 0.0;
//             let c1r1 = 2.0 / (top - bottom);
//             let c1r2 = 0.0;
//             let c1r3 = 0.0;

//             let c2r0 = 0.0;
//             let c2r1 = 0.0;
//             let c2r2 = -2.0 / (far - near);
//             let c2r3 = 0.0;

//             let c3r0 = -(right + left) / (right - left);
//             let c3r1 = -(top + bottom) / (top - bottom);
//             let c3r2 = -(far + near) / (far - near);
//             let c3r3 = 1.0;

//             // our matrices are column-major, so here we are.
//             [
//                 [c0r0, c0r1, c0r2, c0r3],
//                 [c1r0, c1r1, c1r2, c1r3],
//                 [c2r0, c2r1, c2r2, c2r3],
//                 [c3r0, c3r1, c3r2, c3r3],
//             ]
//         }

//         self.screen_rect = rect;
//         self.projection = Matrix4::from(ortho(
//             rect.x,
//             rect.x + rect.w,
//             rect.y,
//             rect.y + rect.h,
//             -1.0,
//             1.0,
//         ));
//     }

//     /// Sets the raw projection matrix to the given Matrix.
//     ///
//     /// Call `update_globals()` to apply after calling this.
//     pub(crate) fn set_projection(&mut self, mat: Matrix4) {
//         self.projection = mat;
//     }

//     /// Gets a copy of the raw projection matrix.
//     pub(crate) fn projection(&self) -> Matrix4 {
//         self.projection
//     }

//     /// Sets window mode from a WindowMode object.
//     pub(crate) fn set_window_mode(&mut self, mode: WindowMode) -> GameResult {
//         let window = &self.window;

//         if mode.hidpi {
//             self.hidpi_factor = window.get_hidpi_factor() as f32;
//         } else {
//             self.hidpi_factor = 1.0;
//         }

//         window.set_maximized(mode.maximized);

//         // TODO: find out if single-dimension constraints are possible.
//         let min_dimensions = if mode.min_width > 0.0 && mode.min_height > 0.0 {
//             Some(dpi::LogicalSize {
//                 width: mode.min_width.into(),
//                 height: mode.min_height.into(),
//             })
//         } else {
//             None
//         };
//         window.set_min_dimensions(min_dimensions);

//         let max_dimensions = if mode.max_width > 0.0 && mode.max_height > 0.0 {
//             Some(dpi::LogicalSize {
//                 width: mode.max_width.into(),
//                 height: mode.max_height.into(),
//             })
//         } else {
//             None
//         };
//         window.set_max_dimensions(max_dimensions);

//         let monitor = window.get_current_monitor();
//         match mode.fullscreen_type {
//             FullscreenType::Windowed => {
//                 window.set_fullscreen(None);
//                 window.set_decorations(!mode.borderless);
//                 window.set_inner_size(dpi::LogicalSize {
//                     width: mode.width.into(),
//                     height: mode.height.into(),
//                 });
//             }
//             FullscreenType::True => {
//                 window.set_fullscreen(Some(monitor));
//                 window.set_inner_size(dpi::LogicalSize {
//                     width: mode.width.into(),
//                     height: mode.height.into(),
//                 });
//             }
//             FullscreenType::Desktop => {
//                 let position = monitor.get_position();
//                 let dimensions = monitor.get_dimensions();
//                 let hidpi_factor = window.get_hidpi_factor();
//                 self.hidpi_factor = hidpi_factor as f32;
//                 window.set_fullscreen(None);
//                 window.set_decorations(false);
//                 window.set_inner_size(dimensions.to_logical(hidpi_factor));
//                 window.set_position(position.to_logical(hidpi_factor));
//             }
//         }
//         Ok(())
//     }

//     /// Communicates changes in the viewport size between glutin and gfx.
//     ///
//     /// Also replaces gfx.screen_render_target and gfx.depth_view,
//     /// so it may cause squirrelliness to
//     /// happen with canvases or other things that touch it.
//     pub(crate) fn resize_viewport(&mut self) {
//         // Basically taken from the definition of
//         // gfx_window_glutin::update_views()
//         if let Some((cv, dv)) = self.backend_spec.resize_viewport(
//             &self.screen_render_target,
//             &self.depth_view,
//             self.color_format(),
//             self.depth_format(),
//             &self.window,
//         ) {
//             self.screen_render_target = cv;
//             self.depth_view = dv;
//         }
//     }

//     /// Returns the screen color format used by the context.
//     pub(crate) fn color_format(&self) -> gfx::format::Format {
//         self.color_format
//     }

//     /// Returns the screen depth format used by the context.
//     ///
//     pub(crate) fn depth_format(&self) -> gfx::format::Format {
//         self.depth_format
//     }

//     /// Simple shortcut to check whether the context's color
//     /// format is SRGB or not.
//     pub(crate) fn is_srgb(&self) -> bool {
//         if let gfx::format::Format(_, gfx::format::ChannelType::Srgb) = self.color_format() {
//             true
//         } else {
//             false
//         }
//     }

//     /// This is a filthy hack allow users to override hidpi
//     /// scaling if they want to.  Everything that winit touches
//     /// is scaled by the hidpi factor that it uses, such as monitor
//     /// resolutions and mouse positions.  If you want display-independent
//     /// scaling this is Good, if you want pixel-perfect scaling this
//     /// is Bad.  We are currently operating on the assumption that you want
//     /// pixel-perfect scaling.
//     ///
//     /// See <https://github.com/tomaka/winit/issues/591#issuecomment-403096230>
//     /// and related issues for fuller discussion.
//     pub(crate) fn hack_event_hidpi(&self, event: &winit::Event) -> winit::Event {
//         event.clone()
//     }

//     /// Takes a coordinate in winit's Logical scale (aka everything we touch)
//     /// and turns it into the equivalent in PhysicalScale, allowing us to
//     /// override the DPI if necessary.
//     pub(crate) fn to_physical_dpi(&self, x: f32, y: f32) -> (f32, f32) {
//         let logical = dpi::LogicalPosition::new(f64::from(x), f64::from(y));
//         let physical = dpi::PhysicalPosition::from_logical(logical, self.hidpi_factor.into());
//         (physical.x as f32, physical.y as f32)
//     }
// }
