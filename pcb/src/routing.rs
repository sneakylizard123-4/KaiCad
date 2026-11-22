//! Deterministic single-layer PCB routing on a configurable grid.
//!
//! [`route`] snaps the requested endpoints to a grid, runs four-neighbour A*
//! over nodes inside the board outline, avoids existing copper and returns one
//! simplified track. All ties are broken deterministically and track ids are
//! UUIDv5 values derived from the request, so repeated routing is byte-identical.

use std::cmp::{Ordering, Reverse};
use std::collections::{BinaryHeap, HashMap, HashSet};

use uuid::Uuid;

use crate::{PcbDesign, Track, rect_distance_to_point, rect_distance_to_segment};

/// Router for generating copper traces.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Router {
    pub algorithm: RoutingAlgorithm,
    pub optimization: RoutingOptimization,
    /// Costs and limits for the grid router.
    #[serde(default)]
    pub config: RouterConfig,
}

impl Router {
    /// Create a router with the default algorithm, optimization and config.
    pub fn new() -> Self {
        Self::default()
    }
}

/// Routing algorithm type.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RoutingAlgorithm {
    #[default]
    PushAndShove,
    LengthMatching,
    Hybrid,
}

/// Routing optimization strategy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RoutingOptimization {
    #[default]
    MinimalLength,
    MinimalArea,
    Balanced,
}

fn default_grid() -> f64 {
    0.5
}

fn default_bend_cost() -> f64 {
    1.0
}

fn default_max_expansions() -> usize {
    200_000
}

/// Costs and limits for the grid router.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RouterConfig {
    /// Grid spacing in millimetres. Must be finite and positive.
    #[serde(default = "default_grid")]
    pub grid: f64,
    /// Extra path cost, in grid steps, for each direction change.
    #[serde(default = "default_bend_cost")]
    pub bend_cost: f64,
    /// Maximum number of grid nodes the search may expand.
    #[serde(default = "default_max_expansions")]
    pub max_expansions: usize,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            grid: default_grid(),
            bend_cost: default_bend_cost(),
            max_expansions: default_max_expansions(),
        }
    }
}

fn default_route_width() -> f64 {
    0.25
}

/// A single-net, single-layer routing request, in millimetres.
#[derive(Debug, Clone, PartialEq)]
pub struct RouteRequest {
    pub net_id: Uuid,
    pub start: core::Point,
    pub end: core::Point,
    pub layer: u16,
    /// Copper width in millimetres.
    pub width: f64,
}

impl Default for RouteRequest {
    fn default() -> Self {
        Self {
            net_id: Uuid::nil(),
            start: core::Point::default(),
            end: core::Point::default(),
            layer: 0,
            width: default_route_width(),
        }
    }
}

/// Why a route could not be produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteError {
    /// The request or configuration is unusable; the message names the field.
    InvalidRequest(String),
    /// No path exists within the search bounds.
    NoPath,
    /// The search hit [`RouterConfig::max_expansions`] before finding a path.
    ExpansionLimit,
}

impl std::fmt::Display for RouteError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRequest(message) => write!(formatter, "invalid route request: {message}"),
            Self::NoPath => write!(formatter, "no path exists within the search bounds"),
            Self::ExpansionLimit => write!(formatter, "routing exceeded the expansion limit"),
        }
    }
}

impl std::error::Error for RouteError {}

