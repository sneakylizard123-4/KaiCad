//! Headless schematic document, canvas transform and wire-capture state.
//!
//! The document owns [`core::project::SchematicData`] and a bounded snapshot
//! history. It does not depend on egui: viewport coordinates are converted by
//! [`SchematicTransform`], and the Studio adapter draws the result. The first
//! editing tool is wire capture; symbol and label placement are later
//! increments.

use std::fmt;
use std::path::{Path, PathBuf};

use core::Point;
use core::component::{Component, ComponentPlacement, ComponentProperty};
use core::project::SchematicData;
use core::sheet::{Junction, Label, LabelPosition, LabelType, NoConnect, Sheet, Wire};
use renderer::BoardView;
use uuid::Uuid;

use crate::document::DocumentError;

/// Smallest 2D schematic zoom multiplier.
pub const MIN_SCHEMATIC_ZOOM: f64 = 0.25;
/// Largest 2D schematic zoom multiplier.
pub const MAX_SCHEMATIC_ZOOM: f64 = 32.0;
/// Multiplier applied by one schematic zoom step.
pub const SCHEMATIC_ZOOM_STEP: f64 = 1.25;
/// Wire endpoint snap grid in schematic millimetres.
pub const WIRE_GRID: f64 = 1.27;
/// Maximum number of schematic snapshots retained for undo.
pub const MAX_SCHEMATIC_HISTORY: usize = 64;
/// Placeholder component body width in schematic millimetres.
pub(crate) const COMPONENT_WIDTH: f64 = 6.35;
/// Placeholder component body height in schematic millimetres.
pub(crate) const COMPONENT_HEIGHT: f64 = 5.08;

/// What a schematic click did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireClick {
    /// The first click set a wire start point.
    Started,
    /// The second click added a wire segment.
    Added,
}

/// Why a wire-capture click could not change the schematic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchematicEditError {
    /// The schematic has no root sheet.
    NoRootSheet,
    /// The point is not finite.
    InvalidPoint,
    /// The second click arrived before a start point was set.
    NotStarted,
    /// The start and destination snap to the same grid point.
    SamePoint,
}

impl fmt::Display for SchematicEditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoRootSheet => write!(formatter, "the schematic has no root sheet"),
            Self::InvalidPoint => write!(formatter, "the schematic point is not finite"),
            Self::NotStarted => write!(formatter, "click a wire start point first"),
            Self::SamePoint => write!(formatter, "the wire start and end are the same point"),
        }
    }
}

impl std::error::Error for SchematicEditError {}

/// Mapping between schematic millimetres and logical viewport points.
///
/// Screen coordinates have their origin at the top-left of the canvas;
/// schematic Y increases upward.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SchematicTransform {
    center_x: f64,
    center_y: f64,
    scale: f64,
    width: f64,
    height: f64,
}

impl SchematicTransform {
    /// Fit the schematic content into a `width` by `height` viewport.
    pub fn for_data(data: &SchematicData, width: f64, height: f64) -> Option<Self> {
        Self::for_view(data, width, height, BoardView::default())
    }

    /// Fit the schematic content and apply a zoom and pan view.
    pub fn for_view(
        data: &SchematicData,
        width: f64,
        height: f64,
        view: BoardView,
    ) -> Option<Self> {
        if !width.is_finite()
            || !height.is_finite()
            || width <= 0.0
            || height <= 0.0
            || !view.zoom.is_finite()
            || view.zoom <= 0.0
        {
            return None;
        }
        let bounds = content_bounds(data).unwrap_or([-25.0, -25.0, 25.0, 25.0]);
        let center =
            view.center.unwrap_or([(bounds[0] + bounds[2]) / 2.0, (bounds[1] + bounds[3]) / 2.0]);
        if center.iter().any(|value| !value.is_finite()) {
            return None;
        }

        let margin = (width.min(height) * 0.08).max(12.0);
        let usable_width = (width - margin * 2.0).max(1.0);
        let usable_height = (height - margin * 2.0).max(1.0);
        let content_width = (bounds[2] - bounds[0]).max(1e-9);
        let content_height = (bounds[3] - bounds[1]).max(1e-9);
        let fit_scale = (usable_width / content_width).min(usable_height / content_height);
        let scale = fit_scale * view.zoom;
        if !scale.is_finite() || scale <= 0.0 {
            return None;
        }

        Some(Self { center_x: center[0], center_y: center[1], scale, width, height })
    }

