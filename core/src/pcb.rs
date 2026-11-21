//! PCB-specific core types: board outline, layers, and stackup.

use serde::{Deserialize, Serialize};

use crate::geometry::Point;

/// A closed polygonal board outline, in millimetres.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PcbOutline {
    pub points: Vec<Point>,
}

/// An axis-aligned bounding box in millimetres.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct BoundingBox {
    pub min: Point,
    pub max: Point,
}

impl PcbOutline {
    /// Build an outline from its corner points.
    pub fn new(points: Vec<Point>) -> Self {
        Self { points }
    }

    /// Build a rectangular outline anchored at the origin.
    ///
    /// The corners are `(0,0)`, `(w,0)`, `(w,h)`, `(0,h)`.
    pub fn rectangle(width: f64, height: f64) -> Self {
        Self::new(vec![
            Point::new(0.0, 0.0),
            Point::new(width, 0.0),
            Point::new(width, height),
            Point::new(0.0, height),
        ])
    }

    /// Whether the outline encloses a non-zero area.
    ///
    /// The points are the corners in order, and the edge from the last point
    /// back to the first is implicit, so a rectangle need not repeat its
    /// starting corner. Repeated or collinear corners enclose no area and are
    /// therefore rejected.
    pub fn is_closed(&self) -> bool {
        if self.points.len() < 3 {
            return false;
        }

        let doubled_area: f64 = self
            .points
            .iter()
            .zip(self.points.iter().cycle().skip(1))
            .map(|(point, next)| point.x * next.y - next.x * point.y)
            .sum();

        doubled_area.abs() > f64::EPSILON
    }

    /// Signed enclosed area in square millimetres.
    ///
    /// The sign follows the winding order: counter-clockwise outlines are
    /// positive, clockwise outlines negative. Degenerate outlines enclose zero
    /// area.
    pub fn signed_area(&self) -> f64 {
        if self.points.len() < 3 {
            return 0.0;
        }

        let doubled_area: f64 = self
            .points
            .iter()
            .zip(self.points.iter().cycle().skip(1))
            .map(|(point, next)| point.x * next.y - next.x * point.y)
            .sum();

        doubled_area / 2.0
    }

    /// Absolute enclosed area in square millimetres.
    ///
    /// Winding order does not matter; see [`PcbOutline::signed_area`] when it
    /// does.
    pub fn area(&self) -> f64 {
        self.signed_area().abs()
    }

    /// Total length of the closed perimeter in millimetres.
    ///
    /// The implicit edge from the last point back to the first is included.
    pub fn perimeter(&self) -> f64 {
        if self.points.len() < 2 {
            return 0.0;
        }

        self.points
            .iter()
            .zip(self.points.iter().cycle().skip(1))
            .map(|(point, next)| point.distance_to(next))
            .sum()
    }

    /// Whether `point` lies inside or on the outline.
    ///
    /// Even-odd ray casting on the closed polygon: concave outlines work, and
    /// points exactly on an edge or corner count as inside. Fewer than three
    /// points, or a non-finite coordinate, is never inside.
    pub fn contains_point(&self, point: &Point) -> bool {
        if self.points.len() < 3 || !point.x.is_finite() || !point.y.is_finite() {
            return false;
        }

        let mut inside = false;
        for (start, end) in self.points.iter().zip(self.points.iter().cycle().skip(1)) {
            if point_on_segment(start, end, point) {
                return true;
            }
            if (start.y > point.y) != (end.y > point.y) {
                let crossing_x =
                    start.x + (point.y - start.y) * (end.x - start.x) / (end.y - start.y);
                if point.x < crossing_x {
                    inside = !inside;
                }
            }
        }
        inside
    }

