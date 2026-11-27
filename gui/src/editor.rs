//! Headless board-editing state for KaiCad Studio.
//!
//! The editor owns routing options, the current selection and a bounded
//! snapshot history. It does not own the board: every operation receives a
//! mutable [`PcbDesign`], and a failed operation leaves that design untouched.
//! Clicks from the 2D viewport are converted to board millimetres by the
//! application layer before they reach this module.

use std::fmt;

use core::Point;
use pcb::footprint::{Footprint, OutlineShape};
use pcb::routing::{RouteError, RouteRequest, RouterConfig, route_multilayer};
use pcb::{PcbDesign, run_drc};
use uuid::Uuid;

/// Distance within which a click snaps to a pad centre, in millimetres.
pub const PAD_SNAP_RADIUS: f64 = 2.0;
/// Distance the arrow keys move the current selection, in millimetres.
pub const MOVE_GRID: f64 = 0.5;

/// Maximum number of board snapshots retained for undo.
pub const MAX_EDIT_HISTORY: usize = 64;

/// A selected object on the board.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoardSelection {
    /// Footprint index in [`PcbDesign::footprints`].
    Footprint(usize),
    /// Track id.
    Track(Uuid),
    /// Via id.
    Via(Uuid),
}

impl BoardSelection {
    /// Short object kind for status messages.
    pub fn kind(self) -> &'static str {
        match self {
            Self::Footprint(_) => "footprint",
            Self::Track(_) => "track",
            Self::Via(_) => "via",
        }
    }
}

/// Why a selection edit could not change the board.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionEditError {
    /// Nothing is selected.
    NoSelection,
    /// The selected object is no longer in the design.
    MissingObject,
    /// The selected object has no rotation, such as a track or via.
    NotRotatable,
    /// Only footprints have a board side.
    NotFlippable,
    /// The requested translation is not finite.
    InvalidDelta,
}

impl fmt::Display for SelectionEditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSelection => write!(formatter, "select a footprint, track or via first"),
            Self::MissingObject => {
                write!(formatter, "the selected object is no longer on the board")
            }
            Self::NotRotatable => write!(formatter, "only footprints can be rotated here"),
            Self::NotFlippable => write!(formatter, "only footprints can be flipped"),
            Self::InvalidDelta => write!(formatter, "the move distance must be finite"),
        }
    }
}

impl std::error::Error for SelectionEditError {}

/// What a viewport click did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteClick {
    /// The first click set a routing start point.
    Started,
    /// The second click committed a route.
    Routed { tracks: usize, vias: usize },
}

/// Why a track-routing click could not change the board.
#[derive(Debug)]
pub enum RouteEditError {
    /// The routing point is outside the board outline.
    OutsideBoard,
    /// No net is selected and no pad at the click supplied one.
    MissingNet,
    /// A destination was clicked before a start point was set.
    NotStarted,
    /// The start and destination resolve to the same grid point.
    SamePoint,
    /// The destination pad belongs to a different net than the start.
    PadNetMismatch,
    /// The grid router did not find a path.
    Route(RouteError),
    /// The candidate route would introduce new DRC findings.
    DrcRejected { violations: usize },
}

impl fmt::Display for RouteEditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutsideBoard => write!(formatter, "the routing point is outside the board"),
            Self::MissingNet => write!(formatter, "select a net or start on a pad"),
            Self::NotStarted => write!(formatter, "click a start point before a destination"),
            Self::SamePoint => write!(formatter, "the destination is the same as the start point"),
            Self::PadNetMismatch => {
                write!(formatter, "the destination pad belongs to a different net")
            }
            Self::Route(error) => write!(formatter, "{error}"),
            Self::DrcRejected { violations } => {
                write!(
                    formatter,
                    "the route was rejected because it introduces {violations} DRC violation(s)"
                )
            }
        }
    }
}

impl std::error::Error for RouteEditError {}

impl From<RouteError> for RouteEditError {
    fn from(error: RouteError) -> Self {
        Self::Route(error)
    }
}

#[derive(Debug, Clone, Copy)]
struct PadHit {
    center: Point,
    net_id: Option<Uuid>,
}

/// Headless track-routing editor with snapshot undo and redo.
#[derive(Debug)]
pub struct TrackEditor {
    layer: u16,
    net_id: Uuid,
    width: f64,
    config: RouterConfig,
    pending_start: Option<Point>,
    selection: Option<BoardSelection>,
    undo_stack: Vec<PcbDesign>,
    redo_stack: Vec<PcbDesign>,
}

