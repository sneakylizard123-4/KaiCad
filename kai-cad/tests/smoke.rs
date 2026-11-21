//! Workspace-level integration smoke tests.
//!
//! The root manifest is virtual and cannot host a `tests/` directory, so these
//! tests live in the `kai-cad` application crate and exercise the public API of
//! every workspace library together.

use uuid::Uuid;

fn two_pin_symbol_and_net() -> (core::Symbol, core::Net) {
    let symbol = core::Symbol {
        id: Uuid::new_v4(),
        name: "R1".to_string(),
        pins: vec![
            core::Pin {
                id: Uuid::new_v4(),
                number: "1".to_string(),
                name: "A".to_string(),
                ..core::Pin::default()
            },
            core::Pin {
                id: Uuid::new_v4(),
                number: "2".to_string(),
                name: "B".to_string(),
                ..core::Pin::default()
            },
        ],
        library: "integration".to_string(),
    };
    let net = core::Net {
        id: Uuid::new_v4(),
        name: "N1".to_string(),
        pins: symbol.pins.iter().map(|pin| pin.id).collect(),
        ..core::Net::default()
    };

    (symbol, net)
}

#[test]
fn project_json_round_trip_preserves_identity() {
    let project = core::Project::new("integration-smoke");

    let json = core::serialization::to_json_pretty(&project).expect("project serializes");
    let restored: core::Project =
        core::serialization::from_json(&json).expect("project deserializes");

    assert_eq!(restored.id, project.id);
    assert_eq!(restored.name, "integration-smoke");
    assert_eq!(restored.version, project.version);
}

#[test]
fn netlist_generation_and_erc_accept_a_clean_schematic() {
    let (symbol, net) = two_pin_symbol_and_net();

    let netlist =
        schematic::generate_netlist(std::slice::from_ref(&symbol), std::slice::from_ref(&net));

    assert_eq!(netlist.nets.len(), 1);
    assert_eq!(netlist.symbols.len(), 1);
    assert_eq!(netlist.connections.len(), 1);

    let connection = &netlist.connections[0];
    assert_eq!(connection.source_pin.symbol_id, symbol.id);
    assert_eq!(connection.source_pin.pin_number, "1");
    assert_eq!(connection.target_pin.pin_number, "2");

    let erc = schematic::run_erc(&netlist);
    assert!(erc.valid);
    assert!(erc.violations.is_empty());
}

#[test]
fn kicad_netlist_export_contains_references_and_nets() {
    let (symbol, net) = two_pin_symbol_and_net();

    let netlist =
        schematic::generate_netlist(std::slice::from_ref(&symbol), std::slice::from_ref(&net));
    let document = schematic::to_kicad_netlist(&netlist);

    assert!(document.starts_with("(export"));
    assert!(document.contains("(comp (ref \"R1\")"));
    assert!(document.contains("(name \"N1\")"));
    assert!(document.contains("(node (ref \"R1\") (pin \"1\"))"));
    assert!(document.ends_with("\n"));
}

#[test]
fn erc_rejects_duplicate_net_names() {
    let (symbol, net) = two_pin_symbol_and_net();
    let duplicate = core::Net { id: Uuid::new_v4(), ..net.clone() };

    let netlist = schematic::generate_netlist(&[symbol], &[net, duplicate]);
    let erc = schematic::run_erc(&netlist);

    assert!(!erc.valid);
    assert!(erc.violations.iter().any(|violation| violation.rule_id == "duplicate-net-name"));
}

#[test]
fn pcb_copper_layer_count_tracks_copper_layers() {
    let design = pcb::PcbDesign {
        outline: core::PcbOutline::rectangle(100.0, 80.0),
        layers: vec![
            core::PcbLayer::new(0, "F.Cu", core::LayerKind::Copper),
            core::PcbLayer::new(1, "F.SilkS", core::LayerKind::SilkScreen),
            core::PcbLayer::new(2, "B.Cu", core::LayerKind::Copper),
        ],
        ..pcb::PcbDesign::default()
    };

    assert_eq!(design.outline.points.len(), 4);
    assert!(design.outline.is_closed());
    assert_eq!(design.layers.len(), 3);
    assert_eq!(design.copper_layer_count(), 2);
}

#[test]
fn footprint_library_round_trips_through_json() {
    let library = pcb::FootprintLibrary {
        name: "Smoke".to_string(),
        footprints: vec![pcb::Footprint {
            name: "R_0402".to_string(),
            reference_designator: "R".to_string(),
            pad_positions: vec![pcb::PadPosition {
                x: -0.5,
                y: 0.0,
                pad_type: pcb::PadType::SurfaceMount,
            }],
            outline: pcb::footprint::Outline {
                shape: pcb::footprint::OutlineShape::rectangle(-1.0, -0.5, 2.0, 1.0),
                copper: false,
            },
        }],
    };

    let json = library.to_json().expect("serialize footprint library");
    assert!(json.ends_with('\n'));

    let restored = pcb::FootprintLibrary::from_json(&json).expect("parse footprint library");
    assert_eq!(restored.name, "Smoke");
    assert_eq!(restored.footprints.len(), 1);
    assert_eq!(restored.footprints[0].name, "R_0402");
    assert_eq!(restored.footprints[0].pad_positions[0].x, -0.5);
    assert_eq!(restored.footprints[0].pad_positions[0].pad_type, pcb::PadType::SurfaceMount);
    assert!(matches!(
        restored.footprints[0].outline.shape,
        pcb::footprint::OutlineShape::Rectangle { width, height, .. } if width == 2.0 && height == 1.0
    ));
}

