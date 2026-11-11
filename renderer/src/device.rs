//! Vulkan device management.

use ash::Device;
use ash::vk::PhysicalDevice;

/// Vulkan GPU device wrapper.
pub struct VulkanDevice {
    pub physical: PhysicalDevice,
    pub logical: Device,
}