impl TrackEditor {
    /// Create an editor configured for `design`.
    ///
    /// The first copper layer is selected, the first assigned net becomes the
    /// current net, and the route width is at least the design's minimum track
    /// width. The routing grid is 0.1 mm so common 0.1 mm footprints can be
    /// reached without a gap.
    pub fn new(design: &PcbDesign) -> Self {
        let config = RouterConfig { grid: 0.1, max_expansions: 300_000, ..RouterConfig::default() };
        let mut editor = Self {
            layer: 0,
            net_id: Uuid::nil(),
            width: 0.25,
            config,
            pending_start: None,
            selection: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        };
        editor.reset(design);
        editor
    }

    /// Reset routing options and clear both history stacks for a new board.
    pub fn reset(&mut self, design: &PcbDesign) {
        self.layer = design.copper_layer_ids().first().copied().unwrap_or(0);
        self.net_id = design.net_ids().first().copied().unwrap_or(Uuid::nil());
        self.width = design.rules.min_track_width.max(0.25);
        self.pending_start = None;
        self.selection = None;
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    /// Copper layer used for new routes.
    pub fn layer(&self) -> u16 {
        self.layer
    }

    /// Net assigned to new routes.
    pub fn net_id(&self) -> Uuid {
        self.net_id
    }

    /// Width of new tracks in millimetres.
    pub fn width(&self) -> f64 {
        self.width
    }

    /// Routing grid in millimetres.
    pub fn grid(&self) -> f64 {
        self.config.grid
    }

    /// Pending start point, if the first click has been made.
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

    /// Number of board snapshots available for undo.
    pub fn undo_len(&self) -> usize {
        self.undo_stack.len()
    }

    /// Select the route layer. A pending start is discarded.
    pub fn set_layer(&mut self, layer: u16) {
        self.layer = layer;
        self.pending_start = None;
    }

    /// Select the route net. A pending start is discarded.
    pub fn set_net(&mut self, net_id: Uuid) {
        self.net_id = net_id;
        self.pending_start = None;
    }

    /// Set the route width. Invalid widths are ignored.
    pub fn set_width(&mut self, width: f64) -> bool {
        if !width.is_finite() || width <= 0.0 {
            return false;
        }
        self.width = width;
        self.pending_start = None;
        true
    }

    /// Set the first endpoint of a route.
    ///
    /// A click within [`PAD_SNAP_RADIUS`] of a pad uses the pad centre and, if
    /// the pad has one, the pad's net. Existing same-net tracks and vias are
    /// valid start points, so a route can form a T-junction.
    pub fn begin_route(
        &mut self,
        design: &PcbDesign,
        point: Point,
    ) -> Result<RouteClick, RouteEditError> {
        if !design.outline.contains_point(&point) {
            return Err(RouteEditError::OutsideBoard);
        }
        let (point, net_id) = match nearest_pad(design, point) {
            Some(pad) => (pad.center, pad.net_id),
            None => (point, None),
        };
        if let Some(net_id) = net_id {
            self.net_id = net_id;
        }
        if self.net_id.is_nil() {
            return Err(RouteEditError::MissingNet);
        }
        self.pending_start = Some(snap_point(point, self.config.grid));
        Ok(RouteClick::Started)
    }

    /// Route from the pending start to `point` and commit the result.
    ///
    /// The candidate board is checked with DRC before it replaces the current
    /// design. If routing or DRC fails, the board is left unchanged and the
    /// start point remains available for another attempt.
    pub fn route_to(
        &mut self,
        design: &mut PcbDesign,
        point: Point,
    ) -> Result<RouteClick, RouteEditError> {
        let start = self.pending_start.ok_or(RouteEditError::NotStarted)?;
        if !design.outline.contains_point(&point) {
            return Err(RouteEditError::OutsideBoard);
        }

        let (point, pad_net) = match nearest_pad(design, point) {
            Some(pad) => (pad.center, pad.net_id),
            None => (point, None),
        };
        if let Some(net_id) = pad_net {
            if self.net_id.is_nil() {
                self.net_id = net_id;
            } else if self.net_id != net_id {
                return Err(RouteEditError::PadNetMismatch);
            }
        }
        if self.net_id.is_nil() {
            return Err(RouteEditError::MissingNet);
        }

        let end = snap_point(point, self.config.grid);
        if start == end {
            return Err(RouteEditError::SamePoint);
        }

        let request =
            RouteRequest { net_id: self.net_id, start, end, layer: self.layer, width: self.width };
        let route = route_multilayer(&request, design, &self.config)?;

        let mut candidate = design.clone();
        candidate.tracks.extend(route.tracks.iter().cloned());
        candidate.vias.extend(route.vias.iter().cloned());
        let before = run_drc(design, &design.rules).violations.len();
        let after = run_drc(&candidate, &candidate.rules).violations.len();
        if after > before {
            return Err(RouteEditError::DrcRejected { violations: after - before });
        }

        push_bounded(&mut self.undo_stack, design.clone());
        self.redo_stack.clear();
        *design = candidate;
        self.pending_start = None;
        Ok(RouteClick::Routed { tracks: route.tracks.len(), vias: route.vias.len() })
    }

    /// Discard a pending start point.
    pub fn cancel_route(&mut self) {
        self.pending_start = None;
    }

    /// Restore the board before the most recent committed edit.
    pub fn undo(&mut self, design: &mut PcbDesign) -> bool {
        let Some(previous) = self.undo_stack.pop() else {
            return false;
        };
        self.pending_start = None;
        let current = std::mem::replace(design, previous);
        self.redo_stack.push(current);
        self.selection = self.selection.filter(|selection| selection_exists(design, *selection));
        true
    }

    /// Reapply the most recently undone edit.
    pub fn redo(&mut self, design: &mut PcbDesign) -> bool {
        let Some(next) = self.redo_stack.pop() else {
            return false;
        };
        self.pending_start = None;
        let current = std::mem::replace(design, next);
        push_bounded(&mut self.undo_stack, current);
        self.selection = self.selection.filter(|selection| selection_exists(design, *selection));
        true
    }

    /// The currently selected board object.
    pub fn selection(&self) -> Option<BoardSelection> {
        self.selection
    }

    /// Select a footprint by index.
    pub fn select_footprint(&mut self, index: usize) {
        self.selection = Some(BoardSelection::Footprint(index));
    }

    /// Clear the current selection.
    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    /// Select the object nearest `point`, within `tolerance` millimetres.
    ///
    /// Vias take priority over footprints, and footprints over tracks. A click
    /// that hits nothing clears the selection and returns `None`.
    pub fn select_at(
        &mut self,
        design: &PcbDesign,
        point: Point,
        tolerance: f64,
    ) -> Option<BoardSelection> {
        if !point.x.is_finite() || !point.y.is_finite() {
            self.selection = None;
            return None;
        }
        let tolerance = if tolerance.is_finite() { tolerance.max(0.0) } else { 0.0 };

        let via = design
            .vias
            .iter()
            .rev()
            .find(|via| via_hit(via, &point, tolerance))
            .map(|via| BoardSelection::Via(via.id));
        let footprint = design
            .footprints
            .iter()
            .enumerate()
            .rev()
            .find(|(_, footprint)| footprint_hit(footprint, &point, tolerance))
            .map(|(index, _)| BoardSelection::Footprint(index));
        let track = design
            .tracks
            .iter()
            .rev()
            .find(|track| track_hit(track, &point, tolerance))
            .map(|track| BoardSelection::Track(track.id));

        self.selection = via.or(footprint).or(track);
        self.selection
    }

    /// Move the selected object by a board-space delta.
    ///
    /// The move is one undoable edit. A zero delta is accepted without
    /// recording history.
    pub fn move_selected(
        &mut self,
        design: &mut PcbDesign,
        dx: f64,
        dy: f64,
    ) -> Result<BoardSelection, SelectionEditError> {
        if !dx.is_finite() || !dy.is_finite() {
            return Err(SelectionEditError::InvalidDelta);
        }
        let selection = self.selection.ok_or(SelectionEditError::NoSelection)?;
        if dx == 0.0 && dy == 0.0 {
            return Ok(selection);
        }
        let before = design.clone();

        match selection {
            BoardSelection::Footprint(index) => {
                let footprint =
                    design.footprints.get_mut(index).ok_or(SelectionEditError::MissingObject)?;
                let before = footprint.position;
                footprint.position = Point::new(before.x + dx, before.y + dy);
                if !footprint.position.x.is_finite() || !footprint.position.y.is_finite() {
                    footprint.position = before;
                    return Err(SelectionEditError::InvalidDelta);
                }
            }
            BoardSelection::Track(id) => {
                let track = design
                    .tracks
                    .iter_mut()
                    .find(|track| track.id == id)
                    .ok_or(SelectionEditError::MissingObject)?;
                for point in &mut track.path {
                    *point = Point::new(point.x + dx, point.y + dy);
                }
            }
            BoardSelection::Via(id) => {
                let via = design
                    .vias
                    .iter_mut()
                    .find(|via| via.id == id)
                    .ok_or(SelectionEditError::MissingObject)?;
                via.position = Point::new(via.position.x + dx, via.position.y + dy);
            }
        }

        self.record_edit(before);
        Ok(selection)
    }

    /// Rotate the selected footprint in 90-degree steps.
    ///
    /// Returns the normalized rotation in degrees. Tracks and vias report
    /// [`SelectionEditError::NotRotatable`].
    pub fn rotate_selected(
        &mut self,
        design: &mut PcbDesign,
        quarter_turns: i32,
    ) -> Result<f64, SelectionEditError> {
        let selection = self.selection.ok_or(SelectionEditError::NoSelection)?;
        let BoardSelection::Footprint(index) = selection else {
            return Err(SelectionEditError::NotRotatable);
        };
        if index >= design.footprints.len() {
            return Err(SelectionEditError::MissingObject);
        }
        let before = design.clone();
        let footprint = &mut design.footprints[index];
        footprint.rotation = (footprint.rotation + 90.0 * quarter_turns as f64).rem_euclid(360.0);
        let rotation = footprint.rotation;
        self.record_edit(before);
        Ok(rotation)
    }

    /// Move the selected footprint to the other board side.
    ///
    /// Returns the new layer name. Only footprints have a board side.
    pub fn flip_selected(&mut self, design: &mut PcbDesign) -> Result<String, SelectionEditError> {
        let selection = self.selection.ok_or(SelectionEditError::NoSelection)?;
        let BoardSelection::Footprint(index) = selection else {
            return Err(SelectionEditError::NotFlippable);
        };
        if index >= design.footprints.len() {
            return Err(SelectionEditError::MissingObject);
        }
        let before = design.clone();
        let footprint = &mut design.footprints[index];
        let layer = if footprint.layer.starts_with('B') { "F.Cu" } else { "B.Cu" }.to_string();
        footprint.layer = layer.clone();
        self.record_edit(before);
        Ok(layer)
    }

    /// Delete the selected object and clear the selection.
    pub fn delete_selected(
        &mut self,
        design: &mut PcbDesign,
    ) -> Result<BoardSelection, SelectionEditError> {
        let selection = self.selection.ok_or(SelectionEditError::NoSelection)?;
        if !selection_exists(design, selection) {
            self.selection = None;
            return Err(SelectionEditError::MissingObject);
        }
        let before = design.clone();
        match selection {
            BoardSelection::Footprint(index) => {
                design.footprints.remove(index);
            }
            BoardSelection::Track(id) => {
                if let Some(position) = design.tracks.iter().position(|track| track.id == id) {
                    design.tracks.remove(position);
                }
            }
            BoardSelection::Via(id) => {
                if let Some(position) = design.vias.iter().position(|via| via.id == id) {
                    design.vias.remove(position);
                }
            }
        }
        self.record_edit(before);
        self.selection = None;
        Ok(selection)
    }

    fn record_edit(&mut self, before: PcbDesign) {
        push_bounded(&mut self.undo_stack, before);
        self.redo_stack.clear();
    }
}

fn selection_exists(design: &PcbDesign, selection: BoardSelection) -> bool {
    match selection {
        BoardSelection::Footprint(index) => index < design.footprints.len(),
        BoardSelection::Track(id) => design.tracks.iter().any(|track| track.id == id),
        BoardSelection::Via(id) => design.vias.iter().any(|via| via.id == id),
    }
}

fn via_hit(via: &pcb::Via, point: &Point, tolerance: f64) -> bool {
    via.diameter.is_finite()
        && via.diameter > 0.0
        && via.position.distance_to(point) <= via.diameter / 2.0 + tolerance
}

fn footprint_hit(footprint: &Footprint, point: &Point, tolerance: f64) -> bool {
    let local = footprint_local_point(footprint, point);
    let Some(local) = local else {
        return false;
    };
    let bounds = footprint_local_bounds(footprint);
    if local.x >= bounds[0] - tolerance
        && local.x <= bounds[2] + tolerance
        && local.y >= bounds[1] - tolerance
        && local.y <= bounds[3] + tolerance
    {
        return true;
    }
    footprint.pad_positions.iter().any(|pad| {
        (local.x - pad.x).abs() <= pad.width.max(0.0) / 2.0 + tolerance
            && (local.y - pad.y).abs() <= pad.height.max(0.0) / 2.0 + tolerance
    })
}

fn track_hit(track: &pcb::Track, point: &Point, tolerance: f64) -> bool {
    let reach = track.width.max(0.0) / 2.0 + tolerance;
    match track.path.as_slice() {
        [] => false,
        [only] => only.distance_to(point) <= reach,
        path => path
            .windows(2)
            .any(|segment| point.distance_to_segment(&segment[0], &segment[1]) <= reach),
    }
}

/// Axis-aligned bounds of a placed footprint in board millimetres.
///
/// The bounds cover the footprint outline and every pad, rotated with the
/// footprint. They ignore pad corner rounding, so the result is conservative.
pub fn footprint_board_bounds(footprint: &Footprint) -> [f64; 4] {
    let local = footprint_local_bounds(footprint);
    let (sin, cos) = footprint.rotation.to_radians().sin_cos();
    let mut bounds = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
    for (x, y) in
        [(local[0], local[1]), (local[2], local[1]), (local[2], local[3]), (local[0], local[3])]
    {
        let placed_x = footprint.position.x + x * cos - y * sin;
        let placed_y = footprint.position.y + x * sin + y * cos;
        bounds[0] = bounds[0].min(placed_x);
        bounds[1] = bounds[1].min(placed_y);
        bounds[2] = bounds[2].max(placed_x);
        bounds[3] = bounds[3].max(placed_y);
    }
    bounds
}

fn footprint_local_point(footprint: &Footprint, point: &Point) -> Option<Point> {
    if !point.x.is_finite() || !point.y.is_finite() || !footprint.rotation.is_finite() {
        return None;
    }
    let (sin, cos) = (-footprint.rotation).to_radians().sin_cos();
    let dx = point.x - footprint.position.x;
    let dy = point.y - footprint.position.y;
    Some(Point::new(dx * cos - dy * sin, dx * sin + dy * cos))
}

fn footprint_local_bounds(footprint: &Footprint) -> [f64; 4] {
    let (x, y, width, height) = match footprint.outline.shape {
        OutlineShape::Rectangle { x, y, width, height }
        | OutlineShape::RoundedRectangle { x, y, width, height, .. } => (x, y, width, height),
    };
    let mut bounds = [x, y, x + width, y + height];
    for pad in &footprint.pad_positions {
        let half = pad.width.abs().max(pad.height.abs()) / 2.0;
        bounds[0] = bounds[0].min(pad.x - half);
        bounds[1] = bounds[1].min(pad.y - half);
        bounds[2] = bounds[2].max(pad.x + half);
        bounds[3] = bounds[3].max(pad.y + half);
    }
    bounds
}

fn push_bounded(stack: &mut Vec<PcbDesign>, design: PcbDesign) {
    if stack.len() == MAX_EDIT_HISTORY {
        stack.remove(0);
    }
    stack.push(design);
}

fn snap_point(point: Point, grid: f64) -> Point {
    Point::new((point.x / grid).round() * grid, (point.y / grid).round() * grid)
}

fn nearest_pad(design: &PcbDesign, point: Point) -> Option<PadHit> {
    let mut best: Option<(f64, PadHit)> = None;
    for footprint in &design.footprints {
        for pad in &footprint.pad_positions {
            let center = Point::new(footprint.position.x + pad.x, footprint.position.y + pad.y);
            let distance = center.distance_to(&point);
            if distance > PAD_SNAP_RADIUS {
                continue;
            }
            if best.as_ref().is_none_or(|(best_distance, _)| distance < *best_distance) {
                best = Some((distance, PadHit { center, net_id: pad.net_id }));
            }
        }
    }
    best.map(|(_, pad)| pad)
}

#[cfg(test)]
mod tests {
    use super::{
        BoardSelection, MOVE_GRID, RouteClick, RouteEditError, SelectionEditError, TrackEditor,
        footprint_board_bounds,
    };
    use core::{LayerKind, PcbLayer, PcbOutline, Point, Stackup};
    use pcb::footprint::{Footprint, PadPosition, PadType};
    use pcb::{DrcRules, PcbDesign};
    use uuid::Uuid;

