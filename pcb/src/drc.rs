//! Deterministic design-rule checking for PCB designs.
//!
//! Checks are pure functions of a [`PcbDesign`] and [`DrcRules`]: the same
//! input always produces the same report, byte for byte, because violations
//! are sorted by rule id and object ids. Non-finite geometry is reported by
//! the rules rather than silently skipped, and no rule panics on degenerate
//! input. Limits live on the design and are validated before use; an invalid
//! set is reported as a `drc-rules` error and the defaults are applied.

use std::cmp::Ordering;

use uuid::Uuid;

use crate::{
    CopperRect, PadCopper, PcbDesign, Track, Via, rect_distance_to_point, rect_distance_to_rect,
    rect_distance_to_segment,
};

/// Minimum copper dimensions enforced by the built-in rules.
///
/// Limits are stored on [`PcbDesign::rules`](crate::PcbDesign::rules) and are
/// serialized with the design. Every limit must be finite and greater than
/// zero; [`DrcRules::validate`] checks this, and [`run_drc`] reports an invalid
/// set as a `drc-rules` error and falls back to the defaults.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DrcRules {
    /// Minimum acceptable copper width in millimetres.
    #[serde(default = "default_min_track_width")]
    pub min_track_width: f64,
    /// Minimum copper-to-copper gap in millimetres.
    #[serde(default = "default_clearance")]
    pub clearance: f64,
    /// Minimum annular ring around a via in millimetres.
    #[serde(default = "default_min_annular_ring")]
    pub min_annular_ring: f64,
    /// Minimum finished via drill diameter in millimetres.
    #[serde(default = "default_min_via_drill")]
    pub min_via_drill: f64,
}

fn default_min_track_width() -> f64 {
    0.15
}

fn default_clearance() -> f64 {
    0.2
}

fn default_min_annular_ring() -> f64 {
    0.05
}

fn default_min_via_drill() -> f64 {
    0.2
}

impl Default for DrcRules {
    fn default() -> Self {
        Self {
            min_track_width: default_min_track_width(),
            clearance: default_clearance(),
            min_annular_ring: default_min_annular_ring(),
            min_via_drill: default_min_via_drill(),
        }
    }
}

impl DrcRules {
    /// Check that every limit is finite and greater than zero.
    pub fn validate(&self) -> Result<(), DrcRulesError> {
        for (field, value) in [
            ("min_track_width", self.min_track_width),
            ("clearance", self.clearance),
            ("min_annular_ring", self.min_annular_ring),
            ("min_via_drill", self.min_via_drill),
        ] {
            if !value.is_finite() || value <= 0.0 {
                return Err(DrcRulesError { field });
            }
        }
        Ok(())
    }
}

/// A [`DrcRules`] limit that is not finite and greater than zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrcRulesError {
    /// Name of the offending limit, for example `clearance`.
    pub field: &'static str,
}

impl std::fmt::Display for DrcRulesError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "DRC limit `{}` must be finite and greater than zero", self.field)
    }
}

impl std::error::Error for DrcRulesError {}

/// How serious a [`DrcViolation`] is.
///
/// `Warning` sorts before `Error`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DrcSeverity {
    /// Something is close to a limit but the design may still be usable.
    Warning,
    /// The design is invalid until the problem is fixed.
    Error,
}

/// One finding from a design-rule check.
#[derive(Debug, Clone, PartialEq)]
pub struct DrcViolation {
    /// Stable rule identifier, for example `clearance`.
    pub rule_id: String,
    pub severity: DrcSeverity,
    /// Human-readable description of the problem.
    pub message: String,
    /// Ids of the objects involved, in a deterministic order.
    pub objects: Vec<Uuid>,
    /// Board location most representative of the problem, when there is one.
    pub location: Option<core::Point>,
}

/// The result of running every built-in rule.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DrcReport {
    pub violations: Vec<DrcViolation>,
}

impl DrcReport {
    /// True when there are no [`DrcSeverity::Error`] violations.
    pub fn is_valid(&self) -> bool {
        !self.violations.iter().any(|violation| violation.severity == DrcSeverity::Error)
    }

    /// Count of violations with the given severity.
    pub fn count(&self, severity: DrcSeverity) -> usize {
        self.violations.iter().filter(|violation| violation.severity == severity).count()
    }
}

/// Run every built-in rule on `design`.
///
/// The report contains at most one violation per offending object or pair.
/// Track clearance is measured edge to edge on the same copper layer between
/// tracks on different nets, and every unordered pair is reported once. An
/// outline with fewer than three points disables the board-containment rules,
/// and an empty stackup means layer order is unknown, so via spans only need
/// distinct, existing copper layers. An invalid [`DrcRules`] set adds one
/// `drc-rules` error and the checks run with [`DrcRules::default`].
pub fn run_drc(design: &PcbDesign, rules: &DrcRules) -> DrcReport {
    let mut violations = Vec::new();
    let active_rules = match rules.validate() {
        Ok(()) => rules.clone(),
        Err(error) => {
            push(
                &mut violations,
                "drc-rules",
                DrcSeverity::Error,
                error.to_string(),
                Vec::new(),
                None,
            );
            DrcRules::default()
        }
    };
    let rules = &active_rules;

    for track in &design.tracks {
        check_track_width(track, rules, &mut violations);
        check_track_layer(design, track, &mut violations);
        check_track_path(track, &mut violations);
        check_track_outside(design, track, &mut violations);
    }

    for via in &design.vias {
        check_via_outside(design, via, &mut violations);
        check_via_span(design, via, &mut violations);
        check_via_drill(via, rules, &mut violations);
    }

    let pads = design.pad_copper();
    for pad in &pads {
        check_pad_size(pad, &mut violations);
        check_pad_rotation(pad, &mut violations);
        check_pad_outside(design, pad, &mut violations);
    }

    check_clearance(design, rules, &pads, &mut violations);

    violations.sort_by(|left, right| {
        left.rule_id
            .cmp(&right.rule_id)
            .then_with(|| compare_objects(&left.objects, &right.objects))
    });

    DrcReport { violations }
}

