//! Deterministic software renderer for KaiCad boards.
//!
//! The renderer rasterizes a [`pcb::PcbDesign`] on the CPU into an RGBA
//! [`Framebuffer`]. [`ViewMode::TwoD`] draws a framed top view of the board,
//! tracks, pads and vias. [`ViewMode::ThreeD`] extrudes the board, draws
//! copper and vias, and renders the result through a fitted perspective
//! camera with a depth buffer.
//!
//! Rendering is synchronous and headless: no window, GPU or Vulkan loader is
//! required. The `device`, `pipeline`, `render_pass`, `swapchain` and `window`
//! modules remain as configuration scaffolding for a future GPU backend and
//! are not used by this renderer.
//!
//! The output is deterministic for a given design, camera, view and image
//! size. [`Framebuffer::save_ppm`] writes the result as a binary PPM file.

pub mod device;
pub mod framebuffer;
pub mod pipeline;
pub mod render_pass;
pub mod swapchain;
pub mod window;

mod scene;

use pcb::PcbDesign;

pub use framebuffer::{Color, Framebuffer};
pub use window::WindowConfig;

/// Which projection the renderer uses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ViewMode {
    /// Framed orthographic top view.
    #[default]
    TwoD,
    /// Extruded perspective view with depth testing.
    ThreeD,
}

/// Renderer configuration, including the window, vsync and background colour.
#[derive(Debug, Clone, PartialEq)]
pub struct RendererConfig {
    pub window: WindowConfig,
    pub vsync: bool,
    /// Colour used for pixels not covered by the board or its copper.
    pub background: Color,
}

impl Default for RendererConfig {
    fn default() -> Self {
        Self { window: WindowConfig::default(), vsync: true, background: Color::rgb(10, 13, 19) }
    }
}

impl RendererConfig {
    /// Create a renderer configuration for the given window.
    pub fn new(window: WindowConfig) -> Self {
        Self { window, ..Self::default() }
    }
}

/// Perspective camera used by [`ViewMode::ThreeD`].
///
/// Coordinates are in millimetres with Z pointing out of the board. The
/// camera is not validated until rendering, so [`Renderer::render_pcb`]
/// reports a degenerate eye/target/up combination as
/// [`RendererError::InvalidConfig`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    /// Camera position in millimetres.
    pub eye: [f64; 3],
    /// Look-at point in millimetres.
    pub target: [f64; 3],
    /// Up direction, normally `[0.0, 0.0, 1.0]`.
    pub up: [f64; 3],
    /// Vertical field of view in degrees, between 1 and 179.
    pub fov_y_degrees: f64,
    /// Near clip distance in millimetres.
    pub near: f64,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            eye: [0.0, -150.0, 120.0],
            target: [0.0, 0.0, 0.8],
            up: [0.0, 0.0, 1.0],
            fov_y_degrees: 45.0,
            near: 0.1,
        }
    }
}

impl Camera {
    /// Build a perspective camera.
    pub fn new(eye: [f64; 3], target: [f64; 3], up: [f64; 3], fov_y_degrees: f64) -> Self {
        Self { eye, target, up, fov_y_degrees, ..Self::default() }
    }

    /// A three-quarter camera framing the board's bounding box.
    ///
    /// Empty or non-finite outlines fall back to [`Camera::default`].
    pub fn fit(design: &PcbDesign) -> Self {
        let Some(bounds) = design.outline.bounding_box() else {
            return Self::default();
        };
        let values = [bounds.min.x, bounds.min.y, bounds.max.x, bounds.max.y];
        if values.iter().any(|value| !value.is_finite())
            || bounds.max.x < bounds.min.x
            || bounds.max.y < bounds.min.y
        {
            return Self::default();
        }
        let center_x = (bounds.min.x + bounds.max.x) / 2.0;
        let center_y = (bounds.min.y + bounds.max.y) / 2.0;
        let span = (bounds.max.x - bounds.min.x).max(bounds.max.y - bounds.min.y).max(1.0);
        Self {
            eye: [center_x + span * 0.55, center_y - span * 1.15, 0.8 + span * 0.95],
            target: [center_x, center_y, 0.8],
            ..Self::default()
        }
    }
}

