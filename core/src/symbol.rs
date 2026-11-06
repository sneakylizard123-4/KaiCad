//! Symbol definitions for schematic components.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A schematic symbol with pins and visual representation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Symbol {
    pub id: Uuid,
    pub name: String,
    pub pins: Vec<Pin>,
    pub library: String,
}

/// A single pin on a schematic symbol.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Pin {
    pub id: Uuid,
    pub number: String,
    pub name: String,
    pub electrical_type: PinType,
    pub position: PinPosition,
    pub direction: PinDirection,
}

/// Electrical type of a pin.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub enum PinType {
    #[default]
    Unspecified,
    Input,
    Output,
    Bidirectional,
    Passive,
    Power,
    OpenCollector,
    OpenEmitter,
    TriState,
}

/// Position of a pin on a symbol.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct PinPosition {
    pub x: f64,
    pub y: f64,
    pub angle: f64,
}

/// Direction of a pin on a symbol.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub enum PinDirection {
    #[default]
    In,
    Out,
    InOut,
    Unspecified,
}