use std::io::Read;
use std::path;
use std::sync::Arc;

use image;
use vulkano::buffer::BufferAccess;
use vulkano::command_buffer::{AutoCommandBufferBuilder, DrawIndirectCommand};
use vulkano::device::Queue;
use vulkano::format::{AcceptsPixels, Format, FormatDesc};
use vulkano::image::{Dimensions, ImageAccess, ImageViewAccess, ImmutableImage};
use vulkano::pipeline::GraphicsPipelineAbstract;
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
    pub(crate) texture: Arc<dyn ImageViewAccess + Send + Sync>,
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
    /* TODO: Needs generic Context to work.
     */

    /// Load a new image from the file at the given path.
    pub fn new<P: AsRef<path::Path>>(context: &mut Context, path: P) -> GameResult<Self> {
        let img = {
            let mut buf = Vec::new();
            let mut reader = context.filesystem.open(path)?;
            let _ = reader.read_to_end(&mut buf)?;
            image::load_from_memory(&buf)?.to_rgba()
        };
        let (width, height) = img.dimensions();
        Self::from_rgba8(context, width, height, img.into_raw().iter().cloned())
    }

    /// Creates a new `Image` from the given buffer of `u8` RGBA values.
    pub fn from_rgba8<P, I>(
        context: &mut Context,
        width: u32,
        height: u32,
        rgba: I,
    ) -> GameResult<Self>
    where
        P: Send + Sync + Clone + 'static,
        I: ExactSizeIterator<Item = P>,
        Format: AcceptsPixels<P>,
    {
        let debug_id = DebugId::get(context);
        let gfx = &context.gfx_context;
        Self::make_raw(
            &gfx.queue.clone(),
            gfx.default_sampler.clone(),
            width,
            height,
            rgba,
            gfx.get_format(),
            // debug_id,
        )
    }

    /// Dumps the `Image`'s data to a `Vec` of `u8` RGBA values.
    pub fn to_rgba8(&self, ctx: &mut Context) -> GameResult<Vec<u8>> {
        unimplemented!()
    }

    pub(crate) fn make_raw<P, F, I>(
        queue: &Arc<Queue>,
        sampler: Arc<Sampler>,
        width: u32,
        height: u32,
        rgba: I,
        format: F,
        // debug_id: DebugId,
    ) -> GameResult<Self>
    where
        P: Send + Sync + Clone + 'static,
        F: FormatDesc + AcceptsPixels<P> + 'static + Send + Sync,
        I: ExactSizeIterator<Item = P>,
        Format: AcceptsPixels<P>,
    {
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

        let (texture, _future) = ImmutableImage::from_iter(
            rgba,
            Dimensions::Dim2d { width, height },
            format,
            queue.clone(),
        ).unwrap();

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

    /* TODO: Needs generic context

    /// A little helper function that creates a new Image that is just
    /// a solid square of the given size and color.  Mainly useful for
    /// debugging.
    pub fn solid(context: &mut Context, size: u16, color: Color) -> GameResult<Self> {
        // let pixel_array: [u8; 4] = color.into();
        let (r, g, b, a) = color.into();
        let pixel_array: [u8; 4] = [r, g, b, a];
        let size_squared = size as usize * size as usize;
        let mut buffer = Vec::with_capacity(size_squared);
        for _i in 0..size_squared {
            buffer.extend(&pixel_array[..]);
        }
        Image::from_rgba8(context, size, size, &buffer)
    }
    */

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

        gfx.draw(&[new_param], None, None, Some(self.texture.clone()));

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