/// Mapping between board millimetres and 2D viewport pixels.
///
/// The transform matches the framing used by [`ViewMode::TwoD`] exactly, so a
/// click in the presented viewport can be converted back into board
/// coordinates. Pixel coordinates have their origin at the top-left of the
/// rendered image; board Y increases upward.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoardTransform {
    min_x: f64,
    min_y: f64,
    scale: f64,
    x_offset: f64,
    y_offset: f64,
    image_height: f64,
}

impl BoardTransform {
    /// Fit a board into a rendered image of `width` by `height` pixels.
    ///
    /// Returns [`RendererError::InvalidConfig`] when the image size or board
    /// outline cannot be transformed.
    pub fn for_board(design: &PcbDesign, width: u32, height: u32) -> Result<Self, RendererError> {
        validate_size(width, height)?;
        let bbox = design
            .outline
            .bounding_box()
            .ok_or_else(|| RendererError::InvalidConfig("board outline is empty".to_string()))?;
        let values = [bbox.min.x, bbox.min.y, bbox.max.x, bbox.max.y];
        if values.iter().any(|value| !value.is_finite())
            || bbox.max.x < bbox.min.x
            || bbox.max.y < bbox.min.y
        {
            return Err(RendererError::InvalidConfig(
                "board outline has invalid bounds".to_string(),
            ));
        }

        let width = width as f64;
        let height = height as f64;
        let margin = (width.min(height) * 0.08).max(4.0);
        let usable_width = (width - margin * 2.0).max(1.0);
        let usable_height = (height - margin * 2.0).max(1.0);
        let board_width = (bbox.max.x - bbox.min.x).max(1e-9);
        let board_height = (bbox.max.y - bbox.min.y).max(1e-9);
        let scale = (usable_width / board_width).min(usable_height / board_height);
        let x_offset = margin + (usable_width - board_width * scale) / 2.0;
        let y_offset = margin + (usable_height - board_height * scale) / 2.0;

        Ok(Self {
            min_x: bbox.min.x,
            min_y: bbox.min.y,
            scale,
            x_offset,
            y_offset,
            image_height: height,
        })
    }

    /// Pixels per board millimetre.
    pub fn scale(&self) -> f64 {
        self.scale
    }

    /// Convert a board point in millimetres to image pixels.
    pub fn board_to_pixel(&self, x: f64, y: f64) -> [f64; 2] {
        [
            self.x_offset + (x - self.min_x) * self.scale,
            self.image_height - self.y_offset - (y - self.min_y) * self.scale,
        ]
    }

    /// Convert image pixels to a board point in millimetres.
    pub fn pixel_to_board(&self, x: f64, y: f64) -> [f64; 2] {
        [
            self.min_x + (x - self.x_offset) / self.scale,
            self.min_y + (self.image_height - y - self.y_offset) / self.scale,
        ]
    }
}

/// Main renderer handle.
pub struct Renderer {
    pub config: RendererConfig,
    framebuffer: Framebuffer,
    depth: Vec<f64>,
}

impl Renderer {
    /// Create a renderer and allocate its framebuffer.
    ///
    /// Returns [`RendererError::InvalidConfig`] for a zero-sized or oversized
    /// window (more than 16 million pixels).
    pub fn new(config: RendererConfig) -> Result<Self, RendererError> {
        validate_size(config.window.width, config.window.height)?;
        let framebuffer = Framebuffer::new(config.window.width, config.window.height)
            .ok_or_else(|| RendererError::InvalidConfig("window size is unusable".to_string()))?;
        let pixels = framebuffer::pixel_count(config.window.width, config.window.height)
            .ok_or_else(|| RendererError::InvalidConfig("window size is unusable".to_string()))?;
        Ok(Self { config, framebuffer, depth: vec![f64::INFINITY; pixels] })
    }

    /// The current image. It is cleared by [`render`](Self::render) and
    /// rewritten by [`render_pcb`](Self::render_pcb).
    pub fn framebuffer(&self) -> &Framebuffer {
        &self.framebuffer
    }

    /// Clear the framebuffer to the configured background colour.
    pub fn render(&mut self) -> Result<(), RendererError> {
        self.framebuffer.clear(self.config.background);
        self.depth.fill(f64::INFINITY);
        Ok(())
    }

    /// Render a board using an automatically fitted camera for the 3D view.
    pub fn render_pcb(&mut self, design: &PcbDesign, view: ViewMode) -> Result<(), RendererError> {
        let camera = Camera::fit(design);
        self.render_pcb_with_camera(design, view, &camera)
    }

