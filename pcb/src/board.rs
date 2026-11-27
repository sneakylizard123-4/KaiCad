//! Typed board constructs that KiCad stores outside pads, tracks and vias.
//!
//! Zones, copper arcs and board graphics used to be retained only as opaque
//! source nodes. These types make their geometry and connectivity available to
//! KaiCad's engines while keeping unmodelled children on
//! [`core::PreservedNode`] so a round trip still preserves them.

use core::{Point, PreservedNode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// One filled area produced by a copper zone.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ZoneFill {
    /// Copper layer containing the fill, when KiCad records one per fill.
    #[serde(default)]
    pub layer: Option<u16>,
    /// Filled polygon in board coordinates.
    #[serde(default)]
    pub points: Vec<Point>,
    /// Whether KiCad marked this polygon as an isolated island.
    #[serde(default)]
    pub island: bool,
}

/// The object types a keepout zone forbids.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZoneKeepout {
    /// Tracks and copper arcs may not enter the zone.
    #[serde(default)]
    pub tracks: bool,
    /// Vias may not enter the zone.
    #[serde(default)]
    pub vias: bool,
    /// Pads may not enter the zone.
    #[serde(default)]
    pub pads: bool,
    /// Copper pours may not enter the zone.
    #[serde(default)]
    pub copper_pour: bool,
    /// Footprints may not enter the zone.
    #[serde(default)]
    pub footprints: bool,
}

impl ZoneKeepout {
    /// Whether every object flag is clear.
    pub fn is_empty(&self) -> bool {
        !self.tracks && !self.vias && !self.pads && !self.copper_pour && !self.footprints
    }
}

/// A copper zone, rule area or keepout.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BoardZone {
    /// Stable source UUID, or a deterministic id when the file has none.
    #[serde(default)]
    pub id: Uuid,
    /// Net connected to the zone, when it has one.
    #[serde(default)]
    pub net_id: Option<Uuid>,
    /// Layers the zone applies to.
    #[serde(default)]
    pub layers: Vec<u16>,
    /// Zone outline polygons in board coordinates.
    #[serde(default)]
    pub polygons: Vec<Vec<Point>>,
    /// Filled copper polygons from the last zone fill.
    #[serde(default)]
    pub fills: Vec<ZoneFill>,
    /// Keepout rules, when this is a rule area rather than a copper zone.
    #[serde(default)]
    pub keepout: Option<ZoneKeepout>,
    /// User-visible zone name.
    #[serde(default)]
    pub name: String,
    /// Zone priority; higher-priority zones win where fills overlap.
    #[serde(default)]
    pub priority: i32,
    /// Unmodelled zone children retained for export.
    #[serde(default)]
    pub preserved: Vec<PreservedNode>,
}

impl BoardZone {
    /// Whether this zone is a rule area.
    pub fn is_keepout(&self) -> bool {
        self.keepout.is_some()
    }

    /// Filled copper on `layer`, including fills without an explicit layer.
    pub fn filled_polygons(&self, layer: u16) -> impl Iterator<Item = &[Point]> {
        self.fills.iter().filter_map(move |fill| {
            let belongs = match fill.layer {
                Some(fill_layer) => fill_layer == layer,
                None => self.layers.contains(&layer),
            };
            (belongs && fill.points.len() >= 3).then_some(fill.points.as_slice())
        })
    }
}

/// A copper arc on a single copper layer.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BoardArc {
    /// Stable source UUID, or a deterministic id when the file has none.
    pub id: Uuid,
    /// Net connected by the arc.
    #[serde(default)]
    pub net_id: Uuid,
    /// Copper layer id.
    pub layer: u16,
    /// Copper width in millimetres.
    #[serde(default = "crate::default_track_width")]
    pub width: f64,
    /// Arc start point.
    pub start: Point,
    /// Point on the arc between its endpoints.
    pub mid: Point,
    /// Arc end point.
    pub end: Point,
    /// Unmodelled arc children retained for export.
    #[serde(default)]
    pub preserved: Vec<PreservedNode>,
}

/// Geometry of a graphical board object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BoardGraphicKind {
    /// A straight line from `start` to `end`.
    Line { start: Point, end: Point },
    /// An axis-aligned rectangle.
    Rectangle { start: Point, end: Point },
    /// A circle from its center to a point on its radius.
    Circle { center: Point, end: Point },
    /// A circular arc through three points.
    Arc { start: Point, mid: Point, end: Point },
    /// A closed polygon.
    Polygon { points: Vec<Point> },
    /// Text at a board position.
    Text { text: String, position: Point, rotation: f64 },
}