/// Route one net on one layer, returning simplified tracks.
///
/// Endpoints snap to the nearest node of an origin-anchored grid, so exact
/// half-grid ties round away from zero. The search avoids existing tracks,
/// vias and pads on the requested copper layer, using the design's clearance,
/// and keeps every node inside the board outline. The returned track uses
/// `request.net_id`, `request.width` and `request.layer`; adding it to the
/// design produces no DRC error for clearance or containment.
pub fn route(
    request: &RouteRequest,
    design: &PcbDesign,
    config: &RouterConfig,
) -> Result<Vec<Track>, RouteError> {
    validate(request, design, config)?;

    let start_node = snap_node(request.start, config.grid);
    let end_node = snap_node(request.end, config.grid);
    let start_point = node_point(start_node, config.grid);
    let end_point = node_point(end_node, config.grid);

    if start_node == end_node {
        return Err(invalid("start and end snap to the same grid node"));
    }
    if !design.outline.contains_point(&start_point) {
        return Err(invalid("start point is outside the board outline"));
    }
    if !design.outline.contains_point(&end_point) {
        return Err(invalid("end point is outside the board outline"));
    }

    let pads = design.pad_copper();
    let context = SearchContext {
        design,
        layer: request.layer,
        width: request.width,
        clearance: design.rules.clearance,
        net_id: request.net_id,
        pads: &pads,
    };

    if context.point_blocked(&start_point) {
        return Err(invalid("start point is blocked by existing copper"));
    }
    if context.point_blocked(&end_point) {
        return Err(invalid("end point is blocked by existing copper"));
    }

    let bbox = design
        .outline
        .bounding_box()
        .ok_or_else(|| invalid("board outline has no bounding box"))?;
    let min_x = (bbox.min.x / config.grid).floor() as i64;
    let max_x = (bbox.max.x / config.grid).ceil() as i64;
    let min_y = (bbox.min.y / config.grid).floor() as i64;
    let max_y = (bbox.max.y / config.grid).ceil() as i64;

    let start_state = State { node: start_node, direction: START_DIRECTION };
    let mut open = BinaryHeap::new();
    let mut best = HashMap::new();
    let mut came = HashMap::new();
    let mut closed = HashSet::new();

    best.insert(start_state, 0.0);
    open.push(Reverse(QueueItem {
        f: heuristic(start_node, end_node),
        g: 0.0,
        state: start_state,
    }));

    let mut expansions = 0usize;
    let mut goal = None;

    while let Some(Reverse(item)) = open.pop() {
        if closed.contains(&item.state) {
            continue;
        }
        if item.state.node == end_node {
            goal = Some(item.state);
            break;
        }

        closed.insert(item.state);
        expansions += 1;
        if expansions > config.max_expansions {
            return Err(RouteError::ExpansionLimit);
        }

        let current_point = node_point(item.state.node, config.grid);
        for (dx, dy, direction) in MOVES {
            let next = Node { x: item.state.node.x + dx, y: item.state.node.y + dy };
            if next.x < min_x || next.x > max_x || next.y < min_y || next.y > max_y {
                continue;
            }

            let next_point = node_point(next, config.grid);
            if !design.outline.contains_point(&next_point) {
                continue;
            }
            if context.segment_blocked(&current_point, &next_point) {
                continue;
            }

            let bend =
                if item.state.direction != START_DIRECTION && item.state.direction != direction {
                    config.bend_cost
                } else {
                    0.0
                };
            let g = item.g + 1.0 + bend;
            let state = State { node: next, direction };

            if best.get(&state).is_some_and(|existing| *existing <= g) {
                continue;
            }
            best.insert(state, g);
            came.insert(state, item.state);
            open.push(Reverse(QueueItem { f: g + heuristic(next, end_node), g, state }));
        }
    }

    let goal = goal.ok_or(RouteError::NoPath)?;
    let nodes = reconstruct(goal, &came);
    let path = nodes.into_iter().map(|node| node_point(node, config.grid)).collect();

    Ok(vec![Track {
        id: route_id(request, config, start_node, end_node),
        net_id: request.net_id,
        layer: request.layer,
        width: request.width,
        path,
    }])
}

fn validate(
    request: &RouteRequest,
    design: &PcbDesign,
    config: &RouterConfig,
) -> Result<(), RouteError> {
    if !config.grid.is_finite() || config.grid <= 0.0 {
        return Err(invalid("grid must be finite and greater than zero"));
    }
    if !config.bend_cost.is_finite() || config.bend_cost < 0.0 {
        return Err(invalid("bend_cost must be finite and non-negative"));
    }
    if config.max_expansions == 0 {
        return Err(invalid("max_expansions must be greater than zero"));
    }
    if !request.width.is_finite() || request.width <= 0.0 {
        return Err(invalid("width must be finite and greater than zero"));
    }
    if !request.start.x.is_finite() || !request.start.y.is_finite() {
        return Err(invalid("start must be finite"));
    }
    if !request.end.x.is_finite() || !request.end.y.is_finite() {
        return Err(invalid("end must be finite"));
    }
    if design.outline.points.len() < 3 {
        return Err(invalid("board outline needs at least three points"));
    }
    if !design.layers.iter().any(|layer| layer.id == request.layer && layer.is_copper()) {
        return Err(invalid("layer is missing or not copper"));
    }
    design
        .rules
        .validate()
        .map_err(|error| invalid(format!("design DRC rules are invalid: {error}")))?;
    Ok(())
}

fn invalid(message: impl Into<String>) -> RouteError {
    RouteError::InvalidRequest(message.into())
}

