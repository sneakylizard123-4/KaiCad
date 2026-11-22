//! Deterministic PCB routing on a configurable grid.
//!
//! [`route`] snaps the requested endpoints to a grid, runs four-neighbour A*
//! over nodes inside the board outline, avoids copper on other nets and returns
//! simplified single-layer tracks. [`route_multilayer`] uses the same search
//! but may insert vias and continue on other copper layers.
//! [`route_push_and_shove`] retries a blocked route after removing a bounded
//! set of conflicting tracks and rerouting them around the result, and
//! [`InteractiveRouter`] holds a preview that can be committed or cancelled.
//!
//! All ties are broken deterministically and every id is a UUIDv5 value
//! derived from the request and the corresponding path segment, so repeated
//! routing is byte-identical.

use std::cmp::{Ordering, Reverse};
use std::collections::{BinaryHeap, HashMap, HashSet};

use uuid::Uuid;

use crate::{PcbDesign, Track, Via, rect_distance_to_point, rect_distance_to_segment};

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

fn default_allow_vias() -> bool {
    true
}

fn default_via_cost() -> f64 {
    5.0
}

fn default_max_vias() -> usize {
    64
}

fn default_via_diameter() -> f64 {
    0.6
}

fn default_via_drill() -> f64 {
    0.3
}

fn default_allow_blind_buried() -> bool {
    false
}

fn default_max_shoved_tracks() -> usize {
    8
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
    /// Whether [`route_multilayer`] may insert vias. Generated vias are
    /// through vias unless [`RouterConfig::allow_blind_buried`] is also set.
    #[serde(default = "default_allow_vias")]
    pub allow_vias: bool,
    /// Cost of one layer transition, in grid steps.
    #[serde(default = "default_via_cost")]
    pub via_cost: f64,
    /// Maximum number of vias in one route.
    #[serde(default = "default_max_vias")]
    pub max_vias: usize,
    /// Outer copper diameter of generated vias, in millimetres.
    #[serde(default = "default_via_diameter")]
    pub via_diameter: f64,
    /// Finished drill diameter of generated vias, in millimetres.
    #[serde(default = "default_via_drill")]
    pub via_drill: f64,
    /// Whether layer changes may use blind and buried vias instead of always
    /// spanning the full copper stackup.
    #[serde(default = "default_allow_blind_buried")]
    pub allow_blind_buried: bool,
    /// Maximum number of existing tracks [`route_push_and_shove`] may remove
    /// in one attempt. Zero disables rip-up.
    #[serde(default = "default_max_shoved_tracks")]
    pub max_shoved_tracks: usize,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            grid: default_grid(),
            bend_cost: default_bend_cost(),
            max_expansions: default_max_expansions(),
            allow_vias: default_allow_vias(),
            via_cost: default_via_cost(),
            max_vias: default_max_vias(),
            via_diameter: default_via_diameter(),
            via_drill: default_via_drill(),
            allow_blind_buried: default_allow_blind_buried(),
            max_shoved_tracks: default_max_shoved_tracks(),
        }
    }
}

fn default_route_width() -> f64 {
    0.25
}

/// A single-net routing request, in millimetres.
///
/// Both endpoints are on `layer`, but [`route_multilayer`] may leave that
/// layer and return to it before reaching `end`.
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

/// Tracks and vias produced by a multi-layer route.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RouteResult {
    pub tracks: Vec<Track>,
    pub vias: Vec<Via>,
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
/// vias and pads on other nets on the requested copper layer, using the
/// design's clearance; same-net copper is traversable. It keeps every node
/// inside the board outline. The returned track uses
/// `request.net_id`, `request.width` and `request.layer`; adding it to the
/// design produces no DRC error for clearance or containment.
///
/// This entry point never inserts a via, even when `config.allow_vias` is
/// true. Use [`route_multilayer`] to allow layer changes.
pub fn route(
    request: &RouteRequest,
    design: &PcbDesign,
    config: &RouterConfig,
) -> Result<Vec<Track>, RouteError> {
    let mut single_layer = config.clone();
    single_layer.allow_vias = false;
    route_multilayer(request, design, &single_layer).map(|result| result.tracks)
}

