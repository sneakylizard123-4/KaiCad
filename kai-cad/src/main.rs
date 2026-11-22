//! KaiCad main entry point.
//!
//! The GUI is not wired up yet, so the binary acts as a deterministic smoke
//! test of the workspace's public library surface: it serializes a project,
//! generates and checks a netlist, builds a PCB design, and reads the default
//! configuration of the renderer, AI, and collaboration subsystems.

use std::process::ExitCode;

use uuid::Uuid;

const USAGE: &str = "kai-cad [--version|--help]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.as_slice() {
        [] => run_self_check(),
        [flag] if flag == "--help" || flag == "-h" => {
            print_help();
            ExitCode::SUCCESS
        }
        [flag] if flag == "--version" || flag == "-V" => {
            println!("kai-cad {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("error: unrecognized arguments: {}", args.join(" "));
            eprintln!("usage: {USAGE}");
            ExitCode::FAILURE
        }
    }
}

fn print_help() {
    println!("KaiCad — modern EDA application in Rust (pre-alpha).");
    println!();
    println!("usage: {USAGE}");
    println!();
    println!("  -h, --help     print this help and exit");
    println!("  -V, --version  print the version and exit");
    println!();
    println!("With no arguments, runs a deterministic self-check of every workspace subsystem.");
}

fn run_self_check() -> ExitCode {
    println!("KaiCad — Modern EDA Application in Rust");
    println!("Version: {} (pre-alpha)", env!("CARGO_PKG_VERSION"));
    println!("License: GPL-3.0-or-later");
    println!();
    println!("subsystem self-check:");

    match self_check() {
        Ok(()) => {
            println!("self-check: ok");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("self-check: FAILED: {error}");
            ExitCode::FAILURE
        }
    }
}

fn self_check() -> Result<(), String> {
    check_core()?;
    check_plugin()?;
    check_schematic()?;
    check_pcb()?;
    check_renderer()?;
    check_ai()?;
    check_collab()?;
    check_store()?;
    check_kicad_parser()?;
    Ok(())
}

/// Fail the self-check with `message` unless `condition` holds.
fn ensure(condition: bool, message: &str) -> Result<(), String> {
    if condition { Ok(()) } else { Err(message.to_string()) }
}

fn check_core() -> Result<(), String> {
    let project = core::Project::new("self-check");
    let json = core::serialization::to_json_pretty(&project)
        .map_err(|error| format!("core: project serialization failed: {error}"))?;
    let restored: core::Project = core::serialization::from_json(&json)
        .map_err(|error| format!("core: project deserialization failed: {error}"))?;

    ensure(restored.id == project.id, "core: project id changed across a JSON round trip")?;
    ensure(restored.name == project.name, "core: project name changed across a JSON round trip")?;

    let mut history_project = core::Project::new("history-self-check");
    history_project.schematic.components.push(core::Component {
        id: Uuid::new_v4(),
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
                &mut history_project,
                label,
                &[core::ChangeProposal::new(
                    "component:R1",
                    "value",
                    serde_json::json!(old),
                    serde_json::json!(new),
                )],
            )
            .map_err(|error| format!("core: history apply failed: {error}"))?;
    }
    history.undo(&mut history_project).map_err(|error| format!("core: undo failed: {error}"))?;
    history.undo(&mut history_project).map_err(|error| format!("core: undo failed: {error}"))?;
    ensure(
        history_project.schematic.components[0].properties[0].value == "22k",
        "core: two undos did not restore the earlier value",
    )?;
    history.redo(&mut history_project).map_err(|error| format!("core: redo failed: {error}"))?;
    ensure(
        history_project.schematic.components[0].properties[0].value == "33k",
        "core: redo did not advance the value",
    )?;
    history
        .apply(
            &mut history_project,
            "branch",
            &[core::ChangeProposal::new(
                "component:R1",
                "value",
                serde_json::json!("33k"),
                serde_json::json!("56k"),
            )],
        )
        .map_err(|error| format!("core: branch apply failed: {error}"))?;
    ensure(history.redo_len() == 0, "core: new edit did not clear the redo branch")?;
    ensure(history.undo_label() == Some("branch"), "core: undo label is not the branch edit")?;

    let mut bounded = core::EditHistory::new(2)
        .map_err(|error| format!("core: invalid history limit: {error}"))?;
    for (label, old, new) in
        [("bounded-1", "56k", "68k"), ("bounded-2", "68k", "82k"), ("bounded-3", "82k", "91k")]
    {
        bounded
            .apply(
                &mut history_project,
                label,
                &[core::ChangeProposal::new(
                    "component:R1",
                    "value",
                    serde_json::json!(old),
                    serde_json::json!(new),
                )],
            )
            .map_err(|error| format!("core: bounded history apply failed: {error}"))?;
    }
    ensure(bounded.undo_len() == 2, "core: history limit did not bound the undo stack")?;
    bounded.undo(&mut history_project).map_err(|error| format!("core: undo failed: {error}"))?;
    bounded.undo(&mut history_project).map_err(|error| format!("core: undo failed: {error}"))?;
    ensure(!bounded.can_undo(), "core: evicted history entry was still undoable")?;

    println!(
        "  core: project JSON round trip ok ({} bytes), history {} undo / {} redo, limit {}",
        json.len(),
        history.undo_len(),
        history.redo_len(),
        bounded.limit()
    );
    Ok(())
}

