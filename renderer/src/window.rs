//! Window and surface configuration for the renderer.

/// Configuration for the application window that owns the Vulkan surface.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub resizable: bool,
    pub fullscreen: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: "KaiCad".to_string(),
            width: 1920,
            height: 1080,
            resizable: true,
            fullscreen: false,
        }
    }
}

impl WindowConfig {
    /// Create a window configuration with the given title and size.
    pub fn new(title: impl Into<String>, width: u32, height: u32) -> Self {
        Self { title: title.into(), width, height, ..Self::default() }
    }

    /// Width / height aspect ratio, guarding against a zero height.
    pub fn aspect_ratio(&self) -> f32 {
        if self.height == 0 { 1.0 } else { self.width as f32 / self.height as f32 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_window_is_1080p_windowed() {
        let config = WindowConfig::default();
        assert_eq!(config.title, "KaiCad");
        assert_eq!(config.width, 1920);
        assert_eq!(config.height, 1080);
        assert!(config.resizable);
        assert!(!config.fullscreen);
    }

    #[test]
    fn new_sets_title_and_size() {
        let config = WindowConfig::new("Viewport", 1280, 720);
        assert_eq!(config.title, "Viewport");
        assert_eq!(config.width, 1280);
        assert_eq!(config.height, 720);
        // Unspecified options keep their defaults.
        assert!(config.resizable);
        assert!(!config.fullscreen);
    }

    #[test]
    fn aspect_ratio_is_width_over_height() {
        let config = WindowConfig::new("Viewport", 1920, 1080);
        assert!((config.aspect_ratio() - 16.0 / 9.0).abs() < f32::EPSILON);
    }

    #[test]
    fn aspect_ratio_handles_zero_height() {
        let config = WindowConfig::new("Viewport", 800, 0);
        assert_eq!(config.aspect_ratio(), 1.0);
    }
}
