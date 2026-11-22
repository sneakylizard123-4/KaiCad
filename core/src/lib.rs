//! KaiCad core library — shared types, enums, and traits used across all crates.

pub mod component;
pub mod edit;
pub mod geometry;
pub mod history;
pub mod net;
pub mod pcb;
pub mod project;
pub mod proposal;
pub mod serialization;
pub mod sheet;
pub mod source;
pub mod symbol;
pub mod symbol_library;

pub use component::{Component, ComponentPlacement, ComponentProperty};
pub use edit::{AppliedChange, EditError, EditTransaction};
pub use geometry::Point;
pub use history::{EditHistory, HistoryError};
pub use net::{Net, NetClass};
pub use pcb::{BoundingBox, LayerKind, PcbLayer, PcbOutline, Stackup};
pub use project::{PcbData, Project, ProjectMetadata, SchematicData};
pub use proposal::ChangeProposal;
pub use sheet::{Junction, Label, LabelPosition, LabelType, NoConnect, Sheet, Wire, WirePoint};
pub use source::{PreservedNode, SourceDocument};
pub use symbol::{Pin, PinDirection, PinPosition, PinType, Symbol};
pub use symbol_library::{SymbolLibrary, SymbolLibrarySource};

#[cfg(test)]
mod tests;
