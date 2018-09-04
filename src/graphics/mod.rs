//! The `graphics` module performs the actual drawing of images, text, and other
//! objects with the `Drawable` trait.  It also handles basic loading of images
//! and text.
//!
//! This module also manages graphics state, coordinate systems, etc.
//! The default coordinate system has the origin in the upper-left
//! corner of the screen, with Y increasing downwards.

use std::collections::HashMap;
use std::convert::From;
use std::fmt;

use winit;

use conf::{self, WindowMode};
use context::{Context, DebugId};
use {GameError, GameResult};

// mod canvas;
mod context;
mod drawparam;
mod image;
mod mesh;
mod shader;
// mod text;
mod types;
use mint;
pub(crate) mod vulkan;
use nalgebra as na;

// pub mod spritebatch;

// pub use self::canvas::*;
pub(crate) use self::context::*;
pub use self::drawparam::*;
pub use self::image::*;
pub use self::mesh::*;
pub use self::shader::*;
// pub use self::text::*;
pub use self::types::*;

/// Internal structure containing vertex data.
#[derive(Clone, Copy, Debug)]
pub struct Vertex {
    /// Position component
    pub pos: [f32; 2],
    /// Texture coordinates component
    pub uv: [f32; 2],
    /// Color component
    pub color: [f32; 4],
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct InstanceProperties {
    pub src: [f32; 4],
    pub col1: [f32; 4],
    pub col2: [f32; 4],
    pub col3: [f32; 4],
    pub col4: [f32; 4],
    pub color: [f32; 4],
}

impl Default for InstanceProperties {
    fn default() -> Self {
        InstanceProperties {
            src: [0.0, 0.0, 1.0, 1.0],
            col1: [1.0, 0.0, 0.0, 0.0],
            col2: [0.0, 1.0, 0.0, 0.0],
            col3: [1.0, 0.0, 1.0, 0.0],
            col4: [1.0, 0.0, 0.0, 1.0],
            color: [1.0, 1.0, 1.0, 1.0],
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct Globals {
    pub mvp: [[f32; 4]; 4],
}

const QUAD_VERTICES: [Vertex; 4] = [
    Vertex {
        pos: [0.0, 0.0],
        uv: [0.0, 0.0],
        color: [1.0, 1.0, 1.0, 1.0],
    },
    Vertex {
        pos: [1.0, 0.0],
        uv: [1.0, 0.0],
        color: [1.0, 1.0, 1.0, 1.0],
    },
    Vertex {
        pos: [1.0, 1.0],
        uv: [1.0, 1.0],
        color: [1.0, 1.0, 1.0, 1.0],
    },
    Vertex {
        pos: [0.0, 1.0],
        uv: [0.0, 1.0],
        color: [1.0, 1.0, 1.0, 1.0],
    },
];

const QUAD_INDICES: [u16; 6] = [0, 1, 2, 0, 2, 3];

// **********************************************************************
// DRAWING
// **********************************************************************

/// Clear the screen to the background color.
/// TODO: Into<Color> ?
pub fn clear(ctx: &mut Context, color: Color) -> GameResult {
    // This doesn't actually clear the screen, it just sets the clear color to use during the render pass
    let gfx = &mut ctx.gfx_context;
    gfx.clear_color = color.into();
    gfx.begin_frame()
}

/// Draws the given `Drawable` object to the screen by calling its
/// `draw()` method.
pub fn draw<D, T>(ctx: &mut Context, drawable: &D, params: T) -> GameResult
where
    D: Drawable,
    T: Into<DrawTransform>,
{
    let params = params.into();
    drawable.draw(ctx, params)
}

/// Tells the graphics system to actually put everything on the screen.
/// Call this at the end of your `EventHandler`'s `draw()` method.
///
/// Unsets any active canvas.
pub fn present(ctx: &mut Context) -> GameResult {
    ctx.gfx_context.end_frame()
}

/// Take a screenshot by outputting the current render surface
/// (screen or selected canvas) to a PNG file.
pub fn screenshot(ctx: &mut Context) -> GameResult<Image> {
    unimplemented!("graphics::screenshot");
}

// TODO: Make all of these take Into<Color>???

/// Draw a circle.
///
/// Allocates a new `Mesh`, draws it, and throws it away, so if you are drawing many of them
/// it will be quite slow.  To speed it up you should create the `Mesh` yourself and cache it.
///
/// For the meaning of the `tolerance` parameter, [see here](https://docs.rs/lyon_geom/0.9.0/lyon_geom/#flattening).
pub fn circle<P>(
    ctx: &mut Context,
    color: Color,
    mode: DrawMode,
    point: P,
    radius: f32,
    tolerance: f32,
) -> GameResult
where
    P: Into<mint::Point2<f32>>,
{
    let m = Mesh::new_circle(ctx, mode, point, radius, tolerance, color)?;
    m.draw(ctx, DrawParam::new().color(color))
}

/// Draw an ellipse.
///
/// Allocates a new `Mesh`, draws it, and throws it away, so if you are drawing many of them
/// it will be quite slow.  To speed it up you should create the `Mesh` yourself and cache it.
///
/// For the meaning of the `tolerance` parameter, [see here](https://docs.rs/lyon_geom/0.9.0/lyon_geom/#flattening).
pub fn ellipse<P>(
    ctx: &mut Context,
    color: Color,
    mode: DrawMode,
    point: P,
    radius1: f32,
    radius2: f32,
    tolerance: f32,
) -> GameResult
where
    P: Into<mint::Point2<f32>>,
{
    let m = Mesh::new_ellipse(ctx, mode, point, radius1, radius2, tolerance, color)?;
    m.draw(ctx, DrawParam::new())
}

/// Draws a line of one or more connected segments.
///
/// Allocates a new `Mesh`, draws it, and throws it away, so if you are drawing many of them
/// it will be quite slow.  To speed it up you should create the `Mesh` yourself and cache it.
pub fn line<P>(ctx: &mut Context, color: Color, points: &[P], width: f32) -> GameResult
where
    P: Into<mint::Point2<f32>> + Clone,
{
    let m = Mesh::new_line(ctx, points, width, color)?;
    m.draw(ctx, DrawParam::new())
}

/// Draws points (as rectangles).
///
/// Allocates a new `Mesh`, draws it, and throws it away, so if you are drawing many of them
/// it will be quite slow.  To speed it up you should create the `Mesh` yourself and cache it.
pub fn points<P>(ctx: &mut Context, color: Color, points: &[P], point_size: f32) -> GameResult
where
    P: Into<mint::Point2<f32>> + Clone,
{
    let points = points.into_iter().cloned().map(P::into);
    for p in points {
        let r = Rect::new(p.x, p.y, point_size, point_size);
        rectangle(ctx, color, DrawMode::Fill, r)?;
    }
    Ok(())
}

/// Draws a closed polygon.
///
/// Allocates a new `Mesh`, draws it, and throws it away, so if you are drawing many of them
/// it will be quite slow.  To speed it up you should create the `Mesh` yourself and cache it.
pub fn polygon<P>(ctx: &mut Context, color: Color, mode: DrawMode, vertices: &[P]) -> GameResult
where
    P: Into<mint::Point2<f32>> + Clone,
{
    let m = Mesh::new_polygon(ctx, mode, vertices, color)?;
    m.draw(ctx, DrawParam::new())
}

/// Draws a rectangle.
///
/// Allocates a new `Mesh`, draws it, and throws it away, so if you are drawing many of them
/// it will be quite slow.  To speed it up you should create the `Mesh` yourself and cache it.
pub fn rectangle(ctx: &mut Context, color: Color, mode: DrawMode, rect: Rect) -> GameResult {
    let x1 = rect.x;
    let x2 = rect.x + rect.w;
    let y1 = rect.y;
    let y2 = rect.y + rect.h;
    let pts = [
        Point2::new(x1, y1),
        Point2::new(x2, y1),
        Point2::new(x2, y2),
        Point2::new(x1, y2),
    ];
    polygon(ctx, color, mode, &pts)
}

// **********************************************************************
// GRAPHICS STATE
// **********************************************************************

/// Get the default filter mode for new images.
pub fn default_filter(ctx: &Context) -> FilterMode {
    unimplemented!("graphics::default_filter");
}

/// Returns a string that tells a little about the obtained rendering mode.
/// It is supposed to be human-readable and will change; do not try to parse
/// information out of it!
pub fn renderer_info(ctx: &Context) -> GameResult<String> {
    Ok(String::new())
}

/// Returns a rectangle defining the coordinate system of the screen.
/// It will be `Rect { x: left, y: top, w: width, h: height }`
///
/// If the Y axis increases downwards, the `height` of the Rect
/// will be negative.
pub fn screen_coordinates(ctx: &Context) -> Rect {
    ctx.gfx_context.screen_rect
}

/// Sets the default filter mode used to scale images.
///
/// This does not apply retroactively to already created images.
pub fn set_default_filter(ctx: &mut Context, mode: FilterMode) {
    unimplemented!("graphics::set_default_filter");
}

/// Sets the bounds of the screen viewport.
///
/// The default coordinate system has (0,0) at the top-left corner
/// with X increasing to the right and Y increasing down, with the
/// viewport scaled such that one coordinate unit is one pixel on the
/// screen.  This function lets you change this coordinate system to
/// be whatever you prefer.
///
/// The `Rect`'s x and y will define the top-left corner of the screen,
/// and that plus its w and h will define the bottom-right corner.
pub fn set_screen_coordinates(context: &mut Context, rect: Rect) -> GameResult {
    let gfx = &mut context.gfx_context;
    gfx.set_projection_rect(rect);
    gfx.calculate_transform_matrix();
    gfx.update_globals()
}

/// Sets the raw projection matrix to the given homogeneous
/// transformation matrix.
///
/// You must call `apply_transformations(ctx)` after calling this to apply
/// these changes and recalculate the underlying MVP matrix.
pub fn set_projection(context: &mut Context, proj: Matrix4) {
    let gfx = &mut context.gfx_context;
    gfx.set_projection(proj);
}

/// Premultiplies the given transformation matrix with the current projection matrix
///
/// You must call `apply_transformations(ctx)` after calling this to apply
/// these changes and recalculate the underlying MVP matrix.
pub fn mul_projection(context: &mut Context, transform: Matrix4) {
    let gfx = &mut context.gfx_context;
    let curr = gfx.projection();
    gfx.set_projection(transform * curr);
}

/// Gets a copy of the context's raw projection matrix
pub fn projection(context: &Context) -> Matrix4 {
    let gfx = &context.gfx_context;
    gfx.projection()
}

/// Pushes a homogeneous transform matrix to the top of the transform
/// (model) matrix stack of the `Context`. If no matrix is given, then
/// pushes a copy of the current transform matrix to the top of the stack.
///
/// You must call `apply_transformations(ctx)` after calling this to apply
/// these changes and recalculate the underlying MVP matrix.
///
/// A `DrawParam` can be converted into an appropriate transform
/// matrix by calling `param.into_matrix()`.
pub fn push_transform(context: &mut Context, transform: Option<Matrix4>) {
    let gfx = &mut context.gfx_context;
    if let Some(t) = transform {
        gfx.push_transform(t);
    } else {
        let copy = *gfx
            .modelview_stack
            .last()
            .expect("Matrix stack empty, should never happen");
        gfx.push_transform(copy);
    }
}

/// Pops the transform matrix off the top of the transform
/// (model) matrix stack of the `Context`.
///
/// You must call `apply_transformations(ctx)` after calling this to apply
/// these changes and recalculate the underlying MVP matrix.
pub fn pop_transform(context: &mut Context) {
    let gfx = &mut context.gfx_context;
    gfx.pop_transform();
}

/// Sets the current model transformation to the given homogeneous
/// transformation matrix.
///
/// You must call `apply_transformations(ctx)` after calling this to apply
/// these changes and recalculate the underlying MVP matrix.
///
/// A `DrawParam` can be converted into an appropriate transform
/// matrix by calling `param.into_matrix()`.
pub fn set_transform(context: &mut Context, transform: Matrix4) {
    let gfx = &mut context.gfx_context;
    gfx.set_transform(transform);
}

/// Gets a copy of the context's current transform matrix
pub fn transform(context: &Context) -> Matrix4 {
    let gfx = &context.gfx_context;
    gfx.transform()
}

/// Premultiplies the given transform with the current model transform.
///
/// You must call `apply_transformations(ctx)` after calling this to apply
/// these changes and recalculate the underlying MVP matrix.
///
/// A `DrawParam` can be converted into an appropriate transform
/// matrix by calling `param.into_matrix()`.
pub fn mul_transform(context: &mut Context, transform: Matrix4) {
    let gfx = &mut context.gfx_context;
    let curr = gfx.transform();
    gfx.set_transform(transform * curr);
}

/// Sets the current model transform to the origin transform (no transformation)
///
/// You must call `apply_transformations(ctx)` after calling this to apply
/// these changes and recalculate the underlying MVP matrix.
pub fn origin(context: &mut Context) {
    let gfx = &mut context.gfx_context;
    gfx.set_transform(Matrix4::identity());
}

/// Calculates the new total transformation (Model-View-Projection) matrix
/// based on the matrices at the top of the transform and view matrix stacks
/// and sends it to the graphics card.
pub fn apply_transformations(context: &mut Context) -> GameResult {
    let gfx = &mut context.gfx_context;
    gfx.calculate_transform_matrix();
    gfx.update_globals()
}

/// Sets the blend mode of the currently active shader program
pub fn set_blend_mode(ctx: &mut Context, mode: BlendMode) -> GameResult {
    ctx.gfx_context.set_blend_mode(mode)
}

/// Sets the window mode, such as the size and other properties.
///
/// Setting the window mode may have side effects, such as clearing
/// the screen or setting the screen coordinates viewport to some undefined value.
/// It is recommended to call `set_screen_coordinates()` after changing the window
/// size to make sure everything is what you want it to be.
pub fn set_mode(context: &mut Context, mode: WindowMode) -> GameResult {
    let gfx = &mut context.gfx_context;
    gfx.set_window_mode(mode)?;
    // Save updated mode.
    context.conf.window_mode = mode;
    Ok(())
}

/// Sets the window to fullscreen or back.
pub fn set_fullscreen(context: &mut Context, fullscreen: conf::FullscreenType) -> GameResult {
    let mut window_mode = context.conf.window_mode;
    window_mode.fullscreen_type = fullscreen;
    set_mode(context, window_mode)
}

/// Sets the window size/resolution to the specified width and height.
pub fn set_resolution(context: &mut Context, width: f32, height: f32) -> GameResult {
    let mut window_mode = context.conf.window_mode;
    window_mode.width = width;
    window_mode.height = height;
    set_mode(context, window_mode)
}

/// Sets whether or not the window is resizable.
pub fn set_resizable(context: &mut Context, resizable: bool) -> GameResult {
    let mut window_mode = context.conf.window_mode;
    window_mode.resizable = resizable;
    set_mode(context, window_mode)
}

use std::path::Path;
use winit::Icon;
/// Sets the window icon.
pub fn set_window_icon<P: AsRef<Path>>(context: &Context, path: Option<P>) -> GameResult<()> {
    let icon = match path {
        Some(path) => Some(Icon::from_path(path)?),
        None => None,
    };
    context.gfx_context.window.set_window_icon(icon);
    Ok(())
}

/// Sets the window title.
pub fn set_window_title(context: &Context, title: &str) {
    context.gfx_context.window.set_title(title);
}

/// Returns a reference to the SDL window.
/// Ideally you should not need to use this because ggez
/// would provide all the functions you need without having
/// to dip into SDL itself.  But life isn't always ideal.
pub fn window(context: &Context) -> &winit::Window {
    let gfx = &context.gfx_context;
    &gfx.window
}

/// Returns the size of the window in pixels as (width, height),
/// including borders, titlebar, etc.
/// Returns zeros if window doesn't exist.
/// TODO: Rename, since get_drawable_size is usually what we
/// actually want
pub fn size(context: &Context) -> (f64, f64) {
    let gfx = &context.gfx_context;
    gfx.window
        .get_outer_size()
        .map(|logical_size| (logical_size.width, logical_size.height))
        .unwrap_or((0.0, 0.0))
}

/// Returns the hidpi pixel scaling factor that ggez
/// is currently using.  If  `conf::WindowMode::hidpi`
/// is true this is equal to `get_os_hidpi_factor()`,
/// otherwise it is `1.0`.
pub fn hidpi_factor(context: &Context) -> f32 {
    context.gfx_context.hidpi_factor
}

/// Returns the hidpi pixel scaling factor that the operating
/// system says that ggez should be using.
pub fn os_hidpi_factor(context: &Context) -> f32 {
    context.gfx_context.os_hidpi_factor
}

/// Returns the size of the window's underlying drawable in pixels as (width, height).
/// Returns zeros if window doesn't exist.
pub fn drawable_size(context: &Context) -> (f64, f64) {
    let gfx = &context.gfx_context;
    gfx.window
        .get_inner_size()
        .map(|logical_size| (logical_size.width, logical_size.height))
        .unwrap_or((0.0, 0.0))
}

/// All types that can be drawn on the screen implement the `Drawable` trait.
pub trait Drawable {
    /// Draws the drawable onto the rendering target.
    ///
    /// ALSO TODO: Expand docs
    fn draw<D>(&self, ctx: &mut Context, param: D) -> GameResult
    where
        D: Into<DrawTransform>;

    /// Sets the blend mode to be used when drawing this drawable.
    /// This overrides the general `graphics::set_blend_mode()`.
    /// If `None` is set, defers to the blend mode set by
    /// `graphics::set_blend_mode()`.
    fn set_blend_mode(&mut self, mode: Option<BlendMode>);

    /// Gets the blend mode to be used when drawing this drawable.
    fn blend_mode(&self) -> Option<BlendMode>;
}

#[cfg(test)]
mod tests {}
