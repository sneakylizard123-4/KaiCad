//! KaiCad schematic engine — netlist generation, symbol management, ERC.

pub mod netlist;
pub mod graph;
pub mod erc;

use core::{symbol::Symbol, net::Net};
use uuid::Uuid;
use serde::{Serialize, Deserialize};

/// Represents a complete netlist extracted from a schematic.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Netlist {
    pub id: uuid::Uuid,
    pub nets: Vec<Net>,
    pub symbols: Vec<Symbol>,
    pub connections: Vec<Connection>,
}

impl Default for Netlist {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            nets: Vec::new(),
            symbols: Vec::new(),
            connections: Vec::new(),
        }
    }
}

/// A single connection between two pins in the schematic.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Connection {
    pub source_pin: PinRef,
    pub target_pin: PinRef,
    pub wire_id: uuid::Uuid,
}

/// Reference to a pin on a symbol instance.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PinRef {
    pub symbol_id: uuid::Uuid,
    pub pin_number: String,
}

/// Generate a netlist from schematic data.
pub fn generate_netlist(_symbols: &[Symbol], _nets: &[Net]) -> Netlist {
    Netlist::default()
}