const START_DIRECTION: u8 = 4;
const MOVES: [(i64, i64, u8); 4] = [(1, 0, 0), (-1, 0, 1), (0, 1, 2), (0, -1, 3)];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct Node {
    x: i64,
    y: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct State {
    node: Node,
    direction: u8,
}

#[derive(Debug, Clone, Copy)]
struct QueueItem {
    f: f64,
    g: f64,
    state: State,
}

impl PartialEq for QueueItem {
    fn eq(&self, other: &Self) -> bool {
        self.f.to_bits() == other.f.to_bits()
            && self.g.to_bits() == other.g.to_bits()
            && self.state == other.state
    }
}

impl Eq for QueueItem {}

impl Ord for QueueItem {
    fn cmp(&self, other: &Self) -> Ordering {
        self.f
            .total_cmp(&other.f)
            .then_with(|| self.g.total_cmp(&other.g))
            .then_with(|| self.state.node.y.cmp(&other.state.node.y))
            .then_with(|| self.state.node.x.cmp(&other.state.node.x))
            .then_with(|| self.state.direction.cmp(&other.state.direction))
    }
}

impl PartialOrd for QueueItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

struct SearchContext<'a> {
    design: &'a PcbDesign,
    layer: u16,
    width: f64,
    clearance: f64,
    net_id: Uuid,
    pads: &'a [crate::PadCopper],
}

impl SearchContext<'_> {
    fn point_blocked(&self, point: &core::Point) -> bool {
        let gap = self.clearance + self.width / 2.0;

        for track in &self.design.tracks {
            if track.layer != self.layer || !track.width.is_finite() {
                continue;
            }
            let required = gap + track.width / 2.0;
            for window in track.path.windows(2) {
                if point.distance_to_segment(&window[0], &window[1]) < required {
                    return true;
                }
            }
        }

        for via in &self.design.vias {
            if !self.design.via_covers_layer(via, self.layer) || !via.diameter.is_finite() {
                continue;
            }
            if point.distance_to(&via.position) < gap + via.diameter / 2.0 {
                return true;
            }
        }

        for pad in self.pads {
            if !pad.has_valid_size()
                || !pad.layers.contains(&self.layer)
                || pad.net_id == Some(self.net_id)
            {
                continue;
            }
            if rect_distance_to_point(&pad.rect, point) < gap {
                return true;
            }
        }
        false
    }

    fn segment_blocked(&self, start: &core::Point, end: &core::Point) -> bool {
        let gap = self.clearance + self.width / 2.0;

        for track in &self.design.tracks {
            if track.layer != self.layer || !track.width.is_finite() {
                continue;
            }
            let required = gap + track.width / 2.0;
            for window in track.path.windows(2) {
                if core::geometry::segment_distance(&window[0], &window[1], start, end) < required {
                    return true;
                }
            }
        }

        for via in &self.design.vias {
            if !self.design.via_covers_layer(via, self.layer) || !via.diameter.is_finite() {
                continue;
            }
            if via.position.distance_to_segment(start, end) < gap + via.diameter / 2.0 {
                return true;
            }
        }

        for pad in self.pads {
            if !pad.has_valid_size()
                || !pad.layers.contains(&self.layer)
                || pad.net_id == Some(self.net_id)
            {
                continue;
            }
            if rect_distance_to_segment(&pad.rect, start, end) < gap {
                return true;
            }
        }
        false
    }
}

fn snap_node(point: core::Point, grid: f64) -> Node {
    Node { x: (point.x / grid).round() as i64, y: (point.y / grid).round() as i64 }
}

fn node_point(node: Node, grid: f64) -> core::Point {
    core::Point::new(node.x as f64 * grid, node.y as f64 * grid)
}

fn heuristic(node: Node, goal: Node) -> f64 {
    ((node.x - goal.x).abs() + (node.y - goal.y).abs()) as f64
}

fn reconstruct(goal: State, came: &HashMap<State, State>) -> Vec<Node> {
    let mut states = vec![goal];
    let mut current = goal;
    while let Some(previous) = came.get(&current).copied() {
        states.push(previous);
        current = previous;
    }
    states.reverse();

    let mut nodes: Vec<Node> = Vec::new();
    for state in states {
        if nodes.last() != Some(&state.node) {
            nodes.push(state.node);
        }
    }
    simplify(nodes)
}

fn simplify(nodes: Vec<Node>) -> Vec<Node> {
    let mut simplified: Vec<Node> = Vec::new();
    for node in nodes {
        while simplified.len() >= 2 {
            let last = simplified[simplified.len() - 1];
            let before = simplified[simplified.len() - 2];
            if (last.x - before.x, last.y - before.y) == (node.x - last.x, node.y - last.y) {
                simplified.pop();
            } else {
                break;
            }
        }
        simplified.push(node);
    }
    simplified
}

