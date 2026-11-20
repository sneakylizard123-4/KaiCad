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

    println!(
        "  schematic: {} net(s), {} connection(s), ERC valid={}, {} violation(s)",
        netlist.nets.len(),
        netlist.connections.len(),
        erc.valid,
        erc.violations.len()
    );
    Ok(())
}

fn check_pcb() -> Result<(), String> {
    let design = pcb::PcbDesign {
        outline: core::PcbOutline::rectangle(100.0, 80.0),
        layers: vec![core::PcbLayer::new(0, "F.Cu", core::LayerKind::Copper)],
        ..pcb::PcbDesign::default()
    };

    ensure(
        design.outline.points.len() == 4,
        "pcb: rectangular outline should have four corner points",
    )?;
    ensure(design.copper_layer_count() == 1, "pcb: expected exactly one copper layer")?;

    println!(
        "  pcb: outline {} points, {} layer(s), {} copper layer(s)",
        design.outline.points.len(),
        design.layers.len(),
        design.copper_layer_count()
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

    ensure(
        config.enable_deterministic_validation,
        "ai: deterministic validation must be enabled by default",
    )?;

    println!(
        "  ai: endpoint={}, deterministic validation={}",
        config.model_endpoint, config.enable_deterministic_validation
    );
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
