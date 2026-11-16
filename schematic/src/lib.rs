//! KaiCad schematic engine — netlist generation, symbol management, ERC.

pub mod erc;
pub mod graph;
pub mod netlist;

pub use erc::{ERCResult, Violation, ViolationSeverity, run_erc};
pub use graph::{Edge, EdgeProperty, Graph, Node, NodeProperty, NodeType};
pub use netlist::{Connection, Netlist, PinRef, generate_netlist};