impl Default for BoardGraphicKind {
    fn default() -> Self {
        Self::Line { start: Point::default(), end: Point::default() }
    }
}

/// A standalone graphical object on a board layer.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BoardGraphic {
    /// Stable source UUID, or a deterministic id when the file has none.
    #[serde(default)]
    pub id: Uuid,
    /// Shape and coordinates.
    #[serde(default)]
    pub kind: BoardGraphicKind,
    /// Layer id the object is drawn on.
    pub layer: u16,
    /// Stroke width in millimetres, or zero for unfilled text.
    #[serde(default)]
    pub width: f64,
    /// Fill mode from the source, for example `none` or `solid`.
    #[serde(default)]
    pub fill: String,
    /// Unmodelled graphic children retained for export.
    #[serde(default)]
    pub preserved: Vec<PreservedNode>,
}

impl BoardGraphic {
    /// Whether the source marks this graphic as filled.
    pub fn is_filled(&self) -> bool {
        !matches!(self.fill.as_str(), "" | "none" | "no")
    }
}

/// Sample a circular arc through three points.
///
/// The result starts at `start`, ends at `end`, and contains `segments + 1`
/// points when the points are not collinear. Collinear or non-finite input
/// falls back to the straight segment between the endpoints. The arc always
/// follows the side containing `mid`.
pub fn sample_arc(start: Point, mid: Point, end: Point, segments: usize) -> Vec<Point> {
    if !finite(start) || !finite(mid) || !finite(end) {
        return vec![start, end];
    }
    let determinant =
        2.0 * (start.x * (mid.y - end.y) + mid.x * (end.y - start.y) + end.x * (start.y - mid.y));
    if determinant.abs() <= 1e-12 {
        if start == end {
            return vec![start];
        }
        return vec![start, end];
    }

    let start_sq = start.x * start.x + start.y * start.y;
    let mid_sq = mid.x * mid.x + mid.y * mid.y;
    let end_sq = end.x * end.x + end.y * end.y;
    let center = Point::new(
        (start_sq * (mid.y - end.y) + mid_sq * (end.y - start.y) + end_sq * (start.y - mid.y))
            / determinant,
        (start_sq * (end.x - mid.x) + mid_sq * (start.x - end.x) + end_sq * (mid.x - start.x))
            / determinant,
    );
    let radius = center.distance_to(&start);
    let start_angle = (start.y - center.y).atan2(start.x - center.x);
    let mid_angle = (mid.y - center.y).atan2(mid.x - center.x);
    let end_angle = (end.y - center.y).atan2(end.x - center.x);

    let ccw_total = positive_angle(end_angle - start_angle);
    let ccw_mid = positive_angle(mid_angle - start_angle);
    let sweep = if ccw_mid <= ccw_total + 1e-12 {
        ccw_total
    } else {
        ccw_total - 2.0 * std::f64::consts::PI
    };

    let segments = segments.max(2);
    let mut points = Vec::with_capacity(segments + 1);
    for index in 0..=segments {
        let fraction = index as f64 / segments as f64;
        let angle = start_angle + sweep * fraction;
        points.push(Point::new(center.x + radius * angle.cos(), center.y + radius * angle.sin()));
    }
    points[0] = start;
    if let Some(last) = points.last_mut() {
        *last = end;
    }
    points
}

fn positive_angle(angle: f64) -> f64 {
    let full = 2.0 * std::f64::consts::PI;
    ((angle % full) + full) % full
}

fn finite(point: Point) -> bool {
    point.x.is_finite() && point.y.is_finite()
}

/// Whether `point` lies inside or on a polygon.
pub(crate) fn polygon_contains(polygon: &[Point], point: &Point) -> bool {
    if polygon.len() < 3 || !point.x.is_finite() || !point.y.is_finite() {
        return false;
    }
    let mut inside = false;
    for (index, start) in polygon.iter().enumerate() {
        let end = &polygon[(index + 1) % polygon.len()];
        if point.distance_to_segment(start, end) <= 1e-9 {
            return true;
        }
        if (start.y > point.y) != (end.y > point.y) {
            let crossing_x = start.x + (point.y - start.y) * (end.x - start.x) / (end.y - start.y);
            if point.x < crossing_x {
                inside = !inside;
            }
        }
    }
    inside
}