    /// Screen points per schematic millimetre.
    pub fn scale(&self) -> f64 {
        self.scale
    }

    /// Convert schematic millimetres to viewport points.
    pub fn schematic_to_screen(&self, x: f64, y: f64) -> [f64; 2] {
        [
            self.width / 2.0 + (x - self.center_x) * self.scale,
            self.height / 2.0 - (y - self.center_y) * self.scale,
        ]
    }

    /// Convert viewport points to schematic millimetres.
    pub fn screen_to_schematic(&self, x: f64, y: f64) -> [f64; 2] {
        [
            self.center_x + (x - self.width / 2.0) / self.scale,
            self.center_y - (y - self.height / 2.0) / self.scale,
        ]
    }
}

/// A loaded schematic and its wire-capture history.
#[derive(Debug)]
pub struct SchematicDocument {
    data: SchematicData,
    path: Option<PathBuf>,
    view: BoardView,
    pending_start: Option<Point>,
    undo_stack: Vec<SchematicData>,
    redo_stack: Vec<SchematicData>,
    dirty: bool,
    warnings: Vec<String>,
}

impl SchematicDocument {
    /// Create an empty schematic with one root sheet.
    pub fn empty() -> Self {
        let id = Uuid::new_v5(&Uuid::NAMESPACE_OID, b"kaicad-empty-schematic");
        let root = Sheet { id, name: "Root".to_string(), ..Sheet::default() };
        Self::from_data(SchematicData { sheets: vec![root], ..SchematicData::default() })
    }

    /// Build the built-in schematic shown when Studio starts.
    ///
    /// It exercises every construct the first canvas renders: wires, a
    /// junction, local/global labels, a no-connect, a component placeholder and
    /// a child-sheet symbol.
    pub fn demo() -> Self {
        let root_id = Uuid::from_u128(0x00CA_1CAD_5C01);
        let child_id = Uuid::from_u128(0x00CA_1CAD_5C02);
        let component_id = Uuid::from_u128(0x00CA_1CAD_5C03);
        let mut root = Sheet { id: root_id, name: "Root".to_string(), ..Sheet::default() };
        root.wires = vec![
            Wire {
                id: Uuid::from_u128(0x00CA_1CAD_5C11),
                points: vec![Point::new(30.0, 50.0), Point::new(40.0, 50.0)],
                preserved: Vec::new(),
            },
            Wire {
                id: Uuid::from_u128(0x00CA_1CAD_5C12),
                points: vec![Point::new(40.0, 50.0), Point::new(40.0, 40.0)],
                preserved: Vec::new(),
            },
            Wire {
                id: Uuid::from_u128(0x00CA_1CAD_5C13),
                points: vec![Point::new(40.0, 50.0), Point::new(52.0, 50.0)],
                preserved: Vec::new(),
            },
            Wire {
                id: Uuid::from_u128(0x00CA_1CAD_5C14),
                points: vec![Point::new(62.0, 50.0), Point::new(70.0, 50.0)],
                preserved: Vec::new(),
            },
        ];
        root.junctions = vec![Junction {
            id: Uuid::from_u128(0x00CA_1CAD_5C21),
            position: Point::new(40.0, 50.0),
            preserved: Vec::new(),
        }];
        root.no_connects = vec![NoConnect {
            id: Uuid::from_u128(0x00CA_1CAD_5C22),
            position: Point::new(70.0, 50.0),
            preserved: Vec::new(),
        }];
        root.labels = vec![
            Label {
                id: Uuid::from_u128(0x00CA_1CAD_5C31),
                name: "IN".to_string(),
                label_type: LabelType::Local,
                position: LabelPosition { x: 30.0, y: 50.0, rotation: 0.0 },
                preserved: Vec::new(),
            },
            Label {
                id: Uuid::from_u128(0x00CA_1CAD_5C32),
                name: "GND".to_string(),
                label_type: LabelType::Global,
                position: LabelPosition { x: 40.0, y: 40.0, rotation: 0.0 },
                preserved: Vec::new(),
            },
        ];
        root.components.push(component_id);
        root.children.push(child_id);

        let component = Component {
            id: component_id,
            reference_designator: "R1".to_string(),
            symbol_id: Uuid::new_v5(&Uuid::NAMESPACE_OID, b"Device:R"),
            footprint_id: Uuid::nil(),
            properties: vec![
                ComponentProperty { name: "Reference".to_string(), value: "R1".to_string() },
                ComponentProperty { name: "Value".to_string(), value: "10k".to_string() },
                ComponentProperty { name: "Footprint".to_string(), value: String::new() },
            ],
            placement: ComponentPlacement {
                position: Point::new(57.0, 50.0),
                ..Default::default()
            },
            library_id: "Device:R".to_string(),
            preserved: Vec::new(),
        };
        let child = Sheet {
            id: child_id,
            name: "Power".to_string(),
            parent: Some(root_id),
            file_name: "power.kicad_sch".to_string(),
            position: Point::new(90.0, 25.0),
            size: Point::new(25.0, 20.0),
            ..Sheet::default()
        };

        Self::from_data(SchematicData {
            sheets: vec![root, child],
            components: vec![component],
            ..SchematicData::default()
        })
    }

