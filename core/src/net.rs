//! Net definitions for electrical connectivity.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A named electrical net connecting multiple pins.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Net {
    pub id: Uuid,
    pub name: String,
    pub pins: Vec<Uuid>,
    pub netclass: NetClass,
}

/// Electrical net class for design rules.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub enum NetClass {
    #[default]
    Default,
    Power,
    Signal,
    HighSpeed,
    Differential,
}