/// Distance from a point to a polygon boundary, or zero when inside.
pub(crate) fn polygon_distance_to_point(polygon: &[Point], point: &Point) -> f64 {
    if polygon.len() < 2 {
        return f64::INFINITY;
    }
    if polygon_contains(polygon, point) {
        return 0.0;
    }
    polygon
        .iter()
        .enumerate()
        .map(|(index, start)| {
            let end = &polygon[(index + 1) % polygon.len()];
            point.distance_to_segment(start, end)
        })
        .fold(f64::INFINITY, f64::min)
}

/// Distance between two polygons, or zero when they overlap or contain one
/// another.
pub(crate) fn polygon_distance(first: &[Point], second: &[Point]) -> f64 {
    if first.len() < 2 || second.len() < 2 {
        return f64::INFINITY;
    }
    if first.iter().any(|point| polygon_contains(second, point))
        || second.iter().any(|point| polygon_contains(first, point))
    {
        return 0.0;
    }
    let mut distance = f64::INFINITY;
    for (first_index, first_start) in first.iter().enumerate() {
        let first_end = &first[(first_index + 1) % first.len()];
        for (second_index, second_start) in second.iter().enumerate() {
            let second_end = &second[(second_index + 1) % second.len()];
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

/// Distance between a path and a polygon, or zero when they touch or overlap.
pub(crate) fn polygon_distance_to_path(polygon: &[Point], path: &[Point]) -> f64 {
    if polygon.len() < 2 || path.is_empty() {
        return f64::INFINITY;
    }
    if path.iter().any(|point| polygon_contains(polygon, point)) {
        return 0.0;
    }
    if path.len() == 1 {
        return polygon_distance_to_point(polygon, &path[0]);
    }
    let mut distance = f64::INFINITY;
    for window in path.windows(2) {
        let start = &window[0];
        let end = &window[1];
        for (polygon_index, polygon_start) in polygon.iter().enumerate() {
            let polygon_end = &polygon[(polygon_index + 1) % polygon.len()];
            distance = distance.min(core::geometry::segment_distance(
                start,
                end,
                polygon_start,
                polygon_end,
            ));
        }
    }
    distance
}

#[cfg(test)]
mod tests {
    use super::{
        BoardZone, ZoneFill, ZoneKeepout, polygon_contains, polygon_distance,
        polygon_distance_to_path, sample_arc,
    };
    use core::Point;
    use uuid::Uuid;

    #[test]
    fn arc_sampling_starts_and_ends_at_the_requested_points() {
        let points =
            sample_arc(Point::new(0.0, 0.0), Point::new(1.0, 1.0), Point::new(2.0, 0.0), 8);

        assert_eq!(points.len(), 9);
        assert_eq!(points.first(), Some(&Point::new(0.0, 0.0)));
        assert_eq!(points.last(), Some(&Point::new(2.0, 0.0)));
        assert!(points.iter().any(|point| point.y > 0.9));
    }

    #[test]
    fn collinear_arcs_keep_both_endpoints() {
        let points =
            sample_arc(Point::new(0.0, 0.0), Point::new(1.0, 0.0), Point::new(2.0, 0.0), 8);

        assert_eq!(points, vec![Point::new(0.0, 0.0), Point::new(2.0, 0.0)]);
    }

    #[test]
    fn polygon_helpers_cover_inside_boundary_and_separation() {
        let square = vec![
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(0.0, 10.0),
        ];
        assert!(polygon_contains(&square, &Point::new(5.0, 5.0)));
        assert!(polygon_contains(&square, &Point::new(0.0, 5.0)));
        assert!(!polygon_contains(&square, &Point::new(11.0, 5.0)));
        assert_eq!(polygon_distance(&square, &square), 0.0);
        assert_eq!(
            polygon_distance_to_path(&square, &[Point::new(11.0, 5.0), Point::new(12.0, 5.0)]),
            1.0
        );
    }

    #[test]
    fn zone_fills_are_filtered_by_layer() {
        let zone = BoardZone {
            id: Uuid::new_v4(),
            layers: vec![0, 31],
            fills: vec![ZoneFill {
                layer: Some(31),
                points: vec![Point::new(0.0, 0.0), Point::new(1.0, 0.0), Point::new(0.0, 1.0)],
                island: false,
            }],
            ..BoardZone::default()
        };

        assert_eq!(zone.filled_polygons(31).count(), 1);
        assert_eq!(zone.filled_polygons(0).count(), 0);
        assert!(ZoneKeepout::default().is_empty());
    }
}