    /// Wrap existing schematic data, discarding any previous path and history.
    pub fn from_data(data: SchematicData) -> Self {
        Self {
            data,
            path: None,
            view: BoardView::default(),
            pending_start: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            dirty: false,
            warnings: Vec::new(),
        }
    }

    /// Open a `.kicad_sch` file. On failure the previous document is unchanged.
    pub fn open_path(&mut self, path: impl AsRef<Path>) -> Result<(), DocumentError> {
        let path = path.as_ref();
        match crate::document::load_schematic(path) {
            Ok(loaded) => {
                self.data = loaded.data;
                self.path = Some(path.to_path_buf());
                self.warnings = loaded.warnings;
                self.reset_state();
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    /// Save the schematic to `path`, creating parent directories.
    pub fn save_path(&mut self, path: impl AsRef<Path>) -> Result<(), DocumentError> {
        let path = path.as_ref();
        crate::document::save_schematic(&self.data, path)?;
        self.path = Some(path.to_path_buf());
        self.dirty = false;
        Ok(())
    }

    /// The current schematic data.
    pub fn data(&self) -> &SchematicData {
        &self.data
    }

    /// File path when opened or previously saved.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Current 2D zoom and pan.
    pub fn view(&self) -> BoardView {
        self.view
    }

    /// Whether the schematic has unsaved wire edits.
    pub fn dirty(&self) -> bool {
        self.dirty
    }

    /// Import warnings retained from the last open.
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// Pending wire start point, if the first click has been made.
    pub fn pending_start(&self) -> Option<Point> {
        self.pending_start
    }

    /// Whether an edit can be undone.
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Whether an edit can be redone.
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Number of snapshots available for undo.
    pub fn undo_len(&self) -> usize {
        self.undo_stack.len()
    }

    /// The root sheet, if there is one.
    pub fn root_sheet(&self) -> Option<&Sheet> {
        self.root_sheet_index().map(|index| &self.data.sheets[index])
    }

    /// Zoom by `factor` around the current centre.
    pub fn zoom_by(&mut self, factor: f64) -> bool {
        let Some(zoom) = clamped_zoom(self.view.zoom, factor) else {
            return false;
        };
        self.view.zoom = zoom;
        true
    }

    /// Zoom by `factor` around a viewport point, keeping the anchor fixed.
    pub fn zoom_at(&mut self, factor: f64, pixel: [f64; 2], size: [f64; 2]) -> bool {
        let Some(zoom) = clamped_zoom(self.view.zoom, factor) else {
            return false;
        };
        let Some(fit) = SchematicTransform::for_data(&self.data, size[0], size[1]) else {
            return false;
        };
        let Some(current) = SchematicTransform::for_view(&self.data, size[0], size[1], self.view)
        else {
            return false;
        };
        let [anchor_x, anchor_y] = current.screen_to_schematic(pixel[0], pixel[1]);
        let scale = fit.scale() * zoom;
        let center = [
            anchor_x - (pixel[0] - size[0] / 2.0) / scale,
            anchor_y + (pixel[1] - size[1] / 2.0) / scale,
        ];
        if !scale.is_finite() || scale <= 0.0 || center.iter().any(|value| !value.is_finite()) {
            return false;
        }
        self.view = BoardView { center: Some(center), zoom };
        true
    }

    /// Pan the view by a schematic-space delta in millimetres.
    pub fn pan(&mut self, dx: f64, dy: f64) -> bool {
        if !dx.is_finite() || !dy.is_finite() {
            return false;
        }
        let center = self.view.center.unwrap_or_else(|| {
            content_bounds(&self.data)
                .map(|bounds| [(bounds[0] + bounds[2]) / 2.0, (bounds[1] + bounds[3]) / 2.0])
                .unwrap_or([0.0, 0.0])
        });
        let next = [center[0] + dx, center[1] + dy];
        if next.iter().any(|value| !value.is_finite()) {
            return false;
        }
        self.view.center = Some(next);
        true
    }

    /// Restore the fitted view.
    pub fn reset_view(&mut self) {
        self.view = BoardView::default();
    }

    /// Set the first endpoint of a wire.
    pub fn begin_wire(&mut self, point: Point) -> Result<WireClick, SchematicEditError> {
        if self.root_sheet_index().is_none() {
            return Err(SchematicEditError::NoRootSheet);
        }
        if !point.x.is_finite() || !point.y.is_finite() {
            return Err(SchematicEditError::InvalidPoint);
        }
        self.pending_start = Some(snap_point(point, WIRE_GRID));
        Ok(WireClick::Started)
    }

    /// Finish the pending wire at `point` and commit it.
    pub fn add_wire(&mut self, point: Point) -> Result<WireClick, SchematicEditError> {
        let start = self.pending_start.ok_or(SchematicEditError::NotStarted)?;
        if !point.x.is_finite() || !point.y.is_finite() {
            return Err(SchematicEditError::InvalidPoint);
        }
        let index = self.root_sheet_index().ok_or(SchematicEditError::NoRootSheet)?;
        let end = snap_point(point, WIRE_GRID);
        if start.x == end.x && start.y == end.y {
            return Err(SchematicEditError::SamePoint);
        }

        let mut candidate = self.data.clone();
        let sheet = &mut candidate.sheets[index];
        let id = Uuid::new_v5(
            &sheet.id,
            format!(
                "kaicad-schematic-wire|{}|{}|{}|{}|{}",
                start.x,
                start.y,
                end.x,
                end.y,
                sheet.wires.len()
            )
            .as_bytes(),
        );
        sheet.wires.push(Wire { id, points: vec![start, end], preserved: Vec::new() });
        push_bounded(&mut self.undo_stack, self.data.clone());
        self.redo_stack.clear();
        self.data = candidate;
        self.pending_start = None;
        self.dirty = true;
        Ok(WireClick::Added)
    }

    /// Discard a pending wire start.
    pub fn cancel_wire(&mut self) {
        self.pending_start = None;
    }

    /// Restore the schematic before the most recent wire edit.
    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo_stack.pop() else {
            return false;
        };
        self.pending_start = None;
        let current = std::mem::replace(&mut self.data, previous);
        self.redo_stack.push(current);
        self.dirty = true;
        true
    }

    /// Reapply the most recently undone wire edit.
    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo_stack.pop() else {
            return false;
        };
        self.pending_start = None;
        let current = std::mem::replace(&mut self.data, next);
        push_bounded(&mut self.undo_stack, current);
        self.dirty = true;
        true
    }

    fn root_sheet_index(&self) -> Option<usize> {
        self.data
            .sheets
            .iter()
            .position(|sheet| sheet.parent.is_none())
            .or_else(|| (!self.data.sheets.is_empty()).then_some(0))
    }

    fn reset_state(&mut self) {
        self.view = BoardView::default();
        self.pending_start = None;
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.dirty = false;
    }
}

fn clamped_zoom(current: f64, factor: f64) -> Option<f64> {
    if !factor.is_finite() || factor <= 0.0 {
        return None;
    }
    let zoom = (current * factor).clamp(MIN_SCHEMATIC_ZOOM, MAX_SCHEMATIC_ZOOM);
    (zoom != current).then_some(zoom)
}

fn snap_point(point: Point, grid: f64) -> Point {
    Point::new((point.x / grid).round() * grid, (point.y / grid).round() * grid)
}

fn push_bounded(stack: &mut Vec<SchematicData>, data: SchematicData) {
    if stack.len() == MAX_SCHEMATIC_HISTORY {
        stack.remove(0);
    }
    stack.push(data);
}

fn content_bounds(data: &SchematicData) -> Option<[f64; 4]> {
    let mut bounds: Option<[f64; 4]> = None;
    let mut include = |x: f64, y: f64| {
        if !x.is_finite() || !y.is_finite() {
            return;
        }
        match &mut bounds {
            Some(values) => {
                values[0] = values[0].min(x);
                values[1] = values[1].min(y);
                values[2] = values[2].max(x);
                values[3] = values[3].max(y);
            }
            None => bounds = Some([x, y, x, y]),
        }
    };

    if let Some(sheet) = data.sheets.iter().find(|sheet| sheet.parent.is_none()) {
        for wire in &sheet.wires {
            for point in &wire.points {
                include(point.x, point.y);
            }
        }
        for junction in &sheet.junctions {
            include(junction.position.x, junction.position.y);
        }
        for no_connect in &sheet.no_connects {
            include(no_connect.position.x, no_connect.position.y);
        }
        for label in &sheet.labels {
            include(label.position.x, label.position.y);
        }
    }

    for component in &data.components {
        let position = component.placement.position;
        include(position.x - COMPONENT_WIDTH / 2.0, position.y - COMPONENT_HEIGHT / 2.0);
        include(position.x + COMPONENT_WIDTH / 2.0, position.y + COMPONENT_HEIGHT / 2.0);
    }

    for sheet in data.sheets.iter().filter(|sheet| sheet.parent.is_some()) {
        let width = sheet.size.x.max(10.0);
        let height = sheet.size.y.max(7.5);
        include(sheet.position.x, sheet.position.y);
        include(sheet.position.x + width, sheet.position.y + height);
    }

    bounds
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> SchematicData {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../kicad-parser/tests/fixtures/compat.kicad_sch");
        crate::document::load_schematic(path).expect("fixture loads").data
    }

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("kaicad-gui-schematic-{}-{}", std::process::id(), Uuid::new_v4()))
            .join(name)
    }

    #[test]
    fn demo_document_has_content_and_fits() {
        let document = SchematicDocument::demo();
        let root = document.root_sheet().expect("root");
        assert_eq!(root.wires.len(), 4);
        assert_eq!(root.labels.len(), 2);
        assert_eq!(root.junctions.len(), 1);
        assert_eq!(root.no_connects.len(), 1);
        assert_eq!(document.data().components.len(), 1);
        assert_eq!(document.data().sheets.len(), 2);
        assert!(SchematicTransform::for_data(document.data(), 800.0, 500.0).is_some());
    }

    #[test]
    fn empty_document_has_a_root_sheet() {
        let document = SchematicDocument::empty();
        assert_eq!(document.root_sheet().expect("root").name, "Root");
        assert!(document.root_sheet().expect("root").wires.is_empty());
        assert!(!document.dirty());
    }

    #[test]
    fn wire_capture_adds_undoes_and_redoes_a_wire() {
        let mut document = SchematicDocument::empty();
        assert_eq!(document.begin_wire(Point::new(10.0, 10.0)).expect("start"), WireClick::Started);
        assert_eq!(document.add_wire(Point::new(20.0, 10.0)).expect("add"), WireClick::Added);

        let root = document.root_sheet().expect("root");
        assert_eq!(root.wires.len(), 1);
        assert_eq!(root.wires[0].points.len(), 2);
        assert!(document.dirty());
        assert!(document.can_undo());

        assert!(document.undo());
        assert!(document.root_sheet().expect("root").wires.is_empty());
        assert!(document.can_redo());

        assert!(document.redo());
        assert_eq!(document.root_sheet().expect("root").wires.len(), 1);
    }

    #[test]
    fn wire_endpoints_snap_to_the_grid_and_reject_a_same_point() {
        let mut document = SchematicDocument::empty();
        document.begin_wire(Point::new(10.1, 9.9)).expect("start");
        document.add_wire(Point::new(20.1, 9.9)).expect("add");
        let wire = &document.root_sheet().expect("root").wires[0];
        assert!((wire.points[0].x - 10.16).abs() < 1e-9);
        assert!((wire.points[0].y - 10.16).abs() < 1e-9);
        assert!((wire.points[1].x - 20.32).abs() < 1e-9);

        document.begin_wire(Point::new(5.0, 5.0)).expect("start");
        assert_eq!(document.add_wire(Point::new(5.0, 5.0)), Err(SchematicEditError::SamePoint));
    }

    #[test]
    fn captured_wire_saves_and_reloads() {
        let path = temp_path("nested/captured.kicad_sch");
        let mut document = SchematicDocument::empty();
        document.begin_wire(Point::new(10.0, 10.0)).expect("start");
        document.add_wire(Point::new(20.0, 10.0)).expect("add");
        document.save_path(&path).expect("save");
        assert!(!document.dirty());

        let mut reopened = SchematicDocument::empty();
        reopened.open_path(&path).expect("reopen");
        assert_eq!(reopened.root_sheet().expect("root").wires.len(), 1);
        assert_eq!(reopened.data().sheets.len(), 1);
        assert_eq!(reopened.path(), Some(path.as_path()));
        let _ = std::fs::remove_dir_all(path.parent().and_then(Path::parent).expect("temp root"));
    }

    #[test]
    fn failed_open_preserves_the_current_schematic() {
        let mut document = SchematicDocument::demo();
        let wires = document.root_sheet().expect("root").wires.len();
        let path = temp_path("not-a-schematic.kicad_pcb");
        assert!(document.open_path(path).is_err());
        assert_eq!(document.root_sheet().expect("root").wires.len(), wires);
        assert_eq!(document.path(), None);
    }

    #[test]
    fn transform_round_trips_coordinates_at_zoom() {
        let data = fixture();
        let transform = SchematicTransform::for_view(
            &data,
            800.0,
            500.0,
            BoardView { center: Some([40.0, 50.0]), zoom: 2.0 },
        )
        .expect("transform");
        for (x, y) in [(40.0, 50.0), (20.0, 20.0), (60.0, 50.0)] {
            let [screen_x, screen_y] = transform.schematic_to_screen(x, y);
            let [round_x, round_y] = transform.screen_to_schematic(screen_x, screen_y);
            assert!((round_x - x).abs() < 1e-9);
            assert!((round_y - y).abs() < 1e-9);
        }
        assert_eq!(transform.schematic_to_screen(40.0, 50.0), [400.0, 250.0]);
    }

    #[test]
    fn pointer_zoom_keeps_the_schematic_anchor_fixed() {
        let mut document = SchematicDocument::from_data(fixture());
        let size = [800.0, 500.0];
        let pixel = [180.0, 120.0];
        let fitted = SchematicTransform::for_data(document.data(), size[0], size[1]).expect("fit");
        let [anchor_x, anchor_y] = fitted.screen_to_schematic(pixel[0], pixel[1]);
        assert!(document.zoom_at(2.0, pixel, size));

        let zoomed =
            SchematicTransform::for_view(document.data(), size[0], size[1], document.view())
                .expect("zoomed");
        let [x, y] = zoomed.screen_to_schematic(pixel[0], pixel[1]);
        assert!((x - anchor_x).abs() < 1e-9);
        assert!((y - anchor_y).abs() < 1e-9);
    }

    #[test]
    fn zoom_pan_and_reset_are_bounded() {
        let mut document = SchematicDocument::from_data(fixture());
        assert!(document.zoom_by(1e9));
        assert_eq!(document.view().zoom, MAX_SCHEMATIC_ZOOM);
        assert!(document.zoom_by(1e-9));
        assert_eq!(document.view().zoom, MIN_SCHEMATIC_ZOOM);
        assert!(!document.zoom_by(0.0));

        document.reset_view();
        assert_eq!(document.view(), BoardView::default());
        assert!(document.pan(4.0, -2.0));
        assert!(document.view().center.is_some());
        assert!(!document.pan(f64::NAN, 0.0));
    }
}
