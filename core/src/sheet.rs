//! Schematic sheet definitions for hierarchical designs.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A single schematic sheet in a hierarchical design.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Sheet {
    pub id: Uuid,
    pub name: String,
    pub parent: Option<Uuid>,
    pub children: Vec<Uuid>,
    pub components: Vec<Uuid>,
    pub wires: Vec<Wire>,
    pub labels: Vec<Label>,
    /// Source schematic filename for a child sheet; empty for the root sheet.
    #[serde(default)]
    pub file_name: String,
    /// Sheet-symbol placement in schematic millimetres.
    #[serde(default)]
    pub position: crate::geometry::Point,
    /// Sheet-symbol size in schematic millimetres.
    #[serde(default)]
    pub size: crate::geometry::Point,
    /// Electrical junctions on this sheet.
    #[serde(default)]
    pub junctions: Vec<Junction>,
    /// Explicit no-connect markers on this sheet.
    #[serde(default)]
    pub no_connects: Vec<NoConnect>,
    /// Source constructs with no KaiCad model field, retained for export.
    #[serde(default)]
    pub preserved: Vec<crate::PreservedNode>,
    /// Source document for a child sheet file, retained for bundle export.
    #[serde(default)]
    pub source: Option<crate::SourceDocument>,
}

/// An electrical junction where three or more wires meet.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Junction {
    pub id: Uuid,
    pub position: crate::geometry::Point,
    /// Source diameter, colour and other unmodelled fields.
    #[serde(default)]
    pub preserved: Vec<crate::PreservedNode>,
}

/// An explicit no-connect marker.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NoConnect {
    pub id: Uuid,
    pub position: crate::geometry::Point,
    /// Source unmodelled fields.
    #[serde(default)]
    pub preserved: Vec<crate::PreservedNode>,
}

/// A wire connecting two points on a schematic sheet.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Wire {
    pub id: uuid::Uuid,
    pub points: Vec<WirePoint>,
    /// Source stroke and other unmodelled fields.
    #[serde(default)]
    pub preserved: Vec<crate::PreservedNode>,
}

/// A point on a wire.
pub type WirePoint = crate::geometry::Point;

/// A label on a schematic sheet (net naming).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Label {
    pub id: uuid::Uuid,
    pub name: String,
    pub label_type: LabelType,
    pub position: LabelPosition,
    /// Source effects and other unmodelled fields.
    #[serde(default)]
    pub preserved: Vec<crate::PreservedNode>,
}

/// Type of label.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub enum LabelType {
    #[default]
    Local,
    Global,
    Hierarchical,
    Power,
}

/// Position of a label on a schematic sheet.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct LabelPosition {
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
}
