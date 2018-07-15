use std::io::Read;
use std::path;
use std::sync::Arc;

use image;
use vulkano::buffer::BufferUsage;
use vulkano::command_buffer::{AutoCommandBufferBuilder, CommandBuffer};
use vulkano::device::Queue;
use vulkano::format;
use vulkano::image::{Dimensions, StorageImage};
use vulkano::sampler::Sampler;
use vulkano::sync::GpuFuture;

use context::{Context, DebugId};
use filesystem;
// use graphics::shader::*;
use graphics::*;
use GameError;
use GameResult;

/// Generic in-GPU-memory image data available to be drawn on the screen.
#[derive(Clone)]
pub struct Image {
    // TODO: Rename to shader_view or such.
    pub(crate) texture: Arc<StorageImage<format::R8G8B8A8Srgb>>,
    pub(crate) sampler: Arc<Sampler>,
    // pub(crate) blend_mode: Option<BlendMode>,
    pub(crate) width: u32,
    pub(crate) height: u32,
    // pub(crate) debug_id: DebugId,
}

/// The supported formats for saving an image.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ImageFormat {
    /// .png image format (defaults to RGBA with 8-bit channels.)
    Png,
}

impl Image {
    /// Load a new image from the file at the given path.
    pub fn new<P: AsRef<path::Path>>(context: &mut Context, path: P) -> GameResult<Self> {
        let img = {
            let mut buf = Vec::new();
            let mut reader = context.filesystem.open(path)?;
            let _ = reader.read_to_end(&mut buf)?;
            image::load_from_memory(&buf)?.to_rgba()
        };
        let (width, height) = img.dimensions();
        Self::from_rgba8(context, width, height, &img)
    }

