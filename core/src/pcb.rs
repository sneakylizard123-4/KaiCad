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

    /// Whether the outline has enough points to enclose an area.
    pub fn is_closed(&self) -> bool {
        self.points.len() >= 3
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
    use super::{LayerKind, PcbLayer, Stackup};

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
}