    /// Bounding box of the corner points, or `None` for an empty outline.
    ///
    /// A single point produces a zero-size box rather than `None`.
    pub fn bounding_box(&self) -> Option<BoundingBox> {
        let mut points = self.points.iter();
        let first = points.next()?;
        let mut bounding = BoundingBox { min: *first, max: *first };

        for point in points {
            bounding.min.x = bounding.min.x.min(point.x);
            bounding.min.y = bounding.min.y.min(point.y);
            bounding.max.x = bounding.max.x.max(point.x);
            bounding.max.y = bounding.max.y.max(point.y);
        }
        Some(bounding)
    }
}

/// Tolerance used when deciding whether a point lies on an outline edge.
const BOUNDARY_EPSILON: f64 = 1e-9;

/// Whether `point` lies on the segment from `start` to `end`.
fn point_on_segment(start: &Point, end: &Point, point: &Point) -> bool {
    let cross = (end.x - start.x) * (point.y - start.y) - (end.y - start.y) * (point.x - start.x);
    if cross.abs() > BOUNDARY_EPSILON {
        return false;
    }

    let within_x = point.x >= start.x.min(end.x) - BOUNDARY_EPSILON
        && point.x <= start.x.max(end.x) + BOUNDARY_EPSILON;
    let within_y = point.y >= start.y.min(end.y) - BOUNDARY_EPSILON
        && point.y <= start.y.max(end.y) + BOUNDARY_EPSILON;
    within_x && within_y
}

/// What a PCB layer is used for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayerKind {
    /// A conductive copper layer.
    #[default]
    Copper,
    /// A silkscreen legend layer.
    SilkScreen,
    /// A solder mask layer.
    SolderMask,
    /// A mechanical or documentation layer.
    Mechanical,
    /// Any other layer type.
    Other,
}

/// A single PCB layer.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PcbLayer {
    pub id: u16,
    pub name: String,
    pub kind: LayerKind,
}

impl PcbLayer {
    /// Create a layer with the given id, name, and kind.
    pub fn new(id: u16, name: impl Into<String>, kind: LayerKind) -> Self {
        Self { id, name: name.into(), kind }
    }

    /// Whether this layer is made of copper.
    pub fn is_copper(&self) -> bool {
        self.kind == LayerKind::Copper
    }
}

/// Ordered layer ids from top to bottom.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Stackup {
    pub layers: Vec<u16>,
}

#[cfg(test)]
mod tests {
    use super::{BoundingBox, LayerKind, PcbLayer, PcbOutline, Stackup};
    use crate::geometry::Point;

    #[test]
    fn copper_layers_are_recognised() {
        let copper = PcbLayer::new(0, "F.Cu", LayerKind::Copper);
        let silk = PcbLayer::new(1, "F.SilkS", LayerKind::SilkScreen);

        assert!(copper.is_copper());
        assert!(!silk.is_copper());
    }

    #[test]
    fn stackup_default_is_empty() {
        let stackup = Stackup::default();

        assert!(stackup.layers.is_empty());
    }

    #[test]
    fn degenerate_outlines_do_not_enclose_area() {
        let point = Point::new(0.0, 0.0);

        assert!(!PcbOutline::default().is_closed());
        assert!(!PcbOutline::new(vec![point, point, point]).is_closed());
        assert!(
            !PcbOutline::new(vec![
                Point::new(0.0, 0.0),
                Point::new(1.0, 1.0),
                Point::new(2.0, 2.0),
            ])
            .is_closed()
        );
    }

    #[test]
    fn explicitly_closed_outline_encloses_area() {
        let outline = PcbOutline::new(vec![
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(0.0, 10.0),
            Point::new(0.0, 0.0),
        ]);

        assert!(outline.is_closed());
    }

    #[test]
    fn rectangle_area_and_perimeter_are_exact() {
        let outline = PcbOutline::rectangle(9.0, 5.0);

        assert_eq!(outline.signed_area(), 45.0);
        assert_eq!(outline.area(), 45.0);
        assert_eq!(outline.perimeter(), 28.0);
    }

