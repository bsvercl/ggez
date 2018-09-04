use std::fmt;
use std::io::Read;
use std::path;
use std::ptr;

use ash::version::{DeviceV1_0, V1_0};
use ash::vk;
use ash::Device;
use image;

use context::{Context, DebugId};
use filesystem;
use graphics::shader::*;
use graphics::*;
use {GameError, GameResult};

/// The supported formats for saving an image.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ImageFormat {
    /// .png image format (defaults to RGBA with 8-bit channels.)
    Png,
}

/// In-GPU-memory image data available to be drawn on the screen,
/// using the OpenGL backend.
///
/// Under the hood this is just an `Arc`'ed texture handle and
/// some metadata, so cloning it is fairly cheap; it doesn't
/// make another copy of the underlying image data.
pub struct Image {
    device: Device<V1_0>,
    image: vk::Image,
    memory: vk::DeviceMemory,
    pub(crate) image_view: vk::ImageView,
    blend_mode: Option<BlendMode>,
    width: u32,
    height: u32,
    debug_id: DebugId,
}

impl fmt::Debug for Image {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "TODO")
    }
}

impl Image {
    /// Load a new image from the file at the given path. The documentation for the
    /// `filesystem` module explains how the path must be specified.
    pub fn new<P>(ctx: &mut Context, path: P) -> GameResult<Self>
    where
        P: AsRef<path::Path>,
    {
        let img = {
            let mut buf = Vec::new();
            let mut reader = ctx.filesystem.open(path)?;
            let _ = reader.read_to_end(&mut buf)?;
            image::load_from_memory(&buf)?.to_rgba()
        };
        let (width, height) = img.dimensions();
        Self::from_rgba8(ctx, width, height, &img)
    }

