//! Vulkan pipeline management.

use ash::{Instance, Device};
use ash::vk::{SurfaceKHR, Format, Extent2D, ShaderModule, PrimitiveTopology, PhysicalDevice};
use ash::khr::swapchain;

/// Graphics pipeline state.
#[derive(Debug, Clone, Default)]
pub struct GraphicsPipelineConfig {
    pub vertex_shader: ShaderModule,
    pub fragment_shader: ShaderModule,
    pub topology: PrimitiveTopology,
}