    fn design() -> PcbDesign {
        PcbDesign {
            outline: PcbOutline::rectangle(20.0, 20.0),
            layers: vec![
                PcbLayer::new(0, "F.Cu", LayerKind::Copper),
                PcbLayer::new(1, "B.Cu", LayerKind::Copper),
            ],
            stackup: Stackup { layers: vec![0, 1] },
            rules: DrcRules::default(),
            ..PcbDesign::default()
        }
    }

    fn with_nets() -> (PcbDesign, Uuid) {
        let mut design = design();
        let net = Uuid::from_u128(1);
        design.footprints.push(Footprint {
            name: "R_0603".into(),
            reference_designator: "R1".into(),
            position: Point::new(2.0, 2.0),
            pad_positions: vec![PadPosition {
                number: "1".to_string(),
                x: 0.0,
                y: 0.0,
                pad_type: PadType::SurfaceMount,
                width: 1.4,
                height: 0.8,
                rotation: 0.0,
                net_id: Some(net),
                ..PadPosition::default()
            }],
            ..Footprint::default()
        });
        (design, net)
    }

    #[test]
    fn a_route_commits_copper_and_can_be_undone_and_redone() {
        let (mut design, net) = with_nets();
        let mut editor = TrackEditor::new(&design);
        assert_eq!(editor.net_id(), net);

        let clicked = editor.begin_route(&design, Point::new(2.0, 2.0)).expect("start");
        assert_eq!(clicked, RouteClick::Started);
        assert!(editor.pending_start().is_some());

        let route = editor.route_to(&mut design, Point::new(12.0, 2.0)).expect("route");
        assert!(matches!(route, RouteClick::Routed { tracks, .. } if tracks > 0));
        assert!(!design.tracks.is_empty());
        assert!(editor.can_undo());
        assert!(editor.pending_start().is_none());

        assert!(editor.undo(&mut design));
        assert!(design.tracks.is_empty());
        assert!(editor.can_redo());

        assert!(editor.redo(&mut design));
        assert!(!design.tracks.is_empty());
    }