    /// Render a board with an explicit camera. The camera is ignored by
    /// [`ViewMode::TwoD`].
    pub fn render_pcb_with_camera(
        &mut self,
        design: &PcbDesign,
        view: ViewMode,
        camera: &Camera,
    ) -> Result<(), RendererError> {
        scene::render_pcb(design, &mut self.framebuffer, &mut self.depth, view, camera)
    }

    /// Write the current framebuffer as a binary PPM image.
    pub fn save_ppm(&self, path: impl AsRef<std::path::Path>) -> std::io::Result<()> {
        self.framebuffer.save_ppm(path)
    }
}

fn validate_size(width: u32, height: u32) -> Result<(), RendererError> {
    if width == 0 || height == 0 {
        return Err(RendererError::InvalidConfig("window dimensions must be non-zero".to_string()));
    }
    if framebuffer::pixel_count(width, height).is_none() {
        return Err(RendererError::InvalidConfig(
            "window exceeds the 16 million pixel limit".to_string(),
        ));
    }
    Ok(())
}

/// Renderer error types.
#[derive(Debug)]
pub enum RendererError {
    NotImplemented,
    VulkanError(String),
    InitializationFailed,
    /// The design, camera or image size cannot be rendered.
    InvalidConfig(String),
}

impl std::fmt::Display for RendererError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RendererError::NotImplemented => write!(f, "Feature not yet implemented"),
            RendererError::VulkanError(msg) => write!(f, "Vulkan error: {}", msg),
            RendererError::InitializationFailed => write!(f, "Renderer initialization failed"),
            RendererError::InvalidConfig(msg) => write!(f, "Invalid renderer configuration: {msg}"),
        }
    }
}

impl std::error::Error for RendererError {}

#[cfg(test)]
mod tests {
    use super::*;
    use core::{LayerKind, PcbLayer, Point, Stackup};
    use pcb::footprint::{Footprint, Outline, OutlineShape, PadPosition, PadType};
    use std::collections::HashSet;

    fn test_design() -> PcbDesign {
        PcbDesign {
            outline: core::PcbOutline::rectangle(20.0, 16.0),
            layers: vec![
                PcbLayer::new(0, "F.Cu", LayerKind::Copper),
                PcbLayer::new(1, "B.Cu", LayerKind::Copper),
                PcbLayer::new(2, "F.SilkS", LayerKind::SilkScreen),
            ],
            stackup: Stackup { layers: vec![0, 1] },
            footprints: vec![Footprint {
                name: "R1".to_string(),
                reference_designator: "R".to_string(),
                position: Point::new(10.0, 8.0),
                pad_positions: vec![PadPosition {
                    number: "1".to_string(),
                    x: -1.5,
                    y: 0.0,
                    pad_type: PadType::ThroughHole,
                    width: 1.4,
                    height: 0.8,
                    rotation: 30.0,
                    net_id: None,
                    ..PadPosition::default()
                }],
                outline: Outline {
                    shape: OutlineShape::rectangle(-2.0, -1.0, 4.0, 2.0),
                    copper: false,
                },
                ..Footprint::default()
            }],
            tracks: vec![pcb::Track {
                layer: 0,
                width: 0.4,
                path: vec![Point::new(2.0, 2.0), Point::new(17.0, 12.0)],
                ..pcb::Track::default()
            }],
            vias: vec![pcb::Via {
                position: Point::new(6.0, 5.0),
                layer_start: 0,
                layer_end: 1,
                diameter: 0.8,
                drill: 0.4,
                ..pcb::Via::default()
            }],
            ..PcbDesign::default()
        }
    }

    fn renderer(width: u32, height: u32) -> Renderer {
        Renderer::new(RendererConfig {
            window: WindowConfig::new("test", width, height),
            ..RendererConfig::default()
        })
        .expect("test renderer")
    }

    fn non_background_pixels(renderer: &Renderer) -> usize {
        let background = renderer.config.background;
        renderer
            .framebuffer()
            .pixels()
            .chunks_exact(4)
            .filter(|pixel| pixel[..3] != [background.r, background.g, background.b])
            .count()
    }