    /// Creates a new `Image` from the given buffer of `u8` RGBA values.
    pub fn from_rgba8(
        context: &mut Context,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> GameResult<Self> {
        let debug_id = DebugId::get(context);
        let gfx = &context.gfx_context;
        Self::make_raw(
            &gfx.queue.clone(),
            gfx.default_sampler.clone(),
            width,
            height,
            rgba,
        )
    }

    /// Dumps the `Image`'s data to a `Vec` of `u8` RGBA values.
    pub fn to_rgba8(&self, ctx: &mut Context) -> GameResult<Vec<u8>> {
        // TODO: Use a different type when it's possible to read from an ImmutableBuffer
        use vulkano::buffer::CpuAccessibleBuffer;

        let gfx = &mut ctx.gfx_context;

        let buffer = CpuAccessibleBuffer::from_iter(
            gfx.device.clone(),
            BufferUsage::transfer_destination(),
            (0..self.width as usize * self.height as usize * 4).map(|_| 0u8),
        ).unwrap();

        let cb = AutoCommandBufferBuilder::primary(gfx.device.clone(), gfx.queue.family())
            .unwrap()
            .copy_image_to_buffer(self.texture.clone(), buffer.clone())
            .unwrap()
            .build()
            .unwrap();
        let _ = cb.execute(gfx.queue.clone())
            .unwrap()
            .then_signal_fence_and_flush()
            .unwrap()
            .wait(None)
            .unwrap();

        let content = buffer.read().unwrap();
        Ok(content.to_vec())
    }

    pub(crate) fn make_raw(
        queue: &Arc<Queue>,
        sampler: Arc<Sampler>,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> GameResult<Self> {
        use std::iter;
        use vulkano::buffer::ImmutableBuffer;
        use vulkano::image::ImageUsage;

        if width == 0 || height == 0 {
            let msg = format!(
                "Tried to create a texture of size {}x{}, each dimension must
                be >0",
                width, height
            );
            return Err(GameError::ResourceLoadError(msg));
        }
        // TODO: Check for overflow on 32-bit systems here
        let expected_bytes = width as usize * height as usize * 4;
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

        let (buffer, buffer_future) = ImmutableBuffer::from_iter(
            rgba.iter().cloned(),
            BufferUsage::transfer_source(),
            queue.clone(),
        ).unwrap();

        let texture = StorageImage::with_usage(
            queue.device().clone(),
            Dimensions::Dim2d { width, height },
            format::R8G8B8A8Srgb,
            ImageUsage {
                transfer_source: true,
                transfer_destination: true,
                sampled: true,
                ..ImageUsage::none()
            },
            iter::empty(),
        ).unwrap();

        let cb = AutoCommandBufferBuilder::primary(queue.device().clone(), queue.family())
            .unwrap()
            .copy_buffer_to_image(buffer, texture.clone())
            .unwrap()
            .build()
            .unwrap();
        let _ = buffer_future
            .then_execute(queue.clone(), cb)
            .unwrap()
            .then_signal_fence_and_flush()
            .unwrap()
            .wait(None)
            .unwrap();

        Ok(Self {
            texture,
            sampler,
            width,
            height,
            // debug_id,
        })
    }

    /// Encode the `Image` to the given file format and
    /// write it out to the given path.
    ///
    /// See the `filesystem` module docs for where exactly
    /// the file will end up.
    pub fn encode<P: AsRef<path::Path>>(
        &self,
        ctx: &mut Context,
        format: ImageFormat,
        path: P,
    ) -> GameResult {
        use std::io;
        let data = self.to_rgba8(ctx)?;
        let f = filesystem::create(ctx, path)?;
        let writer = &mut io::BufWriter::new(f);
        let color_format = image::ColorType::RGBA(8);
        match format {
            ImageFormat::Png => image::png::PNGEncoder::new(writer)
                .encode(&data, self.width, self.height, color_format)
                .map_err(|e| e.into()),
        }
    }

    /// A little helper function that creates a new Image that is just
    /// a solid square of the given size and color.  Mainly useful for
    /// debugging.
    pub fn solid(ctx: &mut Context, size: u32, color: Color) -> GameResult<Self> {
        let (r, g, b, a) = color.into();
        let pixel_array: [u8; 4] = [r, g, b, a];
        let size_squared = size as usize * size as usize;
        let mut buffer = Vec::with_capacity(size_squared);
        for _ in 0..size_squared {
            buffer.extend(&pixel_array[..]);
        }
        Self::from_rgba8(ctx, size, size, &buffer)
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
    // pub fn get_filter(&self) -> FilterMode {
    //     self.sampler_info.filter.into()
    // }

    /// Set the filter mode for the image.
    // pub fn set_filter(&mut self, mode: FilterMode) {
    //     self.sampler_info.filter = mode.into();
    // }

    /// Returns the dimensions of the image.
    pub fn get_dimensions(&self) -> Rect {
        Rect::new(0.0, 0.0, self.width() as f32, self.height() as f32)
    }
}

impl fmt::Debug for Image {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "<Image: {}x{}, {:p}, texture address {:p}, sampler: {:?}>",
            self.width(),
            self.height(),
            self,
            &self.texture,
            &self.sampler
        )
    }
}

impl Drawable for Image {
    fn draw<D>(&self, ctx: &mut Context, param: D) -> GameResult
    where
        D: Into<DrawTransform>,
    {
        let param = param.into();
        // self.debug_id.assert(ctx);

        // println!("Matrix: {:#?}", param.matrix);
        let gfx = &mut ctx.gfx_context;
        let src_width = param.src.w;
        let src_height = param.src.h;
        // We have to mess with the scale to make everything
        // be its-unit-size-in-pixels.
        use nalgebra;
        let real_scale = nalgebra::Vector3::new(
            src_width * self.width as f32,
            src_height * self.height as f32,
            1.0,
        );
        let new_param = param * Matrix4::new_nonuniform_scaling(&real_scale);

        gfx.draw(&[new_param], None, None, Some(self.texture.clone()), None);

        Ok(())
    }

    // fn set_blend_mode(&mut self, mode: Option<BlendMode>) {
    //     self.blend_mode = mode;
    // }

    // fn get_blend_mode(&self) -> Option<BlendMode> {
    //     self.blend_mode
    // }
}

#[cfg(test)]
mod tests {
    use super::*;
    // We need to set up separate unit tests for CI vs non-CI environments; see issue #234
    // #[test]
    #[allow(dead_code)]
    fn test_invalid_image_size() {
        // let c = conf::Conf::new();
        // let (ctx, _) = &mut Context::load_from_conf("unittest", "unittest", c).unwrap();
        // let _i = assert!(Image::from_rgba8(ctx, 0, 0, &vec![]).is_err());
        // let _i = assert!(Image::from_rgba8(ctx, 3432, 432, &vec![]).is_err());
        // let _i = Image::from_rgba8(ctx, 2, 2, &vec![99; 16]).unwrap();
    }
}
