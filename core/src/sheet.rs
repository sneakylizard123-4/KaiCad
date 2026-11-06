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
}

/// A wire connecting two points on a schematic sheet.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Wire {
    pub id: uuid::Uuid,
    pub points: Vec<WirePoint>,
}

/// A point on a wire.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct WirePoint {
    pub x: f64,
    pub y: f64,
}

/// A label on a schematic sheet (net naming).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Label {
    pub id: uuid::Uuid,
    pub name: String,
    pub label_type: LabelType,
    pub position: LabelPosition,
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