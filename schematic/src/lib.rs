//! KaiCad schematic engine — netlist generation and export, connectivity graph
//! traversal and electrical rule checking.

pub mod erc;
pub mod graph;
pub mod netlist;

pub use erc::{ERCResult, Violation, ViolationSeverity, run_erc};
pub use graph::{Edge, EdgeProperty, Graph, Node, NodeProperty, NodeType};
pub use netlist::{Connection, Netlist, PinRef, generate_netlist, to_kicad_netlist};