    #[test]
    fn failed_clicks_leave_the_board_unchanged() {
        let (mut design, _) = with_nets();
        let before = serde_json::to_string(&design).expect("serialize");
        let mut editor = TrackEditor::new(&design);

        let error = editor.route_to(&mut design, Point::new(12.0, 2.0)).expect_err("no start");
        assert!(matches!(error, RouteEditError::NotStarted));

        let error = editor.begin_route(&design, Point::new(25.0, 2.0)).expect_err("outside");
        assert!(matches!(error, RouteEditError::OutsideBoard));

        assert_eq!(before, serde_json::to_string(&design).expect("serialize"));
        assert!(editor.pending_start().is_none());
    }

    #[test]
    fn a_pad_net_is_inferred_and_a_different_pad_net_is_rejected() {
        let mut design = design();
        let first_net = Uuid::from_u128(1);
        let second_net = Uuid::from_u128(2);
        design.footprints.push(Footprint {
            name: "R_0603".into(),
            reference_designator: "R1".into(),
            position: Point::new(2.0, 2.0),
            pad_positions: vec![
                PadPosition {
                    number: "1".to_string(),
                    x: 0.0,
                    y: 0.0,
                    pad_type: PadType::SurfaceMount,
                    width: 1.4,
                    height: 0.8,
                    rotation: 0.0,
                    net_id: Some(first_net),
                    ..PadPosition::default()
                },
                PadPosition {
                    number: "2".to_string(),
                    x: 8.0,
                    y: 0.0,
                    pad_type: PadType::SurfaceMount,
                    width: 1.4,
                    height: 0.8,
                    rotation: 0.0,
                    net_id: Some(second_net),
                    ..PadPosition::default()
                },
            ],
            ..Footprint::default()
        });
        design.tracks.push(pcb::Track {
            net_id: first_net,
            path: vec![Point::new(2.0, 2.0), Point::new(4.0, 2.0)],
            ..pcb::Track::default()
        });
        let mut editor = TrackEditor::new(&design);

        editor.begin_route(&design, Point::new(2.0, 2.0)).expect("start");
        assert_eq!(editor.net_id(), first_net);

        let error = editor.route_to(&mut design, Point::new(10.0, 2.0)).expect_err("net mismatch");
        assert!(matches!(error, RouteEditError::PadNetMismatch));
    }

