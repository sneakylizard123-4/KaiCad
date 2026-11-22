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
                ..pcb::PadPosition::default()
            }],
            outline: pcb::footprint::Outline {
                shape: pcb::footprint::OutlineShape::rectangle(-1.0, -0.5, 2.0, 1.0),
                copper: false,
            },
            ..pcb::Footprint::default()
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

    let report = pcb::run_drc(&design, &design.rules);
    assert!(report.is_valid());
    assert!(report.violations.is_empty());

    let mut outside = design;
    outside.tracks[0].path = vec![core::Point::new(5.0, 5.0), core::Point::new(80.0, 5.0)];
    let report = pcb::run_drc(&outside, &outside.rules);

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
fn edit_history_supports_multi_step_undo_redo_and_branching() {
    let mut project = core::Project::new("smoke-history");
    project.schematic.components.push(core::Component {
        id: Uuid::from_u128(500),
        reference_designator: "R1".to_string(),
        properties: vec![core::ComponentProperty {
            name: "Value".to_string(),
            value: "10k".to_string(),
        }],
        ..core::Component::default()
    });
    let mut history = core::EditHistory::default();
    for (label, old, new) in [("one", "10k", "22k"), ("two", "22k", "33k"), ("three", "33k", "47k")]
    {
        history
            .apply(
                &mut project,
                label,
                &[core::ChangeProposal::new(
                    "component:R1",
                    "value",
                    serde_json::json!(old),
                    serde_json::json!(new),
                )],
            )
            .expect("history apply");
    }

    history.undo(&mut project).expect("undo three");
    history.undo(&mut project).expect("undo two");
    assert_eq!(project.schematic.components[0].properties[0].value, "22k");
    assert_eq!(history.undo_len(), 1);
    assert_eq!(history.redo_len(), 2);

    history.redo(&mut project).expect("redo two");
    assert_eq!(project.schematic.components[0].properties[0].value, "33k");

    history
        .apply(
            &mut project,
            "branch",
            &[core::ChangeProposal::new(
                "component:R1",
                "value",
                serde_json::json!("33k"),
                serde_json::json!("56k"),
            )],
        )
        .expect("branch apply");
    assert_eq!(history.redo_len(), 0);
    assert_eq!(history.undo_label(), Some("branch"));

    let undo_len = history.undo_len();
    assert!(
        history
            .apply(
                &mut project,
                "stale",
                &[core::ChangeProposal::new(
                    "component:R1",
                    "value",
                    serde_json::json!("99k"),
                    serde_json::json!("1k"),
                )],
            )
            .is_err()
    );
    assert_eq!(history.undo_len(), undo_len);

    let mut bounded = core::EditHistory::new(2).expect("valid limit");
    for (label, old, new) in
        [("bounded-1", "56k", "68k"), ("bounded-2", "68k", "82k"), ("bounded-3", "82k", "91k")]
    {
        bounded
            .apply(
                &mut project,
                label,
                &[core::ChangeProposal::new(
                    "component:R1",
                    "value",
                    serde_json::json!(old),
                    serde_json::json!(new),
                )],
            )
            .expect("bounded apply");
    }
    assert_eq!(bounded.undo_len(), 2);
    assert_eq!(bounded.undo_label(), Some("bounded-3"));
    bounded.undo(&mut project).expect("undo bounded three");
    bounded.undo(&mut project).expect("undo bounded two");
    assert_eq!(project.schematic.components[0].properties[0].value, "68k");
    assert!(!bounded.can_undo());
    assert_eq!(bounded.redo_len(), 2);
}

