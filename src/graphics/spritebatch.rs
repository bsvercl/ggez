//! A `SpriteBatch` is a way to efficiently draw a large
//! number of copies of the same image, or part of the same image.  It's
//! useful for implementing tiled maps, spritesheets, particles, and
//! other such things.
//!
//! Essentially this uses a technique called "instancing" to queue up
//! a large amount of location/position data in a buffer, then feed it
//! to the graphics card all in one go.

// use super::shader::BlendMode;
use super::types::FilterMode;
use context::Context;
use error;
use graphics::{self, DrawTransform};
use GameResult;

/// A `SpriteBatch` draws a number of copies of the same image, using a single draw call.
///
/// This is generally faster than drawing the same sprite with many invocations of `draw()`,
/// though it has a bit of overhead to set up the batch.  This makes it run very slowly
/// in Debug mode because it spends a lot of time on array bounds checking and
/// un-optimized math; you need to build with optimizations enabled to really get the
/// speed boost.
#[derive(Debug, Clone)]
pub struct SpriteBatch {
    image: graphics::Image,
    sprites: Vec<graphics::DrawParam>,
    // blend_mode: Option<BlendMode>,
}

/// An index of a particular sprite in a `SpriteBatch`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SpriteIdx(usize);

impl SpriteBatch {
    /// Creates a new `SpriteBatch`, drawing with the given image.
    ///
    /// Takes ownership of the `Image`, but cloning an `Image` is
    /// cheap since they have an internal `Arc` containing the actual
    /// image data.
    pub fn new(image: graphics::Image) -> Self {
        Self {
            image,
            sprites: vec![],
            // blend_mode: None,
        }
    }

    /// Adds a new sprite to the sprite batch.
    ///
    /// Returns a handle with which type to modify the sprite using `set()`
    ///
    /// TODO: Into<DrawParam> and such
    pub fn add(&mut self, param: graphics::DrawParam) -> SpriteIdx {
        self.sprites.push(param);
        SpriteIdx(self.sprites.len() - 1)
    }

    /// Alters a sprite in the batch to use the given draw params
    pub fn set(&mut self, handle: SpriteIdx, param: graphics::DrawParam) -> GameResult {
        if handle.0 < self.sprites.len() {
            self.sprites[handle.0] = param;
            Ok(())
        } else {
            Err(error::GameError::RenderError(String::from(
                "Provided index is out of bounds.",
            )))
        }
    }

    /// Removes all data from the sprite batch.
    pub fn clear(&mut self) {
        self.sprites.clear();
    }

    /// Unwraps and returns the contained `Image`
    pub fn into_inner(self) -> graphics::Image {
        self.image
    }

    /// Replaces the contained `Image`, returning the old one.
    pub fn set_image(&mut self, image: graphics::Image) -> graphics::Image {
        use std::mem;
        mem::replace(&mut self.image, image)
    }
}

impl graphics::Drawable for SpriteBatch {
    fn draw<D>(&self, ctx: &mut Context, param: D) -> GameResult
    where
        D: Into<DrawTransform>,
    {
        let param = param.into();
        let gfx = &mut ctx.gfx_context;
        use rayon::prelude::*;
        // TODO: This doesn't have to happen every frame
        let params = self.sprites
            .par_iter()
            .map(|param| {
                // Copy old params
                let mut new_param = *param;
                let src_width = param.src.w;
                let src_height = param.src.h;
                let real_scale = graphics::Vector2::new(
                    src_width * param.scale.x * self.image.width() as f32,
                    src_height * param.scale.y * self.image.height() as f32,
                );
                new_param.scale = real_scale;
                new_param.into()
            })
            .collect::<Vec<_>>();
        let current_transform = gfx.get_transform();
        gfx.push_transform(param.matrix * current_transform);
        gfx.calculate_transform_matrix();
        gfx.draw(&params, None, None, Some(self.image.texture.clone()), None);
        gfx.pop_transform();
        gfx.calculate_transform_matrix();
        Ok(())
    }
    // fn set_blend_mode(&mut self, mode: Option<BlendMode>) {
    //     self.blend_mode = mode;
    // }
    // fn get_blend_mode(&self) -> Option<BlendMode> {
    //     self.blend_mode
    // }
}