/// Route one net, optionally changing copper layers through vias.
///
/// The search state is `(node, layer, direction)` plus the number of vias used
/// so far. In-layer moves cost one grid step, with `config.bend_cost` added on
/// a direction change. A via stays on the same node, costs `config.via_cost`
/// and may move to any other copper layer in stackup order.
///
/// A candidate via is only used when its diameter/drill satisfy the design's
/// `via-drill` and annular-ring limits, and when it clears every track, pad and
/// via on the copper layers it spans. Generated vias are through vias from the
/// top to the bottom copper layer unless `config.allow_blind_buried` is set,
/// in which case they span only the layers used by the layer change. Tracks
/// are split at each layer change and simplified independently; every track
/// uses `request.net_id` and `request.width`. With `allow_vias == false` this
/// is equivalent to [`route`].
pub fn route_multilayer(
    request: &RouteRequest,
    design: &PcbDesign,
    config: &RouterConfig,
) -> Result<RouteResult, RouteError> {
    validate(request, design, config)?;

    let copper = design.copper_layer_ids();
    if !copper.contains(&request.layer) {
        return Err(invalid("layer is not in the copper stackup"));
    }

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
        width: request.width,
        clearance: design.rules.clearance,
        net_id: request.net_id,
        pads: &pads,
        copper: &copper,
    };

    if context.point_blocked(&start_point, request.layer) {
        return Err(invalid("start point is blocked by existing copper"));
    }
    if context.point_blocked(&end_point, request.layer) {
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

    let start_state =
        State { node: start_node, layer: request.layer, direction: START_DIRECTION, vias: 0 };
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
        if item.state.node == end_node && item.state.layer == request.layer {
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
            if context.segment_blocked(&current_point, &next_point, item.state.layer) {
                continue;
            }

            let bend =
                if item.state.direction != START_DIRECTION && item.state.direction != direction {
                    config.bend_cost
                } else {
                    0.0
                };
            let g = item.g + 1.0 + bend;
            let state =
                State { node: next, layer: item.state.layer, direction, vias: item.state.vias };

            relax(&mut open, &mut best, &mut came, item.state, state, g, end_node);
        }

        if !config.allow_vias || copper.len() < 2 || item.state.vias as usize >= config.max_vias {
            continue;
        }

        let Some(from_index) = copper.iter().position(|id| *id == item.state.layer) else {
            continue;
        };
        for offset in 1..copper.len() {
            let target = copper[(from_index + offset) % copper.len()];
            if context.point_blocked(&current_point, target) {
                continue;
            }
            let (via_start, via_end) = if config.allow_blind_buried {
                (item.state.layer, target)
            } else {
                (*copper.first().unwrap_or(&item.state.layer), *copper.last().unwrap_or(&target))
            };
            if !context.via_allowed(&current_point, config, via_start, via_end) {
                continue;
            }

            let g = item.g + config.via_cost;
            let state = State {
                node: item.state.node,
                layer: target,
                direction: START_DIRECTION,
                vias: item.state.vias + 1,
            };
            relax(&mut open, &mut best, &mut came, item.state, state, g, end_node);
        }
    }

    let goal = goal.ok_or(RouteError::NoPath)?;
    let states = reconstruct(goal, &came);
    Ok(build_result(request, config, &copper, &states))
}

/// Tracks removed and rerouted by [`route_push_and_shove`].
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PushAndShoveResult {
    /// The requested route through the cleared area.
    pub route: RouteResult,
    /// Existing tracks removed from the design, ordered by id.
    pub removed_track_ids: Vec<Uuid>,
    /// Replacements for the removed tracks.
    pub rerouted: RouteResult,
}

impl PushAndShoveResult {
    /// Apply this result to `design`, removing the ripped-up tracks and adding
    /// both the new route and its rerouted replacements.
    pub fn apply_to(&self, design: &mut PcbDesign) {
        let removed: HashSet<Uuid> = self.removed_track_ids.iter().copied().collect();
        design.tracks.retain(|track| !removed.contains(&track.id));
        design.tracks.extend(self.route.tracks.iter().cloned());
        design.vias.extend(self.route.vias.iter().cloned());
        design.tracks.extend(self.rerouted.tracks.iter().cloned());
        design.vias.extend(self.rerouted.vias.iter().cloned());
    }
}

/// Route one net, removing a bounded deterministic set of conflicting tracks
/// and rerouting them around the new path when a direct route fails.
///
/// Candidates are existing tracks on other nets, ordered by id and limited by
/// [`RouterConfig::max_shoved_tracks`]. For each cumulative prefix of that
/// list, the function removes the prefix from a copy of the design, routes the
/// request, then tries to reroute every removed track around the new copper.
/// The first prefix for which every reroute succeeds is returned; nothing is
/// applied to `design` until [`PushAndShoveResult::apply_to`] is called.
///
/// A direct route is always preferred and returns empty removed/rerouted
/// results. Requests or configurations rejected by [`route_multilayer`] keep
/// their error, while an exhausted candidate list returns
/// [`RouteError::NoPath`].
pub fn route_push_and_shove(
    request: &RouteRequest,
    design: &PcbDesign,
    config: &RouterConfig,
) -> Result<PushAndShoveResult, RouteError> {
    match route_multilayer(request, design, config) {
        Ok(route) => {
            return Ok(PushAndShoveResult {
                route,
                removed_track_ids: Vec::new(),
                rerouted: RouteResult { tracks: Vec::new(), vias: Vec::new() },
            });
        }
        Err(RouteError::NoPath) => {}
        Err(error) => return Err(error),
    }

    let mut candidates: Vec<Track> = design
        .tracks
        .iter()
        .filter(|track| {
            track.net_id != request.net_id
                && track.path.len() >= 2
                && track.width.is_finite()
                && track.width > 0.0
                && design.copper_layer_ids().contains(&track.layer)
        })
        .cloned()
        .collect();
    candidates.sort_by_key(|track| track.id);
    candidates.truncate(config.max_shoved_tracks);

    'removed: for count in 1..=candidates.len() {
        let removed = &candidates[..count];
        let removed_ids: HashSet<Uuid> = removed.iter().map(|track| track.id).collect();
        let mut trial = design.clone();
        trial.tracks.retain(|track| !removed_ids.contains(&track.id));

        let Ok(route) = route_multilayer(request, &trial, config) else {
            continue;
        };
        trial.tracks.extend(route.tracks.iter().cloned());
        trial.vias.extend(route.vias.iter().cloned());

        let mut rerouted = RouteResult { tracks: Vec::new(), vias: Vec::new() };
        for track in removed {
            let replacement = RouteRequest {
                net_id: track.net_id,
                start: track.path[0],
                end: *track.path.last().unwrap(),
                layer: track.layer,
                width: track.width,
            };
            let Ok(replacement_route) = route_multilayer(&replacement, &trial, config) else {
                continue 'removed;
            };
            trial.tracks.extend(replacement_route.tracks.iter().cloned());
            trial.vias.extend(replacement_route.vias.iter().cloned());
            rerouted.tracks.extend(replacement_route.tracks);
            rerouted.vias.extend(replacement_route.vias);
        }

        return Ok(PushAndShoveResult {
            route,
            removed_track_ids: removed.iter().map(|track| track.id).collect(),
            rerouted,
        });
    }

    Err(RouteError::NoPath)
}

