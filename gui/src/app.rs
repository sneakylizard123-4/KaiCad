//! Application state and the `eframe` window adapter for KaiCad Studio.

use std::path::{Path, PathBuf};

use core::Point;
use core::sheet::LabelType;
use eframe::egui::{self, Color32, RichText, TextureHandle, TextureOptions};
use pcb::{DrcReport, DrcSeverity, PcbDesign, run_drc};
use renderer::{
    BoardTransform, BoardView, Framebuffer, Renderer, RendererConfig, ViewMode, WindowConfig,
};
use uuid::Uuid;

use crate::demo::demo_design;
use crate::document::{DocumentError, save_design};
use crate::editor::{RouteClick, RouteEditError, TrackEditor};
use crate::schematic::{
    SCHEMATIC_ZOOM_STEP, SchematicDocument, SchematicTransform, WIRE_GRID, WireClick,
};

/// Largest viewport side, in pixels, sent to the software rasterizer.
pub const MAX_VIEWPORT_SIDE: u32 = 2048;
const MIN_ZOOM: f64 = 0.25;
const MAX_ZOOM: f64 = 32.0;
const ZOOM_STEP: f64 = 1.25;
const MAX_LISTED_VIOLATIONS: usize = 100;
const ACCENT: Color32 = Color32::from_rgb(232, 145, 51);
const ERROR_COLOR: Color32 = Color32::from_rgb(244, 112, 112);
const WARNING_COLOR: Color32 = Color32::from_rgb(238, 193, 92);
const OK_COLOR: Color32 = Color32::from_rgb(111, 207, 151);
const LAYER_COLORS: [Color32; 4] = [
    Color32::from_rgb(232, 145, 51),
    Color32::from_rgb(74, 139, 231),
    Color32::from_rgb(183, 82, 190),
    Color32::from_rgb(73, 190, 151),
];

/// Whether a status message is informational or reports a failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    Info,
    Error,
}

/// A short message shown in the status bar.
#[derive(Debug, Clone)]
pub struct Notice {
    pub text: String,
    pub kind: NoticeKind,
}

impl Notice {
    fn info(text: impl Into<String>) -> Self {
        Self { text: text.into(), kind: NoticeKind::Info }
    }

    fn error(text: impl Into<String>) -> Self {
        Self { text: text.into(), kind: NoticeKind::Error }
    }
}

/// Active pointer tool in the 2D viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StudioTool {
    Inspect,
    Route,
}

/// Top-level document shown in Studio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StudioMode {
    Board,
    Schematic,
}

/// Headless application state: the board, editing history, DRC and status.
#[derive(Debug)]
pub struct StudioState {
    design: PcbDesign,
    path: Option<PathBuf>,
    view: ViewMode,
    board_view: BoardView,
    report: DrcReport,
    notice: Notice,
    revision: u64,
    editor: TrackEditor,
    tool: StudioTool,
    dirty: bool,
}

impl StudioState {
    /// Start with the built-in demo board.
    pub fn new_demo() -> Self {
        let design = demo_design();
        let report = run_drc(&design, &design.rules);
        let editor = TrackEditor::new(&design);
        Self {
            design,
            path: None,
            view: ViewMode::TwoD,
            board_view: BoardView::default(),
            report,
            notice: Notice::info("Demo board loaded. Select Route to edit copper."),
            revision: 0,
            editor,
            tool: StudioTool::Inspect,
            dirty: false,
        }
    }

    /// Start with a caller-supplied board.
    pub fn from_design(design: PcbDesign) -> Self {
        let report = run_drc(&design, &design.rules);
        let notice = report_notice(&report, "Board loaded");
        let editor = TrackEditor::new(&design);
        Self {
            design,
            path: None,
            view: ViewMode::TwoD,
            board_view: BoardView::default(),
            report,
            notice,
            revision: 0,
            editor,
            tool: StudioTool::Inspect,
            dirty: false,
        }
    }

    /// Replace the current board with a fresh demo and clear its path.
    pub fn reset_demo(&mut self) {
        self.design = demo_design();
        self.path = None;
        self.board_view = BoardView::default();
        self.revision = self.revision.wrapping_add(1);
        self.report = run_drc(&self.design, &self.design.rules);
        self.editor.reset(&self.design);
        self.tool = StudioTool::Inspect;
        self.dirty = false;
        self.notice = Notice::info("Demo board loaded.");
    }

    /// Open a board file. On failure the current board is left unchanged.
    pub fn open_path(&mut self, path: impl AsRef<Path>) -> Result<(), DocumentError> {
        let path = path.as_ref();
        match crate::document::load_design(path) {
            Ok(loaded) => {
                let preserved = loaded.warnings.len();
                self.design = loaded.design;
                self.path = Some(path.to_path_buf());
                self.board_view = BoardView::default();
                self.revision = self.revision.wrapping_add(1);
                self.report = run_drc(&self.design, &self.design.rules);
                self.editor.reset(&self.design);
                self.tool = StudioTool::Inspect;
                self.dirty = false;
                let mut notice = report_notice(&self.report, &format!("Opened {}", path.display()));
                if preserved > 0 {
                    notice.text = format!(
                        "{}; {} KiCad construct(s) preserved for export",
                        notice.text, preserved
                    );
                }
                self.notice = notice;
                Ok(())
            }
            Err(error) => {
                self.notice = Notice::error(error.to_string());
                Err(error)
            }
        }
    }

    /// Save the current board to `path`.
    pub fn save_path(&mut self, path: impl AsRef<Path>) -> Result<(), DocumentError> {
        let path = path.as_ref();
        match save_design(&self.design, path) {
            Ok(()) => {
                self.path = Some(path.to_path_buf());
                self.dirty = false;
                self.notice = Notice::info(format!("Saved {}", path.display()));
                Ok(())
            }
            Err(error) => {
                self.notice = Notice::error(error.to_string());
                Err(error)
            }
        }
    }

    /// Run DRC again and update the report and status message.
    pub fn run_drc_checks(&mut self) {
        self.report = run_drc(&self.design, &self.design.rules);
        self.notice = report_notice(&self.report, "DRC complete");
    }

    /// Update the status bar directly, for UI-only failures such as a blank path.
    pub fn set_status(&mut self, text: impl Into<String>, kind: NoticeKind) {
        self.notice = Notice { text: text.into(), kind };
    }

    pub fn set_view(&mut self, view: ViewMode) {
        self.view = view;
    }

    /// Select the active viewport tool.
    pub fn set_tool(&mut self, tool: StudioTool) {
        self.tool = tool;
        self.notice = match tool {
            StudioTool::Inspect => Notice::info("Inspect tool active."),
            StudioTool::Route => {
                Notice::info("Route tool: click a start point, then a destination.")
            }
        };
    }

    /// Active viewport tool.
    pub fn tool(&self) -> StudioTool {
        self.tool
    }

    /// Whether the board has unsaved edits.
    pub fn dirty(&self) -> bool {
        self.dirty
    }

    /// Headless routing state and history.
    pub fn editor(&self) -> &TrackEditor {
        &self.editor
    }

    /// Select the copper layer for new routes.
    pub fn set_route_layer(&mut self, layer: u16) {
        if self.design.copper_layer_ids().contains(&layer) {
            self.editor.set_layer(layer);
            self.notice = Notice::info(format!("Route layer set to id {layer}."));
        } else {
            self.notice = Notice::error(format!("Layer {layer} is not a copper layer."));
        }
    }

    /// Select the net for new routes.
    pub fn set_route_net(&mut self, net_id: Uuid) {
        self.editor.set_net(net_id);
        self.notice = Notice::info("Route net updated.");
    }

    /// Set the width for new tracks.
    pub fn set_route_width(&mut self, width: f64) {
        if self.editor.set_width(width) {
            self.notice = Notice::info(format!("Track width set to {width:.2} mm."));
        } else {
            self.notice = Notice::error("Track width must be finite and positive.");
        }
    }

    /// Handle a 2D viewport click at `point` in board millimetres.
    ///
    /// The first click sets a start, the second routes and commits copper. A
    /// failed attempt leaves the board and any pending start unchanged.
    pub fn route_at(&mut self, point: Point) -> Result<RouteClick, RouteEditError> {
        let result = if self.editor.pending_start().is_some() {
            self.editor.route_to(&mut self.design, point)
        } else {
            self.editor.begin_route(&self.design, point)
        };

        match result {
            Ok(click) => {
                match click {
                    RouteClick::Started => {
                        if let Some(start) = self.editor.pending_start() {
                            self.notice = Notice::info(format!(
                                "Start set at {:.2}, {:.2} mm. Click a destination.",
                                start.x, start.y
                            ));
                        }
                    }
                    RouteClick::Routed { tracks, vias } => {
                        self.revision = self.revision.wrapping_add(1);
                        self.report = run_drc(&self.design, &self.design.rules);
                        self.dirty = true;
                        let errors = self.report.count(DrcSeverity::Error);
                        self.notice = Notice {
                            text: format!(
                                "Routed {tracks} track(s) and {vias} via(s); {errors} DRC error(s)."
                            ),
                            kind: if errors == 0 { NoticeKind::Info } else { NoticeKind::Error },
                        };
                    }
                }
                Ok(click)
            }
            Err(error) => {
                self.notice = Notice::error(error.to_string());
                Err(error)
            }
        }
    }

    /// Cancel a pending route start.
    pub fn cancel_route(&mut self) {
        self.editor.cancel_route();
        self.notice = Notice::info("Route cancelled.");
    }

    /// Undo the most recent committed route.
    pub fn undo_edit(&mut self) -> bool {
        if !self.editor.undo(&mut self.design) {
            self.notice = Notice::info("Nothing to undo.");
            return false;
        }
        self.revision = self.revision.wrapping_add(1);
        self.report = run_drc(&self.design, &self.design.rules);
        self.dirty = true;
        self.notice = Notice::info("Undid the last track edit.");
        true
    }

    /// Redo the most recently undone route.
    pub fn redo_edit(&mut self) -> bool {
        if !self.editor.redo(&mut self.design) {
            self.notice = Notice::info("Nothing to redo.");
            return false;
        }
        self.revision = self.revision.wrapping_add(1);
        self.report = run_drc(&self.design, &self.design.rules);
        self.dirty = true;
        self.notice = Notice::info("Redid the last track edit.");
        true
    }