/// Whether `layer` is defined and made of copper.
fn is_copper(design: &PcbDesign, layer: u16) -> bool {
    design.layers.iter().any(|candidate| candidate.id == layer && candidate.is_copper())
}

/// Compare object id lists by their string form.
fn compare_objects(left: &[Uuid], right: &[Uuid]) -> Ordering {
    left.iter().map(Uuid::to_string).cmp(right.iter().map(Uuid::to_string))
}

/// Represent a finite point, or `None` when it cannot be placed on the board.
fn finite_location(point: &core::Point) -> Option<core::Point> {
    if point.x.is_finite() && point.y.is_finite() { Some(*point) } else { None }
}

fn push(
    violations: &mut Vec<DrcViolation>,
    rule_id: &str,
    severity: DrcSeverity,
    message: String,
    objects: Vec<Uuid>,
    location: Option<core::Point>,
) {
    violations.push(DrcViolation {
        rule_id: rule_id.to_string(),
        severity,
        message,
        objects,
        location,
    });
}

fn check_track_width(track: &Track, rules: &DrcRules, violations: &mut Vec<DrcViolation>) {
    let location = track.path.first().and_then(finite_location);

    if !track.width.is_finite() {
        push(
            violations,
            "track-width",
            DrcSeverity::Error,
            format!("track {} has non-finite width {}", track.id, track.width),
            vec![track.id],
            location,
        );
    } else if track.width <= 0.0 {
        push(
            violations,
            "track-width",
            DrcSeverity::Error,
            format!("track {} has non-positive width {} mm", track.id, track.width),
            vec![track.id],
            location,
        );
    } else if track.width < rules.min_track_width {
        push(
            violations,
            "track-width",
            DrcSeverity::Warning,
            format!(
                "track {} width {} mm is below the minimum {} mm",
                track.id, track.width, rules.min_track_width
            ),
            vec![track.id],
            location,
        );
    }
}

fn check_track_layer(design: &PcbDesign, track: &Track, violations: &mut Vec<DrcViolation>) {
    let location = track.path.first().and_then(finite_location);
    let message = match design.layers.iter().find(|layer| layer.id == track.layer) {
        None => format!("track {} uses layer {} which does not exist", track.id, track.layer),
        Some(layer) if !layer.is_copper() => {
            format!("track {} uses non-copper layer {} ({:?})", track.id, layer.id, layer.name)
        }
        Some(_) => return,
    };

    push(violations, "track-layer", DrcSeverity::Error, message, vec![track.id], location);
}

fn check_track_path(track: &Track, violations: &mut Vec<DrcViolation>) {
    if track.path.len() < 2 {
        push(
            violations,
            "track-path",
            DrcSeverity::Error,
            format!("track {} has fewer than two path points", track.id),
            vec![track.id],
            track.path.first().and_then(finite_location),
        );
    } else if track.path.iter().any(|point| !point.x.is_finite() || !point.y.is_finite()) {
        push(
            violations,
            "track-path",
            DrcSeverity::Error,
            format!("track {} has a non-finite path coordinate", track.id),
            vec![track.id],
            None,
        );
    }
}

fn check_track_outside(design: &PcbDesign, track: &Track, violations: &mut Vec<DrcViolation>) {
    if design.outline.points.len() < 3 {
        return;
    }

    if let Some(point) = track.path.iter().find(|point| !design.outline.contains_point(point)) {
        push(
            violations,
            "track-outside-board",
            DrcSeverity::Error,
            format!("track {} leaves the board outline", track.id),
            vec![track.id],
            finite_location(point),
        );
    }
}

fn check_via_outside(design: &PcbDesign, via: &Via, violations: &mut Vec<DrcViolation>) {
    if design.outline.points.len() < 3 {
        return;
    }

    if !design.outline.contains_point(&via.position) {
        push(
            violations,
            "via-outside-board",
            DrcSeverity::Error,
            format!("via {} lies outside the board outline", via.id),
            vec![via.id],
            finite_location(&via.position),
        );
    }
}

fn check_via_span(design: &PcbDesign, via: &Via, violations: &mut Vec<DrcViolation>) {
    let location = finite_location(&via.position);

    if via.layer_start == via.layer_end {
        push(
            violations,
            "via-span",
            DrcSeverity::Error,
            format!(
                "via {} starts and ends on layer {}, so it connects nothing",
                via.id, via.layer_start
            ),
            vec![via.id],
            location,
        );
        return;
    }

    if !is_copper(design, via.layer_start) || !is_copper(design, via.layer_end) {
        push(
            violations,
            "via-span",
            DrcSeverity::Error,
            format!(
                "via {} connects layers {} and {}, which are missing or not copper",
                via.id, via.layer_start, via.layer_end
            ),
            vec![via.id],
            location,
        );
    }
}

fn check_pad_rotation(pad: &PadCopper, violations: &mut Vec<DrcViolation>) {
    if pad.has_valid_rotation() {
        return;
    }

    push(
        violations,
        "pad-rotation",
        DrcSeverity::Error,
        format!("pad {} has non-finite rotation {}", pad.id, pad.rotation),
        vec![pad.id],
        finite_location(&pad.center),
    );
}

