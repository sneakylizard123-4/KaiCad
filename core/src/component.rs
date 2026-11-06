//! Component definitions and library management.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Component {
    pub id: Uuid,
    pub reference_designator: String,
    pub symbol_id: Uuid,
    pub footprint_id: Uuid,
    pub properties: Vec<ComponentProperty>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentProperty {
    pub name: String,
    pub value: String,
}