    pub fn design(&self) -> &PcbDesign {
        &self.design
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn view(&self) -> ViewMode {
        self.view
    }

    /// Current 2D zoom and pan.
    pub fn board_view(&self) -> BoardView {
        self.board_view
    }

    /// Zoom the 2D view by `factor` around the current centre.
    ///
    /// The zoom is clamped to the `0.25..=32.0` range. Non-finite or
    /// non-positive factors are rejected. Returns whether the view changed.
    pub fn zoom_by(&mut self, factor: f64) -> bool {
        let Some(zoom) = clamped_zoom(self.board_view.zoom, factor) else {
            return false;
        };
        self.board_view.zoom = zoom;
        true
    }

    /// Zoom the 2D view around an image pixel.
    ///
    /// `pixel` is in raster coordinates (top-left origin) for an image of
    /// `image` size. The board point under that pixel stays fixed while the
    /// zoom changes, so wheel zoom follows the pointer. Returns whether the
    /// view changed.
    pub fn zoom_at(&mut self, factor: f64, pixel: [f64; 2], image: [u32; 2]) -> bool {
        let Some(zoom) = clamped_zoom(self.board_view.zoom, factor) else {
            return false;
        };
        let Ok(fit) = BoardTransform::for_board(&self.design, image[0], image[1]) else {
            return false;
        };
        let Ok(current) =
            BoardTransform::for_view(&self.design, image[0], image[1], self.board_view)
        else {
            return false;
        };
        let [anchor_x, anchor_y] = current.pixel_to_board(pixel[0], pixel[1]);
        let scale = fit.scale() * zoom;
        let center = [
            anchor_x - (pixel[0] - image[0] as f64 / 2.0) / scale,
            anchor_y + (pixel[1] - image[1] as f64 / 2.0) / scale,
        ];
        if !scale.is_finite() || scale <= 0.0 || center.iter().any(|value| !value.is_finite()) {
            return false;
        }
        self.board_view = BoardView { center: Some(center), zoom };
        true
    }

    /// Pan the 2D view by a board-space delta in millimetres.
    ///
    /// A positive `dx` moves the visible board left; a positive `dy` moves it
    /// down. Returns whether the view changed.
    pub fn pan_by_board(&mut self, dx: f64, dy: f64) -> bool {
        if !dx.is_finite() || !dy.is_finite() {
            return false;
        }
        let center = self.board_view.center.unwrap_or_else(|| {
            self.design
                .outline
                .bounding_box()
                .map(|bounds| {
                    [(bounds.min.x + bounds.max.x) / 2.0, (bounds.min.y + bounds.max.y) / 2.0]
                })
                .unwrap_or([0.0, 0.0])
        });
        let next = [center[0] + dx, center[1] + dy];
        if next.iter().any(|value| !value.is_finite()) {
            return false;
        }
        self.board_view.center = Some(next);
        true
    }

    /// Restore the automatically fitted 2D view.
    pub fn reset_board_view(&mut self) {
        self.board_view = BoardView::default();
    }

    pub fn report(&self) -> &DrcReport {
        &self.report
    }

    pub fn notice(&self) -> &Notice {
        &self.notice
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// File name when opened, otherwise a friendly demo label.
    pub fn display_name(&self) -> String {
        self.path
            .as_deref()
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Demo board".to_string())
    }
}

fn clamped_zoom(current: f64, factor: f64) -> Option<f64> {
    if !factor.is_finite() || factor <= 0.0 {
        return None;
    }
    let zoom = (current * factor).clamp(MIN_ZOOM, MAX_ZOOM);
    (zoom != current).then_some(zoom)
}

fn report_notice(report: &DrcReport, prefix: &str) -> Notice {
    let errors = report.count(DrcSeverity::Error);
    let warnings = report.count(DrcSeverity::Warning);
    let kind = if errors == 0 { NoticeKind::Info } else { NoticeKind::Error };
    Notice { text: format!("{prefix}: {errors} error(s), {warnings} warning(s)"), kind }
}

/// Convert an available UI size into a bounded raster size.
///
/// The result keeps the panel's aspect ratio and caps the longest side at
/// [`MAX_VIEWPORT_SIDE`]. Non-finite or empty inputs produce a 1x1 image.
pub fn viewport_size(available: [f32; 2], pixels_per_point: f32) -> [u32; 2] {
    let scale =
        if pixels_per_point.is_finite() && pixels_per_point > 0.0 { pixels_per_point } else { 1.0 };
    let width = if available[0].is_finite() { available[0].max(0.0) } else { 0.0 };
    let height = if available[1].is_finite() { available[1].max(0.0) } else { 0.0 };
    let mut pixel_width = (width * scale).max(1.0);
    let mut pixel_height = (height * scale).max(1.0);
    let shrink = (MAX_VIEWPORT_SIDE as f32 / pixel_width.max(pixel_height)).min(1.0);
    pixel_width *= shrink;
    pixel_height *= shrink;

    [pixel_width.round().max(1.0) as u32, pixel_height.round().max(1.0) as u32]
}

fn color_image(framebuffer: &Framebuffer) -> egui::ColorImage {
    egui::ColorImage::from_rgba_unmultiplied(
        [framebuffer.width() as usize, framebuffer.height() as usize],
        framebuffer.pixels(),
    )
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct RenderKey {
    width: u32,
    height: u32,
    view: ViewMode,
    revision: u64,
    board_view: BoardView,
}

struct Viewport {
    texture: Option<TextureHandle>,
    key: Option<RenderKey>,
    error: Option<String>,
}

impl Viewport {
    fn new() -> Self {
        Self { texture: None, key: None, error: None }
    }

    fn prepare(
        &mut self,
        ctx: &egui::Context,
        design: &PcbDesign,
        view: ViewMode,
        revision: u64,
        board_view: BoardView,
        available: [f32; 2],
    ) -> Option<egui::TextureId> {
        let [width, height] = viewport_size(available, ctx.pixels_per_point());
        if width < 2 || height < 2 {
            return None;
        }
        let key = RenderKey { width, height, view, revision, board_view };
        if self.key != Some(key) {
            self.key = Some(key);
            match rasterize(design, view, board_view, width, height) {
                Ok(image) => {
                    self.error = None;
                    match self.texture.take() {
                        Some(mut texture) => {
                            texture.set(image, TextureOptions::NEAREST);
                            self.texture = Some(texture);
                        }
                        None => {
                            self.texture = Some(ctx.load_texture(
                                "kaicad-board",
                                image,
                                TextureOptions::NEAREST,
                            ));
                        }
                    }
                }
                Err(error) => {
                    self.error = Some(error);
                    self.texture = None;
                }
            }
        }
        self.texture.as_ref().map(TextureHandle::id)
    }

    fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

fn rasterize(
    design: &PcbDesign,
    view: ViewMode,
    board_view: BoardView,
    width: u32,
    height: u32,
) -> Result<egui::ColorImage, String> {
    let config = RendererConfig {
        window: WindowConfig::new("KaiCad viewport", width, height),
        ..RendererConfig::default()
    };
    let mut renderer =
        Renderer::new(config).map_err(|error| format!("renderer setup failed: {error}"))?;
    match view {
        ViewMode::TwoD => renderer.render_pcb_view(design, board_view),
        ViewMode::ThreeD => renderer.render_pcb(design, view),
    }
    .map_err(|error| format!("renderer failed: {error}"))?;
    Ok(color_image(renderer.framebuffer()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileMode {
    OpenBoard,
    SaveBoard,
    OpenSchematic,
    SaveSchematic,
}

impl FileMode {
    fn title(self) -> &'static str {
        match self {
            Self::OpenBoard => "Open KiCad board",
            Self::SaveBoard => "Save KiCad board as",
            Self::OpenSchematic => "Open KiCad schematic",
            Self::SaveSchematic => "Save KiCad schematic as",
        }
    }

    fn action(self) -> &'static str {
        match self {
            Self::OpenBoard | Self::OpenSchematic => "Open",
            Self::SaveBoard | Self::SaveSchematic => "Save",
        }
    }
}

struct FileDialog {
    mode: FileMode,
    path: String,
    focused: bool,
}

/// Native KaiCad Studio window.
pub struct StudioApp {
    state: StudioState,
    mode: StudioMode,
    schematic: Option<SchematicDocument>,
    schematic_path_input: String,
    schematic_notice: Notice,
    path_input: String,
    viewport: Viewport,
    file_dialog: Option<FileDialog>,
    show_about: bool,
}

impl StudioApp {
    /// Create the app, optionally opening a board or schematic supplied on the command line.
    pub fn new(cc: &eframe::CreationContext<'_>, initial_path: Option<PathBuf>) -> Self {
        configure_style(&cc.egui_ctx);
        let mut app = Self {
            state: StudioState::new_demo(),
            mode: StudioMode::Board,
            schematic: Some(SchematicDocument::demo()),
            schematic_path_input: String::new(),
            schematic_notice: Notice::info(
                "Demo schematic loaded. Click two points to add a wire.",
            ),
            path_input: String::new(),
            viewport: Viewport::new(),
            file_dialog: None,
            show_about: false,
        };
        if let Some(path) = initial_path {
            if is_schematic_path(&path) {
                app.open_schematic(path);
            } else {
                app.open_board(path);
            }
        }
        app
    }

    fn open_board(&mut self, path: PathBuf) {
        self.path_input = path.display().to_string();
        let _ = self.state.open_path(&path);
        self.mode = StudioMode::Board;
    }

    fn new_schematic(&mut self) {
        self.schematic = Some(SchematicDocument::empty());
        self.schematic_path_input.clear();
        self.mode = StudioMode::Schematic;
        self.schematic_notice =
            Notice::info("New schematic created. Click two points to add a wire.");
    }

    fn open_schematic(&mut self, path: PathBuf) {
        self.schematic_path_input = path.display().to_string();
        let mut document = self.schematic.take().unwrap_or_else(SchematicDocument::empty);
        match document.open_path(&path) {
            Ok(()) => {
                let preserved = document.warnings().len();
                self.schematic = Some(document);
                self.mode = StudioMode::Schematic;
                let mut text = format!("Opened {}", path.display());
                if preserved > 0 {
                    text.push_str(&format!("; {preserved} schematic construct(s) preserved"));
                }
                self.schematic_notice = Notice::info(text);
            }
            Err(error) => {
                self.schematic = Some(document);
                self.mode = StudioMode::Schematic;
                self.schematic_notice = Notice::error(error.to_string());
            }
        }
    }

    fn save_schematic_to(&mut self, path: PathBuf) {
        let Some(document) = self.schematic.as_mut() else {
            return;
        };
        match document.save_path(&path) {
            Ok(()) => {
                self.schematic_path_input = path.display().to_string();
                self.schematic_notice = Notice::info(format!("Saved {}", path.display()));
            }
            Err(error) => {
                self.schematic_notice = Notice::error(error.to_string());
            }
        }
    }

    fn save_schematic_current(&mut self) {
        let path =
            self.schematic.as_ref().and_then(|document| document.path()).map(Path::to_path_buf);
        match path {
            Some(path) => self.save_schematic_to(path),
            None => self.open_save_schematic_dialog(),
        }
    }

    fn save_board(&mut self, path: PathBuf) {
        if self.state.save_path(&path).is_ok() {
            self.path_input = path.display().to_string();
        }
    }

    fn save_current(&mut self) {
        if let Some(path) = self.state.path().map(Path::to_path_buf) {
            self.save_board(path);
        } else {
            self.open_save_dialog();
        }
    }

    fn suggested_path(&self) -> String {
        if let Some(path) = self.state.path() {
            return path.display().to_string();
        }
        if !self.path_input.trim().is_empty() {
            return self.path_input.trim().to_string();
        }
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("board.kicad_pcb")
            .display()
            .to_string()
    }

    fn suggested_schematic_path(&self) -> String {
        if let Some(path) = self.schematic.as_ref().and_then(SchematicDocument::path) {
            return path.display().to_string();
        }
        if !self.schematic_path_input.trim().is_empty() {
            return self.schematic_path_input.trim().to_string();
        }
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("schematic.kicad_sch")
            .display()
            .to_string()
    }

    fn open_dialog(&mut self) {
        self.file_dialog = Some(FileDialog {
            mode: FileMode::OpenBoard,
            path: self.suggested_path(),
            focused: false,
        });
    }

    fn open_save_dialog(&mut self) {
        self.file_dialog = Some(FileDialog {
            mode: FileMode::SaveBoard,
            path: self.suggested_path(),
            focused: false,
        });
    }

    fn open_schematic_dialog(&mut self) {
        self.file_dialog = Some(FileDialog {
            mode: FileMode::OpenSchematic,
            path: self.suggested_schematic_path(),
            focused: false,
        });
    }

    fn open_save_schematic_dialog(&mut self) {
        self.file_dialog = Some(FileDialog {
            mode: FileMode::SaveSchematic,
            path: self.suggested_schematic_path(),
            focused: false,
        });
    }

    fn confirm_dialog(&mut self) {
        let Some(dialog) = self.file_dialog.take() else {
            return;
        };
        let mut path = PathBuf::from(dialog.path.trim());
        if path.as_os_str().is_empty() {
            match dialog.mode {
                FileMode::OpenSchematic | FileMode::SaveSchematic => {
                    self.mode = StudioMode::Schematic;
                    self.schematic_notice = Notice::error("Enter a schematic path first.");
                }
                FileMode::OpenBoard | FileMode::SaveBoard => {
                    self.state.set_status("Enter a board path first.", NoticeKind::Error);
                }
            }
            return;
        }
        if path.extension().is_none() {
            match dialog.mode {
                FileMode::SaveBoard => {
                    path.set_extension("kicad_pcb");
                }
                FileMode::SaveSchematic => {
                    path.set_extension("kicad_sch");
                }
                FileMode::OpenBoard | FileMode::OpenSchematic => {}
            }
        }
        match dialog.mode {
            FileMode::OpenBoard => self.open_board(path),
            FileMode::SaveBoard => self.save_board(path),
            FileMode::OpenSchematic => self.open_schematic(path),
            FileMode::SaveSchematic => self.save_schematic_to(path),
        }
    }

    fn handle_input(&mut self, ctx: &egui::Context) {
        let typing = ctx.wants_keyboard_input();
        let (save, open) = ctx.input_mut(|input| {
            (
                input.consume_key(egui::Modifiers::COMMAND, egui::Key::S),
                input.consume_key(egui::Modifiers::COMMAND, egui::Key::O),
            )
        });
        if save {
            if self.mode == StudioMode::Schematic {
                self.save_schematic_current();
            } else {
                self.save_current();
            }
        }
        if open {
            if self.mode == StudioMode::Schematic {
                self.open_schematic_dialog();
            } else {
                self.open_dialog();
            }
        }

        let (undo, redo) = if typing || self.file_dialog.is_some() {
            (false, false)
        } else {
            ctx.input_mut(|input| {
                (
                    input.consume_key(egui::Modifiers::COMMAND, egui::Key::Z),
                    input.consume_key(egui::Modifiers::COMMAND, egui::Key::Y),
                )
            })
        };
        if undo {
            if self.mode == StudioMode::Schematic {
                self.undo_schematic();
            } else {
                self.state.undo_edit();
            }
        }
        if redo {
            if self.mode == StudioMode::Schematic {
                self.redo_schematic();
            } else {
                self.state.redo_edit();
            }
        }

        if self.mode == StudioMode::Board
            && !typing
            && self.file_dialog.is_none()
            && ctx.input(|input| input.key_pressed(egui::Key::R))
        {
            let tool = if self.state.tool() == StudioTool::Route {
                StudioTool::Inspect
            } else if self.state.view() == ViewMode::TwoD {
                StudioTool::Route
            } else {
                self.state.set_status("Switch to the 2D view to route tracks.", NoticeKind::Error);
                self.state.tool()
            };
            self.state.set_tool(tool);
        }

        if !typing && self.file_dialog.is_none() {
            let (zoom_in, zoom_out, fit) = ctx.input(|input| {
                (
                    input.key_pressed(egui::Key::Plus) || input.key_pressed(egui::Key::Equals),
                    input.key_pressed(egui::Key::Minus),
                    input.key_pressed(egui::Key::Num0),
                )
            });
            let zoomable =
                self.mode == StudioMode::Schematic || self.state.view() == ViewMode::TwoD;
            if zoomable {
                if zoom_in {
                    self.zoom_active(SCHEMATIC_ZOOM_STEP);
                }
                if zoom_out {
                    self.zoom_active(1.0 / SCHEMATIC_ZOOM_STEP);
                }
                if fit {
                    self.fit_active();
                }
            }
        }

        if self.mode == StudioMode::Board
            && !typing
            && ctx.input(|input| input.key_pressed(egui::Key::F5))
        {
            self.state.run_drc_checks();
        }
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            if self.file_dialog.is_some() {
                self.file_dialog = None;
            } else if self.mode == StudioMode::Schematic
                && self.schematic.as_ref().is_some_and(|doc| doc.pending_start().is_some())
            {
                self.cancel_schematic_wire();
            } else if self.mode == StudioMode::Board
                && self.state.editor().pending_start().is_some()
            {
                self.state.cancel_route();
            }
        }

        let dropped =
            ctx.input(|input| input.raw.dropped_files.first().and_then(|file| file.path.clone()));
        if let Some(path) = dropped {
            if is_schematic_path(&path) {
                self.open_schematic(path);
            } else {
                self.open_board(path);
            }
        }
    }

    fn zoom_active(&mut self, factor: f64) {
        if self.mode == StudioMode::Schematic {
            if let Some(document) = self.schematic.as_mut() {
                document.zoom_by(factor);
            }
        } else {
            self.state.zoom_by(factor);
        }
    }

    fn fit_active(&mut self) {
        if self.mode == StudioMode::Schematic {
            if let Some(document) = self.schematic.as_mut() {
                document.reset_view();
            }
        } else {
            self.state.reset_board_view();
        }
    }

    fn undo_schematic(&mut self) {
        let changed = self.schematic.as_mut().is_some_and(SchematicDocument::undo);
        self.schematic_notice = if changed {
            Notice::info("Undid the last schematic edit.")
        } else {
            Notice::info("Nothing to undo in the schematic.")
        };
    }

    fn redo_schematic(&mut self) {
        let changed = self.schematic.as_mut().is_some_and(SchematicDocument::redo);
        self.schematic_notice = if changed {
            Notice::info("Redid the last schematic edit.")
        } else {
            Notice::info("Nothing to redo in the schematic.")
        };
    }

    fn cancel_schematic_wire(&mut self) {
        if let Some(document) = self.schematic.as_mut() {
            document.cancel_wire();
        }
        self.schematic_notice = Notice::info("Schematic wire cancelled.");
    }

    fn menu_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("kaicad-menu").show(ctx, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.label(RichText::new("KaiCad").strong().size(17.0).color(ACCENT));
                ui.add_space(4.0);
                ui.menu_button("File", |ui| {
                    if ui.button("New demo board").clicked() {
                        self.state.reset_demo();
                        self.path_input.clear();
                        self.mode = StudioMode::Board;
                    }
                    if ui.button("Open board…").clicked() {
                        self.open_dialog();
                    }
                    if ui
                        .add_enabled(self.state.path().is_some(), egui::Button::new("Save board"))
                        .clicked()
                    {
                        self.save_current();
                    }
                    if ui.button("Save board as…").clicked() {
                        self.open_save_dialog();
                    }
                    ui.separator();
                    if ui.button("New schematic").clicked() {
                        self.new_schematic();
                    }
                    if ui.button("Open schematic…").clicked() {
                        self.open_schematic_dialog();
                    }
                    if ui
                        .add_enabled(
                            self.schematic
                                .as_ref()
                                .is_some_and(|document| document.path().is_some()),
                            egui::Button::new("Save schematic"),
                        )
                        .clicked()
                    {
                        self.save_schematic_current();
                    }
                    if ui.button("Save schematic as…").clicked() {
                        self.open_save_schematic_dialog();
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("Edit", |ui| {
                    if self.mode == StudioMode::Schematic {
                        let can_undo =
                            self.schematic.as_ref().is_some_and(SchematicDocument::can_undo);
                        let can_redo =
                            self.schematic.as_ref().is_some_and(SchematicDocument::can_redo);
                        let pending = self
                            .schematic
                            .as_ref()
                            .is_some_and(|document| document.pending_start().is_some());
                        if ui
                            .add_enabled(
                                can_undo,
                                egui::Button::new("Undo schematic edit (Ctrl+Z)"),
                            )
                            .clicked()
                        {
                            self.undo_schematic();
                        }
                        if ui
                            .add_enabled(
                                can_redo,
                                egui::Button::new("Redo schematic edit (Ctrl+Y)"),
                            )
                            .clicked()
                        {
                            self.redo_schematic();
                        }
                        if ui.add_enabled(pending, egui::Button::new("Cancel wire (Esc)")).clicked()
                        {
                            self.cancel_schematic_wire();
                        }
                    } else {
                        if ui
                            .add_enabled(
                                self.state.editor().can_undo(),
                                egui::Button::new("Undo track edit (Ctrl+Z)"),
                            )
                            .clicked()
                        {
                            self.state.undo_edit();
                        }
                        if ui
                            .add_enabled(
                                self.state.editor().can_redo(),
                                egui::Button::new("Redo track edit (Ctrl+Y)"),
                            )
                            .clicked()
                        {
                            self.state.redo_edit();
                        }
                        if ui
                            .add_enabled(
                                self.state.editor().pending_start().is_some(),
                                egui::Button::new("Cancel route (Esc)"),
                            )
                            .clicked()
                        {
                            self.state.cancel_route();
                        }
                    }
                });
                ui.menu_button("View", |ui| {
                    if ui
                        .selectable_label(self.mode == StudioMode::Board, "Board document")
                        .clicked()
                    {
                        self.mode = StudioMode::Board;
                    }
                    let has_schematic = self.schematic.is_some();
                    ui.add_enabled_ui(has_schematic, |ui| {
                        if ui
                            .selectable_label(
                                self.mode == StudioMode::Schematic,
                                "Schematic document",
                            )
                            .clicked()
                        {
                            self.mode = StudioMode::Schematic;
                        }
                    });
                    ui.separator();
                    let board_mode = self.mode == StudioMode::Board;
                    ui.add_enabled_ui(board_mode, |ui| {
                        if ui
                            .selectable_label(self.state.view() == ViewMode::TwoD, "2D top view")
                            .clicked()
                        {
                            self.state.set_view(ViewMode::TwoD);
                        }
                        if ui
                            .selectable_label(
                                self.state.view() == ViewMode::ThreeD,
                                "3D perspective",
                            )
                            .clicked()
                        {
                            self.state.set_view(ViewMode::ThreeD);
                        }
                    });
                    ui.separator();
                    let zoomable =
                        self.mode == StudioMode::Schematic || self.state.view() == ViewMode::TwoD;
                    if ui.add_enabled(zoomable, egui::Button::new("Zoom in (+)")).clicked() {
                        self.zoom_active(ZOOM_STEP);
                    }
                    if ui.add_enabled(zoomable, egui::Button::new("Zoom out (-)")).clicked() {
                        self.zoom_active(1.0 / ZOOM_STEP);
                    }
                    if ui.add_enabled(zoomable, egui::Button::new("Fit (0)")).clicked() {
                        self.fit_active();
                    }
                });
                ui.menu_button("Tools", |ui| {
                    if self.mode == StudioMode::Schematic {
                        ui.label("Schematic wire capture");
                        if ui.button("Cancel wire (Esc)").clicked() {
                            self.cancel_schematic_wire();
                        }
                        if ui.button("Fit schematic (0)").clicked() {
                            self.fit_active();
                        }
                    } else {
                        if ui
                            .selectable_label(self.state.tool() == StudioTool::Inspect, "Inspect")
                            .clicked()
                        {
                            self.state.set_tool(StudioTool::Inspect);
                        }
                        if ui
                            .selectable_label(
                                self.state.tool() == StudioTool::Route,
                                "Route tracks (R)",
                            )
                            .clicked()
                        {
                            if self.state.view() != ViewMode::TwoD {
                                self.state.set_status(
                                    "Switch to the 2D view to route tracks.",
                                    NoticeKind::Error,
                                );
                            } else {
                                self.state.set_tool(StudioTool::Route);
                            }
                        }
                        ui.separator();
                        if ui.button("Run DRC (F5)").clicked() {
                            self.state.run_drc_checks();
                        }
                    }
                });
                ui.menu_button("Help", |ui| {
                    if ui.button("About KaiCad Studio").clicked() {
                        self.show_about = true;
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new("Studio · pre-alpha").weak().small());
                });
            });
        });
    }

    fn left_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("kaicad-inspector")
            .resizable(true)
            .default_width(290.0)
            .width_range(250.0..=420.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.add_space(6.0);
                    if self.mode == StudioMode::Schematic {
                        self.schematic_inspector(ui);
                    } else {
                        ui.heading("Board");
                        ui.label(RichText::new(self.state.display_name()).strong().size(16.0));
                        ui.label(RichText::new(self.view_label()).color(ACCENT).small());
                        ui.horizontal(|ui| {
                            if ui
                                .selectable_label(self.state.view() == ViewMode::TwoD, "2D")
                                .clicked()
                            {
                                self.state.set_view(ViewMode::TwoD);
                            }
                            if ui
                                .selectable_label(self.state.view() == ViewMode::ThreeD, "3D")
                                .clicked()
                            {
                                self.state.set_view(ViewMode::ThreeD);
                            }
                        });
                        if self.state.view() == ViewMode::TwoD {
                            ui.horizontal(|ui| {
                                if ui.small_button("−").on_hover_text("Zoom out (-)").clicked() {
                                    self.state.zoom_by(1.0 / ZOOM_STEP);
                                }
                                ui.label(format!("{:.0}%", self.state.board_view().zoom * 100.0));
                                if ui.small_button("+").on_hover_text("Zoom in (+)").clicked() {
                                    self.state.zoom_by(ZOOM_STEP);
                                }
                                if ui.small_button("Fit").on_hover_text("Fit board (0)").clicked() {
                                    self.state.reset_board_view();
                                }
                            });
                            ui.label(
                                RichText::new("Wheel to zoom · middle-drag to pan").weak().small(),
                            );
                        }
                        ui.separator();
                        self.document_section(ui);
                        ui.separator();
                        self.routing_section(ui);
                        ui.separator();
                        self.board_summary(ui);
                        ui.separator();
                        self.rules_summary(ui);
                        ui.separator();
                        self.drc_section(ui);
                    }
                    ui.add_space(12.0);
                });
            });
    }

    fn schematic_inspector(&mut self, ui: &mut egui::Ui) {
        ui.heading("Schematic");
        let name = self
            .schematic
            .as_ref()
            .and_then(|document| document.path())
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Demo schematic".to_string());
        ui.label(RichText::new(name).strong().size(16.0));
        ui.label(RichText::new("KiCad .kicad_sch workspace").color(ACCENT).small());

        let zoom = self.schematic.as_ref().map(|document| document.view().zoom).unwrap_or(1.0);
        ui.horizontal(|ui| {
            if ui.small_button("−").on_hover_text("Zoom out (-)").clicked() {
                if let Some(document) = self.schematic.as_mut() {
                    document.zoom_by(1.0 / SCHEMATIC_ZOOM_STEP);
                }
            }
            ui.label(format!("{zoom:.0}%"));
            if ui.small_button("+").on_hover_text("Zoom in (+)").clicked() {
                if let Some(document) = self.schematic.as_mut() {
                    document.zoom_by(SCHEMATIC_ZOOM_STEP);
                }
            }
            if ui.small_button("Fit").on_hover_text("Fit schematic (0)").clicked() {
                self.fit_active();
            }
        });
        ui.label(RichText::new("Wheel to zoom · drag to pan").weak().small());

        ui.separator();
        ui.heading("Document");
        ui.add(
            egui::TextEdit::singleline(&mut self.schematic_path_input)
                .hint_text("path to schematic.kicad_sch")
                .desired_width(f32::INFINITY),
        );
        ui.label(
            RichText::new("Schematics open and save as .kicad_sch; unknown nodes are retained.")
                .weak()
                .small(),
        );
        ui.horizontal(|ui| {
            if ui.button("Open").clicked() {
                let path = PathBuf::from(self.schematic_path_input.trim());
                if path.as_os_str().is_empty() {
                    self.schematic_notice = Notice::error("Enter a schematic path first.");
                } else {
                    self.open_schematic(path);
                }
            }
            if ui.button("Save").clicked() {
                let path = PathBuf::from(self.schematic_path_input.trim());
                if path.as_os_str().is_empty() {
                    self.open_save_schematic_dialog();
                } else {
                    self.save_schematic_to(path);
                }
            }
            if ui.button("Save as…").clicked() {
                self.open_save_schematic_dialog();
            }
            if ui.button("New").clicked() {
                self.new_schematic();
            }
        });

        ui.separator();
        ui.heading("Wire capture");
        let (wires, pending, can_undo, can_redo) = self
            .schematic
            .as_ref()
            .map(|document| {
                let wires = document.root_sheet().map_or(0, |sheet| sheet.wires.len());
                (wires, document.pending_start(), document.can_undo(), document.can_redo())
            })
            .unwrap_or((0, None, false, false));
        ui.label(format!("{wires} wire(s) on the root sheet"));
        ui.label(
            RichText::new(format!(
                "Click a wire start, then a destination. Endpoints snap to {WIRE_GRID:.2} mm."
            ))
            .weak()
            .small(),
        );
        if let Some(start) = pending {
            ui.label(
                RichText::new(format!("Start: {:.2}, {:.2} mm", start.x, start.y)).color(ACCENT),
            );
            if ui.button("Cancel wire").clicked() {
                self.cancel_schematic_wire();
            }
        } else {
            ui.label(RichText::new("No wire start point set.").weak().small());
        }
        ui.horizontal(|ui| {
            if ui.add_enabled(can_undo, egui::Button::new("Undo")).clicked() {
                self.undo_schematic();
            }
            if ui.add_enabled(can_redo, egui::Button::new("Redo")).clicked() {
                self.redo_schematic();
            }
        });
        ui.label(
            RichText::new(
                "Symbols are drawn as labeled bodies until schematic symbol graphics are modeled.",
            )
            .weak()
            .small(),
        );

        ui.separator();
        ui.heading("Schematic summary");
        let (sheets, components, labels, junctions, no_connects) = self
            .schematic
            .as_ref()
            .map(|document| {
                let root = document.root_sheet();
                (
                    document.data().sheets.len(),
                    document.data().components.len(),
                    root.map_or(0, |sheet| sheet.labels.len()),
                    root.map_or(0, |sheet| sheet.junctions.len()),
                    root.map_or(0, |sheet| sheet.no_connects.len()),
                )
            })
            .unwrap_or((0, 0, 0, 0, 0));
        egui::Grid::new("kaicad-schematic-summary").num_columns(2).spacing([14.0, 4.0]).show(
            ui,
            |ui| {
                ui.label("Sheets");
                ui.label(sheets.to_string());
                ui.end_row();
                ui.label("Components");
                ui.label(components.to_string());
                ui.end_row();
                ui.label("Labels");
                ui.label(labels.to_string());
                ui.end_row();
                ui.label("Junctions");
                ui.label(junctions.to_string());
                ui.end_row();
                ui.label("No-connects");
                ui.label(no_connects.to_string());
                ui.end_row();
            },
        );

        if let Some(document) = self.schematic.as_ref() {
            if !document.warnings().is_empty() {
                ui.separator();
                ui.heading("Preserved constructs");
                for warning in document.warnings().iter().take(5) {
                    ui.label(RichText::new(warning).weak().small());
                }
            }
        }
    }

    fn view_label(&self) -> &'static str {
        match self.state.view() {
            ViewMode::TwoD => "2D top view",
            ViewMode::ThreeD => "3D perspective",
        }
    }

    fn document_section(&mut self, ui: &mut egui::Ui) {
        ui.heading("Document");
        ui.add(
            egui::TextEdit::singleline(&mut self.path_input)
                .hint_text("path to board.kicad_pcb")
                .desired_width(f32::INFINITY),
        );
        ui.label(
            RichText::new("Boards use .kicad_pcb natively; .json remains supported.")
                .weak()
                .small(),
        );
        let input_path =
            (!self.path_input.trim().is_empty()).then(|| PathBuf::from(self.path_input.trim()));

        ui.horizontal(|ui| {
            if ui.button("Open").clicked() {
                match input_path.clone() {
                    Some(path) => self.open_board(path),
                    None => {
                        self.state.set_status("Enter a board path first.", NoticeKind::Error);
                    }
                }
            }
            if ui.button("Save").clicked() {
                match input_path.clone() {
                    Some(path) => self.save_board(path),
                    None => self.open_save_dialog(),
                }
            }
            if ui.button("Save as…").clicked() {
                self.open_save_dialog();
            }
        });
    }

    fn routing_section(&mut self, ui: &mut egui::Ui) {
        let layers = self.state.design().copper_layer_ids();
        let nets = self.state.design().net_ids();
        let layer = self.state.editor().layer();
        let net_id = self.state.editor().net_id();
        let mut width = self.state.editor().width();
        let pending = self.state.editor().pending_start();
        let can_undo = self.state.editor().can_undo();
        let can_redo = self.state.editor().can_redo();
        let view = self.state.view();

        ui.heading("Route copper");
        ui.horizontal(|ui| {
            if ui.selectable_label(self.state.tool() == StudioTool::Inspect, "Inspect").clicked() {
                self.state.set_tool(StudioTool::Inspect);
            }
            if ui.selectable_label(self.state.tool() == StudioTool::Route, "Route").clicked() {
                if view == ViewMode::TwoD {
                    self.state.set_tool(StudioTool::Route);
                } else {
                    self.state
                        .set_status("Switch to the 2D view to route tracks.", NoticeKind::Error);
                }
            }
        });

        if self.state.tool() == StudioTool::Route {
            ui.label(
                RichText::new("Click a pad, same-net copper or free point, then a destination.")
                    .weak()
                    .small(),
            );
            if view != ViewMode::TwoD {
                ui.label(
                    RichText::new("Routing is only available in the 2D view.")
                        .color(WARNING_COLOR)
                        .small(),
                );
            }

            let mut selected_layer = layer;
            egui::ComboBox::from_id_salt("route-layer")
                .selected_text(layer_label(self.state.design(), layer))
                .show_ui(ui, |ui| {
                    for id in &layers {
                        ui.selectable_value(
                            &mut selected_layer,
                            *id,
                            layer_label(self.state.design(), *id),
                        );
                    }
                });
            if selected_layer != layer {
                self.state.set_route_layer(selected_layer);
            }

            if nets.is_empty() {
                ui.label(RichText::new("No assigned nets on this board.").weak().small());
            } else {
                let mut selected_net = net_id;
                egui::ComboBox::from_id_salt("route-net").selected_text(net_label(net_id)).show_ui(
                    ui,
                    |ui| {
                        for id in &nets {
                            ui.selectable_value(&mut selected_net, *id, net_label(*id));
                        }
                    },
                );
                if selected_net != net_id {
                    self.state.set_route_net(selected_net);
                }
            }

            ui.horizontal(|ui| {
                ui.label("Width");
                if ui
                    .add(
                        egui::DragValue::new(&mut width)
                            .range(0.05..=5.0)
                            .speed(0.01)
                            .suffix(" mm"),
                    )
                    .changed()
                {
                    self.state.set_route_width(width);
                }
            });

            if let Some(start) = pending {
                ui.label(
                    RichText::new(format!("Start: {:.2}, {:.2} mm", start.x, start.y))
                        .color(ACCENT),
                );
                if ui.button("Cancel route").clicked() {
                    self.state.cancel_route();
                }
            } else {
                ui.label(RichText::new("No start point set.").weak().small());
            }
            ui.label(
                RichText::new("Routes commit immediately and run DRC before applying.")
                    .weak()
                    .small(),
            );
        }

        ui.horizontal(|ui| {
            if ui.add_enabled(can_undo, egui::Button::new("Undo")).clicked() {
                self.state.undo_edit();
            }
            if ui.add_enabled(can_redo, egui::Button::new("Redo")).clicked() {
                self.state.redo_edit();
            }
            if self.state.dirty() {
                ui.label(RichText::new("modified").color(WARNING_COLOR).small());
            }
        });
    }

    fn board_summary(&self, ui: &mut egui::Ui) {
        let design = self.state.design();
        let bounds = design.outline.bounding_box();
        let size = bounds
            .map(|bounds| {
                ((bounds.max.x - bounds.min.x).max(0.0), (bounds.max.y - bounds.min.y).max(0.0))
            })
            .unwrap_or((0.0, 0.0));

        ui.heading("Board summary");
        egui::Grid::new("kaicad-board-summary").num_columns(2).spacing([14.0, 4.0]).show(
            ui,
            |ui| {
                ui.label("Outline");
                ui.label(format!("{} point(s)", design.outline.points.len()));
                ui.end_row();

                ui.label("Size");
                ui.label(format!("{:.1} x {:.1} mm", size.0, size.1));
                ui.end_row();

                ui.label("Area");
                ui.label(format!("{:.1} mm^2", design.outline.area()));
                ui.end_row();

                ui.label("Copper");
                ui.label(format!("{} layer(s)", design.copper_layer_count()));
                ui.end_row();

                ui.label("Footprints");
                ui.label(design.footprints.len().to_string());
                ui.end_row();

                ui.label("Tracks");
                ui.label(design.tracks.len().to_string());
                ui.end_row();

                ui.label("Vias");
                ui.label(design.vias.len().to_string());
                ui.end_row();
            },
        );
    }

    fn rules_summary(&self, ui: &mut egui::Ui) {
        let rules = &self.state.design().rules;
        ui.heading("Design rules");
        egui::Grid::new("kaicad-rule-summary").num_columns(2).spacing([14.0, 4.0]).show(ui, |ui| {
            ui.label("Track width");
            ui.label(format!("{:.2} mm", rules.min_track_width));
            ui.end_row();

            ui.label("Clearance");
            ui.label(format!("{:.2} mm", rules.clearance));
            ui.end_row();

            ui.label("Via drill");
            ui.label(format!("{:.2} mm", rules.min_via_drill));
            ui.end_row();

            ui.label("Annular ring");
            ui.label(format!("{:.2} mm", rules.min_annular_ring));
            ui.end_row();
        });
    }

    fn drc_section(&mut self, ui: &mut egui::Ui) {
        ui.heading("Design-rule check");
        if ui.button("Run DRC").clicked() {
            self.state.run_drc_checks();
        }
        let report = self.state.report();
        let errors = report.count(DrcSeverity::Error);
        let warnings = report.count(DrcSeverity::Warning);
        let (label, color) = if errors > 0 {
            (format!("{errors} error(s), {warnings} warning(s)"), ERROR_COLOR)
        } else if warnings > 0 {
            (format!("{warnings} warning(s)"), WARNING_COLOR)
        } else {
            ("Clean".to_string(), OK_COLOR)
        };
        ui.label(RichText::new(label).color(color).strong());

        if report.violations.is_empty() {
            ui.label(RichText::new("No violations.").weak().small());
            return;
        }

        for violation in report.violations.iter().take(MAX_LISTED_VIOLATIONS) {
            let color = match violation.severity {
                DrcSeverity::Error => ERROR_COLOR,
                DrcSeverity::Warning => WARNING_COLOR,
            };
            let severity = match violation.severity {
                DrcSeverity::Error => "error",
                DrcSeverity::Warning => "warning",
            };
            ui.label(
                RichText::new(format!("{severity} · {}", violation.rule_id))
                    .color(color)
                    .small()
                    .strong(),
            );
            ui.label(RichText::new(&violation.message).small());
            if let Some(location) = violation.location {
                ui.label(
                    RichText::new(format!("at {:.2}, {:.2} mm", location.x, location.y))
                        .weak()
                        .small(),
                );
            }
            ui.add_space(4.0);
        }
        if report.violations.len() > MAX_LISTED_VIOLATIONS {
            ui.label(
                RichText::new(format!(
                    "… and {} more violation(s)",
                    report.violations.len() - MAX_LISTED_VIOLATIONS
                ))
                .weak()
                .small(),
            );
        }
    }

    fn right_panel(&self, ctx: &egui::Context) {
        if self.mode == StudioMode::Schematic {
            self.schematic_right_panel(ctx);
            return;
        }
        let design = self.state.design();
        egui::SidePanel::right("kaicad-stack")
            .resizable(true)
            .default_width(250.0)
            .width_range(220.0..=380.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.add_space(6.0);
                    ui.heading("Layer stack");
                    let layers: Vec<&core::PcbLayer> = if design.stackup.layers.is_empty() {
                        design.layers.iter().filter(|layer| layer.is_copper()).collect()
                    } else {
                        design
                            .stackup
                            .layers
                            .iter()
                            .filter_map(|id| {
                                design
                                    .layers
                                    .iter()
                                    .find(|layer| layer.id == *id && layer.is_copper())
                            })
                            .collect()
                    };
                    if layers.is_empty() {
                        ui.label(RichText::new("No copper layers.").weak());
                    }
                    for (index, layer) in layers.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("●").color(LAYER_COLORS[index % LAYER_COLORS.len()]),
                            );
                            ui.label(&layer.name);
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        RichText::new(format!("id {}", layer.id)).weak().small(),
                                    );
                                },
                            );
                        });
                    }

                    ui.separator();
                    ui.heading("Footprints");
                    if design.footprints.is_empty() {
                        ui.label(RichText::new("None placed.").weak());
                    }
                    for footprint in &design.footprints {
                        ui.label(RichText::new(&footprint.reference_designator).strong());
                        ui.label(
                            RichText::new(format!(
                                "{} · {:.1}, {:.1} mm",
                                footprint.name, footprint.position.x, footprint.position.y
                            ))
                            .weak()
                            .small(),
                        );
                        ui.label(
                            RichText::new(format!("{} pad(s)", footprint.pad_positions.len()))
                                .weak()
                                .small(),
                        );
                        ui.add_space(5.0);
                    }
                });
            });
    }

    fn schematic_right_panel(&self, ctx: &egui::Context) {
        egui::SidePanel::right("kaicad-schematic-stack")
            .resizable(true)
            .default_width(260.0)
            .width_range(220.0..=380.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.add_space(6.0);
                    ui.heading("Components");
                    let Some(document) = self.schematic.as_ref() else {
                        ui.label(RichText::new("No schematic open.").weak());
                        return;
                    };
                    if document.data().components.is_empty() {
                        ui.label(RichText::new("None placed.").weak());
                    }
                    for component in &document.data().components {
                        let value = component
                            .properties
                            .iter()
                            .find(|property| property.name == "Value")
                            .map(|property| property.value.as_str())
                            .unwrap_or_default();
                        ui.label(RichText::new(&component.reference_designator).strong());
                        ui.label(
                            RichText::new(format!("{} · {}", component.library_id, value))
                                .weak()
                                .small(),
                        );
                        ui.label(
                            RichText::new(format!(
                                "{:.1}, {:.1} mm",
                                component.placement.position.x, component.placement.position.y
                            ))
                            .weak()
                            .small(),
                        );
                        ui.add_space(5.0);
                    }

                    ui.separator();
                    ui.heading("Sheets");
                    for sheet in &document.data().sheets {
                        ui.label(RichText::new(&sheet.name).strong());
                        if !sheet.file_name.is_empty() {
                            ui.label(RichText::new(&sheet.file_name).weak().small());
                        }
                        ui.add_space(4.0);
                    }
                });
            });
    }

    fn status_bar(&self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("kaicad-status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let notice = if self.mode == StudioMode::Schematic {
                    &self.schematic_notice
                } else {
                    self.state.notice()
                };
                let color = match notice.kind {
                    NoticeKind::Info => ui.visuals().text_color(),
                    NoticeKind::Error => ERROR_COLOR,
                };
                ui.label(RichText::new(&notice.text).color(color));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (renderer_label, path) = if self.mode == StudioMode::Schematic {
                        let zoom = self
                            .schematic
                            .as_ref()
                            .map(|document| document.view().zoom)
                            .unwrap_or(1.0);
                        (
                            format!("{zoom:.0}% · schematic"),
                            self.schematic
                                .as_ref()
                                .and_then(SchematicDocument::path)
                                .map(Path::to_path_buf),
                        )
                    } else {
                        let label = if self.state.view() == ViewMode::TwoD {
                            format!("{:.0}% · software 2D/3D", self.state.board_view().zoom * 100.0)
                        } else {
                            "software 3D".to_string()
                        };
                        (label, self.state.path().map(Path::to_path_buf))
                    };
                    ui.label(RichText::new(renderer_label).weak().small());
                    ui.separator();
                    if let Some(path) = path {
                        ui.label(RichText::new(path.display().to_string()).weak().small());
                    }
                });
            });
        });
    }

    fn central_panel(&mut self, ctx: &egui::Context) {
        if self.mode == StudioMode::Schematic {
            self.schematic_canvas(ctx);
            return;
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            let available = ui.available_size();
            if available.x < 2.0 || available.y < 2.0 {
                return;
            }

            let view = self.state.view();
            let revision = self.state.revision();
            let board_view = self.state.board_view();
            let texture = self.viewport.prepare(
                ctx,
                self.state.design(),
                view,
                revision,
                board_view,
                [available.x, available.y],
            );
            let [pixel_width, pixel_height] =
                viewport_size([available.x, available.y], ctx.pixels_per_point());
            let initial_transform = if view == ViewMode::TwoD {
                BoardTransform::for_view(self.state.design(), pixel_width, pixel_height, board_view)
                    .ok()
            } else {
                None
            };
            let error = self.viewport.error().map(str::to_owned);

            let (rect, response) = ui.allocate_exact_size(available, egui::Sense::click_and_drag());
            let response = if self.state.tool() == StudioTool::Route && view == ViewMode::TwoD {
                response.on_hover_cursor(egui::CursorIcon::Crosshair)
            } else if view == ViewMode::TwoD {
                response.on_hover_cursor(egui::CursorIcon::Grab)
            } else {
                response
            };

            if view == ViewMode::TwoD {
                let pan_drag = response.dragged_by(egui::PointerButton::Middle)
                    || (self.state.tool() == StudioTool::Inspect
                        && response.dragged_by(egui::PointerButton::Primary));
                if pan_drag {
                    let delta = ui.input(|input| input.pointer.delta());
                    if let Some(transform) = initial_transform {
                        let board_dx = -(delta.x as f64) * pixel_width as f64
                            / rect.width().max(1.0) as f64
                            / transform.scale();
                        let board_dy = delta.y as f64 * pixel_height as f64
                            / rect.height().max(1.0) as f64
                            / transform.scale();
                        if board_dx != 0.0 || board_dy != 0.0 {
                            self.state.pan_by_board(board_dx, board_dy);
                        }
                    }
                }

                if response.hovered() {
                    let (scroll, pinch) =
                        ui.input(|input| (input.smooth_scroll_delta.y, input.zoom_delta()));
                    let factor = if (pinch - 1.0).abs() > 1e-4 {
                        pinch as f64
                    } else if scroll.abs() > 0.01 {
                        (scroll as f64 * 0.002).exp()
                    } else {
                        1.0
                    };
                    if factor != 1.0 {
                        if let Some(pointer) = response.hover_pos() {
                            let local = pointer - rect.min;
                            let pixel = [
                                local.x as f64 / rect.width().max(1.0) as f64 * pixel_width as f64,
                                local.y as f64 / rect.height().max(1.0) as f64
                                    * pixel_height as f64,
                            ];
                            self.state.zoom_at(factor, pixel, [pixel_width, pixel_height]);
                        }
                    }
                }
            }

            let transform = if view == ViewMode::TwoD {
                BoardTransform::for_view(
                    self.state.design(),
                    pixel_width,
                    pixel_height,
                    self.state.board_view(),
                )
                .ok()
            } else {
                None
            };

            if let Some(texture_id) = texture {
                ui.painter().image(
                    texture_id,
                    rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            } else {
                ui.painter().rect_filled(rect, 0.0, Color32::from_rgb(10, 13, 19));
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    error.unwrap_or_else(|| "Waiting for a viewport…".to_string()),
                    egui::FontId::proportional(14.0),
                    ERROR_COLOR,
                );
            }

            if response.clicked() && self.state.tool() == StudioTool::Route {
                if view != ViewMode::TwoD {
                    self.state
                        .set_status("Switch to the 2D view to route tracks.", NoticeKind::Error);
                } else if let (Some(transform), Some(pointer)) =
                    (transform, response.interact_pointer_pos())
                {
                    let local = pointer - rect.min;
                    let x = local.x as f64 / rect.width().max(1.0) as f64 * pixel_width as f64;
                    let y = local.y as f64 / rect.height().max(1.0) as f64 * pixel_height as f64;
                    let [board_x, board_y] = transform.pixel_to_board(x, y);
                    let _ = self.state.route_at(Point::new(board_x, board_y));
                }
            }

            if view == ViewMode::TwoD {
                if let (Some(start), Some(transform)) =
                    (self.state.editor().pending_start(), transform)
                {
                    let [x, y] = transform.board_to_pixel(start.x, start.y);
                    let screen = rect.min
                        + egui::vec2(
                            x as f32 / pixel_width as f32 * rect.width(),
                            y as f32 / pixel_height as f32 * rect.height(),
                        );
                    let stroke = egui::Stroke::new(1.5_f32, ACCENT);
                    ui.painter().circle_stroke(screen, 6.0, stroke);
                    ui.painter().line_segment(
                        [screen - egui::vec2(9.0, 0.0), screen + egui::vec2(9.0, 0.0)],
                        stroke,
                    );
                    ui.painter().line_segment(
                        [screen - egui::vec2(0.0, 9.0), screen + egui::vec2(0.0, 9.0)],
                        stroke,
                    );
                }
            }
        });
    }

    fn schematic_canvas(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let available = ui.available_size();
            if available.x < 2.0 || available.y < 2.0 {
                return;
            }
            let size = [available.x as f64, available.y as f64];
            let (rect, response) =
                ui.allocate_exact_size(available, egui::Sense::click_and_drag());
            let response = response.on_hover_cursor(egui::CursorIcon::Crosshair);

            if response.dragged_by(egui::PointerButton::Middle)
                || response.dragged_by(egui::PointerButton::Primary)
            {
                let delta = ui.input(|input| input.pointer.delta());
                let transform = self.schematic.as_ref().and_then(|document| {
                    SchematicTransform::for_view(
                        document.data(),
                        size[0],
                        size[1],
                        document.view(),
                    )
                });
                if let Some(transform) = transform {
                    if let Some(document) = self.schematic.as_mut() {
                        let dx = -(delta.x as f64) / transform.scale();
                        let dy = delta.y as f64 / transform.scale();
                        if dx != 0.0 || dy != 0.0 {
                            document.pan(dx, dy);
                        }
                    }
                }
            }

            if response.hovered() {
                let (scroll, pinch) =
                    ui.input(|input| (input.smooth_scroll_delta.y, input.zoom_delta()));
                let factor = if (pinch - 1.0).abs() > 1e-4 {
                    pinch as f64
                } else if scroll.abs() > 0.01 {
                    (scroll as f64 * 0.002).exp()
                } else {
                    1.0
                };
                if factor != 1.0 {
                    if let Some(pointer) = response.hover_pos() {
                        let local = pointer - rect.min;
                        if let Some(document) = self.schematic.as_mut() {
                            document.zoom_at(
                                factor,
                                [local.x as f64, local.y as f64],
                                size,
                            );
                        }
                    }
                }
            }

            let transform = self.schematic.as_ref().and_then(|document| {
                SchematicTransform::for_view(
                    document.data(),
                    size[0],
                    size[1],
                    document.view(),
                )
            });

            match (&self.schematic, transform) {
                (Some(document), Some(transform)) => {
                    draw_schematic(ui, rect, transform, document);
                }
                _ => {
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "Open or create a schematic document.",
                        egui::FontId::proportional(14.0),
                        ui.visuals().weak_text_color(),
                    );
                }
            }

            if let (Some(document), Some(transform)) = (self.schematic.as_ref(), transform) {
                if let Some(start) = document.pending_start() {
                    let [x, y] = transform.schematic_to_screen(start.x, start.y);
                    let screen = rect.min + egui::vec2(x as f32, y as f32);
                    let stroke = egui::Stroke::new(1.5_f32, ACCENT);
                    ui.painter().circle_stroke(screen, 6.0, stroke);
                    ui.painter().line_segment(
                        [screen - egui::vec2(9.0, 0.0), screen + egui::vec2(9.0, 0.0)],
                        stroke,
                    );
                    ui.painter().line_segment(
                        [screen - egui::vec2(0.0, 9.0), screen + egui::vec2(0.0, 9.0)],
                        stroke,
                    );
                    if let Some(pointer) = response.hover_pos() {
                        let local = pointer - rect.min;
                        let [hover_x, hover_y] = transform.screen_to_schematic(
                            local.x as f64,
                            local.y as f64,
                        );
                        let [end_x, end_y] =
                            transform.schematic_to_screen(hover_x, hover_y);
                        let end = rect.min + egui::vec2(end_x as f32, end_y as f32);
                        ui.painter().line_segment(
                            [screen, end],
                            egui::Stroke::new(1.0_f32, ACCENT),
                        );
                    }
                }
            }

            if response.clicked() {
                if let (Some(transform), Some(pointer)) =
                    (transform, response.interact_pointer_pos())
                {
                    let local = pointer - rect.min;
                    let [x, y] = transform
                        .screen_to_schematic(local.x as f64, local.y as f64);
                    if let Some(document) = self.schematic.as_mut() {
                        let started = document.pending_start().is_some();
                        let result = if started {
                            document.add_wire(Point::new(x, y))
                        } else {
                            document.begin_wire(Point::new(x, y))
                        };
                        self.schematic_notice = match result {
                            Ok(WireClick::Started) => {
                                let snapped_x = (x / WIRE_GRID).round() * WIRE_GRID;
                                let snapped_y = (y / WIRE_GRID).round() * WIRE_GRID;
                                Notice::info(format!(
                                    "Wire start set at {snapped_x:.2}, {snapped_y:.2} mm. Click a destination."
                                ))
                            }
                            Ok(WireClick::Added) => {
                                Notice::info("Wire added. Click another start point.")
                            }
                            Err(error) => Notice::error(error.to_string()),
                        };
                    }
                }
            }
        });
    }

    fn file_dialog_window(&mut self, ctx: &egui::Context) {
        let Some(dialog) = self.file_dialog.as_mut() else {
            return;
        };
        let mut window_open = true;
        let mut cancel = false;
        let mut confirm = false;
        let title = dialog.mode.title();
        let action = dialog.mode.action();

        egui::Window::new(title)
            .id(egui::Id::new("kaicad-file-dialog"))
            .collapsible(false)
            .resizable(false)
            .default_width(460.0)
            .open(&mut window_open)
            .show(ctx, |ui| {
                ui.label("Board path");
                let response = ui.add(
                    egui::TextEdit::singleline(&mut dialog.path)
                        .hint_text("board.kicad_pcb")
                        .desired_width(f32::INFINITY),
                );
                if !dialog.focused {
                    response.request_focus();
                    dialog.focused = true;
                }
                let hint = if matches!(
                    dialog.mode,
                    FileMode::OpenSchematic | FileMode::SaveSchematic
                ) {
                    "Use an absolute path or a path relative to the launch directory. .kicad_sch is required."
                } else {
                    "Use an absolute path or a path relative to the launch directory. .kicad_pcb is native; .json is supported."
                };
                ui.label(RichText::new(hint).weak().small());
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                    if ui
                        .add(
                            egui::Button::new(action)
                                .fill(ACCENT)
                                .stroke(egui::Stroke::new(1.0_f32, ACCENT)),
                        )
                        .clicked()
                    {
                        confirm = true;
                    }
                });
                if ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                    confirm = true;
                }
            });

        if !window_open || cancel {
            self.file_dialog = None;
        } else if confirm {
            self.confirm_dialog();
        }
    }

    fn about_window(&mut self, ctx: &egui::Context) {
        if !self.show_about {
            return;
        }
        let mut open = true;
        egui::Window::new("About KaiCad Studio")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.heading("KaiCad Studio");
                ui.label(format!("Version {} (pre-alpha)", env!("CARGO_PKG_VERSION")));
                ui.label("A native PCB editor and inspection shell.");
                ui.add_space(6.0);
                ui.label(
                    "The viewport is rendered by the deterministic CPU rasterizer. \
                     A Vulkan GPU backend is not implemented.",
                );
                ui.label(
                    "Current shell: board opening/saving, 2D/3D views with 2D zoom/pan, \
                     stackup and footprint inspection, DRC, undoable track routing in 2D, \
                     and a schematic workspace with pan/zoom and wire capture.",
                );
                ui.label(
                    "Footprint placement, schematic symbol placement and cross-probing are not implemented.",
                );
            });
        self.show_about = open;
    }
}