fn check_via_drill(via: &Via, rules: &DrcRules, violations: &mut Vec<DrcViolation>) {
    let location = finite_location(&via.position);

    if !via.diameter.is_finite() || !via.drill.is_finite() {
        push(
            violations,
            "via-drill",
            DrcSeverity::Error,
            format!(
                "via {} has a non-finite drill {} or diameter {}",
                via.id, via.drill, via.diameter
            ),
            vec![via.id],
            location,
        );
    } else if via.diameter <= 0.0 {
        push(
            violations,
            "via-drill",
            DrcSeverity::Error,
            format!("via {} has non-positive diameter {} mm", via.id, via.diameter),
            vec![via.id],
            location,
        );
    } else if via.drill <= 0.0 {
        push(
            violations,
            "via-drill",
            DrcSeverity::Error,
            format!("via {} has non-positive drill {} mm", via.id, via.drill),
            vec![via.id],
            location,
        );
    } else if via.drill >= via.diameter {
        push(
            violations,
            "via-drill",
            DrcSeverity::Error,
            format!(
                "via {} drill {} mm is not smaller than its diameter {} mm",
                via.id, via.drill, via.diameter
            ),
            vec![via.id],
            location,
        );
    } else if via.drill < rules.min_via_drill {
        push(
            violations,
            "via-drill",
            DrcSeverity::Error,
            format!(
                "via {} drill {} mm is below the minimum {} mm",
                via.id, via.drill, rules.min_via_drill
            ),
            vec![via.id],
            location,
        );
    } else {
        let ring = (via.diameter - via.drill) / 2.0;
        if ring < rules.min_annular_ring {
            push(
                violations,
                "via-drill",
                DrcSeverity::Warning,
                format!(
                    "via {} annular ring {:.3} mm is below {:.3} mm",
                    via.id, ring, rules.min_annular_ring
                ),
                vec![via.id],
                location,
            );
        }
    }
}

fn check_pad_size(pad: &PadCopper, violations: &mut Vec<DrcViolation>) {
    if pad.has_valid_size() {
        return;
    }

    push(
        violations,
        "pad-size",
        DrcSeverity::Error,
        format!("pad {} has invalid size {} x {} mm", pad.id, pad.width, pad.height),
        vec![pad.id],
        finite_location(&pad.center),
    );
}

fn check_pad_outside(design: &PcbDesign, pad: &PadCopper, violations: &mut Vec<DrcViolation>) {
    if design.outline.points.len() < 3 || !pad.has_valid_size() || !pad.has_valid_rotation() {
        return;
    }

    if pad.rect.corners().iter().any(|corner| !design.outline.contains_point(corner)) {
        push(
            violations,
            "pad-outside-board",
            DrcSeverity::Error,
            format!("pad {} extends outside the board outline", pad.id),
            vec![pad.id],
            finite_location(&pad.center),
        );
    }
}

fn check_clearance(
    design: &PcbDesign,
    rules: &DrcRules,
    pads: &[PadCopper],
    violations: &mut Vec<DrcViolation>,
) {
    for left in 0..design.tracks.len() {
        for right in left + 1..design.tracks.len() {
            let first = &design.tracks[left];
            let second = &design.tracks[right];

            if first.layer != second.layer || !is_copper(design, first.layer) {
                continue;
            }

            if first.net_id == second.net_id {
                continue;
            }

            if !first.width.is_finite() || !second.width.is_finite() {
                continue;
            }

            let distance = path_distance(&first.path, &second.path);
            if !distance.is_finite() {
                continue;
            }

            let gap = distance - first.width / 2.0 - second.width / 2.0;
            if gap >= rules.clearance {
                continue;
            }

            let objects = pair_objects(first.id, second.id);
            push(
                violations,
                "clearance",
                DrcSeverity::Error,
                format!(
                    "tracks {} and {} on layer {} are {:.3} mm apart, below the {} mm clearance",
                    objects[0], objects[1], first.layer, gap, rules.clearance
                ),
                objects,
                None,
            );
        }
    }

    for track in &design.tracks {
        if !is_copper(design, track.layer) || !track.width.is_finite() {
            continue;
        }

        for via in &design.vias {
            if !design.via_covers_layer(via, track.layer)
                || !via.diameter.is_finite()
                || via.net_id == Some(track.net_id)
            {
                continue;
            }

            let distance = path_point_distance(&track.path, &via.position);
            if !distance.is_finite() {
                continue;
            }

            let gap = distance - track.width / 2.0 - via.diameter / 2.0;
            if gap >= rules.clearance {
                continue;
            }

            let objects = pair_objects(track.id, via.id);
            push(
                violations,
                "clearance",
                DrcSeverity::Error,
                format!(
                    "track {} and via {} on layer {} are {:.3} mm apart, below the {} mm clearance",
                    objects[0], objects[1], track.layer, gap, rules.clearance
                ),
                objects,
                None,
            );
        }
    }

    for pad in pads {
        if !pad.has_valid_size() || pad.layers.is_empty() {
            continue;
        }

        for track in &design.tracks {
            if !pad.layers.contains(&track.layer)
                || !track.width.is_finite()
                || pad.net_id == Some(track.net_id)
            {
                continue;
            }

            let distance = rect_distance_to_track(&pad.rect, &track.path);
            if !distance.is_finite() {
                continue;
            }

            let gap = distance - track.width / 2.0;
            if gap >= rules.clearance {
                continue;
            }

            let objects = pair_objects(pad.id, track.id);
            push(
                violations,
                "clearance",
                DrcSeverity::Error,
                format!(
                    "pad {} and track {} on layer {} are {:.3} mm apart, below the {} mm clearance",
                    objects[0], objects[1], track.layer, gap, rules.clearance
                ),
                objects,
                None,
            );
        }

        for via in &design.vias {
            if !via.diameter.is_finite() || (pad.net_id.is_some() && pad.net_id == via.net_id) {
                continue;
            }

            let Some(layer) =
                pad.layers.iter().copied().find(|layer| design.via_covers_layer(via, *layer))
            else {
                continue;
            };

            let distance = rect_distance_to_point(&pad.rect, &via.position);
            if !distance.is_finite() {
                continue;
            }

            let gap = distance - via.diameter / 2.0;
            if gap >= rules.clearance {
                continue;
            }

            let objects = pair_objects(pad.id, via.id);
            push(
                violations,
                "clearance",
                DrcSeverity::Error,
                format!(
                    "pad {} and via {} on layer {} are {:.3} mm apart, below the {} mm clearance",
                    objects[0], objects[1], layer, gap, rules.clearance
                ),
                objects,
                None,
            );
        }
    }

    for left in 0..pads.len() {
        for right in left + 1..pads.len() {
            let first = &pads[left];
            let second = &pads[right];

            if first.footprint_index == second.footprint_index
                || !first.has_valid_size()
                || !second.has_valid_size()
                || first.layers.is_empty()
                || second.layers.is_empty()
                || (first.net_id.is_some() && first.net_id == second.net_id)
            {
                continue;
            }

            let Some(layer) =
                first.layers.iter().copied().find(|layer| second.layers.contains(layer))
            else {
                continue;
            };

            let distance = rect_distance_to_rect(&first.rect, &second.rect);
            if !distance.is_finite() || distance >= rules.clearance {
                continue;
            }

            let objects = pair_objects(first.id, second.id);
            push(
                violations,
                "clearance",
                DrcSeverity::Error,
                format!(
                    "pads {} and {} on layer {} are {:.3} mm apart, below the {} mm clearance",
                    objects[0], objects[1], layer, distance, rules.clearance
                ),
                objects,
                None,
            );
        }
    }
}

