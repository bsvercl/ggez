use graphics::*;
use std::collections::HashMap;
use std::sync::Arc;
use vulkano::command_buffer::{DynamicState,AutoCommandBufferBuilder};
use vulkano::device::Device;
use vulkano::framebuffer::{RenderPassAbstract, Subpass};
use vulkano::pipeline::blend::AttachmentBlend;
use vulkano::pipeline::vertex::OneVertexOneInstanceDefinition;
use vulkano::pipeline::{GraphicsPipeline, GraphicsPipelineAbstract};
use {GameError, GameResult};
use 

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BlendMode {
    /// When combining two fragments, add their values together, saturating at 1.0
    Add,
    /// When combining two fragments, subtract the source value from the destination value
    Subtract,
    /// When combining two fragments, add the value of the source times its alpha channel with the value of the destination multiplied by the inverse of the source alpha channel. Has the usual transparency effect: mixes the two colors using a fraction of each one specified by the alpha of the source.
    Alpha,
    /// When combining two fragments, subtract the destination color from a constant color using the source color as weight. Has an invert effect with the constant color as base and source color controlling displacement from the base color. A white source color and a white value results in plain invert. The output alpha is same as destination alpha.
    Invert,
    /// When combining two fragments, multiply their values together.
    Multiply,
    /// When combining two fragments, choose the source value
    Replace,
    /// When combining two fragments, choose the lighter value
    Lighten,
    /// When combining two fragments, choose the darker value
    Darken,
}

impl From<BlendMode> for AttachmentBlend {
    fn from(bm: BlendMode) -> Self {
        match bm {
            BlendMode::Add => unimplemented!(),
            BlendMode::Subtract => unimplemented!(),
            BlendMode::Alpha => AttachmentBlend::alpha_blending(),
            BlendMode::Invert => unimplemented!(),
            BlendMode::Multiply => unimplemented!(),
            BlendMode::Replace => unimplemented!(),
            BlendMode::Lighten => unimplemented!(),
            BlendMode::Darken => unimplemented!(),
        }
    }
}

pub struct PipelineSet {
    pipelines: HashMap<BlendMode, Arc<dyn GraphicsPipelineAbstract + Send + Sync>>,
}

impl PipelineSet {
    fn new(capacity: usize) -> Self {
        PipelineSet {
            pipelines: HashMap::with_capacity(capacity),
        }
    }

    fn insert_mode(
        &mut self,
        mode: BlendMode,
        pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
    ) {
        let _ = self.pipelines.insert(mode, pipeline.clone());
    }

    fn get_mode(
        &self,
        mode: &BlendMode,
    ) -> GameResult<Arc<dyn GraphicsPipelineAbstract + Send + Sync>> {
        match self.pipelines.get(mode) {
            Some(pipeline) => Ok(pipeline.clone()),
            None => Err(GameError::RenderError(
                "Could not find a pipeline for the specified shader and BlendMode".into(),
            )),
        }
    }
}

pub(crate) fn create_shader(
    device: &Arc<Device>,
    render_pass: &Arc<dyn RenderPassAbstract + Send + Sync>,
    blend_modes: Option<&[BlendMode]>,
) -> GameResult {
    let blend_modes = blend_modes.unwrap_or_else(|| &[BlendMode::Alpha]);
    let mut pipelines = PipelineSet::new(blend_modes.len());
    let vertex_shader = vs::Shader::load(device.clone()).unwrap();
    let fragment_shader = fs::Shader::load(device.clone()).unwrap();
    for mode in blend_modes {
        let pipeline = Arc::new(
            GraphicsPipeline::start()
                .vertex_input(OneVertexOneInstanceDefinition::<Vertex, InstanceProperties>::new())
                .vertex_shader(vertex_shader.main_entry_point(), ())
                .triangle_list()
                .viewports_scissors_dynamic(1)
                .fragment_shader(fragment_shader.main_entry_point(), ())
                .blend_collective((*mode).into())
                .render_pass(Subpass::from(render_pass.clone(), 0).unwrap())
                .build(device.clone())
                .unwrap(),
        );
        pipelines.insert_mode(*mode, pipeline);
    }
    Ok(())
}

pub trait ShaderHandle {
    fn draw(&self, AutoCommandBufferBuilder, DynamicState) -> GameResult<AutoCommandBufferBuilder>;
    fn draw_indexed(&self, AutoCommandBufferBuilder, DynamicState) -> GameResult<AutoCommandBufferBuilder>;
    fn set_blend_mode(&mut self, BlendMode) -> GameResult;
    fn get_blend_mode(&self) -> BlendMode;
}

struct ShaderProgram {
    pipelines: PipelineSet,
    active_blend_mode: BlendMode,
}

impl ShaderHandle for ShaderProgram {
    fn draw(&self, mut cb: AutoCommandBufferBuilder, dynamic_state: DynamicState) -> GameResult<AutoCommandBufferBuilder> {
        Ok(cb)
    }

    fn draw_indexed(
        &self,
        mut cb: AutoCommandBufferBuilder,
    ) -> GameResult<AutoCommandBufferBuilder> {
        Ok(cb)
    }

    fn set_blend_mode(&mut self, mode: BlendMode) -> GameResult {
        let _ = self.pipelines.get_mode(&mode)?;
        self.active_blend_mode = mode;
        Ok(())
    }

    fn get_blend_mode(&self) -> BlendMode {
        self.active_blend_mode
    }
}
