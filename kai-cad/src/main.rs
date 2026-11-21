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

    println!("  core: project JSON round trip ok ({} bytes)", json.len());
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
    let design = pcb::PcbDesign {
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

    let library = pcb::FootprintLibrary {
        name: "self-check".to_string(),
        footprints: vec![pcb::Footprint {
            name: "R_0603".to_string(),
            reference_designator: "R".to_string(),
            pad_positions: vec![
                pcb::PadPosition { x: -0.8, y: 0.0, pad_type: pcb::PadType::SurfaceMount },
                pcb::PadPosition { x: 0.8, y: 0.0, pad_type: pcb::PadType::SurfaceMount },
            ],
            outline: pcb::footprint::Outline {
                shape: pcb::footprint::OutlineShape::rounded_rectangle(-1.6, -0.8, 3.2, 1.6, 0.2),
                copper: false,
            },
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
        "  pcb: outline {} points, area {:.0} mm^2, {} layer(s), {} copper layer(s), {} footprint(s)",
        design.outline.points.len(),
        design.outline.area(),
        design.layers.len(),
        design.copper_layer_count(),
        loaded.footprints.len()
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

    println!("  ai: endpoint={}, deterministic validation=always", config.model_endpoint);
    Ok(())
}

fn check_collab() -> Result<(), String> {
    let mut manager = collab::CollabManager::new();
    manager.create_session(Uuid::new_v4(), Uuid::new_v4());

    ensure(manager.session_count() == 1, "collab: expected exactly one session")?;

    println!("  collab: {} session(s)", manager.session_count());
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

    let version = store
        .schema_version()
        .map_err(|error| format!("store: failed to read schema version: {error}"))?;

    println!("  store: created, loaded and deleted 1 project (schema v{version})");
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
