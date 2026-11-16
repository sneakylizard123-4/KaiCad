//! KaiCad core library — shared types, enums, and traits used across all crates.

pub mod component;
pub mod geometry;
pub mod net;
pub mod pcb;
pub mod project;
pub mod proposal;
pub mod serialization;
pub mod sheet;
pub mod symbol;

pub use component::{Component, ComponentProperty};
pub use geometry::Point;
pub use net::{Net, NetClass};
pub use pcb::{LayerKind, PcbLayer, PcbOutline, Stackup};
pub use project::{PcbData, Project, ProjectMetadata, SchematicData};
pub use proposal::ChangeProposal;
pub use sheet::{Label, LabelPosition, LabelType, Sheet, Wire, WirePoint};
pub use symbol::{Pin, PinDirection, PinPosition, PinType, Symbol};

#[cfg(test)]
mod tests;
