//! Vulkan device management.

use ash::Device;
use ash::vk::PhysicalDevice;

/// Vulkan GPU device wrapper.
pub struct VulkanDevice {
    /// The physical GPU selected for rendering.
    pub physical: PhysicalDevice,
    /// The logical device created from the physical device.
    pub logical: Device,
}

impl VulkanDevice {
    /// Wrap an existing physical/logical device pair.
    pub fn new(physical: PhysicalDevice, logical: Device) -> Self {
        Self { physical, logical }
    }
}
