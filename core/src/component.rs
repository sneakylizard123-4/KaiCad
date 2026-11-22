//! Component definitions and library management.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Point;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Component {
    pub id: Uuid,
    pub reference_designator: String,
    pub symbol_id: Uuid,
    pub footprint_id: Uuid,
    pub properties: Vec<ComponentProperty>,
    /// Schematic placement of this component.
    #[serde(default)]
    pub placement: ComponentPlacement,
    /// Library identifier from a source file, for example `Device:R`.
    #[serde(default)]
    pub library_id: String,
    /// Source constructs with no KaiCad model field, retained for export.
    #[serde(default)]
    pub preserved: Vec<crate::PreservedNode>,
}

/// Position, rotation and unit of a placed schematic component.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct ComponentPlacement {
    /// Placement origin in schematic millimetres.
    pub position: Point,
    /// Rotation in degrees, counter-clockwise.
    pub rotation: f64,
    /// Symbol unit number; zero means unspecified.
    pub unit: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentProperty {
    pub name: String,
    pub value: String,
}
