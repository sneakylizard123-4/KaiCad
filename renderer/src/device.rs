//! Vulkan device management.

use ash::{Instance, Device};
use ash::vk::{SurfaceKHR, Format, Extent2D, ShaderModule, PrimitiveTopology, PhysicalDevice};
use ash::khr::swapchain;

/// Vulkan GPU device wrapper.

pub struct VulkanDevice {
    pub physical: PhysicalDevice,
    pub logical: Device,
}
