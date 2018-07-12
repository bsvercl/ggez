use std::sync::Arc;
use vulkano::buffer::{BufferUsage, CpuBufferPool, ImmutableBuffer};
use vulkano::command_buffer::pool::standard::StandardCommandPoolAlloc;
use vulkano::command_buffer::{AutoCommandBufferBuilder, CommandBuffer, DynamicState};
use vulkano::descriptor::descriptor_set::FixedSizeDescriptorSetsPool;
use vulkano::device::{Device, DeviceExtensions, Queue};
use vulkano::format::Format;
use vulkano::framebuffer::{Framebuffer, FramebufferAbstract, RenderPassAbstract, Subpass};
use vulkano::image::{Dimensions, ImageViewAccess, SwapchainImage};
use vulkano::instance::{Instance, PhysicalDevice};
use vulkano::pipeline::vertex::OneVertexOneInstanceDefinition;
use vulkano::pipeline::viewport::{Scissor, Viewport};
use vulkano::pipeline::{GraphicsPipeline, GraphicsPipelineAbstract};
use vulkano::sampler::{Filter, MipmapMode, Sampler, SamplerAddressMode};
use vulkano::swapchain::{
    self, AcquireError, PresentMode, Surface, SurfaceTransform, Swapchain, SwapchainCreationError,
};
use vulkano::sync::{self, GpuFuture};
use vulkano_win::{self, VkSurfaceBuild};
use winit;
use winit::dpi;

use conf::{FullscreenType, WindowMode, WindowSetup};
use context::DebugId;
use graphics::*;

use GameResult;

pub(crate) struct GraphicsContext {
    surface: Arc<Surface<winit::Window>>,
    pub(crate) device: Arc<Device>,
    pub(crate) queue: Arc<Queue>,

    swapchain: Arc<Swapchain<winit::Window>>,
    swapchain_images: Vec<Arc<SwapchainImage<winit::Window>>>,
    render_pass: Arc<dyn RenderPassAbstract + Send + Sync>,
    pub(crate) pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
    framebuffers: Option<Vec<Arc<dyn FramebufferAbstract + Send + Sync>>>,
    previous_frame_end: Option<Box<dyn GpuFuture>>,
    recreate_swapchain: bool,
    pub(crate) clear_color: [f32; 4],
    dimensions: [u32; 2],
    pub(crate) multisample_samples: u32,

    pub(crate) white_image: Image,
    pub(crate) projection: Matrix4,
    pub(crate) hidpi_factor: f32,
    pub(crate) mvp: Matrix4,
    pub(crate) modelview_stack: Vec<Matrix4>,
    pub(crate) screen_rect: Rect,

    pub(crate) descriptor_pool:
        FixedSizeDescriptorSetsPool<Arc<dyn GraphicsPipelineAbstract + Send + Sync>>,
    pub(crate) uniform_buffer_pool: CpuBufferPool<vs::ty::Globals>,
    instance_buffer_pool: CpuBufferPool<InstanceProperties>,
    pub(crate) quad_vertex_buffer: Arc<ImmutableBuffer<[Vertex]>>,
    pub(crate) quad_index_buffer: Arc<ImmutableBuffer<[u16]>>,
    pub(crate) default_sampler: Arc<Sampler>,

    secondary_command_buffers:
        Vec<Arc<dyn CommandBuffer<PoolAlloc = StandardCommandPoolAlloc> + Send + Sync>>,
}

