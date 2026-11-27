//! KaiCad Studio — a native desktop PCB editor and inspection shell.
//!
//! The crate separates the testable application state and document I/O from
//! the native window. [`StudioState`] owns a [`pcb::PcbDesign`], runs DRC and
//! tracks the current view and file path; [`StudioApp`] is the `eframe`
//! adapter that draws it. Rendering uses the deterministic CPU rasterizer in
//! the `renderer` crate, so no Vulkan loader is required.
//!
//! The current shell includes board inspection, 2D zoom/pan, track routing,
//! selection and editing of footprints, tracks and vias, and a schematic
//! workspace with wire capture. Footprint placement from a library, pad
//! editing, symbol placement and cross-probing are later increments.

mod app;
mod demo;
mod document;
mod editor;
mod schematic;

pub use app::{
    MAX_VIEWPORT_SIDE, Notice, NoticeKind, StudioApp, StudioState, StudioTool, viewport_size,
};
pub use demo::demo_design;
pub use document::{
    DocumentError, LoadedBoard, LoadedSchematic, design_from_json, design_to_json, load_design,
    load_schematic, save_design, save_schematic,
};
pub use editor::{
    BoardSelection, MAX_EDIT_HISTORY, MOVE_GRID, PAD_SNAP_RADIUS, RouteClick, RouteEditError,
    SelectionEditError, TrackEditor, footprint_board_bounds,
};
pub use schematic::{
    MAX_SCHEMATIC_HISTORY, MAX_SCHEMATIC_ZOOM, MIN_SCHEMATIC_ZOOM, SCHEMATIC_ZOOM_STEP,
    SchematicDocument, SchematicEditError, SchematicTransform, WIRE_GRID, WireClick,
};