/// Smallest distance between a path and a point.
fn path_point_distance(path: &[core::Point], point: &core::Point) -> f64 {
    if path.is_empty() {
        return f64::INFINITY;
    }
    if path.len() == 1 {
        return point.distance_to(&path[0]);
    }

    path.windows(2)
        .map(|window| point.distance_to_segment(&window[0], &window[1]))
        .fold(f64::INFINITY, f64::min)
}

/// Smallest distance between a rectangle and a path.
fn rect_distance_to_track(rect: &CopperRect, path: &[core::Point]) -> f64 {
    if path.is_empty() {
        return f64::INFINITY;
    }
    if path.len() == 1 {
        return rect_distance_to_point(rect, &path[0]);
    }

    path.windows(2)
        .map(|window| rect_distance_to_segment(rect, &window[0], &window[1]))
        .fold(f64::INFINITY, f64::min)
}

fn pair_objects(first: Uuid, second: Uuid) -> Vec<Uuid> {
    let mut objects = vec![first, second];
    objects.sort_by_key(|id| id.to_string());
    objects
}

/// Smallest distance between two track paths, edge to edge before widths are
/// subtracted. Empty paths have no geometry and yield infinity.
fn path_distance(first: &[core::Point], second: &[core::Point]) -> f64 {
    if first.is_empty() || second.is_empty() {
        return f64::INFINITY;
    }

    let mut distance = f64::INFINITY;
    for first_edge in 0..edge_count(first) {
        let (first_start, first_end) = edge(first, first_edge);
        for second_edge in 0..edge_count(second) {
            let (second_start, second_end) = edge(second, second_edge);
            distance = distance.min(core::geometry::segment_distance(
                first_start,
                first_end,
                second_start,
                second_end,
            ));
        }
    }
    distance
}

/// Number of segments in a path; a single point is one zero-length segment.
fn edge_count(path: &[core::Point]) -> usize {
    path.len().max(2) - 1
}

/// Segment `index`; a single point yields a zero-length segment.
fn edge(path: &[core::Point], index: usize) -> (&core::Point, &core::Point) {
    if path.len() == 1 { (&path[0], &path[0]) } else { (&path[index], &path[index + 1]) }
}

#[cfg(test)]
mod tests {
    use super::{DrcReport, DrcRules, DrcSeverity, DrcViolation, run_drc};
    use crate::footprint::{Footprint, PadPosition, PadType};
    use crate::{PcbDesign, Track, Via};
    use core::{LayerKind, PcbLayer, PcbOutline, Point, Stackup};
    use uuid::Uuid;

    fn board(tracks: Vec<Track>, vias: Vec<Via>) -> PcbDesign {
        PcbDesign {
            outline: PcbOutline::rectangle(100.0, 80.0),
            layers: vec![
                PcbLayer::new(0, "F.Cu", LayerKind::Copper),
                PcbLayer::new(1, "B.Cu", LayerKind::Copper),
                PcbLayer::new(2, "F.SilkS", LayerKind::SilkScreen),
            ],
            stackup: Stackup { layers: vec![0, 1] },
            tracks,
            vias,
            ..PcbDesign::default()
        }
    }

    fn track(layer: u16, width: f64, path: Vec<Point>) -> Track {
        Track { id: Uuid::new_v4(), net_id: Uuid::new_v4(), layer, width, path }
    }

    fn via(start: u16, end: u16, position: Point) -> Via {
        Via { id: Uuid::new_v4(), position, layer_start: start, layer_end: end, ..Via::default() }
    }

    fn count_rule(report: &DrcReport, rule_id: &str) -> usize {
        report.violations.iter().filter(|violation| violation.rule_id == rule_id).count()
    }

