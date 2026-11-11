//! Unit tests for the core crate.

use crate::Project;
use crate::Uuid;
use crate::component::Component;
use crate::net::{Net, NetClass};
use crate::sheet::Sheet;
use crate::symbol::Symbol;

#[test]
fn test_project_default() {
    let project = Project::default();
    assert_eq!(project.name, "");
    assert_eq!(project.version, "1.0");
    assert!(project.id != Uuid::nil());
}

#[test]
fn test_project_serialization() {
    let project = Project {
        name: "Test Project".to_string(),
        version: "0.1.0".to_string(),
        ..Default::default()
    };

    let serialized = serde_json::to_string(&project).unwrap();
    let deserialized: Project = serde_json::from_str(&serialized).unwrap();

    assert_eq!(deserialized.name, "Test Project");
    assert_eq!(deserialized.version, "0.1.0");
    assert_eq!(project.id, deserialized.id);
}

#[test]
fn test_symbol_creation() {
    let symbol = Symbol {
        id: Uuid::new_v4(),
        name: "TEST_RESISTOR".to_string(),
        library: "Device".to_string(),
        ..Default::default()
    };

    assert_eq!(symbol.name, "TEST_RESISTOR");
    assert_eq!(symbol.library, "Device");
    assert!(symbol.id != Uuid::nil());
}

#[test]
fn test_net_creation() {
    let net = Net {
        id: Uuid::new_v4(),
        name: "VCC".to_string(),
        netclass: NetClass::Power,
        ..Default::default()
    };

    assert_eq!(net.name, "VCC");
    assert_eq!(net.netclass, NetClass::Power);
    assert!(net.id != Uuid::nil());
}

#[test]
fn test_sheet_creation() {
    let sheet = Sheet { id: Uuid::new_v4(), name: "Sheet1".to_string(), ..Default::default() };

    assert_eq!(sheet.name, "Sheet1");
    assert!(sheet.id != Uuid::nil());
    assert!(sheet.parent.is_none());
    assert!(sheet.children.is_empty());
}

#[test]
fn test_component_creation() {
    let component = Component {
        id: Uuid::new_v4(),
        reference_designator: "R1".to_string(),
        symbol_id: Uuid::new_v4(),
        footprint_id: Uuid::new_v4(),
        ..Default::default()
    };

    assert_eq!(component.reference_designator, "R1");
    assert!(component.id != Uuid::nil());
    assert!(component.symbol_id != Uuid::nil());
    assert!(component.footprint_id != Uuid::nil());
}