    pub(crate) fn make_raw(
        device: &Device<V1_0>,
        pdevice_memory_props: &vk::PhysicalDeviceMemoryProperties,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
        width: u32,
        height: u32,
        rgba: &[u8],
        format: vk::Format,
        debug_id: DebugId,
    ) -> GameResult<Self> {
        if width == 0 || height == 0 {
            let msg = format!(
                "Tried to create a texture of size {}x{}, each dimension must be >0",
                width, height
            );
            return Err(GameError::ResourceLoadError(msg));
        }
        let uwidth = width as usize;
        let uheight = height as usize;
        let expected_bytes = uwidth
            .checked_mul(uheight)
            .and_then(|size| size.checked_mul(4))
            .ok_or_else(|| {
                let msg = format!(
                    "Integer overflow in Image::make_raw, image size is: {}x{}",
                    uwidth, uheight
                );
                GameError::ResourceLoadError(msg)
            })?;
        if expected_bytes != rgba.len() {
            let msg = format!(
                "Tried to create a texture of size {}x{}, but gave {} bytes of data (expected {})",
                width,
                height,
                rgba.len(),
                expected_bytes
            );
            return Err(GameError::ResourceLoadError(msg));
        }
        let buffer = vulkan::Buffer::new(
            device as _,
            pdevice_memory_props,
            rgba,
            vk::BUFFER_USAGE_TRANSFER_SRC_BIT,
            vk::MEMORY_PROPERTY_HOST_VISIBLE_BIT,
        )?;

        let image = {
            let create_info = vk::ImageCreateInfo {
                s_type: vk::StructureType::ImageCreateInfo,
                p_next: ptr::null(),
                flags: vk::ImageCreateFlags::empty(),
                image_type: vk::ImageType::Type2d,
                format,
                extent: vk::Extent3D {
                    width,
                    height,
                    depth: 1,
                },
                mip_levels: 1,
                array_layers: 1,
                samples: vk::SAMPLE_COUNT_1_BIT,
                tiling: vk::ImageTiling::Optimal,
                usage: vk::IMAGE_USAGE_TRANSFER_DST_BIT
                    | vk::IMAGE_USAGE_STORAGE_BIT
                    | vk::IMAGE_USAGE_SAMPLED_BIT,
                sharing_mode: vk::SharingMode::Exclusive,
                queue_family_index_count: 0,
                p_queue_family_indices: ptr::null(),
                initial_layout: vk::ImageLayout::Undefined,
            };
            unsafe { device.create_image(&create_info, None)? }
        };
        let requirements = device.get_image_memory_requirements(image);
        let memory = {
            let allocate_info = vk::MemoryAllocateInfo {
                s_type: vk::StructureType::MemoryAllocateInfo,
                p_next: ptr::null(),
                allocation_size: requirements.size,
                memory_type_index: vulkan::find_memory_type_index(
                    &requirements,
                    pdevice_memory_props,
                    vk::MEMORY_PROPERTY_DEVICE_LOCAL_BIT,
                )?,
            };
            unsafe { device.allocate_memory(&allocate_info, None)? }
        };
        unsafe {
            device.bind_image_memory(image, memory, 0)?;
        }
        vulkan::single_time_commands(
            device,
            queue,
            command_pool,
            &[vk::PIPELINE_STAGE_TRANSFER_BIT],
            &[],
            &[],
            |device, command_buffer| {
                let barrier = vk::ImageMemoryBarrier {
                    s_type: vk::StructureType::ImageMemoryBarrier,
                    p_next: ptr::null(),
                    src_access_mask: vk::AccessFlags::empty(),
                    dst_access_mask: vk::ACCESS_TRANSFER_WRITE_BIT,
                    old_layout: vk::ImageLayout::Undefined,
                    new_layout: vk::ImageLayout::General,
                    src_queue_family_index: vk::VK_QUEUE_FAMILY_IGNORED,
                    dst_queue_family_index: vk::VK_QUEUE_FAMILY_IGNORED,
                    image,
                    subresource_range: vk::ImageSubresourceRange {
                        aspect_mask: vk::IMAGE_ASPECT_COLOR_BIT,
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    },
                };
                unsafe {
                    device.cmd_pipeline_barrier(
                        command_buffer,
                        vk::PIPELINE_STAGE_TRANSFER_BIT,
                        vk::PIPELINE_STAGE_FRAGMENT_SHADER_BIT,
                        vk::DependencyFlags::empty(),
                        &[],
                        &[],
                        &[barrier],
                    );
                }
                let region = vk::BufferImageCopy {
                    buffer_offset: 0,
                    buffer_row_length: 0,
                    buffer_image_height: 0,
                    image_subresource: vk::ImageSubresourceLayers {
                        aspect_mask: vk::IMAGE_ASPECT_COLOR_BIT,
                        mip_level: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    },
                    image_offset: vk::Offset3D { x: 0, y: 0, z: 0 },
                    image_extent: vk::Extent3D {
                        width,
                        height,
                        depth: 1,
                    },
                };
                unsafe {
                    device.cmd_copy_buffer_to_image(
                        command_buffer,
                        buffer.handle(),
                        image,
                        vk::ImageLayout::General,
                        &[region],
                    );
                }
            },
        )?;
        let image_view = {
            let create_info = vk::ImageViewCreateInfo {
                s_type: vk::StructureType::ImageViewCreateInfo,
                p_next: ptr::null(),
                flags: vk::ImageViewCreateFlags::empty(),
                image,
                view_type: vk::ImageViewType::Type2d,
                format,
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
            unsafe { device.create_image_view(&create_info, None)? }
        };
        Ok(Image {
            device: device.clone(),
            image,
            memory,
            image_view,
            blend_mode: None,
            width,
            height,
            debug_id,
        })
    }

    /// Creates a new `Image` from the given buffer of `u8` RGBA values.
    ///
    /// The pixel layout is row-major.  That is,
    /// the first 4 `u8` values make the top-left pixel in the `Image`, the
    /// next 4 make the next pixel in the same row, and so on to the end of
    /// the row.  The next `width * 4` values make up the second row, and so
    /// on.
    pub fn from_rgba8(ctx: &mut Context, width: u32, height: u32, rgba: &[u8]) -> GameResult<Self> {
        let debug_id = DebugId::get(ctx);
        let gfx = &mut ctx.gfx_context;
        let color_format = gfx.color_format();
        Self::make_raw(
            &gfx.device,
            &gfx.pdevice_memory_props,
            gfx.command_pool,
            gfx.graphics_queue,
            width,
            height,
            rgba,
            color_format,
            debug_id,
        )
    }

    /// Dumps the `Image`'s data to a `Vec` of `u8` RGBA values.
    pub fn to_rgba8(&self, ctx: &mut Context) -> GameResult<Vec<u8>> {
        unimplemented!("Image::to_rgba8");
    }

    /// Encode the `Image` to the given file format and
    /// write it out to the given path.
    ///
    /// See the `filesystem` module docs for where exactly
    /// the file will end up.
    pub fn encode<P>(&self, ctx: &mut Context, format: ImageFormat, path: P) -> GameResult
    where
        P: AsRef<path::Path>,
    {
        use std::io;
        let data = self.to_rgba8(ctx)?;
        let f = filesystem::create(ctx, path)?;
        let writer = &mut io::BufWriter::new(f);
        let color_format = image::ColorType::RGBA(8);
        match format {
            ImageFormat::Png => image::png::PNGEncoder::new(writer)
                .encode(
                    &data,
                    u32::from(self.width),
                    u32::from(self.height),
                    color_format,
                )
                .map_err(|e| e.into()),
        }
    }

    /// Return the width of the image.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Return the height of the image.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Get the filter mode for the image.
    pub fn filter(&self) -> FilterMode {
        unimplemented!("Image::filter");
    }

    /// Set the filter mode for the image.
    pub fn set_filter(&mut self, mode: FilterMode) {
        // unimplemented!("Image::set_filter");
    }

    /// Returns the dimensions of the image.
    pub fn dimensions(&self) -> Rect {
        Rect::new(0.0, 0.0, self.width() as f32, self.height() as f32)
    }

    /// Gets the `Image`'s `WrapMode` along the X and Y axes.
    pub fn wrap(&self) -> (WrapMode, WrapMode) {
        unimplemented!("Image::wrap");
    }

    /// Sets the `Image`'s `WrapMode` along the X and Y axes.
    pub fn set_wrap(&mut self, wrap_x: WrapMode, wrap_y: WrapMode) {
        unimplemented!("Image::set_wrap");
    }
}

impl Drop for Image {
    fn drop(&mut self) {
        unsafe {
            self.device.free_memory(self.memory, None);
            self.device.destroy_image(self.image, None);
            self.device.destroy_image_view(self.image_view, None);
        }
    }
}

impl Drawable for Image {
    fn draw<D>(&self, ctx: &mut Context, param: D) -> GameResult
    where
        D: Into<DrawTransform>,
    {
        Ok(())
    }