/// Stateful interactive routing session with a preview that can be committed
/// or cancelled.
///
/// Each call to [`preview`](Self::preview) replaces the previous preview and
/// leaves the design untouched. [`commit`](Self::commit) adds the preview's
/// tracks and vias to the design exactly once. The design must not change
/// between the final preview and the commit; start a new session after any
/// unrelated edit.
#[derive(Debug, Clone)]
pub struct InteractiveRouter {
    request: RouteRequest,
    config: RouterConfig,
    preview: Option<RouteResult>,
}

impl InteractiveRouter {
    /// Start a session at `start` on `layer`, routing `net_id` with `width`.
    pub fn new(
        net_id: Uuid,
        start: core::Point,
        layer: u16,
        width: f64,
        config: RouterConfig,
    ) -> Self {
        Self {
            request: RouteRequest { net_id, start, end: start, layer, width },
            config,
            preview: None,
        }
    }

    /// The request whose start, net, layer and width are held by this session.
    pub fn request(&self) -> &RouteRequest {
        &self.request
    }

    /// The current preview, if the last preview attempt succeeded.
    pub fn preview_result(&self) -> Option<&RouteResult> {
        self.preview.as_ref()
    }

    /// Route from the session start to `end`, replacing the preview.
    ///
    /// On failure the previous preview is cleared and the error is returned.
    pub fn preview(
        &mut self,
        design: &PcbDesign,
        end: core::Point,
    ) -> Result<&RouteResult, RouteError> {
        self.request.end = end;
        self.preview = None;
        let result = route_multilayer(&self.request, design, &self.config)?;
        self.preview = Some(result);
        Ok(self.preview.as_ref().expect("preview was just assigned"))
    }

    /// Add the current preview to `design` and end the preview.
    ///
    /// Returns [`RouteError::NoPath`] when there is no preview.
    pub fn commit(&mut self, design: &mut PcbDesign) -> Result<RouteResult, RouteError> {
        let result = self.preview.take().ok_or(RouteError::NoPath)?;
        design.tracks.extend(result.tracks.iter().cloned());
        design.vias.extend(result.vias.iter().cloned());
        Ok(result)
    }

    /// Discard the current preview without changing the design.
    pub fn cancel(&mut self) {
        self.preview = None;
    }
}

fn relax(
    open: &mut BinaryHeap<Reverse<QueueItem>>,
    best: &mut HashMap<State, f64>,
    came: &mut HashMap<State, State>,
    previous: State,
    state: State,
    g: f64,
    end_node: Node,
) {
    if best.get(&state).is_some_and(|existing| *existing <= g) {
        return;
    }
    best.insert(state, g);
    came.insert(state, previous);
    open.push(Reverse(QueueItem { f: g + heuristic(state.node, end_node), g, state }));
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
    if !config.via_cost.is_finite() || config.via_cost < 0.0 {
        return Err(invalid("via_cost must be finite and non-negative"));
    }
    if config.max_vias == 0 {
        return Err(invalid("max_vias must be greater than zero"));
    }
    if !config.via_diameter.is_finite() || config.via_diameter <= 0.0 {
        return Err(invalid("via_diameter must be finite and greater than zero"));
    }
    if !config.via_drill.is_finite() || config.via_drill <= 0.0 {
        return Err(invalid("via_drill must be finite and greater than zero"));
    }
    if config.via_drill >= config.via_diameter {
        return Err(invalid("via_drill must be smaller than via_diameter"));
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
    layer: u16,
    direction: u8,
    vias: u32,
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
            .then_with(|| self.state.layer.cmp(&other.state.layer))
            .then_with(|| self.state.vias.cmp(&other.state.vias))
    }
}

impl PartialOrd for QueueItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

struct SearchContext<'a> {
    design: &'a PcbDesign,
    width: f64,
    clearance: f64,
    net_id: Uuid,
    pads: &'a [crate::PadCopper],
    copper: &'a [u16],
}

