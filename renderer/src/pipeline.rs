//! Vulkan pipeline management.

use ash::vk::{PrimitiveTopology, ShaderModule};

/// Graphics pipeline state.
#[derive(Debug, Clone, Default)]
pub struct GraphicsPipelineConfig {
    pub vertex_shader: ShaderModule,
    pub fragment_shader: ShaderModule,
    pub topology: PrimitiveTopology,
}