fn route_id(request: &RouteRequest, config: &RouterConfig, start: Node, end: Node) -> Uuid {
    let name = format!(
        "route:{}:{}:{}:{}:{}:{}:{}:{}",
        request.net_id,
        request.layer,
        request.width.to_bits(),
        config.grid.to_bits(),
        start.x,
        start.y,
        end.x,
        end.y
    );
    Uuid::new_v5(&Uuid::NAMESPACE_OID, name.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::{RouteError, RouteRequest, Router, RouterConfig, route};
    use crate::{PcbDesign, Track, Via, run_drc};
    use core::{LayerKind, PcbLayer, PcbOutline, Point, Stackup};
    use uuid::Uuid;

    fn design() -> PcbDesign {
        PcbDesign {
            outline: PcbOutline::rectangle(20.0, 20.0),
            layers: vec![
                PcbLayer::new(0, "F.Cu", LayerKind::Copper),
                PcbLayer::new(1, "B.Cu", LayerKind::Copper),
                PcbLayer::new(2, "F.SilkS", LayerKind::SilkScreen),
            ],
            stackup: Stackup { layers: vec![0, 1] },
            ..PcbDesign::default()
        }
    }

    fn request(start: Point, end: Point) -> RouteRequest {
        RouteRequest { net_id: Uuid::from_u128(7), start, end, layer: 0, width: 0.25 }
    }

    fn config() -> RouterConfig {
        RouterConfig { grid: 1.0, bend_cost: 1.0, max_expansions: 100_000 }
    }

    fn wall(design: &mut PcbDesign, start: Point, end: Point) {
        design.tracks.push(Track {
            id: Uuid::new_v4(),
            net_id: Uuid::new_v4(),
            layer: 0,
            width: 0.25,
            path: vec![start, end],
        });
    }

    fn routed_tracks(
        request: &RouteRequest,
        design: &PcbDesign,
        config: &RouterConfig,
    ) -> Vec<Track> {
        let tracks = route(request, design, config).expect("route should succeed");
        let mut routed = design.clone();
        routed.tracks.extend(tracks.clone());
        let report = run_drc(&routed, &routed.rules);
        assert!(report.is_valid(), "route produced DRC errors: {:?}", report.violations);
        tracks
    }

    #[test]
    fn router_config_defaults_and_round_trip() {
        let router = Router::new();
        assert_eq!(router.config, RouterConfig::default());
        assert_eq!(router.config.grid, 0.5);
        assert_eq!(router.config.bend_cost, 1.0);
        assert_eq!(router.config.max_expansions, 200_000);

        let json = serde_json::to_string(&router).expect("router serializes");
        let restored: Router = serde_json::from_str(&json).expect("router parses");
        assert_eq!(restored.config, router.config);

        let legacy: Router =
            serde_json::from_str(r#"{"algorithm":"Hybrid","optimization":"MinimalArea"}"#)
                .expect("legacy router JSON loads");
        assert_eq!(legacy.algorithm, super::RoutingAlgorithm::Hybrid);
        assert_eq!(legacy.config, RouterConfig::default());
    }

    #[test]
    fn straight_route_is_simplified_and_snapped() {
        let design = design();
        let request = request(Point::new(1.2, 1.2), Point::new(8.7, 8.7));

        let tracks = routed_tracks(&request, &design, &config());

        assert_eq!(tracks.len(), 1);
        let track = &tracks[0];
        assert_eq!(track.net_id, request.net_id);
        assert_eq!(track.layer, 0);
        assert_eq!(track.width, 0.25);
        assert_eq!(track.path.first(), Some(&Point::new(1.0, 1.0)));
        assert_eq!(track.path.last(), Some(&Point::new(9.0, 9.0)));
        assert!(track.path.len() >= 2);
        assert!(
            track
                .path
                .iter()
                .all(|point| { (point.x.fract()).abs() < 1e-9 && (point.y.fract()).abs() < 1e-9 })
        );
    }

    #[test]
    fn route_bends_around_an_existing_track() {
        let mut design = design();
        wall(&mut design, Point::new(0.0, 10.0), Point::new(15.0, 10.0));

        let request = request(Point::new(5.0, 5.0), Point::new(5.0, 15.0));
        let tracks = routed_tracks(&request, &design, &config());

        assert!(tracks[0].path.len() > 2, "expected a detour with bends");
        assert!(tracks[0].path.iter().any(|point| point.x > 15.0));
    }

    #[test]
    fn enclosed_start_returns_no_path() {
        let mut design = design();
        wall(&mut design, Point::new(5.0, 5.0), Point::new(10.0, 5.0));
        wall(&mut design, Point::new(10.0, 5.0), Point::new(10.0, 10.0));
        wall(&mut design, Point::new(10.0, 10.0), Point::new(5.0, 10.0));
        wall(&mut design, Point::new(5.0, 10.0), Point::new(5.0, 5.0));

        let request = request(Point::new(8.0, 8.0), Point::new(15.0, 15.0));
        assert!(matches!(route(&request, &design, &config()), Err(RouteError::NoPath)));
    }

    #[test]
    fn small_expansion_limit_is_reported() {
        let design = design();
        let request = request(Point::new(1.0, 1.0), Point::new(19.0, 19.0));
        let config = RouterConfig { max_expansions: 1, ..config() };

        assert!(matches!(route(&request, &design, &config), Err(RouteError::ExpansionLimit)));
    }

    #[test]
    fn invalid_requests_name_the_problem() {
        let design = design();
        let config = config();

        let cases = [
            RouteRequest {
                start: Point::new(f64::NAN, 1.0),
                ..request(Point::new(1.0, 1.0), Point::new(5.0, 5.0))
            },
            RouteRequest { width: 0.0, ..request(Point::new(1.0, 1.0), Point::new(5.0, 5.0)) },
            RouteRequest { layer: 9, ..request(Point::new(1.0, 1.0), Point::new(5.0, 5.0)) },
            RouteRequest { layer: 2, ..request(Point::new(1.0, 1.0), Point::new(5.0, 5.0)) },
            request(Point::new(1.0, 1.0), Point::new(1.1, 1.1)),
            request(Point::new(1.0, 1.0), Point::new(25.0, 5.0)),
        ];

        for case in cases {
            assert!(matches!(route(&case, &design, &config), Err(RouteError::InvalidRequest(_))));
        }

        let bad_grid = RouterConfig { grid: 0.0, ..config };
        assert!(matches!(
            route(&request(Point::new(1.0, 1.0), Point::new(5.0, 5.0)), &design, &bad_grid),
            Err(RouteError::InvalidRequest(_))
        ));
    }

    #[test]
    fn vias_block_the_layers_they_span() {
        let mut design = design();
        design.vias.push(Via {
            id: Uuid::new_v4(),
            position: Point::new(5.0, 5.0),
            layer_start: 0,
            layer_end: 1,
            diameter: 1.0,
            drill: 0.4,
        });

        let tracks =
            routed_tracks(&request(Point::new(1.0, 5.0), Point::new(9.0, 5.0)), &design, &config());
        assert!(tracks[0].path.len() > 2, "via should force a detour");

        let mut other_layer = design;
        other_layer.vias[0].layer_start = 1;
        other_layer.vias[0].layer_end = 1;
        let straight =
            route(&request(Point::new(1.0, 5.0), Point::new(9.0, 5.0)), &other_layer, &config())
                .expect("a via on another layer should not block");
        assert_eq!(straight[0].path.len(), 2);
    }

    #[test]
    fn routing_is_deterministic_and_does_not_mutate_the_design() {
        let design = design();
        let before = serde_json::to_string(&design).expect("design serializes");
        let request = request(Point::new(2.0, 2.0), Point::new(17.0, 17.0));

        let first = route(&request, &design, &config()).expect("first route");
        let second = route(&request, &design, &config()).expect("second route");

        assert_eq!(
            serde_json::to_string(&first).expect("serialize"),
            serde_json::to_string(&second).expect("serialize")
        );
        assert_eq!(first[0].id, second[0].id);
        assert_eq!(before, serde_json::to_string(&design).expect("design serializes"));
    }

    #[test]
    fn pads_block_routing_unless_they_share_the_net() {
        let mut design = design();
        design.footprints.push(crate::Footprint {
            position: Point::new(5.0, 5.0),
            pad_positions: vec![crate::PadPosition {
                x: 0.0,
                y: 0.0,
                pad_type: crate::PadType::SurfaceMount,
                width: 2.0,
                height: 2.0,
                net_id: None,
            }],
            ..crate::Footprint::default()
        });

        let request = request(Point::new(1.0, 5.0), Point::new(9.0, 5.0));
        let tracks = routed_tracks(&request, &design, &config());
        assert!(tracks[0].path.len() > 2, "pad should force a detour");

        design.footprints[0].pad_positions[0].net_id = Some(request.net_id);
        let straight = route(&request, &design, &config()).expect("same-net pad should not block");
        assert_eq!(straight[0].path.len(), 2);
    }
}