#[test]
fn drc_accepts_a_clean_board_and_flags_a_track_outside_it() {
    let design = pcb::PcbDesign {
        outline: core::PcbOutline::rectangle(50.0, 40.0),
        layers: vec![
            core::PcbLayer::new(0, "F.Cu", core::LayerKind::Copper),
            core::PcbLayer::new(1, "B.Cu", core::LayerKind::Copper),
        ],
        stackup: core::Stackup { layers: vec![0, 1] },
        tracks: vec![pcb::Track {
            id: Uuid::new_v4(),
            net_id: Uuid::new_v4(),
            layer: 0,
            width: 0.3,
            path: vec![core::Point::new(5.0, 5.0), core::Point::new(20.0, 5.0)],
        }],
        vias: vec![pcb::Via {
            id: Uuid::new_v4(),
            position: core::Point::new(25.0, 20.0),
            layer_start: 0,
            layer_end: 1,
            ..pcb::Via::default()
        }],
        ..pcb::PcbDesign::default()
    };

    let report = pcb::run_drc(&design, &pcb::DrcRules::default());
    assert!(report.is_valid());
    assert!(report.violations.is_empty());

    let mut outside = design;
    outside.tracks[0].path = vec![core::Point::new(5.0, 5.0), core::Point::new(80.0, 5.0)];
    let report = pcb::run_drc(&outside, &pcb::DrcRules::default());

    assert!(!report.is_valid());
    assert_eq!(report.count(pcb::DrcSeverity::Error), 1);
    assert!(report.violations.iter().any(|violation| violation.rule_id == "track-outside-board"));
}

#[test]
fn ai_validation_accepts_good_and_rejects_bad_suggestion() {
    let kai = ai::kAI::new(ai::kAIConfig::default());

    let good = ai::OptimizationSuggestion {
        id: Uuid::new_v4(),
        description: "widen R1".to_string(),
        changes: vec![ai::ChangeProposal::new(
            "R1",
            "value",
            serde_json::json!(100),
            serde_json::json!(220),
        )],
        confidence_score: 0.9,
    };
    let bad = ai::OptimizationSuggestion {
        id: Uuid::new_v4(),
        description: "no-op with out-of-range confidence".to_string(),
        changes: Vec::new(),
        confidence_score: 1.5,
    };

    let good_result = kai.validate_suggestion(&good).expect("deterministic validation succeeds");
    assert!(good_result.valid);
    assert!(good_result.errors.is_empty());

    let bad_result = kai.validate_suggestion(&bad).expect("deterministic validation succeeds");
    assert!(!bad_result.valid);
    assert!(bad_result.errors.iter().any(|error| error.field == "confidence_score"));
    assert!(bad_result.errors.iter().any(|error| error.field == "changes"));
}

#[test]
fn collab_session_creation_tracks_sessions() {
    let mut manager = collab::CollabManager::new();
    let project_id = Uuid::new_v4();

    let session = manager.create_session(project_id, Uuid::new_v4());

    assert_eq!(session.project_id, project_id);
    assert_eq!(session.participants.len(), 1);
    assert_eq!(manager.session_count(), 1);

    manager.create_session(Uuid::new_v4(), Uuid::new_v4());
    assert_eq!(manager.session_count(), 2);
}

#[test]
fn store_project_crud_round_trip_in_memory() {
    let mut store = store::Store::open_in_memory().expect("open in-memory store");

    let mut project = core::Project::new("smoke-crud");
    project.schematic.nets.push(core::Net {
        id: Uuid::new_v4(),
        name: "N_SMOKE".to_string(),
        pins: Vec::new(),
        netclass: core::NetClass::Signal,
    });

    store.create_project(&project).expect("create project");

    let loaded = store.load_project(project.id).expect("load project").expect("project exists");
    assert_eq!(loaded.name, "smoke-crud");
    assert_eq!(loaded.schematic.nets.len(), 1);

    let mut updated = loaded.clone();
    updated.name = "smoke-crud-renamed".to_string();
    assert!(store.update_project(&updated).expect("update project"));

    let reloaded =
        store.load_project(project.id).expect("reload project").expect("project still exists");
    assert_eq!(reloaded.name, "smoke-crud-renamed");

    assert!(store.delete_project(project.id).expect("delete project"));
    assert!(store.load_project(project.id).expect("final load").is_none());
}

#[test]
fn kicad_symbol_library_parses_and_persists() {
    let source = r#"(kicad_symbol_lib
      (version 20231120)
      (symbol "Device:R"
        (symbol "R_1_1"
          (pin passive line (name "~") (number "1"))
          (pin passive line (name "~") (number "2"))
        )
      )
    )"#;
    let dir = std::env::temp_dir().join(format!("kaicad-smoke-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join("Device.kicad_sym");
    std::fs::write(&path, source).expect("write fixture");

    let library = kicad_parser::parse_symbol_library(&path).expect("parse library");
    assert_eq!(library.name, "Device");
    assert_eq!(library.symbols.len(), 1);
    assert_eq!(library.symbols[0].pins.len(), 2);

    let mut store = store::Store::open_in_memory().expect("open in-memory store");
    store.create_symbol_library(&library).expect("persist library");
    let loaded =
        store.load_symbol_library(library.id).expect("load library").expect("library exists");

    assert_eq!(loaded.name, "Device");
    assert_eq!(loaded.symbols.len(), 1);

    std::fs::remove_dir_all(&dir).expect("clean up temp dir");
}