    fn with_rule<'a>(report: &'a DrcReport, rule_id: &str) -> Vec<&'a DrcViolation> {
        report.violations.iter().filter(|violation| violation.rule_id == rule_id).collect()
    }

    fn pad(x: f64, y: f64, net_id: Option<Uuid>) -> PadPosition {
        PadPosition { x, y, pad_type: PadType::SurfaceMount, net_id, ..PadPosition::default() }
    }

    fn footprint(position: Point, pads: Vec<PadPosition>) -> Footprint {
        Footprint { position, pad_positions: pads, ..Footprint::default() }
    }

    #[test]
    fn track_width_rule_distinguishes_zero_thin_and_acceptable_widths() {
        let design = board(
            vec![
                track(0, 0.0, vec![Point::new(10.0, 10.0), Point::new(20.0, 10.0)]),
                track(0, 0.1, vec![Point::new(10.0, 20.0), Point::new(20.0, 20.0)]),
                track(0, 0.2, vec![Point::new(10.0, 30.0), Point::new(20.0, 30.0)]),
            ],
            Vec::new(),
        );

        let report = run_drc(&design, &DrcRules::default());
        let widths = with_rule(&report, "track-width");

        assert_eq!(widths.len(), 2);
        assert_eq!(widths.iter().filter(|v| v.severity == DrcSeverity::Error).count(), 1);
        assert_eq!(widths.iter().filter(|v| v.severity == DrcSeverity::Warning).count(), 1);
    }

    #[test]
    fn track_layer_rule_rejects_unknown_and_non_copper_layers() {
        let design = board(
            vec![
                track(9, 0.3, vec![Point::new(10.0, 10.0), Point::new(20.0, 10.0)]),
                track(2, 0.3, vec![Point::new(10.0, 20.0), Point::new(20.0, 20.0)]),
            ],
            Vec::new(),
        );

        let report = run_drc(&design, &DrcRules::default());
        let layers = with_rule(&report, "track-layer");

        assert_eq!(layers.len(), 2);
        assert!(layers.iter().all(|v| v.severity == DrcSeverity::Error));
    }

    #[test]
    fn clearance_rule_measures_same_layer_pairs_edge_to_edge() {
        let overlapping = board(
            vec![
                track(0, 0.25, vec![Point::new(10.0, 10.0), Point::new(30.0, 10.0)]),
                track(0, 0.25, vec![Point::new(10.0, 10.0), Point::new(30.0, 10.0)]),
            ],
            Vec::new(),
        );
        assert_eq!(count_rule(&run_drc(&overlapping, &DrcRules::default()), "clearance"), 1);

        let close = board(
            vec![
                track(0, 0.25, vec![Point::new(10.0, 10.0), Point::new(30.0, 10.0)]),
                track(0, 0.25, vec![Point::new(10.0, 10.4), Point::new(30.0, 10.4)]),
            ],
            Vec::new(),
        );
        assert_eq!(count_rule(&run_drc(&close, &DrcRules::default()), "clearance"), 1);

        let far = board(
            vec![
                track(0, 0.25, vec![Point::new(10.0, 10.0), Point::new(30.0, 10.0)]),
                track(0, 0.25, vec![Point::new(10.0, 10.5), Point::new(30.0, 10.5)]),
            ],
            Vec::new(),
        );
        assert_eq!(count_rule(&run_drc(&far, &DrcRules::default()), "clearance"), 0);

        let other_layer = board(
            vec![
                track(0, 0.25, vec![Point::new(10.0, 10.0), Point::new(30.0, 10.0)]),
                track(1, 0.25, vec![Point::new(10.0, 10.0), Point::new(30.0, 10.0)]),
            ],
            Vec::new(),
        );
        assert_eq!(count_rule(&run_drc(&other_layer, &DrcRules::default()), "clearance"), 0);
    }

    #[test]
    fn track_outside_board_names_the_track_once_and_skips_empty_outlines() {
        let design = board(
            vec![track(
                0,
                0.3,
                vec![Point::new(10.0, 10.0), Point::new(200.0, 10.0), Point::new(300.0, 20.0)],
            )],
            Vec::new(),
        );

        let report = run_drc(&design, &DrcRules::default());
        let outside = with_rule(&report, "track-outside-board");

        assert_eq!(outside.len(), 1);
        assert_eq!(outside[0].location, Some(Point::new(200.0, 10.0)));

        let empty_outline = PcbDesign { tracks: design.tracks.clone(), ..PcbDesign::default() };
        assert_eq!(
            count_rule(&run_drc(&empty_outline, &DrcRules::default()), "track-outside-board"),
            0
        );
    }

    #[test]
    fn via_outside_board_checks_the_centre() {
        let inside = board(Vec::new(), vec![via(0, 1, Point::new(50.0, 40.0))]);
        assert_eq!(count_rule(&run_drc(&inside, &DrcRules::default()), "via-outside-board"), 0);

        let outside = board(Vec::new(), vec![via(0, 1, Point::new(150.0, 40.0))]);
        let report = run_drc(&outside, &DrcRules::default());
        assert_eq!(count_rule(&report, "via-outside-board"), 1);
        assert_eq!(report.violations[0].location, Some(Point::new(150.0, 40.0)));
    }

    #[test]
    fn via_span_rule_covers_same_reversed_unknown_and_non_copper_layers() {
        let invalid = [
            via(0, 0, Point::new(50.0, 40.0)),
            via(0, 9, Point::new(50.0, 40.0)),
            via(0, 2, Point::new(50.0, 40.0)),
        ];

        for case in invalid {
            let design = board(Vec::new(), vec![case]);
            let report = run_drc(&design, &DrcRules::default());
            assert_eq!(count_rule(&report, "via-span"), 1);
            assert_eq!(count_rule(&report, "via-drill"), 0);
        }

        for case in [via(0, 1, Point::new(50.0, 40.0)), via(1, 0, Point::new(50.0, 40.0))] {
            let design = board(Vec::new(), vec![case]);
            assert_eq!(count_rule(&run_drc(&design, &DrcRules::default()), "via-span"), 0);
        }
    }

    #[test]
    fn via_span_rule_accepts_blind_and_buried_vias() {
        let mut design = PcbDesign {
            layers: vec![
                PcbLayer::new(0, "F.Cu", LayerKind::Copper),
                PcbLayer::new(1, "In1.Cu", LayerKind::Copper),
                PcbLayer::new(2, "In2.Cu", LayerKind::Copper),
                PcbLayer::new(3, "B.Cu", LayerKind::Copper),
            ],
            stackup: Stackup { layers: vec![0, 1, 2, 3] },
            ..board(Vec::new(), Vec::new())
        };
        design.vias = vec![
            via(0, 1, Point::new(50.0, 40.0)),
            via(2, 3, Point::new(40.0, 40.0)),
            via(1, 2, Point::new(60.0, 40.0)),
        ];

        let report = run_drc(&design, &DrcRules::default());
        assert_eq!(count_rule(&report, "via-span"), 0);
        assert!(report.is_valid());
    }

    #[test]
    fn rotated_pads_use_oriented_copper_for_containment_and_clearance() {
        let mut unrotated = pad(0.0, 0.0, None);
        unrotated.width = 4.0;
        unrotated.height = 0.4;
        let track = track(0, 0.25, vec![Point::new(46.0, 40.0), Point::new(48.0, 40.0)]);
        let design = PcbDesign {
            footprints: vec![footprint(Point::new(50.0, 40.0), vec![unrotated])],
            tracks: vec![track.clone()],
            ..board(Vec::new(), Vec::new())
        };
        assert_eq!(count_rule(&run_drc(&design, &DrcRules::default()), "clearance"), 1);

        let mut rotated = pad(0.0, 0.0, None);
        rotated.width = 4.0;
        rotated.height = 0.4;
        rotated.rotation = 90.0;
        let design = PcbDesign {
            footprints: vec![footprint(Point::new(50.0, 40.0), vec![rotated.clone()])],
            tracks: vec![track],
            ..board(Vec::new(), Vec::new())
        };
        assert_eq!(count_rule(&run_drc(&design, &DrcRules::default()), "clearance"), 0);

        let mut invalid_rotation = rotated;
        invalid_rotation.rotation = f64::NAN;
        let design = PcbDesign {
            footprints: vec![footprint(Point::new(50.0, 40.0), vec![invalid_rotation])],
            ..board(Vec::new(), Vec::new())
        };
        let report = run_drc(&design, &DrcRules::default());
        assert_eq!(count_rule(&report, "pad-rotation"), 1);
    }

    #[test]
    fn via_drill_rule_covers_zero_large_drill_and_thin_ring() {
        let zero = board(Vec::new(), vec![via(0, 1, Point::new(50.0, 40.0))]);
        let mut zero_drill = zero.clone();
        zero_drill.vias[0].drill = 0.0;
        let report = run_drc(&zero_drill, &DrcRules::default());
        assert_eq!(with_rule(&report, "via-drill")[0].severity, DrcSeverity::Error);

        let mut too_large = zero.clone();
        too_large.vias[0].drill = 0.7;
        let report = run_drc(&too_large, &DrcRules::default());
        assert_eq!(with_rule(&report, "via-drill")[0].severity, DrcSeverity::Error);

        let mut thin_ring = zero;
        thin_ring.vias[0].drill = 0.55;
        let report = run_drc(&thin_ring, &DrcRules::default());
        assert_eq!(with_rule(&report, "via-drill")[0].severity, DrcSeverity::Warning);
        assert!(report.is_valid());

        let acceptable = board(Vec::new(), vec![via(0, 1, Point::new(50.0, 40.0))]);
        assert_eq!(count_rule(&run_drc(&acceptable, &DrcRules::default()), "via-drill"), 0);
    }

    #[test]
    fn track_path_rule_rejects_short_and_non_finite_paths() {
        let short = board(vec![track(0, 0.3, vec![Point::new(10.0, 10.0)])], Vec::new());
        assert_eq!(count_rule(&run_drc(&short, &DrcRules::default()), "track-path"), 1);

        let non_finite = board(
            vec![track(0, 0.3, vec![Point::new(10.0, 10.0), Point::new(f64::NAN, 10.0)])],
            Vec::new(),
        );
        assert_eq!(count_rule(&run_drc(&non_finite, &DrcRules::default()), "track-path"), 1);
    }

    #[test]
    fn empty_design_is_valid_and_counts_filter_by_severity() {
        let report = run_drc(&PcbDesign::default(), &DrcRules::default());
        assert!(report.violations.is_empty());
        assert!(report.is_valid());
        assert_eq!(report.count(DrcSeverity::Error), 0);
        assert_eq!(report.count(DrcSeverity::Warning), 0);

        let thin = board(
            vec![track(0, 0.1, vec![Point::new(10.0, 10.0), Point::new(20.0, 10.0)])],
            Vec::new(),
        );
        let report = run_drc(&thin, &DrcRules::default());
        assert!(report.is_valid());
        assert_eq!(report.count(DrcSeverity::Warning), 1);
        assert_eq!(report.count(DrcSeverity::Error), 0);
    }

    #[test]
    fn report_order_is_deterministic_and_independent_of_track_order() {
        let first = track(0, 0.1, vec![Point::new(10.0, 10.0), Point::new(30.0, 10.0)]);
        let second = track(0, 0.1, vec![Point::new(10.0, 10.0), Point::new(30.0, 10.0)]);

        let forwards = board(vec![first.clone(), second.clone()], Vec::new());
        let backwards = board(vec![second, first], Vec::new());

        let first_report = run_drc(&forwards, &DrcRules::default());
        let second_report = run_drc(&backwards, &DrcRules::default());
        let repeated = run_drc(&forwards, &DrcRules::default());

        assert_eq!(first_report, second_report);
        assert_eq!(first_report, repeated);

        let clearance = with_rule(&first_report, "clearance");
        assert_eq!(clearance.len(), 1);
        assert_eq!(clearance[0].objects.len(), 2);
        let mut sorted = clearance[0].objects.clone();
        sorted.sort_by_key(|id| id.to_string());
        assert_eq!(clearance[0].objects, sorted);
    }

    #[test]
    fn drc_rules_validate_requires_finite_positive_limits() {
        assert!(DrcRules::default().validate().is_ok());

        let cases = [
            (DrcRules { min_track_width: 0.0, ..DrcRules::default() }, "min_track_width"),
            (DrcRules { clearance: f64::NAN, ..DrcRules::default() }, "clearance"),
            (DrcRules { min_annular_ring: -0.1, ..DrcRules::default() }, "min_annular_ring"),
            (DrcRules { min_via_drill: f64::INFINITY, ..DrcRules::default() }, "min_via_drill"),
        ];

        for (rules, field) in cases {
            let error = rules.validate().expect_err("invalid limits must be rejected");
            assert_eq!(error.field, field);
            assert!(error.to_string().contains(field));
        }
    }

    #[test]
    fn run_drc_reports_invalid_rules_and_falls_back_to_defaults() {
        let design = board(
            vec![
                track(0, 0.25, vec![Point::new(10.0, 10.0), Point::new(30.0, 10.0)]),
                track(0, 0.25, vec![Point::new(10.0, 10.4), Point::new(30.0, 10.4)]),
            ],
            Vec::new(),
        );

        let report = run_drc(&design, &DrcRules { clearance: f64::NAN, ..DrcRules::default() });

        assert!(!report.is_valid());
        assert_eq!(count_rule(&report, "drc-rules"), 1);
        assert_eq!(count_rule(&report, "clearance"), 1);
    }

    #[test]
    fn track_width_rule_flags_non_finite_widths() {
        let design = board(
            vec![
                track(0, f64::NAN, vec![Point::new(10.0, 10.0), Point::new(20.0, 10.0)]),
                track(0, f64::INFINITY, vec![Point::new(10.0, 20.0), Point::new(20.0, 20.0)]),
            ],
            Vec::new(),
        );

        let report = run_drc(&design, &DrcRules::default());
        let widths = with_rule(&report, "track-width");

        assert_eq!(widths.len(), 2);
        assert!(widths.iter().all(|violation| violation.severity == DrcSeverity::Error));
        assert!(!report.is_valid());
    }

    #[test]
    fn via_drill_rule_rejects_non_finite_and_non_positive_dimensions() {
        let base = board(Vec::new(), vec![via(0, 1, Point::new(50.0, 40.0))]);

        for (drill, diameter) in [(f64::NAN, 0.6), (0.3, f64::INFINITY), (-0.1, 0.6), (0.3, 0.0)] {
            let mut design = base.clone();
            design.vias[0].drill = drill;
            design.vias[0].diameter = diameter;

            let report = run_drc(&design, &DrcRules::default());
            let violations = with_rule(&report, "via-drill");

            assert_eq!(violations.len(), 1, "drill {drill}, diameter {diameter}");
            assert_eq!(violations[0].severity, DrcSeverity::Error);
        }
    }

    #[test]
    fn via_drill_rule_enforces_the_minimum_drill_limit() {
        let design = board(Vec::new(), vec![via(0, 1, Point::new(50.0, 40.0))]);
        let rules = DrcRules { min_via_drill: 0.4, ..DrcRules::default() };

        let report = run_drc(&design, &rules);
        let violations = with_rule(&report, "via-drill");

        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].severity, DrcSeverity::Error);
        assert!(violations[0].message.contains("minimum"));
    }

    #[test]
    fn via_ring_uses_the_annular_ring_limit_not_the_track_width_limit() {
        let design = board(Vec::new(), vec![via(0, 1, Point::new(50.0, 40.0))]);

        let decoupled =
            DrcRules { min_track_width: 4.0, min_annular_ring: 0.05, ..DrcRules::default() };
        assert_eq!(count_rule(&run_drc(&design, &decoupled), "via-drill"), 0);

        let strict_ring = DrcRules { min_annular_ring: 0.2, ..DrcRules::default() };
        let report = run_drc(&design, &strict_ring);
        let violations = with_rule(&report, "via-drill");

        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].severity, DrcSeverity::Warning);
    }

    #[test]
    fn clearance_rule_ignores_tracks_on_the_same_net() {
        let net = Uuid::new_v4();
        let path = vec![Point::new(10.0, 10.0), Point::new(30.0, 10.0)];
        let mut first = track(0, 0.25, path.clone());
        let mut second = track(0, 0.25, path);
        first.net_id = net;
        second.net_id = net;

        let same_net = board(vec![first, second], Vec::new());
        assert_eq!(count_rule(&run_drc(&same_net, &DrcRules::default()), "clearance"), 0);

        let different_nets = board(
            vec![
                track(0, 0.25, vec![Point::new(10.0, 10.0), Point::new(30.0, 10.0)]),
                track(0, 0.25, vec![Point::new(10.0, 10.0), Point::new(30.0, 10.0)]),
            ],
            Vec::new(),
        );
        assert_eq!(count_rule(&run_drc(&different_nets, &DrcRules::default()), "clearance"), 1);
    }

    #[test]
    fn drc_rules_load_defaults_for_missing_fields() {
        let rules: DrcRules = serde_json::from_str(r#"{"min_track_width":0.2,"clearance":0.25}"#)
            .expect("partial rules should load");

        assert_eq!(rules.min_track_width, 0.2);
        assert_eq!(rules.clearance, 0.25);
        assert_eq!(rules.min_annular_ring, DrcRules::default().min_annular_ring);
        assert_eq!(rules.min_via_drill, DrcRules::default().min_via_drill);
    }

    #[test]
    fn clearance_rule_covers_via_to_track_pairs() {
        let track = track(0, 0.25, vec![Point::new(10.0, 10.0), Point::new(30.0, 10.0)]);

        let close = board(vec![track.clone()], vec![via(0, 1, Point::new(20.0, 10.3))]);
        assert_eq!(count_rule(&run_drc(&close, &DrcRules::default()), "clearance"), 1);

        let far = board(vec![track.clone()], vec![via(0, 1, Point::new(20.0, 15.0))]);
        assert_eq!(count_rule(&run_drc(&far, &DrcRules::default()), "clearance"), 0);

        let other_layer = board(vec![track], vec![via(1, 1, Point::new(20.0, 10.3))]);
        assert_eq!(count_rule(&run_drc(&other_layer, &DrcRules::default()), "clearance"), 0);
    }

    #[test]
    fn pads_are_checked_for_size_and_board_containment() {
        let inside = PcbDesign {
            footprints: vec![footprint(Point::new(20.0, 20.0), vec![pad(0.0, 0.0, None)])],
            ..board(Vec::new(), Vec::new())
        };
        let report = run_drc(&inside, &DrcRules::default());
        assert_eq!(count_rule(&report, "pad-size"), 0);
        assert_eq!(count_rule(&report, "pad-outside-board"), 0);

        let outside = PcbDesign {
            footprints: vec![footprint(Point::new(150.0, 20.0), vec![pad(0.0, 0.0, None)])],
            ..board(Vec::new(), Vec::new())
        };
        let report = run_drc(&outside, &DrcRules::default());
        assert_eq!(count_rule(&report, "pad-outside-board"), 1);

        let mut invalid = inside;
        invalid.footprints[0].pad_positions[0].width = 0.0;
        let report = run_drc(&invalid, &DrcRules::default());
        assert_eq!(count_rule(&report, "pad-size"), 1);
        assert_eq!(count_rule(&report, "pad-outside-board"), 0);
    }

    #[test]
    fn clearance_rule_covers_pad_to_track_and_pad_to_via() {
        let net = Uuid::new_v4();
        let placed = |pad: PadPosition| PcbDesign {
            footprints: vec![footprint(Point::new(20.0, 10.0), vec![pad])],
            ..board(Vec::new(), Vec::new())
        };

        let close_track = PcbDesign {
            tracks: vec![track(0, 0.25, vec![Point::new(10.0, 10.7), Point::new(30.0, 10.7)])],
            ..placed(pad(0.0, 0.0, None))
        };
        assert_eq!(count_rule(&run_drc(&close_track, &DrcRules::default()), "clearance"), 1);

        let same_net = PcbDesign {
            tracks: vec![Track {
                net_id: net,
                ..track(0, 0.25, vec![Point::new(10.0, 10.7), Point::new(30.0, 10.7)])
            }],
            ..placed(pad(0.0, 0.0, Some(net)))
        };
        assert_eq!(count_rule(&run_drc(&same_net, &DrcRules::default()), "clearance"), 0);

        let other_layer = PcbDesign {
            tracks: vec![track(1, 0.25, vec![Point::new(10.0, 10.7), Point::new(30.0, 10.7)])],
            ..placed(pad(0.0, 0.0, None))
        };
        assert_eq!(count_rule(&run_drc(&other_layer, &DrcRules::default()), "clearance"), 0);

        let close_via = PcbDesign {
            vias: vec![via(0, 1, Point::new(20.0, 10.9))],
            ..placed(pad(0.0, 0.0, None))
        };
        assert_eq!(count_rule(&run_drc(&close_via, &DrcRules::default()), "clearance"), 1);
    }

    #[test]
    fn clearance_rule_covers_pads_between_footprints_and_exempts_one_footprint() {
        let apart = PcbDesign {
            footprints: vec![
                footprint(Point::new(20.0, 10.0), vec![pad(0.0, 0.0, None)]),
                footprint(Point::new(21.0, 10.0), vec![pad(0.0, 0.0, None)]),
            ],
            ..board(Vec::new(), Vec::new())
        };
        assert_eq!(count_rule(&run_drc(&apart, &DrcRules::default()), "clearance"), 1);

        let same_footprint = PcbDesign {
            footprints: vec![footprint(
                Point::new(20.0, 10.0),
                vec![pad(0.0, 0.0, None), pad(0.5, 0.0, None)],
            )],
            ..board(Vec::new(), Vec::new())
        };
        assert_eq!(count_rule(&run_drc(&same_footprint, &DrcRules::default()), "clearance"), 0);

        let net = Uuid::new_v4();
        let same_net = PcbDesign {
            footprints: vec![
                footprint(Point::new(20.0, 10.0), vec![pad(0.0, 0.0, Some(net))]),
                footprint(Point::new(21.0, 10.0), vec![pad(0.0, 0.0, Some(net))]),
            ],
            ..board(Vec::new(), Vec::new())
        };
        assert_eq!(count_rule(&run_drc(&same_net, &DrcRules::default()), "clearance"), 0);
    }
}
