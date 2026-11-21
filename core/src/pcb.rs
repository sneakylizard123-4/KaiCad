//! PCB-specific core types: board outline, layers, and stackup.

use serde::{Deserialize, Serialize};

use crate::geometry::Point;

/// A closed polygonal board outline, in millimetres.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PcbOutline {
    pub points: Vec<Point>,
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
    use super::{LayerKind, PcbLayer, PcbOutline, Stackup};
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
}
