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