impl GraphicsContext {
    pub(crate) fn new(
        events_loop: &winit::EventsLoop,
        window_setup: &WindowSetup,
        window_mode: WindowMode,
        debug_id: DebugId,
    ) -> GameResult<Self> {
        let instance = {
            let extensions = vulkano_win::required_extensions();
            Instance::new(None, &extensions, None).unwrap()
        };

        let physical_device = PhysicalDevice::enumerate(&instance).next().unwrap();
        println!(
            "Using device: {} (type: {:?})",
            physical_device.name(),
            physical_device.ty()
        );

        let window_builder = winit::WindowBuilder::new()
            .with_title(window_setup.title.clone())
            .with_transparency(window_setup.transparent)
            .with_resizable(window_mode.resizable);
        let surface = window_builder
            .build_vk_surface(events_loop, instance.clone())
            .unwrap();

        let queue_family = physical_device
            .queue_families()
            .find(|&q| q.supports_graphics() && surface.is_supported(q).unwrap_or(false))
            .unwrap();
        let (device, mut queues) = {
            let extensions = DeviceExtensions {
                khr_swapchain: true,
                ..DeviceExtensions::none()
            };
            Device::new(
                physical_device,
                physical_device.supported_features(),
                &extensions,
                [(queue_family, 0.5)].iter().cloned(),
            ).unwrap()
        };
        let queue = queues.next().unwrap();

        let dimensions;
        let (swapchain, swapchain_images) = {
            let caps = surface.capabilities(physical_device).unwrap();
            dimensions = caps.current_extent
                .unwrap_or([window_mode.width as u32, window_mode.height as u32]);
            let alpha = caps.supported_composite_alpha.iter().next().unwrap();
            // TODO: Srgb?
            let format = caps.supported_formats[0].0;

            let present_mode = if window_setup.vsync {
                PresentMode::Fifo
            } else {
                PresentMode::Immediate
            };

            Swapchain::new(
                device.clone(),
                surface.clone(),
                caps.min_image_count,
                format,
                dimensions,
                1,
                caps.supported_usage_flags,
                &queue,
                SurfaceTransform::Identity,
                alpha,
                present_mode,
                true,
                None,
            ).unwrap()
        };

        let vertex_shader = vs::Shader::load(device.clone()).unwrap();
        let fragment_shader = fs::Shader::load(device.clone()).unwrap();

        let multisample_samples = window_setup.samples as u32;

        let render_pass = Arc::new(
            single_pass_renderpass!(device.clone(),
                attachments: {
                    color: {
                        load: Clear,
                        store: Store,
                        format: swapchain.format(),
                        samples: multisample_samples,
                    }
                },
                pass: {
                    color: [color],
                    depth_stencil: {}
                }
            ).unwrap(),
        );

        let pipeline = Arc::new(
            GraphicsPipeline::start()
                .vertex_input(OneVertexOneInstanceDefinition::<Vertex, InstanceProperties>::new())
                .vertex_shader(vertex_shader.main_entry_point(), ())
                .triangle_list()
                .viewports_scissors_dynamic(1)
                .fragment_shader(fragment_shader.main_entry_point(), ())
                .blend_alpha_blending()
                .render_pass(Subpass::from(render_pass.clone(), 0).unwrap())
                .build(device.clone())
                .unwrap(),
        );

        let (quad_vertex_buffer, _) = ImmutableBuffer::from_iter(
            QUAD_VERTICES.iter().cloned(),
            BufferUsage::vertex_buffer(),
            queue.clone(),
        ).unwrap();
        let (quad_index_buffer, _) = ImmutableBuffer::from_iter(
            QUAD_INDICES.iter().cloned(),
            BufferUsage::index_buffer(),
            queue.clone(),
        ).unwrap();

        // let sampler = Sampler::simple_repeat_linear(device.clone());
        let sampler = Sampler::new(
            device.clone(),
            Filter::Linear,
            Filter::Linear,
            MipmapMode::Nearest,
            SamplerAddressMode::ClampToEdge,
            SamplerAddressMode::ClampToEdge,
            SamplerAddressMode::ClampToEdge,
            0.0,
            1.0,
            0.0,
            0.0,
        ).unwrap();

        // let white_image = Image::make_raw(
        //     queue.clone(),
        //     sampler.clone(),
        //     1,
        //     1,
        //     [255, 255, 255, 255].iter().cloned(),
        //     swapchain.format(),
        // )?;

        // TODO: Workaround this. Use the above instead.
        // I really can't figure it out.
        let white_image = {
            use image;
            use vulkano::image::ImmutableImage;

            let image = image::load_from_memory_with_format(
                include_bytes!("../../resources/white.png"),
                image::ImageFormat::PNG,
            ).unwrap()
                .to_rgba();
            let (width, height) = image.dimensions();
            let image_data = image.into_raw().clone();

            let (texture, _) = ImmutableImage::from_iter(
                image_data.iter().cloned(),
                Dimensions::Dim2d { width, height },
                swapchain.format(),
                queue.clone(),
            ).unwrap();

            Image {
                texture,
                sampler: sampler.clone(),
                width,
                height,
            }
        };

        let initial_projection = Matrix4::identity();
        let initial_transform = Matrix4::identity();
        let left = 0.0;
        let right = window_mode.width;
        let top = 0.0;
        let bottom = window_mode.height;

        let hidpi_factor = if window_mode.hidpi {
            surface.window().get_hidpi_factor() as f32
        } else {
            1.0
        };

        let mut graphics_context = GraphicsContext {
            secondary_command_buffers: vec![],
            projection: initial_projection,
            modelview_stack: vec![initial_transform],
            surface,
            queue,
            swapchain,
            swapchain_images,
            previous_frame_end: None,
            quad_vertex_buffer,
            quad_index_buffer,
            multisample_samples,
            clear_color: [0.2, 0.4, 0.6, 1.0],
            default_sampler: sampler,
            dimensions,
            framebuffers: None,
            recreate_swapchain: true,
            render_pass,
            mvp: Matrix4::identity(),
            screen_rect: Rect::new(left, top, right - left, bottom - top),
            descriptor_pool: FixedSizeDescriptorSetsPool::new(pipeline.clone(), 0),
            uniform_buffer_pool: CpuBufferPool::uniform_buffer(device.clone()),
            instance_buffer_pool: CpuBufferPool::vertex_buffer(device.clone()),
            device,
            pipeline,
            white_image,
            hidpi_factor,
        };
        graphics_context.set_window_mode(window_mode)?;

        let w = window_mode.width;
        let h = window_mode.height;
        let rect = Rect {
            x: 0.0,
            y: 0.0,
            w,
            h,
        };
        graphics_context.set_projection_rect(rect);
        graphics_context.calculate_transform_matrix();

        Ok(graphics_context)
    }