    #[test]
    fn board_transform_round_trips_board_coordinates() {
        let design = test_design();
        let transform = BoardTransform::for_board(&design, 160, 120).expect("transform");
        assert!(transform.scale() > 0.0);

        for (x, y) in [(0.0, 0.0), (20.0, 0.0), (10.0, 8.0), (0.0, 16.0), (20.0, 16.0)] {
            let [pixel_x, pixel_y] = transform.board_to_pixel(x, y);
            let [round_x, round_y] = transform.pixel_to_board(pixel_x, pixel_y);
            assert!((round_x - x).abs() < 1e-9, "x {x} became {round_x}");
            assert!((round_y - y).abs() < 1e-9, "y {y} became {round_y}");
        }
    }

    #[test]
    fn board_transform_rejects_empty_outlines_and_zero_sized_images() {
        let design = test_design();

        assert!(BoardTransform::for_board(&PcbDesign::default(), 160, 120).is_err());
        assert!(BoardTransform::for_board(&design, 0, 120).is_err());
        assert!(BoardTransform::for_board(&design, 160, 0).is_err());
    }

    #[test]
    fn default_config_uses_1080p_window_and_vsync() {
        let config = RendererConfig::default();
        assert_eq!(config.window.width, 1920);
        assert_eq!(config.window.height, 1080);
        assert!(config.vsync);
        assert_eq!(config.background, Color::rgb(10, 13, 19));
    }

    #[test]
    fn two_d_rendering_is_deterministic_and_draws_the_board() {
        let design = test_design();
        let mut renderer = renderer(160, 120);

        renderer.render_pcb(&design, ViewMode::TwoD).expect("2D render");
        let first = renderer.framebuffer().pixels().to_vec();
        assert!(non_background_pixels(&renderer) > 2_000);

        renderer.render_pcb(&design, ViewMode::TwoD).expect("repeat 2D render");
        assert_eq!(first, renderer.framebuffer().pixels());
    }

    #[test]
    fn three_d_rendering_uses_a_depth_tested_perspective_view() {
        let design = test_design();
        let mut renderer = renderer(160, 120);

        renderer.render_pcb(&design, ViewMode::TwoD).expect("2D render");
        let flat = renderer.framebuffer().pixels().to_vec();
        renderer.render_pcb(&design, ViewMode::ThreeD).expect("3D render");
        let shaded = renderer.framebuffer().pixels().to_vec();

        assert!(non_background_pixels(&renderer) > 1_000);
        assert_ne!(flat, shaded);
        let colours: HashSet<&[u8]> = shaded.chunks_exact(4).map(|pixel| &pixel[..3]).collect();
        assert!(colours.len() > 5, "3D board should be flat-shaded, got {} colours", colours.len());
    }

    #[test]
    fn renderer_clears_to_the_background_and_writes_ppm() {
        let mut renderer = renderer(8, 6);
        renderer.render().expect("clear");
        assert_eq!(non_background_pixels(&renderer), 0);

        let path = std::env::temp_dir().join(format!("kaicad-scene-{}.ppm", std::process::id()));
        renderer.save_ppm(&path).expect("save ppm");
        let bytes = std::fs::read(&path).expect("read ppm");
        let _ = std::fs::remove_file(&path);
        assert!(bytes.starts_with(b"P6\n8 6\n255\n"));
        assert_eq!(bytes.len(), b"P6\n8 6\n255\n".len() + 8 * 6 * 3);
    }

    #[test]
    fn empty_outlines_and_invalid_sizes_are_rejected() {
        let mut renderer = renderer(16, 16);
        let empty = PcbDesign::default();
        assert!(matches!(
            renderer.render_pcb(&empty, ViewMode::TwoD),
            Err(RendererError::InvalidConfig(_))
        ));
        assert!(matches!(
            Renderer::new(RendererConfig {
                window: WindowConfig::new("zero", 0, 10),
                ..RendererConfig::default()
            }),
            Err(RendererError::InvalidConfig(_))
        ));
    }

    #[test]
    fn fitted_camera_is_finite_and_renders() {
        let design = test_design();
        let camera = Camera::fit(&design);
        assert!(camera.eye.iter().chain(&camera.target).all(|value| value.is_finite()));

        let mut renderer = renderer(80, 60);
        renderer.render_pcb_with_camera(&design, ViewMode::ThreeD, &camera).expect("camera render");
        assert!(non_background_pixels(&renderer) > 100);

        let invalid = Camera { up: [0.0, 0.0, 0.0], ..camera };
        assert!(matches!(
            renderer.render_pcb_with_camera(&design, ViewMode::ThreeD, &invalid),
            Err(RendererError::InvalidConfig(_))
        ));
    }
}
