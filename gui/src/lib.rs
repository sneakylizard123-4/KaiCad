//! KaiCad Studio — a native desktop shell for inspecting PCB designs.
//!
//! The crate separates the testable application state and document I/O from
//! the native window. [`StudioState`] owns a [`pcb::PcbDesign`], runs DRC and
//! tracks the current view and file path; [`StudioApp`] is the `eframe`
//! adapter that draws it. Rendering uses the deterministic CPU rasterizer in
//! the `renderer` crate, so no Vulkan loader is required.
//!
//! The current shell is a viewer and inspection surface. Copper editing,
//! interactive routing and schematic editing are later increments.

mod app;
mod demo;
mod document;

pub use app::{MAX_VIEWPORT_SIDE, Notice, NoticeKind, StudioApp, StudioState, viewport_size};
pub use demo::demo_design;
pub use document::{DocumentError, design_from_json, design_to_json, load_design, save_design};
