//! Deterministic design-rule checking for PCB designs.
//!
//! Checks are pure functions of a [`PcbDesign`] and [`DrcRules`]: the same
//! input always produces the same report, byte for byte, because violations
//! are sorted by rule id and object ids. Non-finite geometry is reported by
//! the rules rather than silently skipped, and no rule panics on degenerate
//! input.

use std::cmp::Ordering;

use uuid::Uuid;

use crate::{PcbDesign, Track, Via};

/// Minimum copper dimensions enforced by the built-in rules.
#[derive(Debug, Clone, PartialEq)]
pub struct DrcRules {
    /// Minimum acceptable copper width in millimetres.
    pub min_track_width: f64,
    /// Minimum copper-to-copper gap in millimetres.
    pub clearance: f64,
}

impl Default for DrcRules {
    fn default() -> Self {
        Self { min_track_width: 0.15, clearance: 0.2 }
    }
}

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
/// Track clearance is measured edge to edge on the same copper layer, and
/// every unordered pair is reported once. An outline with fewer than three
/// points disables the board-containment rules, and an empty stackup means
/// layer order is unknown, so via spans only need distinct, existing copper
/// layers.
pub fn run_drc(design: &PcbDesign, rules: &DrcRules) -> DrcReport {
    let mut violations = Vec::new();
    let stackup_copper = stackup_copper_ids(design);

    for track in &design.tracks {
        check_track_width(track, rules, &mut violations);
        check_track_layer(design, track, &mut violations);
        check_track_path(track, &mut violations);
        check_track_outside(design, track, &mut violations);
    }

    for via in &design.vias {
        check_via_outside(design, via, &mut violations);
        check_via_span(design, &stackup_copper, via, &mut violations);
        check_via_drill(via, rules, &mut violations);
    }

    check_clearance(design, rules, &mut violations);

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

/// Copper layer ids in stackup order, without duplicates.
///
/// Returns an empty list when the stackup is empty, which means layer order is
/// unknown rather than "no copper layers".
fn stackup_copper_ids(design: &PcbDesign) -> Vec<u16> {
    let mut ids = Vec::new();
    for id in &design.stackup.layers {
        if !ids.contains(id) && is_copper(design, *id) {
            ids.push(*id);
        }
    }
    ids
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

    if track.width <= 0.0 {
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

fn check_via_span(
    design: &PcbDesign,
    stackup_copper: &[u16],
    via: &Via,
    violations: &mut Vec<DrcViolation>,
) {
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
        return;
    }

    if !design.stackup.layers.is_empty() {
        let top = stackup_copper.first().copied();
        let bottom = stackup_copper.last().copied();
        if top != Some(via.layer_start) || bottom != Some(via.layer_end) {
            push(
                violations,
                "via-span",
                DrcSeverity::Error,
                format!(
                    "via {} spans layers {}-{}, not the full top-to-bottom stackup {}-{}",
                    via.id,
                    via.layer_start,
                    via.layer_end,
                    top.map_or_else(|| "none".to_string(), |id| id.to_string()),
                    bottom.map_or_else(|| "none".to_string(), |id| id.to_string()),
                ),
                vec![via.id],
                location,
            );
        }
    }
}

fn check_via_drill(via: &Via, rules: &DrcRules, violations: &mut Vec<DrcViolation>) {
    let location = finite_location(&via.position);

    if via.drill <= 0.0 {
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
    } else {
        let ring = (via.diameter - via.drill) / 2.0;
        if ring < rules.min_track_width / 2.0 {
            push(
                violations,
                "via-drill",
                DrcSeverity::Warning,
                format!(
                    "via {} annular ring {:.3} mm is below {:.3} mm",
                    via.id,
                    ring,
                    rules.min_track_width / 2.0
                ),
                vec![via.id],
                location,
            );
        }
    }
}

fn check_clearance(design: &PcbDesign, rules: &DrcRules, violations: &mut Vec<DrcViolation>) {
    for left in 0..design.tracks.len() {
        for right in left + 1..design.tracks.len() {
            let first = &design.tracks[left];
            let second = &design.tracks[right];

            if first.layer != second.layer || !is_copper(design, first.layer) {
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

            let mut objects = vec![first.id, second.id];
            objects.sort_by_key(|id| id.to_string());
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
        Track { id: Uuid::new_v4(), net_id: Uuid::nil(), layer, width, path }
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
        let cases = [
            via(0, 0, Point::new(50.0, 40.0)),
            via(1, 0, Point::new(50.0, 40.0)),
            via(0, 9, Point::new(50.0, 40.0)),
            via(0, 2, Point::new(50.0, 40.0)),
        ];

        for case in cases {
            let design = board(Vec::new(), vec![case]);
            let report = run_drc(&design, &DrcRules::default());
            assert_eq!(count_rule(&report, "via-span"), 1);
            assert_eq!(count_rule(&report, "via-drill"), 0);
        }

        let valid = board(Vec::new(), vec![via(0, 1, Point::new(50.0, 40.0))]);
        assert_eq!(count_rule(&run_drc(&valid, &DrcRules::default()), "via-span"), 0);
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
}