    #[test]
    fn a_route_that_would_add_drc_errors_is_rejected() {
        let (mut design, _) = with_nets();
        let before = serde_json::to_string(&design).expect("serialize");
        let mut editor = TrackEditor::new(&design);
        assert!(editor.set_width(0.01));
        editor.begin_route(&design, Point::new(2.0, 2.0)).expect("start");

        let error = editor.route_to(&mut design, Point::new(12.0, 2.0)).expect_err("DRC rejects");
        assert!(matches!(error, RouteEditError::DrcRejected { .. }), "unexpected: {error:?}");
        assert_eq!(before, serde_json::to_string(&design).expect("serialize"));
        assert!(editor.pending_start().is_some());
        assert!(!editor.can_undo());
    }

    fn with_track_and_via() -> (PcbDesign, Uuid, Uuid) {
        let mut design = design();
        let net = Uuid::from_u128(7);
        design.tracks.push(pcb::Track {
            id: Uuid::from_u128(11),
            net_id: net,
            layer: 0,
            width: 0.3,
            path: vec![Point::new(4.0, 15.0), Point::new(14.0, 15.0)],
        });
        design.vias.push(pcb::Via {
            id: Uuid::from_u128(12),
            position: Point::new(15.0, 10.0),
            net_id: Some(net),
            ..pcb::Via::default()
        });
        (design, Uuid::from_u128(11), Uuid::from_u128(12))
    }