#[test]
fn collab_edits_enforce_roles_and_track_versions() {
    let owner = Uuid::from_u128(10);
    let editor = Uuid::from_u128(11);
    let viewer = Uuid::from_u128(12);
    let mut manager = collab::CollabManager::new();
    let session = manager.create_session(Uuid::from_u128(1), owner);
    let session_id = session.session_id;
    for (user_id, username, role) in [
        (editor, "editor", collab::ParticipantRole::Editor),
        (viewer, "viewer", collab::ParticipantRole::Viewer),
    ] {
        manager
            .add_participant(
                session_id,
                collab::Participant { user_id, username: username.to_string(), role },
            )
            .expect("add participant");
    }

    let mut project = core::Project::new("smoke-collab");
    project.schematic.components.push(core::Component {
        id: Uuid::from_u128(100),
        reference_designator: "R1".to_string(),
        properties: vec![core::ComponentProperty {
            name: "Value".to_string(),
            value: "10k".to_string(),
        }],
        ..core::Component::default()
    });
    let before = serde_json::to_string(&project).expect("serialize start");
    let changes = [core::ChangeProposal::new(
        "component:R1",
        "value",
        serde_json::json!("10k"),
        serde_json::json!("22k"),
    )];

    assert!(manager.can_edit(session_id, owner));
    assert!(manager.can_edit(session_id, editor));
    assert!(!manager.can_edit(session_id, viewer));
    assert!(matches!(
        manager.apply_edit(session_id, viewer, &mut project, "viewer", &changes),
        Err(collab::CollabError::ReadOnly)
    ));
    assert_eq!(manager.sessions[0].version, 0);
    assert_eq!(before, serde_json::to_string(&project).expect("serialize after reject"));

    let transaction = manager
        .apply_edit(session_id, editor, &mut project, "editor edit", &changes)
        .expect("editor edit");
    assert_eq!(manager.sessions[0].version, 1);
    assert_eq!(project.schematic.components[0].properties[0].value, "22k");

    let mut history = collab::VersioningManager::new();
    assert_eq!(
        history
            .record_transaction(editor, "editor edit", &transaction, &project)
            .expect("record version"),
        1
    );
    let entry = history.find_by_transaction(transaction.id).expect("version exists");
    assert!(entry.project_digest.is_some_and(|digest| !digest.is_nil()));

    let mut sync = collab::sync::SyncManager::default();
    assert!(sync.stage(transaction.clone()));
    assert!(!sync.stage(transaction.clone()));
    let drained = sync.drain();
    assert_eq!(drained.len(), 1);
    assert_eq!(drained[0].id, transaction.id);
    assert_eq!(sync.staged_count(), 0);

    core::edit::undo(&mut project, &transaction).expect("undo");
    assert_eq!(before, serde_json::to_string(&project).expect("serialize undo"));
}

