//! Netlist generation and management.

use uuid::Uuid;
use serde::{Serialize, Deserialize};
use core::symbol::Symbol;

/// Netlist for a schematic design.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Netlist {
    pub id: uuid::Uuid,
    pub nets: Vec<Net>,
    pub symbols: Vec<Symbol>,
    pub connections: Vec<Connection>,
}

/// A net in the netlist.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Net {
    pub id: uuid::Uuid,
    pub name: String,
    pub pins: Vec<uuid::Uuid>,
    pub netclass: NetClass,
}

/// Electrical net class.
#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
pub enum NetClass {
    #[default] Default,
    Power,
    Signal,
    HighSpeed,
    Differential,
}

/// Connection between two pins in a netlist.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Connection {
    pub id: uuid::Uuid,
    pub source_pin: uuid::Uuid,
    pub target_pin: uuid::Uuid,
    pub wire_id: uuid::Uuid,
}
