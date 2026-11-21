//! Basic geometric primitives shared across the design crates.

use serde::{Deserialize, Serialize};

/// A 2D point in millimetres.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    /// Create a point from its coordinates.
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// Euclidean distance between this point and `other`, in millimetres.
    pub fn distance_to(&self, other: &Self) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }

    /// Shortest distance from this point to the segment `start`–`end`.
    ///
    /// The segment is closed, so a projection outside it falls back to the
    /// nearest endpoint. A zero-length segment is a point. Any non-finite
    /// coordinate returns [`f64::INFINITY`]; callers must check for that
    /// instead of treating it as "far away and therefore fine".
    pub fn distance_to_segment(&self, start: &Point, end: &Point) -> f64 {
        if !point_is_finite(self) || !point_is_finite(start) || !point_is_finite(end) {
            return f64::INFINITY;
        }

        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let length_squared = dx * dx + dy * dy;
        if length_squared == 0.0 {
            return self.distance_to(start);
        }

        let projection = ((self.x - start.x) * dx + (self.y - start.y) * dy) / length_squared;
        let t = projection.clamp(0.0, 1.0);
        let closest = Point::new(start.x + t * dx, start.y + t * dy);
        self.distance_to(&closest)
    }
}

/// Shortest distance between two line segments, in millimetres.
///
/// Crossing or touching segments return `0.0`, including shared endpoints and
/// collinear overlap. Zero-length segments are treated as points. The result
/// is symmetric in the two segments. Any non-finite coordinate returns
/// [`f64::INFINITY`], which callers must check for explicitly.
pub fn segment_distance(
    first_start: &Point,
    first_end: &Point,
    second_start: &Point,
    second_end: &Point,
) -> f64 {
    let points = [first_start, first_end, second_start, second_end];
    if points.iter().any(|point| !point_is_finite(point)) {
        return f64::INFINITY;
    }

    if segments_cross(first_start, first_end, second_start, second_end) {
        return 0.0;
    }

    first_start
        .distance_to_segment(second_start, second_end)
        .min(first_end.distance_to_segment(second_start, second_end))
        .min(second_start.distance_to_segment(first_start, first_end))
        .min(second_end.distance_to_segment(first_start, first_end))
}

/// Whether a point has finite coordinates.
fn point_is_finite(point: &Point) -> bool {
    point.x.is_finite() && point.y.is_finite()
}

/// Whether two segments cross away from their endpoints.
///
/// Endpoint touches and collinear overlap are handled by the endpoint
/// distances in [`segment_distance`], which return zero for them.
fn segments_cross(a: &Point, b: &Point, c: &Point, d: &Point) -> bool {
    let first = orientation(a, b, c);
    let second = orientation(a, b, d);
    let third = orientation(c, d, a);
    let fourth = orientation(c, d, b);

    ((first > 0.0 && second < 0.0) || (first < 0.0 && second > 0.0))
        && ((third > 0.0 && fourth < 0.0) || (third < 0.0 && fourth > 0.0))
}

/// Twice the signed area of the triangle `a`, `b`, `c`.
fn orientation(a: &Point, b: &Point, c: &Point) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

#[cfg(test)]
mod tests {
    use super::{Point, segment_distance};

    #[test]
    fn distance_to_is_euclidean() {
        let origin = Point::new(0.0, 0.0);

        assert_eq!(origin.distance_to(&origin), 0.0);
        assert_eq!(origin.distance_to(&Point::new(3.0, 4.0)), 5.0);
        assert_eq!(Point::new(-3.0, -4.0).distance_to(&origin), 5.0);
    }

    #[test]
    fn point_to_segment_handles_inside_before_and_after() {
        let start = Point::new(0.0, 0.0);
        let end = Point::new(6.0, 0.0);

        assert_eq!(Point::new(3.0, 4.0).distance_to_segment(&start, &end), 4.0);
        assert_eq!(Point::new(-3.0, 4.0).distance_to_segment(&start, &end), 5.0);
        assert_eq!(Point::new(9.0, 4.0).distance_to_segment(&start, &end), 5.0);
    }

    #[test]
    fn point_to_segment_handles_zero_length_segments() {
        let point = Point::new(2.0, 2.0);
        let doubled = Point::new(0.0, 0.0);

        assert_eq!(point.distance_to_segment(&doubled, &doubled), (8.0f64).sqrt());
        assert_eq!(point.distance_to_segment(&point, &point), 0.0);
    }

    #[test]
    fn point_to_segment_is_symmetric_for_reversed_endpoints() {
        let point = Point::new(1.5, 2.5);
        let start = Point::new(0.0, 0.0);
        let end = Point::new(3.0, 0.0);

        assert_eq!(
            point.distance_to_segment(&start, &end),
            point.distance_to_segment(&end, &start)
        );
    }

    #[test]
    fn segment_distance_covers_parallel_crossing_touching_and_disjoint() {
        let parallel = segment_distance(
            &Point::new(0.0, 0.0),
            &Point::new(10.0, 0.0),
            &Point::new(0.0, 1.0),
            &Point::new(10.0, 1.0),
        );
        assert_eq!(parallel, 1.0);

        let crossing = segment_distance(
            &Point::new(0.0, 0.0),
            &Point::new(10.0, 10.0),
            &Point::new(0.0, 10.0),
            &Point::new(10.0, 0.0),
        );
        assert_eq!(crossing, 0.0);

        let touching = segment_distance(
            &Point::new(0.0, 0.0),
            &Point::new(5.0, 0.0),
            &Point::new(5.0, 0.0),
            &Point::new(5.0, 5.0),
        );
        assert_eq!(touching, 0.0);

        let disjoint = segment_distance(
            &Point::new(0.0, 0.0),
            &Point::new(1.0, 0.0),
            &Point::new(3.0, 0.0),
            &Point::new(4.0, 0.0),
        );
        assert_eq!(disjoint, 2.0);
    }

    #[test]
    fn segment_distance_is_symmetric_and_non_finite_input_is_infinite() {
        let first_start = Point::new(0.0, 0.0);
        let first_end = Point::new(4.0, 0.0);
        let second_start = Point::new(1.0, 2.0);
        let second_end = Point::new(1.0, 6.0);

        assert_eq!(
            segment_distance(&first_start, &first_end, &second_start, &second_end),
            segment_distance(&second_start, &second_end, &first_start, &first_end)
        );

        assert_eq!(
            segment_distance(&first_start, &first_end, &Point::new(f64::NAN, 0.0), &second_end),
            f64::INFINITY
        );
        assert_eq!(
            Point::new(f64::INFINITY, 0.0).distance_to_segment(&first_start, &first_end),
            f64::INFINITY
        );
    }
}
