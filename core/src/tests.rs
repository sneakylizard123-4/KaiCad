//! Unit tests for the core crate.

use crate::Project;
use crate::component::Component;
use crate::geometry::Point;
use crate::net::{Net, NetClass};
use crate::pcb::PcbOutline;
use crate::proposal::ChangeProposal;
use crate::serialization;
use crate::sheet::Sheet;
use crate::symbol::Symbol;
use uuid::Uuid;

#[test]
fn test_project_default() {
    let project = Project::default();
    assert_eq!(project.name, "");
    assert_eq!(project.version, "1.0");
    assert!(project.id != Uuid::nil());
}

#[test]
fn test_project_new() {
    let project = Project::new("x");

    assert_eq!(project.name, "x");
    assert_eq!(project.version, "1.0");
    assert_ne!(project.id, Uuid::nil());
}

#[test]
fn test_project_touch() {
    let mut project = Project::default();
    let before = project.metadata.modified_at;

    project.touch();

    assert!(project.metadata.modified_at >= before);
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

#[test]
fn test_change_proposal_new_and_round_trip() {
    let proposal = ChangeProposal::new(
        "component:R1",
        "value",
        serde_json::json!("10k"),
        serde_json::json!("22k"),
    );

    assert_ne!(proposal.id, Uuid::nil());
    assert_eq!(proposal.target, "component:R1");
    assert_eq!(proposal.field, "value");

    let json = serialization::to_json(&proposal).unwrap();
    let decoded: ChangeProposal = serialization::from_json(&json).unwrap();

    assert_eq!(decoded, proposal);
}

#[test]
fn test_serialization_json_round_trip() {
    let project = Project::new("json round trip");

    let json = serialization::to_json(&project).unwrap();
    let decoded: Project = serialization::from_json(&json).unwrap();

    assert_eq!(decoded.id, project.id);
    assert_eq!(decoded.name, project.name);

    let pretty = serialization::to_json_pretty(&project).unwrap();
    let decoded_pretty: Project = serialization::from_json(&pretty).unwrap();

    assert_eq!(decoded_pretty.id, project.id);
    assert!(pretty.contains('\n'));
}

#[test]
fn test_serialization_save_and_load() {
    let path = std::env::temp_dir().join(format!("kaicad-core-test-{}.json", Uuid::new_v4()));
    let mut project = Project::new("saved project");
    project.pcb.footprint_ids.push(Uuid::new_v4());

    project.save(&path).unwrap();
    let decoded = Project::load(&path).unwrap();
    std::fs::remove_file(&path).unwrap();

    assert_eq!(decoded.id, project.id);
    assert_eq!(decoded.name, "saved project");
    assert_eq!(decoded.pcb.footprint_ids, project.pcb.footprint_ids);
}

#[test]
fn test_pcb_outline_rectangle() {
    let outline = PcbOutline::rectangle(100.0, 50.0);

    assert_eq!(outline.points.len(), 4);
    assert!(outline.is_closed());
    assert_eq!(outline.points[0], Point::new(0.0, 0.0));
    assert_eq!(outline.points[2], Point::new(100.0, 50.0));
    assert!(!PcbOutline::new(vec![Point::new(0.0, 0.0)]).is_closed());
}