    pub(crate) fn calculate_transform_matrix(&mut self) {
        let modelview = self.modelview_stack
            .last()
            .expect("Transform stack empty; should never happen");
        self.mvp = self.projection * modelview;
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
        let last = self.modelview_stack
            .last_mut()
            .expect("Transform stack empty; should never happen");
        *last = t;
    }

    pub(crate) fn get_transform(&self) -> Matrix4 {
        assert!(
            !self.modelview_stack.is_empty(),
            "Tried to get a transform on an empty transform stack!"
        );
        let last = self.modelview_stack
            .last()
            .expect("Transform stack empty; should never happen!");
        *last
    }

    pub(crate) fn get_format(&self) -> Format {
        self.swapchain.format()
    }

    pub(crate) fn set_projection_rect(&mut self, rect: Rect) {
        self.screen_rect = rect;
        self.projection =
            Matrix4::new_orthographic(rect.x, rect.x + rect.w, rect.y, rect.y + rect.h, -1.0, 1.0);
    }

    pub(crate) fn set_projection(&mut self, t: Matrix4) {
        self.projection = t;
    }

    pub(crate) fn get_projection(&self) -> Matrix4 {
        self.projection
    }

    pub(crate) fn window(&self) -> &winit::Window {
        self.surface.window()
    }

