//! Vulkan swapchain management.

use ash::vk::{Extent2D, Format};

/// Vulkan swapchain for presentation.
#[derive(Debug, Clone, Default)]
pub struct SwapchainInfo {
    pub images: usize,
    pub image_format: Format,
    pub extent: Extent2D,
}
