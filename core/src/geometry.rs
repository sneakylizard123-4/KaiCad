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
}

#[cfg(test)]
mod tests {
    use super::Point;

    #[test]
    fn distance_to_is_euclidean() {
        let origin = Point::new(0.0, 0.0);

        assert_eq!(origin.distance_to(&origin), 0.0);
        assert_eq!(origin.distance_to(&Point::new(3.0, 4.0)), 5.0);
        assert_eq!(Point::new(-3.0, -4.0).distance_to(&origin), 5.0);
    }
}