    pub(crate) fn set_window_mode(&mut self, mode: WindowMode) -> GameResult {
        let window = self.surface.window();
        window.set_maximized(mode.maximized);

        self.hidpi_factor = if mode.hidpi {
            window.get_hidpi_factor() as f32
        } else {
            1.0
        };

        let mut min_dimensions = None;
        if mode.min_width > 0.0 && mode.min_height > 0.0 {
            min_dimensions = Some(dpi::LogicalSize {
                width: mode.min_width.into(),
                height: mode.min_height.into(),
            });
        }
        window.set_min_dimensions(min_dimensions);

        let mut max_dimensions = None;
        if mode.max_width > 0.0 && mode.max_height > 0.0 {
            max_dimensions = Some(dpi::LogicalSize {
                width: mode.max_width.into(),
                height: mode.max_height.into(),
            });
        }
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
                window.set_fullscreen(None);
                window.set_decorations(false);
                // BUGGO: Need to find and store dpi_size
                window.set_inner_size(dimensions.to_logical(1.0));
                window.set_position(position.to_logical(1.0));
            }
        }

        Ok(())
    }

    pub(crate) fn hack_event_hidpi(&self, event: &winit::Event) -> winit::Event {
        event.clone()
    }

    pub(crate) fn draw(
        &mut self,
        params: &[DrawTransform],
        vertex_buffer: Option<Arc<ImmutableBuffer<[Vertex]>>>,
        index_buffer: Option<Arc<ImmutableBuffer<[u16]>>>,
        texture: Option<Arc<dyn ImageViewAccess + Send + Sync>>,
    ) {
        let descriptor = {
            let uniform_buffer = self.uniform_buffer_pool
                .next(vs::ty::Globals {
                    mvp: self.mvp.into(),
                })
                .unwrap();

            let current_texture = texture.unwrap_or_else(|| self.white_image.texture.clone());
            self.descriptor_pool
                .next()
                .add_buffer(uniform_buffer)
                .unwrap()
                .add_sampled_image(current_texture, self.default_sampler.clone())
                .unwrap()
                .build()
                .unwrap()
        };

        let instance_buffer = {
            let instances = params
                .iter()
                // TODO: Use srgb?
                .map(|param| param.to_instance_properties(false))
                .collect::<Vec<_>>();
            Arc::new(self.instance_buffer_pool.chunk(instances).unwrap())
        };

        let vertex_buffer = vertex_buffer.unwrap_or_else(|| self.quad_vertex_buffer.clone());
        // TODO: Use this
        let index_buffer = index_buffer.unwrap_or_else(|| self.quad_index_buffer.clone());

        // let secondary_command_buffer = Arc::new(
        //     AutoCommandBufferBuilder::secondary_graphics_one_time_submit(
        //         self.device.clone(),
        //         self.queue.family(),
        //         self.pipeline.clone().subpass(),
        //     ).unwrap()
        //         .draw_indexed(
        //             self.pipeline.clone(),
        //             self.dynamic_state(),
        //             vec![vertex_buffer, instance_buffer],
        //             index_buffer,
        //             descriptor,
        //             (),
        //         )
        //         .unwrap()
        //         .build()
        //         .unwrap(),
        // );
        let secondary_command_buffer = Arc::new(
            AutoCommandBufferBuilder::secondary_graphics_one_time_submit(
                self.device.clone(),
                self.queue.family(),
                self.pipeline.clone().subpass(),
            ).unwrap()
                .draw(
                    self.pipeline.clone(),
                    self.dynamic_state(),
                    vec![vertex_buffer, instance_buffer],
                    descriptor,
                    (),
                )
                .unwrap()
                .build()
                .unwrap(),
        );
        self.secondary_command_buffers
            .push(secondary_command_buffer);
    }

    pub(crate) fn dynamic_state(&self) -> DynamicState {
        DynamicState {
            line_width: None,
            viewports: Some(vec![Viewport {
                origin: [0.0, 0.0],
                dimensions: [self.dimensions[0] as f32, self.dimensions[1] as f32],
                depth_range: 0.0..1.0,
            }]),
            scissors: Some(vec![Scissor {
                origin: [0, 0],
                dimensions: self.dimensions,
            }]),
        }
    }

    pub(crate) fn resize_viewport(&mut self) {
        self.recreate_swapchain = true;
    }

    pub(crate) fn flush(&mut self) {
        if let Some(ref mut previous_frame_end) = self.previous_frame_end {
            previous_frame_end.cleanup_finished();
        }

        if self.recreate_swapchain {
            let physical_device = self.device.physical_device();

            self.dimensions = self.surface
                .capabilities(physical_device)
                .unwrap()
                .current_extent
                .unwrap();

            let (new_swapchain, new_swapchain_images) =
                match self.swapchain.recreate_with_dimension(self.dimensions) {
                    Ok(r) => r,
                    Err(SwapchainCreationError::UnsupportedDimensions) => {
                        self.recreate_swapchain = true;
                        return;
                    }
                    Err(e) => {
                        panic!("{:?}", e);
                    }
                };
            self.swapchain = new_swapchain;
            self.swapchain_images = new_swapchain_images;

            self.framebuffers = None;
            self.recreate_swapchain = false;
        }

        if self.framebuffers.is_none() {
            let new_framebuffers = Some(
                self.swapchain_images
                    .iter()
                    .map(|image| {
                        Arc::new(
                            Framebuffer::start(self.render_pass.clone())
                                .add(image.clone())
                                .unwrap()
                                .build()
                                .unwrap(),
                        ) as _
                    })
                    .collect::<Vec<_>>(),
            );
            self.framebuffers = new_framebuffers;
        }

        let (image_num, acquire_future) =
            match swapchain::acquire_next_image(self.swapchain.clone(), None) {
                Ok(r) => r,
                Err(AcquireError::OutOfDate) => {
                    self.recreate_swapchain = true;
                    return; // We lose a frame, oh well.
                }
                Err(err) => panic!("{:?}", err),
            };

        let mut command_buffer = AutoCommandBufferBuilder::primary_one_time_submit(
            self.device.clone(),
            self.queue.family(),
        ).unwrap()
            .begin_render_pass(
                self.framebuffers.as_ref().unwrap()[image_num].clone(),
                false,
                vec![self.clear_color.into()],
            )
            .unwrap();

        for secondary_command_buffer in self.secondary_command_buffers.drain(..) {
            unsafe {
                command_buffer = command_buffer
                    .execute_commands(secondary_command_buffer)
                    .unwrap();
            }
        }

        let command_buffer = command_buffer.end_render_pass().unwrap().build().unwrap();

        let previous = self.previous_frame_end
            .take()
            .unwrap_or_else(|| Box::new(sync::now(self.device.clone())));

        let future = previous
            .join(acquire_future)
            .then_execute(self.queue.clone(), command_buffer)
            .unwrap()
            .then_swapchain_present(self.queue.clone(), self.swapchain.clone(), image_num)
            .then_signal_fence_and_flush();

        match future {
            Ok(future) => {
                self.previous_frame_end = Some(Box::new(future));
            }
            Err(sync::FlushError::OutOfDate) => {
                self.recreate_swapchain = true;
            }
            Err(e) => {
                println!("{:?}", e);
            }
        }
    }
}