    #[test]
    fn clicks_select_vias_footprints_tracks_and_empty_space() {
        let (mut design, net) = with_nets();
        let track = Uuid::from_u128(11);
        let via = Uuid::from_u128(12);
        design.tracks.push(pcb::Track {
            id: track,
            net_id: net,
            layer: 0,
            width: 0.3,
            path: vec![Point::new(4.0, 15.0), Point::new(14.0, 15.0)],
        });
        design.vias.push(pcb::Via {
            id: via,
            position: Point::new(15.0, 10.0),
            ..pcb::Via::default()
        });
        let mut editor = TrackEditor::new(&design);

        assert_eq!(
            editor.select_at(&design, Point::new(2.0, 2.0), 0.2),
            Some(BoardSelection::Footprint(0))
        );
        assert_eq!(
            editor.select_at(&design, Point::new(15.0, 10.0), 0.2),
            Some(BoardSelection::Via(via))
        );
        assert_eq!(
            editor.select_at(&design, Point::new(9.0, 15.0), 0.2),
            Some(BoardSelection::Track(track))
        );
        assert_eq!(editor.select_at(&design, Point::new(50.0, 50.0), 0.2), None);
        assert_eq!(editor.selection(), None);
    }

    #[test]
    fn moving_a_footprint_is_one_undoable_edit() {
        let (mut design, _) = with_nets();
        let mut editor = TrackEditor::new(&design);
        editor.select_footprint(0);

        editor.move_selected(&mut design, MOVE_GRID, -MOVE_GRID).expect("move");
        assert_eq!(design.footprints[0].position, Point::new(2.5, 1.5));
        assert_eq!(editor.selection(), Some(BoardSelection::Footprint(0)));

        assert!(editor.undo(&mut design));
        assert_eq!(design.footprints[0].position, Point::new(2.0, 2.0));
        assert!(editor.redo(&mut design));
        assert_eq!(design.footprints[0].position, Point::new(2.5, 1.5));
    }

