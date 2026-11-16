//! KaiCad Vulkan-based rendering engine.
//!
//! Provides GPU-accelerated 2D/3D visualization of schematics and PCBs.
//!
//! Only window and surface *configuration* exists today: no Vulkan objects are
//! created yet, and [`Renderer::new`] returns [`RendererError::NotImplemented`].

pub mod device;
pub mod pipeline;
pub mod render_pass;
pub mod swapchain;
pub mod window;

use ash::khr::swapchain as ash_swapchain;
use ash::vk::SurfaceKHR;
use ash::{Device, Instance};

pub use window::WindowConfig;

/// Vulkan renderer configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct RendererConfig {
    pub window: WindowConfig,
    pub vsync: bool,
}

impl Default for RendererConfig {
    fn default() -> Self {
        Self { window: WindowConfig::default(), vsync: true }
    }
}

impl RendererConfig {
    /// Create a renderer configuration for the given window.
    pub fn new(window: WindowConfig) -> Self {
        Self { window, vsync: true }
    }
}

/// Main renderer handle.
pub struct Renderer {
    pub config: RendererConfig,
    pub instance: Instance,
    pub surface: SurfaceKHR,
    pub device: Device,
    pub swapchain: ash_swapchain::Device,
}

impl Renderer {
    /// Create a new renderer instance.
    pub fn new(_config: RendererConfig) -> Result<Self, RendererError> {
        Err(RendererError::NotImplemented)
    }

    /// Render a single frame.
    pub fn render(&mut self) -> Result<(), RendererError> {
        Err(RendererError::NotImplemented)
    }
}

/// Renderer error types.
#[derive(Debug)]
pub enum RendererError {
    NotImplemented,
    VulkanError(String),
    InitializationFailed,
}

impl std::fmt::Display for RendererError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RendererError::NotImplemented => write!(f, "Feature not yet implemented"),
            RendererError::VulkanError(msg) => write!(f, "Vulkan error: {}", msg),
            RendererError::InitializationFailed => write!(f, "Renderer initialization failed"),
        }
    }
}

impl std::error::Error for RendererError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_uses_1080p_window_and_vsync() {
        let config = RendererConfig::default();
        assert_eq!(config.window.width, 1920);
        assert_eq!(config.window.height, 1080);
        assert!(config.vsync);
    }
}
