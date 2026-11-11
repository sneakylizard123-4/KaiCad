//! Vulkan render pass management.

/// Render pass configuration for a frame.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct RenderPassConfig {
    pub color_attachments: usize,
    pub depth_attachment: bool,
}
