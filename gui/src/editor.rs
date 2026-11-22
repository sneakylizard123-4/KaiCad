//! Headless track-routing state for KaiCad Studio.
//!
//! The editor owns the current routing options and a bounded snapshot history.
//! It does not own the board: every operation receives a mutable
//! [`PcbDesign`], and a failed operation leaves that design untouched. Clicks
//! from the 2D viewport are converted to board millimetres by the application
//! layer before they reach this module.

use std::fmt;

use core::Point;
use pcb::routing::{RouteError, RouteRequest, RouterConfig, route_multilayer};
use pcb::{PcbDesign, run_drc};
use uuid::Uuid;

/// Distance within which a click snaps to a pad centre, in millimetres.
pub const PAD_SNAP_RADIUS: f64 = 2.0;

/// Maximum number of board snapshots retained for undo.
pub const MAX_EDIT_HISTORY: usize = 64;

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

    /// Restore the board before the most recent committed route.
    pub fn undo(&mut self, design: &mut PcbDesign) -> bool {
        let Some(previous) = self.undo_stack.pop() else {
            return false;
        };
        self.pending_start = None;
        let current = std::mem::replace(design, previous);
        self.redo_stack.push(current);
        true
    }

    /// Reapply the most recently undone route.
    pub fn redo(&mut self, design: &mut PcbDesign) -> bool {
        let Some(next) = self.redo_stack.pop() else {
            return false;
        };
        self.pending_start = None;
        let current = std::mem::replace(design, next);
        push_bounded(&mut self.undo_stack, current);
        true
    }
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
    use super::{RouteClick, RouteEditError, TrackEditor};
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
