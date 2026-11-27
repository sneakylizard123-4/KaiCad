//! Application state and the `eframe` window adapter for KaiCad Studio.

use std::path::{Path, PathBuf};

use core::Point;
use eframe::egui::{self, Color32, RichText, TextureHandle, TextureOptions};
use pcb::{DrcReport, DrcSeverity, PcbDesign, run_drc};
use renderer::{BoardTransform, Framebuffer, Renderer, RendererConfig, ViewMode, WindowConfig};
use uuid::Uuid;

use crate::demo::demo_design;
use crate::document::{DocumentError, save_design};
use crate::editor::{RouteClick, RouteEditError, TrackEditor};

/// Largest viewport side, in pixels, sent to the software rasterizer.
pub const MAX_VIEWPORT_SIDE: u32 = 2048;
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

/// Headless application state: the board, editing history, DRC and status.
#[derive(Debug)]
pub struct StudioState {
    design: PcbDesign,
    path: Option<PathBuf>,
    view: ViewMode,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RenderKey {
    width: u32,
    height: u32,
    view: ViewMode,
    revision: u64,
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
        available: [f32; 2],
    ) -> Option<egui::TextureId> {
        let [width, height] = viewport_size(available, ctx.pixels_per_point());
        if width < 2 || height < 2 {
            return None;
        }
        let key = RenderKey { width, height, view, revision };
        if self.key != Some(key) {
            self.key = Some(key);
            match rasterize(design, view, width, height) {
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
    width: u32,
    height: u32,
) -> Result<egui::ColorImage, String> {
    let config = RendererConfig {
        window: WindowConfig::new("KaiCad viewport", width, height),
        ..RendererConfig::default()
    };
    let mut renderer =
        Renderer::new(config).map_err(|error| format!("renderer setup failed: {error}"))?;
    renderer.render_pcb(design, view).map_err(|error| format!("renderer failed: {error}"))?;
    Ok(color_image(renderer.framebuffer()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileMode {
    Open,
    Save,
}

impl FileMode {
    fn title(self) -> &'static str {
        match self {
            Self::Open => "Open KiCad board",
            Self::Save => "Save KiCad board as",
        }
    }

    fn action(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Save => "Save",
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
    path_input: String,
    viewport: Viewport,
    file_dialog: Option<FileDialog>,
    show_about: bool,
}

impl StudioApp {
    /// Create the app, optionally opening a board supplied on the command line.
    pub fn new(cc: &eframe::CreationContext<'_>, initial_path: Option<PathBuf>) -> Self {
        configure_style(&cc.egui_ctx);
        let mut app = Self {
            state: StudioState::new_demo(),
            path_input: String::new(),
            viewport: Viewport::new(),
            file_dialog: None,
            show_about: false,
        };
        if let Some(path) = initial_path {
            app.open_board(path);
        }
        app
    }

    fn open_board(&mut self, path: PathBuf) {
        self.path_input = path.display().to_string();
        let _ = self.state.open_path(&path);
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

    fn open_dialog(&mut self) {
        self.file_dialog =
            Some(FileDialog { mode: FileMode::Open, path: self.suggested_path(), focused: false });
    }

    fn open_save_dialog(&mut self) {
        self.file_dialog =
            Some(FileDialog { mode: FileMode::Save, path: self.suggested_path(), focused: false });
    }

    fn confirm_dialog(&mut self) {
        let Some(dialog) = self.file_dialog.take() else {
            return;
        };
        let mut path = PathBuf::from(dialog.path.trim());
        if path.as_os_str().is_empty() {
            self.state.set_status("Enter a board path first.", NoticeKind::Error);
            return;
        }
        if dialog.mode == FileMode::Save && path.extension().is_none() {
            path.set_extension("kicad_pcb");
        }
        match dialog.mode {
            FileMode::Open => self.open_board(path),
            FileMode::Save => self.save_board(path),
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
            self.save_current();
        }
        if open {
            self.open_dialog();
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
            self.state.undo_edit();
        }
        if redo {
            self.state.redo_edit();
        }

        if !typing
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

        if ctx.input(|input| input.key_pressed(egui::Key::F5)) {
            self.state.run_drc_checks();
        }
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            if self.file_dialog.is_some() {
                self.file_dialog = None;
            } else if self.state.editor().pending_start().is_some() {
                self.state.cancel_route();
            }
        }

        let dropped =
            ctx.input(|input| input.raw.dropped_files.first().and_then(|file| file.path.clone()));
        if let Some(path) = dropped {
            self.open_board(path);
        }
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
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("Edit", |ui| {
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
                });
                ui.menu_button("View", |ui| {
                    if ui
                        .selectable_label(self.state.view() == ViewMode::TwoD, "2D top view")
                        .clicked()
                    {
                        self.state.set_view(ViewMode::TwoD);
                    }
                    if ui
                        .selectable_label(self.state.view() == ViewMode::ThreeD, "3D perspective")
                        .clicked()
                    {
                        self.state.set_view(ViewMode::ThreeD);
                    }
                });
                ui.menu_button("Tools", |ui| {
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
                    ui.heading("Board");
                    ui.label(RichText::new(self.state.display_name()).strong().size(16.0));
                    ui.label(RichText::new(self.view_label()).color(ACCENT).small());
                    ui.horizontal(|ui| {
                        if ui.selectable_label(self.state.view() == ViewMode::TwoD, "2D").clicked()
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
                    ui.add_space(12.0);
                });
            });
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

    fn status_bar(&self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("kaicad-status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let notice = self.state.notice();
                let color = match notice.kind {
                    NoticeKind::Info => ui.visuals().text_color(),
                    NoticeKind::Error => ERROR_COLOR,
                };
                ui.label(RichText::new(&notice.text).color(color));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new("software 2D/3D").weak().small());
                    ui.separator();
                    if let Some(path) = self.state.path() {
                        ui.label(RichText::new(path.display().to_string()).weak().small());
                    }
                });
            });
        });
    }

    fn central_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let available = ui.available_size();
            if available.x < 2.0 || available.y < 2.0 {
                return;
            }

            let view = self.state.view();
            let revision = self.state.revision();
            let texture = self.viewport.prepare(
                ctx,
                self.state.design(),
                view,
                revision,
                [available.x, available.y],
            );
            let [pixel_width, pixel_height] =
                viewport_size([available.x, available.y], ctx.pixels_per_point());
            let transform = if view == ViewMode::TwoD {
                BoardTransform::for_board(self.state.design(), pixel_width, pixel_height).ok()
            } else {
                None
            };
            let error = self.viewport.error().map(str::to_owned);

            let (rect, response) = ui.allocate_exact_size(available, egui::Sense::click());
            let response = if self.state.tool() == StudioTool::Route && view == ViewMode::TwoD {
                response.on_hover_cursor(egui::CursorIcon::Crosshair)
            } else {
                response
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
                ui.label(
                    RichText::new(
                        "Use an absolute path or a path relative to the launch directory. .kicad_pcb is native; .json is supported.",
                    )
                    .weak()
                    .small(),
                );
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
                    "Current shell: board opening/saving, 2D/3D views, stackup and \
                     footprint inspection, DRC, and undoable track routing in 2D.",
                );
                ui.label(
                    "Footprint placement, schematic capture and cross-probing are not implemented.",
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

        state.reset_demo();
        assert_eq!(state.path(), None);
        assert_eq!(state.display_name(), "Demo board");
        assert_eq!(state.revision(), revision + 1);
        assert!(state.report().is_valid());

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
