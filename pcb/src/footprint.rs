//! Footprint definitions for PCB design.

/// A PCB footprint representing a component's physical dimensions and properties.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Footprint {
    pub name: String,
    pub reference_designator: String,
    pub pad_positions: Vec<PadPosition>,
    pub outline: Outline,
}

/// A single pad position on a footprint.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PadPosition {
    pub x: f64,
    pub y: f64,
    pub pad_type: PadType,
}

/// Type of pad (e.g., through-hole, surface-mount, etc.).
#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
pub enum PadType {
    #[default]
    ThroughHole,
    SurfaceMount,
    Tented,
}

/// Outline definition for a footprint.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Outline {
    pub shape: OutlineShape,
    pub copper: bool,
}

/// Shape of the footprint outline.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum OutlineShape {
    Rectangle { x: f64, y: f64, width: f64, height: f64 },
    RoundedRectangle { x: f64, y: f64, width: f64, height: f64, radius: f64 },
}

impl Default for OutlineShape {
    fn default() -> Self {
        Self::Rectangle { x: 0.0, y: 0.0, width: 1.0, height: 1.0 }
    }
}

impl OutlineShape {
    /// Construct an axis-aligned rectangular outline.
    pub fn rectangle(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self::Rectangle { x, y, width, height }
    }

    /// Construct a rectangular outline with rounded corners of the given radius.
    pub fn rounded_rectangle(x: f64, y: f64, width: f64, height: f64, radius: f64) -> Self {
        Self::RoundedRectangle { x, y, width, height, radius }
    }
}

#[cfg(test)]
mod tests {
    use super::OutlineShape;

    #[test]
    fn rectangle_stores_dimensions() {
        match OutlineShape::rectangle(1.0, 2.0, 3.0, 4.0) {
            OutlineShape::Rectangle { x, y, width, height } => {
                assert_eq!(x, 1.0);
                assert_eq!(y, 2.0);
                assert_eq!(width, 3.0);
                assert_eq!(height, 4.0);
            }
            other => panic!("expected Rectangle, got {other:?}"),
        }
    }

    #[test]
    fn rounded_rectangle_stores_dimensions_and_radius() {
        match OutlineShape::rounded_rectangle(1.0, 2.0, 3.0, 4.0, 0.5) {
            OutlineShape::RoundedRectangle { x, y, width, height, radius } => {
                assert_eq!(x, 1.0);
                assert_eq!(y, 2.0);
                assert_eq!(width, 3.0);
                assert_eq!(height, 4.0);
                assert_eq!(radius, 0.5);
            }
            other => panic!("expected RoundedRectangle, got {other:?}"),
        }
    }
}
