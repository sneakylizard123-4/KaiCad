//! Named symbol libraries, independent of any one project.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::symbol::Symbol;

/// A named symbol library containing zero or more symbols.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SymbolLibrary {
    pub id: Uuid,
    pub name: String,
    pub source: SymbolLibrarySource,
    pub symbols: Vec<Symbol>,
}

/// Where a library came from.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SymbolLibrarySource {
    /// Created inside KaiCad.
    #[default]
    Internal,
    /// Imported from a KiCad `.kicad_sym` file.
    KiCad { path: String },
}

impl SymbolLibrary {
    /// Create an empty internal library with a fresh id.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            source: SymbolLibrarySource::Internal,
            symbols: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SymbolLibrary, SymbolLibrarySource};
    use crate::serialization::{from_json, to_json_pretty};
    use crate::symbol::{Pin, Symbol};
    use uuid::Uuid;

    #[test]
    fn library_json_round_trip_preserves_identity_and_source() {
        let sync_pin = Pin {
            id: Uuid::new_v4(),
            number: "1".to_string(),
            name: "CLK".to_string(),
            ..Pin::default()
        };
        let library = SymbolLibrary {
            id: Uuid::new_v4(),
            name: "Device".to_string(),
            source: SymbolLibrarySource::KiCad { path: "/libs/Device.kicad_sym".to_string() },
            symbols: vec![Symbol {
                id: Uuid::new_v4(),
                name: "Device:R".to_string(),
                pins: vec![sync_pin],
                library: "Device".to_string(),
            }],
        };

        let json = to_json_pretty(&library).expect("library serializes");
        let restored: SymbolLibrary = from_json(&json).expect("library deserializes");

        assert_eq!(restored.id, library.id);
        assert_eq!(restored.name, library.name);
        assert_eq!(restored.source, library.source);
        assert_eq!(restored.symbols.len(), 1);
        assert_eq!(restored.symbols[0].id, library.symbols[0].id);
        assert_eq!(restored.symbols[0].pins[0].name, "CLK");
    }

    #[test]
    fn new_library_is_empty_and_internal() {
        let library = SymbolLibrary::new("Scratch");

        assert_eq!(library.name, "Scratch");
        assert_eq!(library.source, SymbolLibrarySource::Internal);
        assert!(library.symbols.is_empty());
    }
}
