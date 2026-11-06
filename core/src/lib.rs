//! KaiCad core library — shared types, enums, and traits used across all crates.

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
pub mod project;
pub mod component;
pub mod symbol;
pub mod net;
pub mod sheet;

/// Root project data structure containing all design data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub version: String,
    pub schematic: SchematicData,
    pub pcb: PcbData,
    pub metadata: ProjectMetadata,
}

impl Default for Project {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            name: String::new(),
            version: "1.0".to_string(),
            schematic: SchematicData::default(),
            pcb: PcbData::default(),
            metadata: ProjectMetadata::default(),
        }
    }
}

/// High-level schematic design data.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SchematicData {
    pub sheets: Vec<sheet::Sheet>,
    pub symbols: Vec<symbol::Symbol>,
    pub nets: Vec<net::Net>,
}

/// High-level PCB design data.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PcbData {
    pub outline: PcbOutline,
    pub layers: Vec<PcbLayer>,
    pub stackup: Stackup,
    pub footprints: Vec<pcb::Footprint>,
}

/// Project metadata for persistence and collaboration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMetadata {
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
    pub author: String,
    pub description: String,
}

impl Default for ProjectMetadata {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            created_at: now,
            modified_at: now,
            author: String::new(),
            description: String::new(),
        }
    }
}

/// Placeholder for PCB outline geometry.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PcbOutline {}

/// Placeholder for a PCB layer definition.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PcbLayer {}

/// Placeholder for PCB layer stackup definition.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Stackup {}

pub mod pcb {
    //! PCB-specific core types.

    use super::{Serialize, Deserialize};

    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    pub struct Footprint {}
}

#[cfg(test)]
mod tests;