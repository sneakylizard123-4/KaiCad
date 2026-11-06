//! PCB layer definitions.

use serde::{Serialize, Deserialize};

/// A PCB layer (copper, silk screen, mask, etc.).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct LayerDefinition {
    pub name: String,
    pub r#type: LayerType,
    pub copper: bool,
}

/// Type of PCB layer.
#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
pub enum LayerType {
    #[default]
    Copper,
    SilkScreen,
    Mask,
    Other,
}