fn check_plugin() -> Result<(), String> {
    let directory =
        std::env::temp_dir().join(format!("kaicad-plugin-self-check-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("plugin: failed to create fixture directory: {error}"))?;

    let outcome = (|| -> Result<(usize, usize), String> {
        let alpha = plugin::PluginManifest {
            id: "com.kaicad.alpha".to_string(),
            name: "Alpha self-check plugin".to_string(),
            version: "0.1.0".to_string(),
            api_version: plugin::SUPPORTED_API_VERSION,
            entry_point: "plugins/alpha.so".to_string(),
            capabilities: vec![plugin::PluginCapability::Schematic, plugin::PluginCapability::Ai],
        };
        let beta = plugin::PluginManifest {
            id: "com.kaicad.beta".to_string(),
            name: "Beta self-check plugin".to_string(),
            version: "1.2.0".to_string(),
            api_version: plugin::SUPPORTED_API_VERSION,
            entry_point: "plugins/beta.so".to_string(),
            capabilities: vec![plugin::PluginCapability::Pcb],
        };
        alpha
            .save(directory.join("alpha.json"))
            .map_err(|error| format!("plugin: failed to save alpha manifest: {error}"))?;
        beta.save(directory.join("beta.json"))
            .map_err(|error| format!("plugin: failed to save beta manifest: {error}"))?;

        let discovered = plugin::discover_manifests(&directory)
            .map_err(|error| format!("plugin: discovery failed: {error}"))?;
        ensure(discovered.len() == 2, "plugin: discovery did not return both manifests")?;
        ensure(
            discovered[0].id == "com.kaicad.alpha" && discovered[1].id == "com.kaicad.beta",
            "plugin: discovery order is not deterministic",
        )?;

        let discovered_count = discovered.len();
        let mut registry = plugin::PluginRegistry::new();
        for manifest in discovered {
            registry
                .register(manifest)
                .map_err(|error| format!("plugin: registry rejected a valid manifest: {error}"))?;
        }
        ensure(registry.len() == 2, "plugin: registry did not retain both manifests")?;
        ensure(
            registry.find("com.kaicad.alpha").is_some_and(|entry| entry.capabilities.len() == 2),
            "plugin: registry lost the alpha capabilities",
        )?;

        let duplicate =
            plugin::PluginManifest { id: "com.kaicad.alpha".to_string(), ..beta.clone() };
        ensure(
            matches!(registry.register(duplicate), Err(plugin::PluginError::DuplicateId(_))),
            "plugin: duplicate id was accepted",
        )?;

        let unsupported = plugin::PluginManifest {
            id: "com.kaicad.future".to_string(),
            api_version: plugin::SUPPORTED_API_VERSION + 1,
            ..beta.clone()
        };
        ensure(
            matches!(
                registry.register(unsupported),
                Err(plugin::PluginError::UnsupportedApiVersion { .. })
            ),
            "plugin: unsupported API version was accepted",
        )?;

        let unsafe_entry = plugin::PluginManifest {
            id: "com.kaicad.unsafe".to_string(),
            entry_point: "../escape.so".to_string(),
            ..beta.clone()
        };
        ensure(
            matches!(
                registry.register(unsafe_entry),
                Err(plugin::PluginError::UnsafeEntryPoint(_))
            ),
            "plugin: unsafe entry point was accepted",
        )?;
        ensure(registry.len() == 2, "plugin: rejected manifests changed the registry")?;

        let registered_count = registry.len();
        ensure(
            registry.remove("com.kaicad.alpha"),
            "plugin: registered plugin could not be removed",
        )?;
        ensure(
            registry.find("com.kaicad.alpha").is_none() && registry.len() == 1,
            "plugin: removal did not update the registry",
        )?;

        Ok((discovered_count, registered_count))
    })();

    let _ = std::fs::remove_dir_all(&directory);
    let (discovered_count, registered_count) = outcome?;

    println!(
        "  plugin: discovered {discovered_count}, registered {registered_count}, deterministic order and validation ok"
    );
    Ok(())
}

fn check_schematic() -> Result<(), String> {
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
        library: "self-check".to_string(),
    };
    let net = core::Net {
        id: Uuid::new_v4(),
        name: "N1".to_string(),
        pins: symbol.pins.iter().map(|pin| pin.id).collect(),
        ..core::Net::default()
    };

    let netlist =
        schematic::generate_netlist(std::slice::from_ref(&symbol), std::slice::from_ref(&net));
    ensure(netlist.nets.len() == 1, "schematic: expected exactly one net in the netlist")?;
    ensure(
        netlist.connections.len() == 1,
        "schematic: expected exactly one connection between the two pins",
    )?;

    let erc = schematic::run_erc(&netlist);
    ensure(erc.valid, "schematic: ERC rejected the self-check netlist")?;

    let exported = schematic::to_kicad_netlist(&netlist);
    ensure(
        exported.starts_with("(export"),
        "schematic: exported netlist is not a KiCad export document",
    )?;

    println!(
        "  schematic: {} net(s), {} connection(s), ERC valid={}, {} violation(s), export {} bytes",
        netlist.nets.len(),
        netlist.connections.len(),
        erc.valid,
        erc.violations.len(),
        exported.len()
    );
    Ok(())
}

fn check_pcb() -> Result<(), String> {
    let mut design = pcb::PcbDesign {
        outline: core::PcbOutline::rectangle(100.0, 80.0),
        layers: vec![
            core::PcbLayer::new(0, "F.Cu", core::LayerKind::Copper),
            core::PcbLayer::new(1, "B.Cu", core::LayerKind::Copper),
        ],
        stackup: core::Stackup { layers: vec![0, 1] },
        ..pcb::PcbDesign::default()
    };

    ensure(
        design.outline.points.len() == 4,
        "pcb: rectangular outline should have four corner points",
    )?;
    ensure(
        (design.outline.area() - 8_000.0).abs() < 1e-9,
        "pcb: rectangular outline area is not 8000 mm^2",
    )?;
    ensure(
        design.outline.contains_point(&core::Point::new(50.0, 40.0)),
        "pcb: outline does not contain its centre",
    )?;
    ensure(
        !design.outline.contains_point(&core::Point::new(150.0, 40.0)),
        "pcb: outline contains a point outside the board",
    )?;
    ensure(design.copper_layer_count() == 2, "pcb: expected exactly two copper layers")?;

    let stack = pcb::LayerStack::new(design.layers.clone());
    let issues = stack.validate(&design.stackup);
    ensure(issues.is_empty(), "pcb: layer stack reported unexpected issues")?;

    design.tracks = vec![
        pcb::Track {
            id: Uuid::new_v4(),
            net_id: Uuid::new_v4(),
            layer: 0,
            width: 0.25,
            path: vec![core::Point::new(10.0, 10.0), core::Point::new(90.0, 10.0)],
        },
        pcb::Track {
            id: Uuid::new_v4(),
            net_id: Uuid::new_v4(),
            layer: 0,
            width: 0.25,
            path: vec![core::Point::new(10.0, 10.2), core::Point::new(90.0, 10.2)],
        },
    ];

    let dirty = pcb::run_drc(&design, &design.rules);
    ensure(!dirty.is_valid(), "pcb: DRC accepted tracks that violate the clearance")?;
    ensure(
        dirty.violations.iter().any(|violation| violation.rule_id == "clearance"),
        "pcb: DRC did not report the clearance violation",
    )?;

    design.tracks[1].path = vec![core::Point::new(10.0, 20.0), core::Point::new(90.0, 20.0)];
    let clean = pcb::run_drc(&design, &design.rules);
    ensure(clean.is_valid(), "pcb: DRC rejected a clean board")?;
    ensure(clean.violations.is_empty(), "pcb: DRC reported a violation on a clean board")?;

    let route_request = pcb::routing::RouteRequest {
        net_id: Uuid::new_v4(),
        start: core::Point::new(5.0, 5.0),
        end: core::Point::new(95.0, 5.0),
        layer: 0,
        width: 0.25,
    };
    let router_config = pcb::routing::RouterConfig::default();
    let routed = pcb::routing::route(&route_request, &design, &router_config)
        .map_err(|error| format!("pcb: routing failed: {error}"))?;
    ensure(!routed.is_empty(), "pcb: router returned no tracks")?;

    let mut routed_design = design.clone();
    routed_design.tracks.extend(routed.clone());
    let routed_report = pcb::run_drc(&routed_design, &routed_design.rules);
    ensure(routed_report.is_valid(), "pcb: routed track fails DRC")?;

    let mut crossing_design = design.clone();
    crossing_design.tracks.push(pcb::Track {
        id: Uuid::new_v4(),
        net_id: Uuid::new_v4(),
        layer: 0,
        width: 0.25,
        path: vec![core::Point::new(0.0, 40.0), core::Point::new(100.0, 40.0)],
    });
    let crossing_request = pcb::routing::RouteRequest {
        start: core::Point::new(10.0, 30.0),
        end: core::Point::new(90.0, 50.0),
        ..route_request.clone()
    };
    let multilayer =
        pcb::routing::route_multilayer(&crossing_request, &crossing_design, &router_config)
            .map_err(|error| format!("pcb: multi-layer routing failed: {error}"))?;
    ensure(!multilayer.vias.is_empty(), "pcb: multi-layer route inserted no via")?;
    ensure(
        multilayer.tracks.len() >= 2,
        "pcb: multi-layer route did not split tracks at the layer change",
    )?;

    let mut multilayer_design = crossing_design.clone();
    multilayer_design.tracks.extend(multilayer.tracks.clone());
    multilayer_design.vias.extend(multilayer.vias.clone());
    let multilayer_report = pcb::run_drc(&multilayer_design, &multilayer_design.rules);
    ensure(multilayer_report.is_valid(), "pcb: multi-layer route fails DRC")?;

    let mut blocked_design = design.clone();
    blocked_design.tracks.push(pcb::Track {
        id: Uuid::new_v4(),
        net_id: Uuid::new_v4(),
        layer: 0,
        width: 0.25,
        path: vec![core::Point::new(0.0, 40.0), core::Point::new(100.0, 40.0)],
    });
    let blocked_request = pcb::routing::RouteRequest {
        net_id: Uuid::new_v4(),
        start: core::Point::new(5.0, 35.0),
        end: core::Point::new(95.0, 45.0),
        ..route_request
    };
    let blocked_config = pcb::routing::RouterConfig { max_expansions: 20_000, ..router_config };
    ensure(
        pcb::routing::route(&blocked_request, &blocked_design, &blocked_config).is_err(),
        "pcb: router crossed a blocking wall",
    )?;

    let library = pcb::FootprintLibrary {
        name: "self-check".to_string(),
        footprints: vec![pcb::Footprint {
            name: "R_0603".to_string(),
            reference_designator: "R".to_string(),
            pad_positions: vec![
                pcb::PadPosition {
                    x: -0.8,
                    y: 0.0,
                    pad_type: pcb::PadType::SurfaceMount,
                    ..pcb::PadPosition::default()
                },
                pcb::PadPosition {
                    x: 0.8,
                    y: 0.0,
                    pad_type: pcb::PadType::SurfaceMount,
                    ..pcb::PadPosition::default()
                },
            ],
            outline: pcb::footprint::Outline {
                shape: pcb::footprint::OutlineShape::rounded_rectangle(-1.6, -0.8, 3.2, 1.6, 0.2),
                copper: false,
            },
            ..pcb::Footprint::default()
        }],
    };

    let directory = std::env::temp_dir().join(format!("kaicad-self-check-{}", Uuid::new_v4()));
    let path = directory.join("footprints.json");
    let round_trip = library.save(&path).and_then(|()| pcb::FootprintLibrary::load(&path));
    let _ = std::fs::remove_dir_all(&directory);
    let loaded =
        round_trip.map_err(|error| format!("pcb: footprint library round trip failed: {error}"))?;
    ensure(loaded.footprints.len() == 1, "pcb: footprint library lost its footprint")?;
    ensure(loaded.footprints[0].pad_positions.len() == 2, "pcb: footprint library lost pads")?;

    println!(
        "  pcb: outline {} points, area {:.0} mm^2, {} layer(s), {} copper layer(s), {} footprint(s), DRC clean={} ({} violation(s) before fixing), routed {} point(s), multi-layer {} track(s)/{} via(s)",
        design.outline.points.len(),
        design.outline.area(),
        design.layers.len(),
        design.copper_layer_count(),
        loaded.footprints.len(),
        clean.violations.is_empty(),
        dirty.violations.len(),
        routed[0].path.len(),
        multilayer.tracks.len(),
        multilayer.vias.len()
    );
    Ok(())
}

fn check_renderer() -> Result<(), String> {
    let config = renderer::RendererConfig::default();

    ensure(
        config.window.width > 0 && config.window.height > 0,
        "renderer: default window size must be non-zero",
    )?;

    println!(
        "  renderer: window {}x{}, vsync={}",
        config.window.width, config.window.height, config.vsync
    );
    Ok(())
}

fn check_ai() -> Result<(), String> {
    let config = ai::kAIConfig::default();

    ensure(!config.model_endpoint.is_empty(), "ai: default model endpoint must be configured")?;

    let kai = ai::kAI::new(config.clone());
    let mut project = core::Project::new("ai-self-check");
    project.schematic.components.push(core::Component {
        id: Uuid::new_v4(),
        reference_designator: "R1".to_string(),
        properties: vec![core::ComponentProperty {
            name: "Value".to_string(),
            value: "10k".to_string(),
        }],
        ..core::Component::default()
    });
    let before = core::serialization::to_json(&project)
        .map_err(|error| format!("ai: project serialization failed: {error}"))?;
    let suggestion = ai::OptimizationSuggestion {
        id: Uuid::new_v4(),
        description: "self-check value update".to_string(),
        changes: vec![ai::ChangeProposal::new(
            "component:R1",
            "value",
            serde_json::json!("10k"),
            serde_json::json!("22k"),
        )],
        confidence_score: 0.9,
    };

    let applied = kai
        .apply_suggestion(&mut project, &suggestion)
        .map_err(|error| format!("ai: suggestion application failed: {error}"))?;
    let edited = core::serialization::to_json(&project)
        .map_err(|error| format!("ai: project serialization failed: {error}"))?;
    ensure(edited != before, "ai: applied suggestion did not change the project")?;
    ensure(
        project.schematic.components[0].properties[0].value == "22k",
        "ai: applied suggestion did not update the component value",
    )?;

    core::edit::undo(&mut project, &applied.transaction)
        .map_err(|error| format!("ai: undo failed: {error}"))?;
    let undone = core::serialization::to_json(&project)
        .map_err(|error| format!("ai: project serialization failed: {error}"))?;
    ensure(undone == before, "ai: undo did not restore the project")?;

    core::edit::redo(&mut project, &applied.transaction)
        .map_err(|error| format!("ai: redo failed: {error}"))?;
    let redone = core::serialization::to_json(&project)
        .map_err(|error| format!("ai: project serialization failed: {error}"))?;
    ensure(redone == edited, "ai: redo did not restore the edit")?;

    let stale = ai::OptimizationSuggestion {
        id: Uuid::new_v4(),
        description: "stale self-check update".to_string(),
        changes: vec![ai::ChangeProposal::new(
            "component:R1",
            "value",
            serde_json::json!("1k"),
            serde_json::json!("33k"),
        )],
        confidence_score: 0.9,
    };
    ensure(
        kai.apply_suggestion(&mut project, &stale).is_err(),
        "ai: stale suggestion was applied",
    )?;
    ensure(
        core::serialization::to_json(&project)
            .map_err(|error| format!("ai: project serialization failed: {error}"))?
            == edited,
        "ai: failed suggestion mutated the project",
    )?;

    println!(
        "  ai: endpoint={}, deterministic validation=always, applied {} change(s), undo/redo ok (tx {})",
        config.model_endpoint,
        applied.transaction.changes.len(),
        &applied.transaction.id.to_string()[..8]
    );
    Ok(())
}

fn check_collab() -> Result<(), String> {
    let mut manager = collab::CollabManager::new();
    let owner = Uuid::new_v4();
    let editor = Uuid::new_v4();
    let viewer = Uuid::new_v4();
    let session = manager.create_session(Uuid::new_v4(), owner);
    let session_id = session.session_id;
    manager
        .add_participant(
            session_id,
            collab::Participant {
                user_id: editor,
                username: "self-check-editor".to_string(),
                role: collab::ParticipantRole::Editor,
            },
        )
        .map_err(|error| format!("collab: failed to add editor: {error}"))?;
    manager
        .add_participant(
            session_id,
            collab::Participant {
                user_id: viewer,
                username: "self-check-viewer".to_string(),
                role: collab::ParticipantRole::Viewer,
            },
        )
        .map_err(|error| format!("collab: failed to add viewer: {error}"))?;

    ensure(manager.session_count() == 1, "collab: expected exactly one session")?;
    ensure(manager.can_edit(session_id, owner), "collab: owner cannot edit")?;
    ensure(manager.can_edit(session_id, editor), "collab: editor cannot edit")?;
    ensure(!manager.can_edit(session_id, viewer), "collab: viewer can edit")?;

    let mut project = core::Project::new("collab-self-check");
    project.schematic.components.push(core::Component {
        id: Uuid::new_v4(),
        reference_designator: "R1".to_string(),
        properties: vec![core::ComponentProperty {
            name: "Value".to_string(),
            value: "10k".to_string(),
        }],
        ..core::Component::default()
    });
    let before = core::serialization::to_json(&project)
        .map_err(|error| format!("collab: project serialization failed: {error}"))?;
    let changes = [core::ChangeProposal::new(
        "component:R1",
        "value",
        serde_json::json!("10k"),
        serde_json::json!("22k"),
    )];

    ensure(
        manager.apply_edit(session_id, viewer, &mut project, "viewer edit", &changes).is_err(),
        "collab: viewer edit was allowed",
    )?;
    ensure(manager.sessions[0].version == 0, "collab: rejected edit bumped the version")?;

    let transaction = manager
        .apply_edit(session_id, editor, &mut project, "editor edit", &changes)
        .map_err(|error| format!("collab: edit failed: {error}"))?;
    ensure(manager.sessions[0].version == 1, "collab: successful edit did not bump version")?;
    ensure(
        project.schematic.components[0].properties[0].value == "22k",
        "collab: edit did not update the project",
    )?;

    let mut history = collab::VersioningManager::new();
    let version = history
        .record_transaction(editor, "editor edit", &transaction, &project)
        .map_err(|error| format!("collab: version recording failed: {error}"))?;
    ensure(version == 1, "collab: version history did not record version 1")?;
    let entry = history.latest().ok_or("collab: version history is empty")?;
    ensure(
        entry.transaction_id == Some(transaction.id) && entry.project_digest.is_some(),
        "collab: version did not retain the transaction and digest",
    )?;

    let mut sync = collab::sync::SyncManager::default();
    ensure(sync.stage(transaction.clone()), "collab: transaction was not staged")?;
    ensure(!sync.stage(transaction.clone()), "collab: duplicate transaction was staged")?;
    ensure(sync.staged_count() == 1, "collab: staged transaction count is wrong")?;
    let drained = sync.drain();
    ensure(
        drained.len() == 1 && drained[0].id == transaction.id && sync.staged_count() == 0,
        "collab: staged transactions were not drained in order",
    )?;

    core::edit::undo(&mut project, &transaction)
        .map_err(|error| format!("collab: undo failed: {error}"))?;
    ensure(
        core::serialization::to_json(&project)
            .map_err(|error| format!("collab: project serialization failed: {error}"))?
            == before,
        "collab: undo did not restore the project",
    )?;

    println!(
        "  collab: {} session(s), {} participant(s), session version {}, history {} entry, staged 1 -> 0",
        manager.session_count(),
        manager.sessions[0].participants.len(),
        manager.sessions[0].version,
        history.versions.len()
    );
    Ok(())
}

fn check_store() -> Result<(), String> {
    let mut store = store::Store::open_in_memory()
        .map_err(|error| format!("store: failed to open in-memory database: {error}"))?;

    let mut project = core::Project::new("store-self-check");
    project.schematic.nets.push(core::Net {
        id: Uuid::new_v4(),
        name: "N1".to_string(),
        pins: Vec::new(),
        netclass: core::NetClass::Signal,
    });

    store
        .create_project(&project)
        .map_err(|error| format!("store: failed to create project: {error}"))?;

    let design = pcb::PcbDesign {
        outline: core::PcbOutline::rectangle(50.0, 40.0),
        layers: vec![core::PcbLayer::new(0, "F.Cu", core::LayerKind::Copper)],
        stackup: core::Stackup { layers: vec![0] },
        rules: pcb::DrcRules { clearance: 0.3, ..pcb::DrcRules::default() },
        ..pcb::PcbDesign::default()
    };
    store
        .save_pcb_design(project.id, &design)
        .map_err(|error| format!("store: failed to save PCB design: {error}"))?;
    let loaded_design = store
        .load_pcb_design(project.id)
        .map_err(|error| format!("store: failed to load PCB design: {error}"))?
        .ok_or("store: saved PCB design disappeared")?;
    ensure(
        loaded_design.rules == design.rules,
        "store: PCB design rules changed across a database round trip",
    )?;

    let loaded = store
        .load_project(project.id)
        .map_err(|error| format!("store: failed to load project: {error}"))?
        .ok_or("store: created project disappeared")?;

    ensure(loaded.id == project.id, "store: project id changed across a database round trip")?;
    ensure(
        loaded.name == project.name,
        "store: project name changed across a database round trip",
    )?;

    let deleted = store
        .delete_project(project.id)
        .map_err(|error| format!("store: failed to delete project: {error}"))?;
    ensure(deleted, "store: project was not deleted")?;
    ensure(
        store
            .load_pcb_design(project.id)
            .map_err(|error| format!("store: failed to load deleted PCB design: {error}"))?
            .is_none(),
        "store: PCB design survived project deletion",
    )?;

    let version = store
        .schema_version()
        .map_err(|error| format!("store: failed to read schema version: {error}"))?;

    println!(
        "  store: created, loaded and deleted 1 project with its PCB design (schema v{version})"
    );
    Ok(())
}

fn check_kicad_parser() -> Result<(), String> {
    const FIXTURE: &str = r#"(kicad_symbol_lib
      (version 20231120)
      (generator kicad_symbol_editor)
      (symbol "Device:R"
        (symbol "R_1_1"
          (pin passive line (name "~") (number "1"))
          (pin passive line (name "~") (number "2"))
        )
      )
    )"#;

    let path = std::env::temp_dir().join(format!("kaicad-self-check-{}.kicad_sym", Uuid::new_v4()));
    std::fs::write(&path, FIXTURE)
        .map_err(|error| format!("kicad-parser: failed to write fixture: {error}"))?;

    let parsed = kicad_parser::parse_symbol_library(&path);
    let _ = std::fs::remove_file(&path);
    let library = parsed.map_err(|error| format!("kicad-parser: {error}"))?;

    ensure(!library.symbols.is_empty(), "kicad-parser: expected at least one parsed symbol")?;
    ensure(
        library.symbols[0].pins.len() == 2,
        "kicad-parser: expected two pins on the fixture symbol",
    )?;

    println!("  kicad-parser: loaded {} symbol(s) from {}", library.symbols.len(), path.display());
    Ok(())
}