    fn set_blend_mode(&mut self, mode: Option<BlendMode>) {
        self.blend_mode = mode;
    }

    fn blend_mode(&self) -> Option<BlendMode> {
        self.blend_mode
    }
}

// impl<B> ImageGeneric<B>
// where
//     B: BackendSpec,
// {
//     /// A helper function that just takes a factory directly so we can make an image
//     /// without needing the full context object, so we can create an Image while still
//     /// creating the GraphicsContext.
//     pub(crate) fn make_raw(
//         factory: &mut <B as BackendSpec>::Factory,
//         sampler_info: &texture::SamplerInfo,
//         width: u16,
//         height: u16,
//         rgba: &[u8],
//         color_format: gfx::format::Format,
//         debug_id: DebugId,
//     ) -> GameResult<Self> {
//         if width == 0 || height == 0 {
//             let msg = format!(
//                 "Tried to create a texture of size {}x{}, each dimension must
//                 be >0",
//                 width, height
//             );
//             return Err(GameError::ResourceLoadError(msg));
//         }
//         // Check for overflow, which might happen on 32-bit systems
//         let uwidth = width as usize;
//         let uheight = height as usize;
//         let expected_bytes = uwidth
//             .checked_mul(uheight)
//             .and_then(|size| size.checked_mul(4))
//             .ok_or_else(|| {
//                 let msg = format!(
//                     "Integer overflow in Image::make_raw, image size: {} {}",
//                     uwidth, uheight
//                 );
//                 GameError::ResourceLoadError(msg)
//             })?;
//         if expected_bytes != rgba.len() {
//             let msg = format!(
//                 "Tried to create a texture of size {}x{}, but gave {} bytes of data (expected {})",
//                 width,
//                 height,
//                 rgba.len(),
//                 expected_bytes
//             );
//             return Err(GameError::ResourceLoadError(msg));
//         }
//         let kind = gfx::texture::Kind::D2(width, height, gfx::texture::AaMode::Single);
//         use gfx::memory::Bind;
//         let gfx::format::Format(surface_format, channel_type) = color_format;
//         let texinfo = gfx::texture::Info {
//             kind,
//             levels: 1,
//             format: surface_format,
//             bind: Bind::SHADER_RESOURCE | Bind::RENDER_TARGET | Bind::TRANSFER_SRC,
//             usage: gfx::memory::Usage::Data,
//         };
//         let raw_tex = factory.create_texture_raw(
//             texinfo,
//             Some(channel_type),
//             Some((&[rgba], gfx::texture::Mipmap::Provided)),
//         )?;
//         let resource_desc = gfx::texture::ResourceDesc {
//             channel: channel_type,
//             layer: None,
//             min: 0,
//             max: raw_tex.get_info().levels - 1,
//             swizzle: gfx::format::Swizzle::new(),
//         };
//         let raw_view = factory.view_texture_as_shader_resource_raw(&raw_tex, resource_desc)?;
//         // gfx::memory::Typed is UNDOCUMENTED, aiee!
//         // However there doesn't seem to be an official way to turn a raw tex/view into a typed
//         // one; this API oversight would probably get fixed, except gfx is moving to a new
//         // API model.  So, that also fortunately means that undocumented features like this
//         // probably won't go away on pre-ll gfx...
//         // let tex = gfx::memory::Typed::new(raw_tex);
//         // let view = gfx::memory::Typed::new(raw_view);
//         Ok(Self {
//             texture: raw_view,
//             texture_handle: raw_tex,
//             sampler_info: *sampler_info,
//             blend_mode: None,
//             width,
//             height,
//             debug_id,
//         })
//     }
// }

// /// In-GPU-memory image data available to be drawn on the screen,
// /// using the OpenGL backend.
// ///
// /// Under the hood this is just an `Arc`'ed texture handle and
// /// some metadata, so cloning it is fairly cheap; it doesn't
// /// make another copy of the underlying image data.
// pub type Image = ImageGeneric<GlBackendSpec>;

// impl Image {
//     /* TODO: Needs generic Context to work.
//      */

//     /// Load a new image from the file at the given path. The documentation for the
//     /// `filesystem` module explains how the path must be specified.
//     pub fn new<P: AsRef<path::Path>>(context: &mut Context, path: P) -> GameResult<Self> {
//         let img = {
//             let mut buf = Vec::new();
//             let mut reader = context.filesystem.open(path)?;
//             let _ = reader.read_to_end(&mut buf)?;
//             image::load_from_memory(&buf)?.to_rgba()
//         };
//         let (width, height) = img.dimensions();
//         Self::from_rgba8(context, width as u16, height as u16, &img)
//     }

//     /// Creates a new `Image` from the given buffer of `u8` RGBA values.
//     ///
//     /// The pixel layout is row-major.  That is,
//     /// the first 4 `u8` values make the top-left pixel in the `Image`, the
//     /// next 4 make the next pixel in the same row, and so on to the end of
//     /// the row.  The next `width * 4` values make up the second row, and so
//     /// on.
//     pub fn from_rgba8(
//         context: &mut Context,
//         width: u16,
//         height: u16,
//         rgba: &[u8],
//     ) -> GameResult<Self> {
//         let debug_id = DebugId::get(context);
//         let color_format = context.gfx_context.color_format();
//         Self::make_raw(
//             &mut *context.gfx_context.factory,
//             &context.gfx_context.default_sampler_info,
//             width,
//             height,
//             rgba,
//             color_format,
//             debug_id,
//         )
//     }

//     /// Dumps the `Image`'s data to a `Vec` of `u8` RGBA values.
//     pub fn to_rgba8(&self, ctx: &mut Context) -> GameResult<Vec<u8>> {
//         use gfx::memory::Typed;
//         use gfx::traits::FactoryExt;

//         let gfx = &mut ctx.gfx_context;
//         let w = self.width;
//         let h = self.height;

//         // Note: In the GFX example, the download buffer is created ahead of time
//         // and updated on screen resize events. This may be preferable, but then
//         // the buffer also needs to be updated when we switch to/from a canvas.
//         // Unsure of the performance impact of creating this as it is needed.
//         // Probably okay for now though, since this probably won't be a super
//         // common operation.
//         let dl_buffer = gfx
//             .factory
//             .create_download_buffer::<[u8; 4]>(w as usize * h as usize)?;

//         let mut local_encoder = gfx.new_encoder();

//         local_encoder.copy_texture_to_buffer_raw(
//             &self.texture_handle,
//             None,
//             gfx::texture::RawImageInfo {
//                 xoffset: 0,
//                 yoffset: 0,
//                 zoffset: 0,
//                 width: w as u16,
//                 height: h as u16,
//                 depth: 0,
//                 format: gfx.color_format(),
//                 mipmap: 0,
//             },
//             dl_buffer.raw(),
//             0,
//         )?;
//         local_encoder.flush(&mut *gfx.device);

//         let reader = gfx.factory.read_mapping(&dl_buffer)?;

//         // intermediary buffer to avoid casting
//         // and also to reverse the order in which we pass the rows
//         // so the screenshot isn't upside-down
//         let mut data = Vec::with_capacity(self.width as usize * self.height as usize * 4);
//         for row in reader.chunks(w as usize).rev() {
//             for pixel in row.iter() {
//                 data.extend(pixel);
//             }
//         }
//         Ok(data)
//     }

//     /// Encode the `Image` to the given file format and
//     /// write it out to the given path.
//     ///
//     /// See the `filesystem` module docs for where exactly
//     /// the file will end up.
//     pub fn encode<P: AsRef<path::Path>>(
//         &self,
//         ctx: &mut Context,
//         format: ImageFormat,
//         path: P,
//     ) -> GameResult {
//         use std::io;
//         let data = self.to_rgba8(ctx)?;
//         let f = filesystem::create(ctx, path)?;
//         let writer = &mut io::BufWriter::new(f);
//         let color_format = image::ColorType::RGBA(8);
//         match format {
//             ImageFormat::Png => image::png::PNGEncoder::new(writer)
//                 .encode(
//                     &data,
//                     u32::from(self.width),
//                     u32::from(self.height),
//                     color_format,
//                 )
//                 .map_err(|e| e.into()),
//         }
//     }

//     /* TODO: Needs generic context

//     /// A little helper function that creates a new Image that is just
//     /// a solid square of the given size and color.  Mainly useful for
//     /// debugging.
//     pub fn solid(context: &mut Context, size: u16, color: Color) -> GameResult<Self> {
//         // let pixel_array: [u8; 4] = color.into();
//         let (r, g, b, a) = color.into();
//         let pixel_array: [u8; 4] = [r, g, b, a];
//         let size_squared = size as usize * size as usize;
//         let mut buffer = Vec::with_capacity(size_squared);
//         for _i in 0..size_squared {
//             buffer.extend(&pixel_array[..]);
//         }
//         Image::from_rgba8(context, size, size, &buffer)
//     }
//     */

//     /// Return the width of the image.
//     pub fn width(&self) -> u16 {
//         self.width
//     }

//     /// Return the height of the image.
//     pub fn height(&self) -> u16 {
//         self.height
//     }

//     /// Get the filter mode for the image.
//     pub fn filter(&self) -> FilterMode {
//         self.sampler_info.filter.into()
//     }

//     /// Set the filter mode for the image.
//     pub fn set_filter(&mut self, mode: FilterMode) {
//         self.sampler_info.filter = mode.into();
//     }

//     /// Returns the dimensions of the image.
//     pub fn dimensions(&self) -> Rect {
//         Rect::new(0.0, 0.0, self.width() as f32, self.height() as f32)
//     }

//     /// Gets the `Image`'s `WrapMode` along the X and Y axes.
//     pub fn wrap(&self) -> (WrapMode, WrapMode) {
//         (self.sampler_info.wrap_mode.0, self.sampler_info.wrap_mode.1)
//     }

//     /// Sets the `Image`'s `WrapMode` along the X and Y axes.
//     pub fn set_wrap(&mut self, wrap_x: WrapMode, wrap_y: WrapMode) {
//         self.sampler_info.wrap_mode.0 = wrap_x;
//         self.sampler_info.wrap_mode.1 = wrap_y;
//     }
// }

// impl fmt::Debug for Image {
//     fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
//         write!(
//             f,
//             "<Image: {}x{}, {:p}, texture address {:p}, sampler: {:?}>",
//             self.width(),
//             self.height(),
//             self,
//             &self.texture,
//             &self.sampler_info
//         )
//     }
// }

// impl Drawable for Image {
//     fn draw<D>(&self, ctx: &mut Context, param: D) -> GameResult
//     where
//         D: Into<DrawTransform>,
//     {
//         let param = param.into();
//         self.debug_id.assert(ctx);

//         // println!("Matrix: {:#?}", param.matrix);
//         let gfx = &mut ctx.gfx_context;
//         let src_width = param.src.w;
//         let src_height = param.src.h;
//         // We have to mess with the scale to make everything
//         // be its-unit-size-in-pixels.
//         // BUGGO: Based on previous ggez code we need
//         // to have param.scale in this math but there's
//         // no way we can get it...
//         // ...or do we, 'cause we do param.mul() afterwards?
//         // but it doesn't seem to have the same effect on
//         // offset, so.
//         use nalgebra;
//         let real_scale = nalgebra::Vector3::new(
//             src_width * f32::from(self.width),
//             src_height * f32::from(self.height),
//             1.0,
//         );
//         let new_param = param.mul(Matrix4::new_nonuniform_scaling(&real_scale));
//         // let new_param = param;

//         gfx.update_instance_properties(new_param)?;
//         let sampler = gfx
//             .samplers
//             .get_or_insert(self.sampler_info, gfx.factory.as_mut());
//         gfx.data.vbuf = gfx.quad_vertex_buffer.clone();
//         let typed_thingy = gfx
//             .backend_spec
//             .raw_to_typed_shader_resource(self.texture.clone());
//         gfx.data.tex = (typed_thingy, sampler);
//         let previous_mode: Option<BlendMode> = if let Some(mode) = self.blend_mode {
//             let current_mode = gfx.blend_mode();
//             if current_mode != mode {
//                 gfx.set_blend_mode(mode)?;
//                 Some(current_mode)
//             } else {
//                 None
//             }
//         } else {
//             None
//         };

//         gfx.draw(None)?;
//         if let Some(mode) = previous_mode {
//             gfx.set_blend_mode(mode)?;
//         }
//         Ok(())
//     }

//     fn set_blend_mode(&mut self, mode: Option<BlendMode>) {
//         self.blend_mode = mode;
//     }

//     fn blend_mode(&self) -> Option<BlendMode> {
//         self.blend_mode
//     }
// }

#[cfg(test)]
mod tests {
    use super::*;
    use ContextBuilder;
    #[test]
    fn test_invalid_image_size() {
        let (ctx, _) = &mut ContextBuilder::new("unittest", "unittest").build().unwrap();
        let _i = assert!(Image::from_rgba8(ctx, 0, 0, &vec![]).is_err());
        let _i = assert!(Image::from_rgba8(ctx, 3432, 432, &vec![]).is_err());
        let _i = Image::from_rgba8(ctx, 2, 2, &vec![99; 16]).unwrap();
    }
}
