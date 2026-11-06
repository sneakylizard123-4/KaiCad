//! Vulkan render pass management.

use ash::{Instance, Device};
use ash::vk::{SurfaceKHR, Format, Extent2D, ShaderModule, PrimitiveTopology, PhysicalDevice};
use ash::khr::swapchain;

/// Render pass configuration for a frame.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct RenderPassConfig {
    pub color_attachments: usize,
    pub depth_attachment: bool,
}