impl SearchContext<'_> {
    fn point_blocked(&self, point: &core::Point, layer: u16) -> bool {
        let gap = self.clearance + self.width / 2.0;

        for track in &self.design.tracks {
            if track.layer != layer || !track.width.is_finite() || track.net_id == self.net_id {
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
            if !self.design.via_covers_layer(via, layer)
                || !via.diameter.is_finite()
                || via.net_id == Some(self.net_id)
            {
                continue;
            }
            if point.distance_to(&via.position) < gap + via.diameter / 2.0 {
                return true;
            }
        }

        for pad in self.pads {
            if !pad.has_valid_size()
                || !pad.layers.contains(&layer)
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

    fn segment_blocked(&self, start: &core::Point, end: &core::Point, layer: u16) -> bool {
        let gap = self.clearance + self.width / 2.0;

        for track in &self.design.tracks {
            if track.layer != layer || !track.width.is_finite() || track.net_id == self.net_id {
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
            if !self.design.via_covers_layer(via, layer)
                || !via.diameter.is_finite()
                || via.net_id == Some(self.net_id)
            {
                continue;
            }
            if via.position.distance_to_segment(start, end) < gap + via.diameter / 2.0 {
                return true;
            }
        }

        for pad in self.pads {
            if !pad.has_valid_size()
                || !pad.layers.contains(&layer)
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

    /// Whether a via centred on `point` and spanning `from` to `to` clears
    /// every layer it covers and satisfies the design's via limits.
    fn via_allowed(&self, point: &core::Point, config: &RouterConfig, from: u16, to: u16) -> bool {
        let (Some(from_index), Some(to_index)) = (
            self.copper.iter().position(|id| *id == from),
            self.copper.iter().position(|id| *id == to),
        ) else {
            return false;
        };
        let (low, high) =
            if from_index <= to_index { (from_index, to_index) } else { (to_index, from_index) };
        if low == high {
            return false;
        }

        let rules = &self.design.rules;
        let ring = (config.via_diameter - config.via_drill) / 2.0;
        if config.via_drill < rules.min_via_drill || ring < rules.min_annular_ring {
            return false;
        }

        let radius = config.via_diameter / 2.0;
        for layer in &self.copper[low..=high] {
            for track in &self.design.tracks {
                if track.layer != *layer || !track.width.is_finite() || track.net_id == self.net_id
                {
                    continue;
                }
                let required = self.clearance + radius + track.width / 2.0;
                if path_point_distance(&track.path, point) < required {
                    return false;
                }
            }

            for pad in self.pads {
                if !pad.has_valid_size()
                    || !pad.layers.contains(layer)
                    || pad.net_id == Some(self.net_id)
                {
                    continue;
                }
                if rect_distance_to_point(&pad.rect, point) < self.clearance + radius {
                    return false;
                }
            }

            for via in &self.design.vias {
                if !self.design.via_covers_layer(via, *layer)
                    || !via.diameter.is_finite()
                    || via.net_id == Some(self.net_id)
                {
                    continue;
                }
                if point.distance_to(&via.position) < self.clearance + radius + via.diameter / 2.0 {
                    return false;
                }
            }
        }
        true
    }
}

fn path_point_distance(path: &[core::Point], point: &core::Point) -> f64 {
    match path {
        [] => f64::INFINITY,
        [single] => single.distance_to(point),
        _ => path
            .windows(2)
            .map(|window| point.distance_to_segment(&window[0], &window[1]))
            .fold(f64::INFINITY, f64::min),
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

fn reconstruct(goal: State, came: &HashMap<State, State>) -> Vec<State> {
    let mut states = vec![goal];
    let mut current = goal;
    while let Some(previous) = came.get(&current).copied() {
        states.push(previous);
        current = previous;
    }
    states.reverse();
    states
}

#[derive(Debug)]
struct Run {
    layer: u16,
    nodes: Vec<Node>,
}

fn build_result(
    request: &RouteRequest,
    config: &RouterConfig,
    copper: &[u16],
    states: &[State],
) -> RouteResult {
    let mut runs: Vec<Run> = Vec::new();
    let mut transitions: Vec<(Node, u16, u16)> = Vec::new();
    let mut current = Run { layer: states[0].layer, nodes: vec![states[0].node] };

    for pair in states.windows(2) {
        let (current_state, next) = (pair[0], pair[1]);
        if current_state.layer != next.layer {
            if current.nodes.len() >= 2 {
                runs.push(current);
            }
            current = Run { layer: next.layer, nodes: vec![next.node] };
            transitions.push((current_state.node, current_state.layer, next.layer));
        } else if current.nodes.last() != Some(&next.node) {
            current.nodes.push(next.node);
        }
    }
    if current.nodes.len() >= 2 {
        runs.push(current);
    }

    let track_count = runs.len();
    let tracks = runs
        .into_iter()
        .enumerate()
        .filter_map(|(index, run)| {
            let simplified = simplify(run.nodes);
            if simplified.len() < 2 {
                return None;
            }
            let path: Vec<core::Point> =
                simplified.iter().map(|node| node_point(*node, config.grid)).collect();
            let id = if track_count == 1 {
                route_id(request, config, simplified[0], *simplified.last().unwrap())
            } else {
                track_segment_id(request, config, run.layer, &simplified, index)
            };
            Some(Track { id, net_id: request.net_id, layer: run.layer, width: request.width, path })
        })
        .collect();

    let (through_start, through_end) = (
        copper.first().copied().unwrap_or(request.layer),
        copper.last().copied().unwrap_or(request.layer),
    );
    let vias = transitions
        .into_iter()
        .enumerate()
        .map(|(index, (node, from, to))| {
            let (layer_start, layer_end) =
                if config.allow_blind_buried { (from, to) } else { (through_start, through_end) };
            Via {
                id: via_id(request, config, node, from, to, index),
                net_id: Some(request.net_id),
                position: node_point(node, config.grid),
                layer_start,
                layer_end,
                diameter: config.via_diameter,
                drill: config.via_drill,
            }
        })
        .collect();

    RouteResult { tracks, vias }
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
        if simplified.last() != Some(&node) {
            simplified.push(node);
        }
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

fn track_segment_id(
    request: &RouteRequest,
    config: &RouterConfig,
    layer: u16,
    nodes: &[Node],
    index: usize,
) -> Uuid {
    let start = nodes[0];
    let end = *nodes.last().unwrap();
    let name = format!(
        "route:{}:{}:{}:{}:multi:{}:{}:{}:{}:{}:{}",
        request.net_id,
        request.layer,
        request.width.to_bits(),
        config.grid.to_bits(),
        index,
        layer,
        start.x,
        start.y,
        end.x,
        end.y
    );
    Uuid::new_v5(&Uuid::NAMESPACE_OID, name.as_bytes())
}

fn via_id(
    request: &RouteRequest,
    config: &RouterConfig,
    node: Node,
    from: u16,
    to: u16,
    index: usize,
) -> Uuid {
    let name = format!(
        "via:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}",
        request.net_id,
        request.width.to_bits(),
        config.grid.to_bits(),
        config.via_diameter.to_bits(),
        config.via_drill.to_bits(),
        node.x,
        node.y,
        from,
        to,
        index
    );
    Uuid::new_v5(&Uuid::NAMESPACE_OID, name.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::{
        InteractiveRouter, RouteError, RouteRequest, RouteResult, Router, RouterConfig, route,
        route_multilayer, route_push_and_shove,
    };
    use crate::{PcbDesign, Track, Via, run_drc};
    use core::{LayerKind, PcbLayer, PcbOutline, Point, Stackup};
    use std::collections::HashSet;
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

    fn four_layer_design() -> PcbDesign {
        PcbDesign {
            outline: PcbOutline::rectangle(20.0, 20.0),
            layers: vec![
                PcbLayer::new(0, "F.Cu", LayerKind::Copper),
                PcbLayer::new(1, "In1.Cu", LayerKind::Copper),
                PcbLayer::new(2, "In2.Cu", LayerKind::Copper),
                PcbLayer::new(3, "B.Cu", LayerKind::Copper),
            ],
            stackup: Stackup { layers: vec![0, 1, 2, 3] },
            ..PcbDesign::default()
        }
    }

    fn request(start: Point, end: Point) -> RouteRequest {
        RouteRequest { net_id: Uuid::from_u128(7), start, end, layer: 0, width: 0.25 }
    }

    fn config() -> RouterConfig {
        RouterConfig {
            grid: 1.0,
            bend_cost: 1.0,
            max_expansions: 100_000,
            ..RouterConfig::default()
        }
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

    fn wall_on(design: &mut PcbDesign, layer: u16, start: Point, end: Point) {
        design.tracks.push(Track {
            id: Uuid::new_v4(),
            net_id: Uuid::new_v4(),
            layer,
            width: 0.25,
            path: vec![start, end],
        });
    }

    fn crossing_design() -> PcbDesign {
        let mut design = design();
        wall(&mut design, Point::new(0.0, 10.0), Point::new(20.0, 10.0));
        design
    }

    fn assert_drc_clean(design: &PcbDesign, result: &RouteResult) {
        let mut routed = design.clone();
        routed.tracks.extend(result.tracks.clone());
        routed.vias.extend(result.vias.clone());
        let report = run_drc(&routed, &routed.rules);
        assert!(report.is_valid(), "route produced DRC errors: {:?}", report.violations);
        assert_eq!(report.violations, Vec::new());
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
        assert!(router.config.allow_vias);
        assert_eq!(router.config.via_cost, 5.0);
        assert_eq!(router.config.max_vias, 64);
        assert_eq!(router.config.via_diameter, 0.6);
        assert_eq!(router.config.via_drill, 0.3);
        assert!(!router.config.allow_blind_buried);
        assert_eq!(router.config.max_shoved_tracks, 8);

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
    fn route_can_start_on_existing_same_net_track() {
        let mut design = design();
        design.tracks.push(Track {
            id: Uuid::new_v4(),
            net_id: Uuid::from_u128(7),
            layer: 0,
            width: 0.25,
            path: vec![Point::new(5.0, 5.0), Point::new(10.0, 5.0)],
        });

        let tracks = routed_tracks(
            &request(Point::new(6.0, 5.0), Point::new(6.0, 10.0)),
            &design,
            &config(),
        );
        assert_eq!(tracks[0].path.first(), Some(&Point::new(6.0, 5.0)));
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
            net_id: None,
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
                number: "1".to_string(),
                x: 0.0,
                y: 0.0,
                pad_type: crate::PadType::SurfaceMount,
                width: 2.0,
                height: 2.0,
                rotation: 0.0,
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

    #[test]
    fn multilayer_config_defaults_and_legacy_json() {
        let config = RouterConfig::default();
        assert!(config.allow_vias);
        assert_eq!(config.via_cost, 5.0);
        assert_eq!(config.max_vias, 64);
        assert_eq!(config.via_diameter, 0.6);
        assert_eq!(config.via_drill, 0.3);
        assert!(!config.allow_blind_buried);
        assert_eq!(config.max_shoved_tracks, 8);

        let json = serde_json::to_string(&config).expect("config serializes");
        let restored: RouterConfig = serde_json::from_str(&json).expect("config parses");
        assert_eq!(restored, config);

        let legacy: RouterConfig =
            serde_json::from_str(r#"{"grid":0.25,"bend_cost":2.0,"max_expansions":1000}"#)
                .expect("legacy config loads");
        assert_eq!(legacy.grid, 0.25);
        assert!(legacy.allow_vias);
        assert_eq!(legacy.via_cost, 5.0);
        assert!(!legacy.allow_blind_buried);
        assert_eq!(legacy.max_shoved_tracks, 8);
    }

    #[test]
    fn multilayer_config_rejects_invalid_via_settings() {
        let design = design();
        let request = request(Point::new(1.0, 1.0), Point::new(5.0, 5.0));
        let cases = [
            RouterConfig { via_cost: -1.0, ..config() },
            RouterConfig { via_cost: f64::NAN, ..config() },
            RouterConfig { max_vias: 0, ..config() },
            RouterConfig { via_diameter: 0.0, ..config() },
            RouterConfig { via_diameter: f64::NAN, ..config() },
            RouterConfig { via_drill: 0.0, ..config() },
            RouterConfig { via_drill: f64::NAN, ..config() },
            RouterConfig { via_drill: 0.6, ..config() },
        ];

        for config in cases {
            assert!(matches!(
                route_multilayer(&request, &design, &config),
                Err(RouteError::InvalidRequest(_))
            ));
        }
    }

    #[test]
    fn multilayer_crosses_a_full_width_top_wall() {
        let design = crossing_design();
        let request = request(Point::new(5.0, 5.0), Point::new(15.0, 15.0));
        let result = route_multilayer(&request, &design, &config())
            .expect("multi-layer route should succeed");

        assert!(!result.vias.is_empty(), "expected at least one via");
        assert!(result.tracks.len() >= 2, "expected at least two layer runs");
        assert!(result.vias.iter().any(|via| via.position.y < 10.0));
        assert!(result.vias.iter().any(|via| via.position.y > 10.0));
        assert!(result.tracks.iter().any(|track| track.layer == 1));
        assert!(result.tracks.iter().all(|track| track.net_id == request.net_id));
        assert!(result.tracks.iter().all(|track| track.width == request.width));
        assert_drc_clean(&design, &result);
    }

    #[test]
    fn multilayer_disabled_returns_no_path_for_the_wall() {
        let design = crossing_design();
        let request = request(Point::new(5.0, 5.0), Point::new(15.0, 15.0));
        let config = RouterConfig { allow_vias: false, ..config() };

        assert!(matches!(route_multilayer(&request, &design, &config), Err(RouteError::NoPath)));
        assert!(matches!(route(&request, &design, &config), Err(RouteError::NoPath)));
    }

    #[test]
    fn multilayer_vias_match_config_and_span_stackup() {
        let design = crossing_design();
        let request = request(Point::new(5.0, 5.0), Point::new(15.0, 15.0));
        let config = config();
        let result =
            route_multilayer(&request, &design, &config).expect("multi-layer route should succeed");

        for via in &result.vias {
            assert_eq!(via.layer_start, 0);
            assert_eq!(via.layer_end, 1);
            assert_eq!(via.diameter, config.via_diameter);
            assert_eq!(via.drill, config.via_drill);
            assert_eq!(via.net_id, Some(request.net_id));
            assert!((via.position.x.fract()).abs() < 1e-9);
            assert!((via.position.y.fract()).abs() < 1e-9);
        }
    }

    #[test]
    fn multilayer_tracks_are_split_and_simplified_per_layer() {
        let design = crossing_design();
        let request = request(Point::new(5.0, 5.0), Point::new(15.0, 15.0));
        let result = route_multilayer(&request, &design, &config())
            .expect("multi-layer route should succeed");

        let mut ids = HashSet::new();
        for track in &result.tracks {
            assert!(track.path.len() >= 2, "track has fewer than two points");
            assert!(track.path.windows(2).all(|pair| pair[0] != pair[1]), "zero-length run");
            assert!(matches!(track.layer, 0 | 1), "track is on a non-copper layer");
            assert!(ids.insert(track.id), "duplicate track id");
        }
        for via in &result.vias {
            assert!(
                result.tracks.iter().any(|track| {
                    track.path.first() == Some(&via.position)
                        || track.path.last() == Some(&via.position)
                }),
                "via is not connected to a track endpoint"
            );
        }
    }

    #[test]
    fn multilayer_vias_avoid_bottom_layer_copper() {
        let mut design = crossing_design();
        wall_on(&mut design, 1, Point::new(0.0, 10.0), Point::new(9.0, 10.0));
        wall_on(&mut design, 1, Point::new(11.0, 10.0), Point::new(20.0, 10.0));

        let request = request(Point::new(5.0, 5.0), Point::new(15.0, 15.0));
        let result =
            route_multilayer(&request, &design, &config()).expect("gap should allow a route");

        assert!(!result.vias.is_empty());
        assert_drc_clean(&design, &result);
    }

    #[test]
    fn multilayer_max_vias_is_enforced() {
        let design = crossing_design();
        let request = request(Point::new(5.0, 5.0), Point::new(15.0, 15.0));

        let one = RouterConfig { max_vias: 1, ..config() };
        assert!(matches!(route_multilayer(&request, &design, &one), Err(RouteError::NoPath)));

        let two = RouterConfig { max_vias: 2, ..config() };
        let result = route_multilayer(&request, &design, &two).expect("two vias should be enough");
        assert!(result.vias.len() <= 2);
    }

    #[test]
    fn multilayer_rejects_vias_that_violate_drc_limits() {
        let design = crossing_design();
        let request = request(Point::new(5.0, 5.0), Point::new(15.0, 15.0));

        let drill_too_small = RouterConfig { via_drill: 0.1, ..config() };
        assert!(matches!(
            route_multilayer(&request, &design, &drill_too_small),
            Err(RouteError::NoPath)
        ));

        let thin_ring = RouterConfig { via_diameter: 0.25, via_drill: 0.2, ..config() };
        assert!(matches!(route_multilayer(&request, &design, &thin_ring), Err(RouteError::NoPath)));
    }

    #[test]
    fn multilayer_is_deterministic() {
        let design = crossing_design();
        let request = request(Point::new(5.0, 5.0), Point::new(15.0, 15.0));

        let first = route_multilayer(&request, &design, &config()).expect("first route");
        let second = route_multilayer(&request, &design, &config()).expect("second route");

        assert_eq!(
            serde_json::to_string(&first).expect("serialize"),
            serde_json::to_string(&second).expect("serialize")
        );
        assert_eq!(
            first.vias.iter().map(|via| via.id).collect::<Vec<_>>(),
            second.vias.iter().map(|via| via.id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn multilayer_does_not_mutate_the_design() {
        let design = crossing_design();
        let before = serde_json::to_string(&design).expect("design serializes");
        let request = request(Point::new(5.0, 5.0), Point::new(15.0, 15.0));

        let _ = route_multilayer(&request, &design, &config()).expect("route");

        assert_eq!(before, serde_json::to_string(&design).expect("design serializes"));
    }

    #[test]
    fn multilayer_result_is_drc_clean() {
        let design = crossing_design();
        let request = request(Point::new(5.0, 5.0), Point::new(15.0, 15.0));
        let result = route_multilayer(&request, &design, &config()).expect("route");

        let mut routed = design;
        routed.tracks.extend(result.tracks);
        routed.vias.extend(result.vias);
        let report = run_drc(&routed, &routed.rules);

        assert!(report.is_valid(), "route produced DRC errors: {:?}", report.violations);
        assert_eq!(report.count(crate::DrcSeverity::Error), 0);
    }

    #[test]
    fn multilayer_uses_inner_layer_on_a_four_layer_board() {
        let mut design = four_layer_design();
        wall_on(&mut design, 0, Point::new(0.0, 10.0), Point::new(20.0, 10.0));
        wall_on(&mut design, 1, Point::new(0.0, 10.0), Point::new(20.0, 10.0));
        wall_on(&mut design, 3, Point::new(0.0, 10.0), Point::new(20.0, 10.0));

        let request = request(Point::new(5.0, 5.0), Point::new(15.0, 15.0));
        let result =
            route_multilayer(&request, &design, &config()).expect("inner layer should be usable");

        assert!(result.tracks.iter().any(|track| track.layer == 2));
        assert!(result.vias.iter().all(|via| via.layer_start == 0 && via.layer_end == 3));
        assert_drc_clean(&design, &result);
    }

    #[test]
    fn multilayer_uses_blind_vias_when_enabled() {
        let mut design = four_layer_design();
        wall_on(&mut design, 0, Point::new(0.0, 10.0), Point::new(20.0, 10.0));
        wall_on(&mut design, 1, Point::new(0.0, 10.0), Point::new(20.0, 10.0));
        wall_on(&mut design, 3, Point::new(0.0, 10.0), Point::new(20.0, 10.0));

        let request = request(Point::new(5.0, 5.0), Point::new(15.0, 15.0));
        let blind = RouterConfig { allow_blind_buried: true, ..config() };
        let result = route_multilayer(&request, &design, &blind)
            .expect("inner layer should be reachable with a blind via");

        assert!(result.tracks.iter().any(|track| track.layer == 2));
        assert!(result.vias.iter().any(|via| via.layer_start == 0 && via.layer_end == 2));
        assert!(
            result.vias.iter().all(|via| !(via.layer_start == 0 && via.layer_end == 3)),
            "blind routing should not fall back to a through via"
        );
        assert_drc_clean(&design, &result);
    }

    #[test]
    fn push_and_shove_rips_up_and_reroutes_a_blocking_track() {
        let design = crossing_design();
        let request = request(Point::new(5.0, 5.0), Point::new(15.0, 15.0));
        let no_vias = RouterConfig { allow_vias: false, ..config() };
        assert!(matches!(route_multilayer(&request, &design, &no_vias), Err(RouteError::NoPath)));

        let shoved =
            route_push_and_shove(&request, &design, &no_vias).expect("shove should clear the wall");
        assert_eq!(shoved.removed_track_ids.len(), 1);
        assert!(!shoved.route.tracks.is_empty());
        assert!(!shoved.rerouted.tracks.is_empty());

        let mut routed = design;
        shoved.apply_to(&mut routed);
        assert_eq!(routed.tracks.len(), 2, "wall should be replaced by one track");
        let report = run_drc(&routed, &routed.rules);
        assert!(report.is_valid(), "shove produced DRC errors: {:?}", report.violations);
    }

    #[test]
    fn push_and_shove_prefers_a_direct_route() {
        let design = design();
        let request = request(Point::new(2.0, 2.0), Point::new(17.0, 17.0));
        let result = route_push_and_shove(&request, &design, &config()).expect("direct route");

        assert!(result.removed_track_ids.is_empty());
        assert!(result.rerouted.tracks.is_empty());
        assert!(result.rerouted.vias.is_empty());
    }

    #[test]
    fn interactive_router_previews_commits_and_cancels() {
        let design = crossing_design();
        let before = serde_json::to_string(&design).expect("design serializes");
        let net = Uuid::from_u128(7);
        let mut router = InteractiveRouter::new(net, Point::new(5.0, 5.0), 0, 0.25, config());

        assert!(router.preview_result().is_none());
        assert_eq!(router.request().end, Point::new(5.0, 5.0));
        let preview = router.preview(&design, Point::new(15.0, 15.0)).expect("preview");
        assert!(!preview.tracks.is_empty());
        assert_eq!(before, serde_json::to_string(&design).expect("design serializes"));

        router.cancel();
        assert!(router.preview_result().is_none());
        assert_eq!(router.request().end, Point::new(15.0, 15.0));
        assert!(matches!(router.commit(&mut PcbDesign::default()), Err(RouteError::NoPath)));

        router.preview(&design, Point::new(14.0, 16.0)).expect("second preview");
        let mut routed = design;
        let committed = router.commit(&mut routed).expect("commit");
        assert!(!committed.tracks.is_empty());
        assert!(router.preview_result().is_none());
        assert!(!routed.tracks.is_empty());
        let report = run_drc(&routed, &routed.rules);
        assert!(report.is_valid(), "commit produced DRC errors: {:?}", report.violations);
    }

    #[test]
    fn single_layer_route_matches_multilayer_without_vias() {
        let design = design();
        let request = request(Point::new(2.0, 2.0), Point::new(17.0, 17.0));
        let config = config();

        let single = route(&request, &design, &config).expect("single-layer route");
        let disabled = RouterConfig { allow_vias: false, ..config };
        let multi = route_multilayer(&request, &design, &disabled).expect("no-via route");

        assert!(multi.vias.is_empty());
        assert_eq!(
            serde_json::to_string(&single).expect("serialize"),
            serde_json::to_string(&multi.tracks).expect("serialize")
        );

        let with_vias = route_multilayer(&request, &design, &config).expect("no-via-needed route");
        assert!(with_vias.vias.is_empty());
        assert_eq!(
            serde_json::to_string(&single).expect("serialize"),
            serde_json::to_string(&with_vias.tracks).expect("serialize")
        );
    }

    #[test]
    fn multilayer_reports_invalid_requests() {
        let design = design();
        let config = config();

        let cases = [
            RouteRequest { layer: 2, ..request(Point::new(1.0, 1.0), Point::new(5.0, 5.0)) },
            RouteRequest { width: 0.0, ..request(Point::new(1.0, 1.0), Point::new(5.0, 5.0)) },
            request(Point::new(1.0, 1.0), Point::new(30.0, 5.0)),
        ];
        for case in cases {
            assert!(matches!(
                route_multilayer(&case, &design, &config),
                Err(RouteError::InvalidRequest(_))
            ));
        }

        let bad_grid = RouterConfig { grid: -0.5, ..config };
        assert!(matches!(
            route_multilayer(
                &request(Point::new(1.0, 1.0), Point::new(5.0, 5.0)),
                &design,
                &bad_grid
            ),
            Err(RouteError::InvalidRequest(_))
        ));
    }

    #[test]
    fn multilayer_via_ids_are_unique() {
        let design = crossing_design();
        let request = request(Point::new(5.0, 5.0), Point::new(15.0, 15.0));
        let result = route_multilayer(&request, &design, &config()).expect("route");

        let mut ids = HashSet::new();
        for via in &result.vias {
            assert!(!via.id.is_nil());
            assert!(ids.insert(via.id), "duplicate via id");
        }
    }
}