    #[test]
    fn winding_order_changes_the_sign_but_not_the_area() {
        let counter_clockwise = PcbOutline::new(vec![
            Point::new(0.0, 0.0),
            Point::new(2.0, 0.0),
            Point::new(2.0, 1.0),
            Point::new(0.0, 1.0),
        ]);
        let clockwise = PcbOutline::new(vec![
            Point::new(0.0, 0.0),
            Point::new(0.0, 1.0),
            Point::new(2.0, 1.0),
            Point::new(2.0, 0.0),
        ]);

        assert_eq!(counter_clockwise.area(), 2.0);
        assert_eq!(clockwise.area(), 2.0);
        assert_eq!(counter_clockwise.signed_area(), -clockwise.signed_area());
    }

    #[test]
    fn contains_point_covers_inside_outside_and_boundary() {
        let outline = PcbOutline::rectangle(10.0, 6.0);

        assert!(outline.contains_point(&Point::new(5.0, 3.0)));
        assert!(!outline.contains_point(&Point::new(11.0, 3.0)));
        assert!(!outline.contains_point(&Point::new(5.0, -1.0)));
        assert!(outline.contains_point(&Point::new(0.0, 0.0)));
        assert!(outline.contains_point(&Point::new(10.0, 6.0)));
        assert!(outline.contains_point(&Point::new(5.0, 0.0)));
    }

    #[test]
    fn contains_point_handles_concave_outlines() {
        // An L shape that is missing the top-right quadrant.
        let outline = PcbOutline::new(vec![
            Point::new(0.0, 0.0),
            Point::new(4.0, 0.0),
            Point::new(4.0, 1.0),
            Point::new(1.0, 1.0),
            Point::new(1.0, 4.0),
            Point::new(0.0, 4.0),
        ]);

        assert!(outline.contains_point(&Point::new(0.5, 3.0)));
        assert!(outline.contains_point(&Point::new(3.0, 0.5)));
        assert!(!outline.contains_point(&Point::new(3.0, 3.0)));
        assert!(outline.contains_point(&Point::new(1.0, 1.0)));
    }

    #[test]
    fn contains_point_rejects_degenerate_and_non_finite_input() {
        let degenerate = PcbOutline::new(vec![Point::new(0.0, 0.0), Point::new(1.0, 1.0)]);

        assert!(!degenerate.contains_point(&Point::new(0.0, 0.0)));
        assert!(!PcbOutline::default().contains_point(&Point::new(0.0, 0.0)));
        assert!(!PcbOutline::rectangle(1.0, 1.0).contains_point(&Point::new(f64::NAN, 0.5)));
        assert!(!PcbOutline::rectangle(1.0, 1.0).contains_point(&Point::new(0.5, f64::INFINITY)));
    }

    #[test]
    fn bounding_box_covers_every_corner() {
        let bounding = PcbOutline::rectangle(100.0, 80.0).bounding_box().expect("non-empty");
        assert_eq!(
            bounding,
            BoundingBox { min: Point::new(0.0, 0.0), max: Point::new(100.0, 80.0) }
        );

        let single =
            PcbOutline::new(vec![Point::new(-3.0, 4.0)]).bounding_box().expect("single point");
        assert_eq!(single.min, single.max);
        assert_eq!(single.min, Point::new(-3.0, 4.0));

        assert!(PcbOutline::default().bounding_box().is_none());
    }

    #[test]
    fn perimeter_and_area_include_the_closing_edge_once() {
        let triangle =
            PcbOutline::new(vec![Point::new(0.0, 0.0), Point::new(3.0, 0.0), Point::new(0.0, 4.0)]);
        assert_eq!(triangle.perimeter(), 12.0);
        assert_eq!(triangle.area(), 6.0);

        let explicitly_closed = PcbOutline::new(vec![
            Point::new(0.0, 0.0),
            Point::new(3.0, 0.0),
            Point::new(0.0, 4.0),
            Point::new(0.0, 0.0),
        ]);
        assert_eq!(explicitly_closed.perimeter(), 12.0);
        assert_eq!(explicitly_closed.area(), 6.0);
    }
}