#[test]
fn kai_suggestion_application_is_atomic_and_reversible() {
    let mut project = core::Project::new("smoke-ai");
    project.schematic.components.push(core::Component {
        id: Uuid::from_u128(1),
        reference_designator: "R1".to_string(),
        properties: vec![core::ComponentProperty {
            name: "Value".to_string(),
            value: "10k".to_string(),
        }],
        ..core::Component::default()
    });
    let before = serde_json::to_string(&project).expect("serialize start");
    let kai = ai::kAI::new(ai::kAIConfig::default());
    let suggestion = ai::OptimizationSuggestion {
        id: Uuid::new_v4(),
        description: "smoke value update".to_string(),
        changes: vec![ai::ChangeProposal::new(
            "component:R1",
            "value",
            serde_json::json!("10k"),
            serde_json::json!("22k"),
        )],
        confidence_score: 0.9,
    };

    let applied = kai.apply_suggestion(&mut project, &suggestion).expect("apply suggestion");
    assert_eq!(project.schematic.components[0].properties[0].value, "22k");
    assert_eq!(applied.transaction.id.get_version_num(), 5);

    core::edit::undo(&mut project, &applied.transaction).expect("undo");
    assert_eq!(before, serde_json::to_string(&project).expect("serialize undo"));

    core::edit::redo(&mut project, &applied.transaction).expect("redo");
    assert_eq!(project.schematic.components[0].properties[0].value, "22k");

    let mut repeated = core::Project::new("smoke-ai");
    repeated.schematic.components = project.schematic.components.clone();
    repeated.schematic.components[0].properties[0].value = "10k".to_string();
    let repeat = kai.apply_suggestion(&mut repeated, &suggestion).expect("repeat apply");
    assert_eq!(applied.transaction.id, repeat.transaction.id);

    let stale = ai::OptimizationSuggestion {
        changes: vec![ai::ChangeProposal::new(
            "component:R1",
            "value",
            serde_json::json!("1k"),
            serde_json::json!("33k"),
        )],
        ..suggestion
    };
    let edited = serde_json::to_string(&project).expect("serialize edited");
    assert!(kai.apply_suggestion(&mut project, &stale).is_err());
    assert_eq!(edited, serde_json::to_string(&project).expect("serialize after failure"));
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

    let design = pcb::PcbDesign {
        outline: core::PcbOutline::rectangle(50.0, 40.0),
        layers: vec![core::PcbLayer::new(0, "F.Cu", core::LayerKind::Copper)],
        stackup: core::Stackup { layers: vec![0] },
        rules: pcb::DrcRules { clearance: 0.3, ..pcb::DrcRules::default() },
        ..pcb::PcbDesign::default()
    };
    store.save_pcb_design(project.id, &design).expect("save pcb design");
    let loaded_design =
        store.load_pcb_design(project.id).expect("load pcb design").expect("pcb design exists");
    assert_eq!(loaded_design.rules, design.rules);

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
    assert!(store.load_pcb_design(project.id).expect("final design load").is_none());
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

#[test]
fn multilayer_routing_crosses_a_top_layer_wall_and_repeats_deterministically() {
    let design = pcb::PcbDesign {
        outline: core::PcbOutline::rectangle(40.0, 30.0),
        layers: vec![
            core::PcbLayer::new(0, "F.Cu", core::LayerKind::Copper),
            core::PcbLayer::new(1, "B.Cu", core::LayerKind::Copper),
        ],
        stackup: core::Stackup { layers: vec![0, 1] },
        tracks: vec![pcb::Track {
            id: Uuid::new_v4(),
            net_id: Uuid::new_v4(),
            layer: 0,
            width: 0.25,
            path: vec![core::Point::new(0.0, 15.0), core::Point::new(40.0, 15.0)],
        }],
        ..pcb::PcbDesign::default()
    };
    let request = pcb::routing::RouteRequest {
        net_id: Uuid::new_v4(),
        start: core::Point::new(5.0, 5.0),
        end: core::Point::new(35.0, 25.0),
        layer: 0,
        width: 0.25,
    };
    let config = pcb::routing::RouterConfig::default();

    let result = pcb::routing::route_multilayer(&request, &design, &config)
        .expect("route across the wall using a via");
    assert!(!result.vias.is_empty());
    assert!(result.tracks.len() >= 2);
    assert!(result.tracks.iter().any(|track| track.layer == 1));

    let mut routed = design.clone();
    routed.tracks.extend(result.tracks.clone());
    routed.vias.extend(result.vias.clone());
    assert!(pcb::run_drc(&routed, &routed.rules).is_valid());

    let repeated =
        pcb::routing::route_multilayer(&request, &design, &config).expect("repeat route");
    assert_eq!(
        serde_json::to_string(&result).expect("serialize first route"),
        serde_json::to_string(&repeated).expect("serialize repeated route")
    );

    let no_vias = pcb::routing::RouterConfig { allow_vias: false, ..config };
    assert!(pcb::routing::route_multilayer(&request, &design, &no_vias).is_err());
}

#[test]
fn routing_produces_a_drc_clean_track_and_reports_a_blocked_path() {
    let design = pcb::PcbDesign {
        outline: core::PcbOutline::rectangle(40.0, 30.0),
        layers: vec![
            core::PcbLayer::new(0, "F.Cu", core::LayerKind::Copper),
            core::PcbLayer::new(1, "B.Cu", core::LayerKind::Copper),
        ],
        stackup: core::Stackup { layers: vec![0, 1] },
        ..pcb::PcbDesign::default()
    };
    let request = pcb::routing::RouteRequest {
        net_id: Uuid::new_v4(),
        start: core::Point::new(3.0, 3.0),
        end: core::Point::new(37.0, 27.0),
        layer: 0,
        width: 0.25,
    };
    let config = pcb::routing::RouterConfig::default();

    let tracks = pcb::routing::route(&request, &design, &config).expect("route clean board");
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].path.first(), Some(&core::Point::new(3.0, 3.0)));
    assert_eq!(tracks[0].path.last(), Some(&core::Point::new(37.0, 27.0)));

    let mut routed = design.clone();
    routed.tracks.extend(tracks);
    assert!(pcb::run_drc(&routed, &routed.rules).is_valid());

    let mut blocked = pcb::PcbDesign {
        tracks: vec![
            pcb::Track {
                id: Uuid::new_v4(),
                net_id: Uuid::new_v4(),
                layer: 0,
                width: 0.25,
                path: vec![core::Point::new(5.0, 5.0), core::Point::new(10.0, 5.0)],
            },
            pcb::Track {
                id: Uuid::new_v4(),
                net_id: Uuid::new_v4(),
                layer: 0,
                width: 0.25,
                path: vec![core::Point::new(10.0, 5.0), core::Point::new(10.0, 10.0)],
            },
            pcb::Track {
                id: Uuid::new_v4(),
                net_id: Uuid::new_v4(),
                layer: 0,
                width: 0.25,
                path: vec![core::Point::new(10.0, 10.0), core::Point::new(5.0, 10.0)],
            },
            pcb::Track {
                id: Uuid::new_v4(),
                net_id: Uuid::new_v4(),
                layer: 0,
                width: 0.25,
                path: vec![core::Point::new(5.0, 10.0), core::Point::new(5.0, 5.0)],
            },
        ],
        ..design
    };
    let blocked_request = pcb::routing::RouteRequest {
        start: core::Point::new(7.0, 7.0),
        end: core::Point::new(30.0, 20.0),
        ..request
    };
    assert!(pcb::routing::route(&blocked_request, &blocked, &config).is_err());
    let _ = &mut blocked;
}