impl eframe::App for StudioApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_input(ctx);
        self.menu_bar(ctx);
        self.left_panel(ctx);
        self.right_panel(ctx);
        self.status_bar(ctx);
        self.central_panel(ctx);
        self.file_dialog_window(ctx);
        self.about_window(ctx);
    }
}

fn is_schematic_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("kicad_sch"))
}

fn draw_schematic(
    ui: &egui::Ui,
    rect: egui::Rect,
    transform: SchematicTransform,
    document: &SchematicDocument,
) {
    let painter = ui.painter().with_clip_rect(rect);
    let map = |x: f64, y: f64| {
        let [screen_x, screen_y] = transform.schematic_to_screen(x, y);
        egui::pos2(rect.min.x + screen_x as f32, rect.min.y + screen_y as f32)
    };
    let data = document.data();

    let sheet_stroke = egui::Stroke::new(1.5_f32, Color32::from_rgb(139, 180, 232));
    for sheet in data.sheets.iter().filter(|sheet| sheet.parent.is_some()) {
        let width = sheet.size.x.max(10.0);
        let height = sheet.size.y.max(7.5);
        let top_left = map(sheet.position.x, sheet.position.y + height);
        let bottom_right = map(sheet.position.x + width, sheet.position.y);
        let sheet_rect = egui::Rect::from_two_pos(top_left, bottom_right);
        painter.rect_filled(sheet_rect, 2.0, Color32::from_rgb(24, 33, 45));
        painter.rect_stroke(sheet_rect, 2.0, sheet_stroke, egui::StrokeKind::Inside);
        painter.text(
            sheet_rect.left_top() + egui::vec2(4.0, 4.0),
            egui::Align2::LEFT_TOP,
            &sheet.name,
            egui::FontId::proportional(12.0),
            Color32::from_rgb(200, 216, 236),
        );
        if !sheet.file_name.is_empty() {
            painter.text(
                sheet_rect.left_bottom() + egui::vec2(4.0, -4.0),
                egui::Align2::LEFT_BOTTOM,
                &sheet.file_name,
                egui::FontId::proportional(10.0),
                Color32::from_rgb(140, 156, 176),
            );
        }
    }

    for component in &data.components {
        let position = component.placement.position;
        let top_left = map(
            position.x - crate::schematic::COMPONENT_WIDTH / 2.0,
            position.y + crate::schematic::COMPONENT_HEIGHT / 2.0,
        );
        let bottom_right = map(
            position.x + crate::schematic::COMPONENT_WIDTH / 2.0,
            position.y - crate::schematic::COMPONENT_HEIGHT / 2.0,
        );
        let body = egui::Rect::from_two_pos(top_left, bottom_right);
        painter.rect_filled(body, 3.0, Color32::from_rgb(30, 42, 55));
        painter.rect_stroke(
            body,
            3.0,
            egui::Stroke::new(1.5_f32, ACCENT),
            egui::StrokeKind::Inside,
        );
        painter.text(
            body.center_top() + egui::vec2(0.0, 4.0),
            egui::Align2::CENTER_TOP,
            &component.reference_designator,
            egui::FontId::proportional(13.0),
            Color32::WHITE,
        );
        if let Some(value) = component.properties.iter().find(|property| property.name == "Value") {
            painter.text(
                body.center_bottom() + egui::vec2(0.0, -4.0),
                egui::Align2::CENTER_BOTTOM,
                &value.value,
                egui::FontId::proportional(12.0),
                Color32::from_rgb(224, 196, 138),
            );
        }
        painter.text(
            body.center(),
            egui::Align2::CENTER_CENTER,
            &component.library_id,
            egui::FontId::proportional(9.0),
            Color32::from_rgb(140, 156, 176),
        );
    }

    if let Some(root) = document.root_sheet() {
        let wire_stroke = egui::Stroke::new(2.0_f32, Color32::from_rgb(122, 202, 143));
        for wire in &root.wires {
            for segment in wire.points.windows(2) {
                painter.line_segment(
                    [map(segment[0].x, segment[0].y), map(segment[1].x, segment[1].y)],
                    wire_stroke,
                );
            }
        }
        for junction in &root.junctions {
            painter.circle_filled(
                map(junction.position.x, junction.position.y),
                4.0,
                Color32::from_rgb(122, 202, 143),
            );
        }
        for no_connect in &root.no_connects {
            let center = map(no_connect.position.x, no_connect.position.y);
            let stroke = egui::Stroke::new(1.5_f32, Color32::from_rgb(232, 120, 120));
            painter.line_segment(
                [center + egui::vec2(-4.0, -4.0), center + egui::vec2(4.0, 4.0)],
                stroke,
            );
            painter.line_segment(
                [center + egui::vec2(-4.0, 4.0), center + egui::vec2(4.0, -4.0)],
                stroke,
            );
        }
        for label in &root.labels {
            let color = match label.label_type {
                LabelType::Local => Color32::from_rgb(222, 218, 160),
                LabelType::Global => Color32::from_rgb(232, 145, 51),
                LabelType::Hierarchical => Color32::from_rgb(139, 180, 232),
                LabelType::Power => Color32::from_rgb(244, 112, 112),
            };
            let center = map(label.position.x, label.position.y);
            painter.circle_filled(center, 2.5, color);
            painter.text(
                center + egui::vec2(4.0, -4.0),
                egui::Align2::LEFT_BOTTOM,
                &label.name,
                egui::FontId::proportional(13.0),
                color,
            );
        }
    }
}

