//! Root project data structures and operations.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::component::Component;
use crate::net::Net;
use crate::pcb::{PcbLayer, PcbOutline, Stackup};
use crate::sheet::Sheet;
use crate::symbol::Symbol;

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

impl Project {
    /// Create a new, empty project with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), ..Self::default() }
    }

    /// Mark the project as modified now.
    pub fn touch(&mut self) {
        self.metadata.modified_at = Utc::now();
    }

    /// Find the root schematic sheet, if the project has one.
    ///
    /// A root sheet is a sheet with no parent. If several sheets qualify, the
    /// first one in insertion order is returned.
    pub fn root_sheet(&self) -> Option<&Sheet> {
        self.schematic.sheets.iter().find(|sheet| sheet.parent.is_none())
    }
}

/// High-level schematic design data.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SchematicData {
    pub sheets: Vec<Sheet>,
    pub symbols: Vec<Symbol>,
    pub nets: Vec<Net>,
    pub components: Vec<Component>,
    /// Opaque source constructs retained for lossless KiCad interchange.
    #[serde(default)]
    pub source: Option<crate::source::SourceDocument>,
}

/// High-level PCB design data.
///
/// Footprints themselves are owned by the `pcb` crate; the project only stores
/// their identifiers so that the core crate stays dependency-free.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PcbData {
    pub outline: PcbOutline,
    pub layers: Vec<PcbLayer>,
    pub stackup: Stackup,
    pub footprint_ids: Vec<Uuid>,
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