    #[test]
    fn rotating_and_flipping_a_footprint_are_undoable() {
        let (mut design, _) = with_nets();
        let mut editor = TrackEditor::new(&design);
        editor.select_footprint(0);

        assert_eq!(editor.rotate_selected(&mut design, 1).expect("rotate"), 90.0);
        assert_eq!(design.footprints[0].rotation, 90.0);
        assert_eq!(editor.rotate_selected(&mut design, 3).expect("rotate back"), 0.0);

        assert_eq!(editor.flip_selected(&mut design).expect("flip"), "B.Cu");
        assert_eq!(design.footprints[0].layer, "B.Cu");
        assert_eq!(editor.flip_selected(&mut design).expect("flip back"), "F.Cu");

        assert!(editor.undo(&mut design));
        assert_eq!(design.footprints[0].layer, "B.Cu");
        assert!(editor.undo(&mut design));
        assert_eq!(design.footprints[0].layer, "F.Cu");
    }

    #[test]
    fn deleting_a_footprint_clears_the_selection_and_can_be_undone() {
        let (mut design, _) = with_nets();
        let mut editor = TrackEditor::new(&design);
        editor.select_footprint(0);

        let removed = editor.delete_selected(&mut design).expect("delete");
        assert_eq!(removed, BoardSelection::Footprint(0));
        assert!(design.footprints.is_empty());
        assert_eq!(editor.selection(), None);

        assert!(editor.undo(&mut design));
        assert_eq!(design.footprints.len(), 1);
    }

