//! Vulkan swapchain management.

use ash::{Instance, Device};
use ash::vk::{SurfaceKHR, Format, Extent2D, ShaderModule, PrimitiveTopology, PhysicalDevice};
use ash::khr::swapchain;

/// Vulkan swapchain for presentation.
#[derive(Debug, Clone, Default)]
pub struct SwapchainInfo {
    pub images: usize,
    pub image_format: Format,
    pub extent: Extent2D,
}