fn layer_label(design: &PcbDesign, id: u16) -> String {
    design
        .layers
        .iter()
        .find(|layer| layer.id == id)
        .map(|layer| layer.name.clone())
        .unwrap_or_else(|| format!("layer {id}"))
}

fn net_label(net_id: Uuid) -> String {
    if net_id.is_nil() {
        return "unassigned".to_string();
    }
    let text = net_id.simple().to_string();
    format!("net {}", &text[..8])
}

fn configure_style(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = Color32::from_rgb(17, 20, 26);
    visuals.window_fill = Color32::from_rgb(24, 28, 36);
    ctx.set_visuals(visuals);

    let mut style = ctx.style().as_ref().clone();
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(8.0, 4.0);
    ctx.set_style(style);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::save_design;

    fn temp_dir() -> PathBuf {
        std::env::temp_dir().join(format!(
            "kaicad-gui-state-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ))
    }

    fn invalid_design() -> PcbDesign {
        let mut design = demo_design();
        design.tracks[1].layer = design.tracks[0].layer;
        design.tracks[1].path = design.tracks[0].path.clone();
        design
    }

    #[test]
    fn state_starts_with_a_clean_demo_board() {
        let state = StudioState::new_demo();

        assert!(state.report().is_valid());
        assert!(state.report().violations.is_empty());
        assert_eq!(state.display_name(), "Demo board");
        assert_eq!(state.path(), None);
        assert_eq!(state.view(), ViewMode::TwoD);
        assert_eq!(state.revision(), 0);
    }

    #[test]
    fn drc_changes_are_reflected_in_the_report_and_notice() {
        let mut state = StudioState::from_design(invalid_design());
        assert!(!state.report().is_valid());
        assert_eq!(state.notice().kind, NoticeKind::Error);

        state.run_drc_checks();
        assert!(state.report().count(DrcSeverity::Error) > 0);
        assert!(state.notice().text.contains("error"));
        assert_eq!(state.notice().kind, NoticeKind::Error);
    }

    #[test]
    fn open_and_save_round_trip_updates_state() {
        let directory = temp_dir();
        let path = directory.join("nested/board.kicad_pcb");
        let original = demo_design();
        save_design(&original, &path).expect("fixture saves");

        let mut state = StudioState::new_demo();
        state.open_path(&path).expect("board opens");
        assert_eq!(state.path(), Some(path.as_path()));
        assert_eq!(state.display_name(), "board.kicad_pcb");
        assert_eq!(state.design().outline, original.outline);
        assert_eq!(state.revision(), 1);

        let saved_path = directory.join("copy.kicad_pcb");
        state.save_path(&saved_path).expect("board saves");
        assert_eq!(state.path(), Some(saved_path.as_path()));
        assert!(state.notice().text.starts_with("Saved"));

        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn a_failed_open_preserves_the_current_board() {
        let mut state = StudioState::new_demo();
        let id = state.design().id;
        let path = temp_dir().join("missing.json");

        let error = state.open_path(&path).expect_err("missing file is an error");
        assert!(matches!(error, DocumentError::Io(_)));
        assert_eq!(state.design().id, id);
        assert_eq!(state.path(), None);
        assert_eq!(state.notice().kind, NoticeKind::Error);
    }

    #[test]
    fn resetting_to_the_demo_clears_the_path_and_invalidates_the_viewport() {
        let directory = temp_dir();
        let path = directory.join("board.kicad_pcb");
        save_design(&demo_design(), &path).expect("fixture saves");
        let mut state = StudioState::new_demo();
        state.open_path(&path).expect("board opens");
        let revision = state.revision();
        assert!(state.zoom_by(2.0));
        assert!(state.pan_by_board(4.0, -2.0));

        state.reset_demo();
        assert_eq!(state.path(), None);
        assert_eq!(state.display_name(), "Demo board");
        assert_eq!(state.revision(), revision + 1);
        assert!(state.report().is_valid());
        assert_eq!(state.board_view(), BoardView::default());

        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn routing_through_state_updates_revision_history_and_dirty_flag() {
        let mut state = StudioState::new_demo();
        let original_tracks = state.design().tracks.len();
        state.set_tool(StudioTool::Route);

        let first = state.route_at(Point::new(10.0, 10.0)).expect("start");
        assert_eq!(first, RouteClick::Started);
        assert!(state.editor().pending_start().is_some());
        let revision = state.revision();

        let second = state.route_at(Point::new(20.0, 10.0)).expect("route");
        assert!(matches!(second, RouteClick::Routed { tracks, .. } if tracks > 0));
        assert_eq!(state.design().tracks.len(), original_tracks + 1);
        assert_eq!(state.revision(), revision + 1);
        assert!(state.dirty());
        assert!(state.editor().can_undo());
        assert!(state.report().is_valid());

        assert!(state.undo_edit());
        assert_eq!(state.design().tracks.len(), original_tracks);
        assert!(state.editor().can_redo());
        assert!(state.dirty());
    }

    #[test]
    fn saving_clears_the_modified_flag_and_opening_resets_history() {
        let directory = temp_dir();
        let path = directory.join("edited.json");
        let mut state = StudioState::new_demo();
        state.route_at(Point::new(10.0, 10.0)).expect("start");
        state.route_at(Point::new(20.0, 10.0)).expect("route");
        assert!(state.dirty());

        state.save_path(&path).expect("save");
        assert!(!state.dirty());

        state.open_path(&path).expect("open");
        assert!(!state.dirty());
        assert!(!state.editor().can_undo());
        assert_eq!(state.tool(), StudioTool::Inspect);

        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn schematic_paths_are_detected_by_extension() {
        assert!(is_schematic_path(Path::new("main.kicad_sch")));
        assert!(is_schematic_path(Path::new("MAIN.KICAD_SCH")));
        assert!(!is_schematic_path(Path::new("main.kicad_pcb")));
        assert!(!is_schematic_path(Path::new("main.json")));
    }

    #[test]
    fn zoom_and_pan_change_the_view_and_reset_restores_fit() {
        let mut state = StudioState::new_demo();
        assert_eq!(state.board_view(), BoardView::default());

        assert!(state.zoom_by(2.0));
        assert_eq!(state.board_view().zoom, 2.0);
        assert!(state.pan_by_board(5.0, -3.0));
        assert_eq!(state.board_view().center, Some([45.0, 22.0]));

        state.reset_board_view();
        assert_eq!(state.board_view(), BoardView::default());
        assert!(!state.zoom_by(0.0));
        assert!(!state.pan_by_board(f64::NAN, 0.0));
    }

    #[test]
    fn zoom_is_clamped_and_pointer_zoom_keeps_the_anchor_fixed() {
        let mut state = StudioState::new_demo();
        assert!(state.zoom_by(1e9));
        assert_eq!(state.board_view().zoom, MAX_ZOOM);
        assert!(state.zoom_by(1e-9));
        assert_eq!(state.board_view().zoom, MIN_ZOOM);

        state.reset_board_view();
        let image = [800, 500];
        let pixel = [160.0, 90.0];
        let fitted = BoardTransform::for_board(state.design(), image[0], image[1]).expect("fit");
        let [anchor_x, anchor_y] = fitted.pixel_to_board(pixel[0], pixel[1]);
        assert!(state.zoom_at(2.0, pixel, image));

        let zoomed =
            BoardTransform::for_view(state.design(), image[0], image[1], state.board_view())
                .expect("zoomed");
        let [x, y] = zoomed.pixel_to_board(pixel[0], pixel[1]);
        assert!((x - anchor_x).abs() < 1e-9);
        assert!((y - anchor_y).abs() < 1e-9);
    }

    #[test]
    fn viewport_sizes_keep_aspect_ratio_and_respect_the_cap() {
        assert_eq!(viewport_size([800.0, 600.0], 1.0), [800, 600]);
        assert_eq!(viewport_size([800.0, 600.0], 2.0), [1600, 1200]);
        assert_eq!(
            viewport_size([4000.0, 2000.0], 1.0),
            [MAX_VIEWPORT_SIDE, MAX_VIEWPORT_SIDE / 2]
        );
        assert_eq!(viewport_size([0.0, 0.0], 1.0), [1, 1]);
        assert_eq!(viewport_size([f32::NAN, 600.0], 1.0), [1, 600]);
    }
}