    #[test]
    fn tracks_and_vias_move_and_delete_as_whole_objects() {
        let (mut design, track, via) = with_track_and_via();
        let mut editor = TrackEditor::new(&design);

        assert_eq!(
            editor.select_at(&design, Point::new(9.0, 15.0), 0.1),
            Some(BoardSelection::Track(track))
        );
        editor.move_selected(&mut design, 1.0, 2.0).expect("move track");
        assert_eq!(design.tracks[0].path[0], Point::new(5.0, 17.0));
        assert_eq!(design.tracks[0].path[1], Point::new(15.0, 17.0));
        editor.delete_selected(&mut design).expect("delete track");
        assert!(design.tracks.is_empty());

        assert_eq!(
            editor.select_at(&design, Point::new(15.0, 10.0), 0.1),
            Some(BoardSelection::Via(via))
        );
        editor.move_selected(&mut design, -2.0, 1.0).expect("move via");
        assert_eq!(design.vias[0].position, Point::new(13.0, 11.0));
        editor.delete_selected(&mut design).expect("delete via");
        assert!(design.vias.is_empty());

        assert!(editor.undo(&mut design));
        assert_eq!(design.vias.len(), 1);
        assert!(editor.undo(&mut design));
        assert_eq!(design.vias[0].position, Point::new(15.0, 10.0));
    }

    #[test]
    fn selection_edits_reject_missing_selection_and_non_rotatable_objects() {
        let (mut design, track, _) = with_track_and_via();
        let before = serde_json::to_string(&design).expect("serialize");
        let mut editor = TrackEditor::new(&design);

        assert_eq!(
            editor.move_selected(&mut design, 1.0, 0.0),
            Err(SelectionEditError::NoSelection)
        );
        assert_eq!(editor.delete_selected(&mut design), Err(SelectionEditError::NoSelection));

        editor.select_at(&design, Point::new(9.0, 15.0), 0.1);
        assert_eq!(editor.selection(), Some(BoardSelection::Track(track)));
        assert_eq!(editor.rotate_selected(&mut design, 1), Err(SelectionEditError::NotRotatable));
        assert_eq!(editor.flip_selected(&mut design), Err(SelectionEditError::NotFlippable));
        assert_eq!(before, serde_json::to_string(&design).expect("serialize"));
    }

    #[test]
    fn footprint_bounds_follow_rotation() {
        let (design, _) = with_nets();
        let flat = footprint_board_bounds(&design.footprints[0]);

        let mut rotated = design.footprints[0].clone();
        rotated.rotation = 90.0;
        let turned = footprint_board_bounds(&rotated);

        assert!(flat[0] <= 2.0 && flat[1] <= 2.0 && flat[2] >= 3.0 && flat[3] >= 3.0);
        assert_ne!(flat, turned);
    }

    #[test]
    fn a_new_route_after_undo_clears_redo() {
        let (mut design, _) = with_nets();
        let mut editor = TrackEditor::new(&design);
        editor.begin_route(&design, Point::new(2.0, 2.0)).expect("start");
        editor.route_to(&mut design, Point::new(12.0, 2.0)).expect("first route");
        editor.undo(&mut design);
        assert!(editor.can_redo());

        editor.begin_route(&design, Point::new(2.0, 2.0)).expect("start");
        editor.route_to(&mut design, Point::new(2.0, 12.0)).expect("second route");
        assert!(!editor.can_redo());
    }
}
