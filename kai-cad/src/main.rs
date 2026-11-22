//! KaiCad main entry point.
//!
//! The desktop Studio GUI is a separate binary, so this binary acts as a deterministic smoke
//! test of the workspace's public library surface: it serializes a project,
//! generates and checks a netlist, builds a PCB design, and reads the default
//! configuration of the renderer, AI, and collaboration subsystems.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use uuid::Uuid;

const USAGE: &str = "kai-cad [--version|--help]";
const PLUGIN_USAGE: &str =
    "kai-cad plugins <list|show|enable|disable|discover> [arguments] [--db <path>]";
const PCB_USAGE: &str =
    "kai-cad pcb <import <input.kicad_pcb> <output.json> | export <input.json> <output.kicad_pcb>>";
const SCH_USAGE: &str =
    "kai-cad sch <import <input.kicad_sch> <output.json> | export <input.json> <output.kicad_sch>>";
const FOOTPRINT_USAGE: &str = "kai-cad footprint <import <input.kicad_mod> <output.json> | export <input.json> <output.kicad_mod>>";
const PROJECT_USAGE: &str = "kai-cad project <import <input.kicad_pro> <output.json> | export <input.json> <output.kicad_pro>>";
const DEFAULT_DB_PATH: &str = "kaicad.db";

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
        [command, rest @ ..] if command == "plugins" => run_plugins(rest),
        [command, rest @ ..] if command == "pcb" => run_pcb(rest),
        [command, rest @ ..] if command == "sch" => run_sch(rest),
        [command, rest @ ..] if command == "footprint" => run_footprint(rest),
        [command, rest @ ..] if command == "project" => run_project(rest),
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
    println!("usage:");
    println!("  {USAGE}");
    println!("  {PLUGIN_USAGE}");
    println!("  {PCB_USAGE}");
    println!("  {SCH_USAGE}");
    println!("  {FOOTPRINT_USAGE}");
    println!("  {PROJECT_USAGE}");
    println!();
    println!("  -h, --help     print this help and exit");
    println!("  -V, --version  print the version and exit");
    println!();
    println!("Plugin commands:");
    print_plugin_help();
    println!();
    println!("PCB compatibility commands:");
    print_pcb_help();
    println!();
    println!("Schematic, footprint and project compatibility commands:");
    print_sch_help();
    print_footprint_help();
    print_project_help();
    println!();
    println!("With no arguments, runs a deterministic self-check of every workspace subsystem.");
}

fn print_plugin_help() {
    println!("  plugins list                 list registered plugins");
    println!("  plugins show <id>            show one plugin manifest and state");
    println!("  plugins enable <id>          enable a registered plugin");
    println!("  plugins disable <id>         disable a registered plugin");
    println!("  plugins discover <directory> register every valid manifest in a directory");
    println!();
    println!("  --db <path>                  registry database (default: {DEFAULT_DB_PATH})");
}

fn print_pcb_help() {
    println!("  pcb import <input.kicad_pcb> <output.json>  import supported KiCad board data");
    println!("  pcb export <input.json> <output.kicad_pcb>  write supported KiCad board data");
    println!();
    println!(
        "  Import reports skipped KiCad constructs on stderr; it does not claim full round-trip fidelity."
    );
}

fn print_sch_help() {
    println!("  sch import <input.kicad_sch> <output.json>      import supported schematic data");
    println!("  sch export <input.json> <output.kicad_sch>      write supported schematic data");
}

fn print_footprint_help() {
    println!("  footprint import <input.kicad_mod> <output.json> read a KiCad footprint");
    println!("  footprint export <input.json> <output.kicad_mod> write a KiCad footprint");
}

fn print_project_help() {
    println!("  project import <input.kicad_pro> <output.json>  preserve project JSON");
    println!("  project export <input.json> <output.kicad_pro>  write project JSON");
}

fn run_sch(args: &[String]) -> ExitCode {
    match sch_command(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            eprintln!("usage: {SCH_USAGE}");
            ExitCode::FAILURE
        }
    }
}

fn sch_command(args: &[String]) -> Result<(), String> {
    match args {
        [] => {
            print_sch_help();
            Ok(())
        }
        [command] if command == "help" || command == "--help" || command == "-h" => {
            print_sch_help();
            Ok(())
        }
        [command, input, output] if command == "import" => {
            let imported = kicad_parser::load_schematic(input)
                .map_err(|error| format!("failed to import {input}: {error}"))?;
            for warning in &imported.warnings {
                eprintln!("warning: {warning}");
            }
            let mut json = serde_json::to_string_pretty(&imported.data)
                .map_err(|error| format!("failed to serialize imported schematic: {error}"))?;
            json.push('\n');
            write_output(output, &json)?;
            println!(
                "imported {} into {} ({} component(s), {} sheet(s), {} wire(s), {} warning(s))",
                input,
                output,
                imported.data.components.len(),
                imported.data.sheets.len(),
                imported.data.sheets.iter().map(|sheet| sheet.wires.len()).sum::<usize>(),
                imported.warnings.len()
            );
            Ok(())
        }
        [command, input, output] if command == "export" => {
            let json = std::fs::read_to_string(input)
                .map_err(|error| format!("failed to read {input}: {error}"))?;
            let data: core::project::SchematicData = serde_json::from_str(&json)
                .map_err(|error| format!("invalid KaiCad schematic JSON in {input}: {error}"))?;
            let document = kicad_parser::write_schematic(&data)
                .map_err(|error| format!("failed to export {input}: {error}"))?;
            write_output(output, &document)?;
            println!(
                "exported {} to {} ({} component(s), {} sheet(s))",
                input,
                output,
                data.components.len(),
                data.sheets.len()
            );
            Ok(())
        }
        [command, ..] if command == "import" || command == "export" => {
            Err(format!("sch {command} expects an input and an output path"))
        }
        [command, ..] => Err(format!("unknown sch command: {command}")),
    }
}

fn run_footprint(args: &[String]) -> ExitCode {
    match footprint_command(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            eprintln!("usage: {FOOTPRINT_USAGE}");
            ExitCode::FAILURE
        }
    }
}

fn footprint_command(args: &[String]) -> Result<(), String> {
    match args {
        [] => {
            print_footprint_help();
            Ok(())
        }
        [command] if command == "help" || command == "--help" || command == "-h" => {
            print_footprint_help();
            Ok(())
        }
        [command, input, output] if command == "import" => {
            let imported = kicad_parser::load_footprint(input)
                .map_err(|error| format!("failed to import {input}: {error}"))?;
            for warning in &imported.warnings {
                eprintln!("warning: {warning}");
            }
            let mut json = serde_json::to_string_pretty(&imported.footprint)
                .map_err(|error| format!("failed to serialize imported footprint: {error}"))?;
            json.push('\n');
            write_output(output, &json)?;
            println!(
                "imported {} into {} ({} pad(s), {} warning(s))",
                input,
                output,
                imported.footprint.pad_positions.len(),
                imported.warnings.len()
            );
            Ok(())
        }
        [command, input, output] if command == "export" => {
            let json = std::fs::read_to_string(input)
                .map_err(|error| format!("failed to read {input}: {error}"))?;
            let footprint: pcb::Footprint = serde_json::from_str(&json)
                .map_err(|error| format!("invalid KaiCad footprint JSON in {input}: {error}"))?;
            let document = kicad_parser::write_footprint(&footprint)
                .map_err(|error| format!("failed to export {input}: {error}"))?;
            write_output(output, &document)?;
            println!("exported {} to {} ({} pad(s))", input, output, footprint.pad_positions.len());
            Ok(())
        }
        [command, ..] if command == "import" || command == "export" => {
            Err(format!("footprint {command} expects an input and an output path"))
        }
        [command, ..] => Err(format!("unknown footprint command: {command}")),
    }
}

fn run_project(args: &[String]) -> ExitCode {
    match project_command(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            eprintln!("usage: {PROJECT_USAGE}");
            ExitCode::FAILURE
        }
    }
}

fn project_command(args: &[String]) -> Result<(), String> {
    match args {
        [] => {
            print_project_help();
            Ok(())
        }
        [command] if command == "help" || command == "--help" || command == "-h" => {
            print_project_help();
            Ok(())
        }
        [command, input, output] if command == "import" => {
            let project = kicad_parser::ProjectFile::load(input)
                .map_err(|error| format!("failed to import {input}: {error}"))?;
            project.save(output).map_err(|error| format!("failed to write {output}: {error}"))?;
            println!(
                "imported {} into {} ({} top-level setting(s))",
                input,
                output,
                project.value.as_object().map(|object| object.len()).unwrap_or(0)
            );
            Ok(())
        }
        [command, input, output] if command == "export" => {
            let json = std::fs::read_to_string(input)
                .map_err(|error| format!("failed to read {input}: {error}"))?;
            let project = kicad_parser::ProjectFile::parse(&json)
                .map_err(|error| format!("invalid project JSON in {input}: {error}"))?;
            project.save(output).map_err(|error| format!("failed to write {output}: {error}"))?;
            println!("exported {} to {}", input, output);
            Ok(())
        }
        [command, ..] if command == "import" || command == "export" => {
            Err(format!("project {command} expects an input and an output path"))
        }
        [command, ..] => Err(format!("unknown project command: {command}")),
    }
}

fn run_pcb(args: &[String]) -> ExitCode {
    match pcb_command(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            eprintln!("usage: {PCB_USAGE}");
            ExitCode::FAILURE
        }
    }
}

fn pcb_command(args: &[String]) -> Result<(), String> {
    match args {
        [] => {
            print_pcb_help();
            Ok(())
        }
        [command] if command == "help" || command == "--help" || command == "-h" => {
            print_pcb_help();
            Ok(())
        }
        [command, input, output] if command == "import" => {
            let imported = kicad_parser::load_pcb(input)
                .map_err(|error| format!("failed to import {input}: {error}"))?;
            for warning in &imported.warnings {
                eprintln!("warning: {warning}");
            }
            let mut json = serde_json::to_string_pretty(&imported.design)
                .map_err(|error| format!("failed to serialize imported board: {error}"))?;
            json.push('\n');
            write_output(output, &json)?;
            println!(
                "imported {} into {} ({} track(s), {} via(s), {} footprint(s), {} warning(s))",
                input,
                output,
                imported.design.tracks.len(),
                imported.design.vias.len(),
                imported.design.footprints.len(),
                imported.warnings.len()
            );
            Ok(())
        }
        [command, input, output] if command == "export" => {
            let json = std::fs::read_to_string(input)
                .map_err(|error| format!("failed to read {input}: {error}"))?;
            let design: pcb::PcbDesign = serde_json::from_str(&json)
                .map_err(|error| format!("invalid KaiCad board JSON in {input}: {error}"))?;
            let document = kicad_parser::write_pcb(&design)
                .map_err(|error| format!("failed to export {input}: {error}"))?;
            write_output(output, &document)?;
            println!(
                "exported {} to {} ({} track(s), {} via(s), {} footprint(s))",
                input,
                output,
                design.tracks.len(),
                design.vias.len(),
                design.footprints.len()
            );
            Ok(())
        }
        [command, ..] if command == "import" || command == "export" => {
            Err(format!("pcb {command} expects an input and an output path"))
        }
        [command, ..] => Err(format!("unknown pcb command: {command}")),
    }
}

fn write_output(path: &str, contents: &str) -> Result<(), String> {
    let path = Path::new(path);
    if path.as_os_str().is_empty() {
        return Err("output path must not be empty".to_string());
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
        }
    }
    std::fs::write(path, contents)
        .map_err(|error| format!("failed to write {}: {error}", path.display()))
}

fn run_plugins(args: &[String]) -> ExitCode {
    match plugins_command(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            eprintln!("usage: {PLUGIN_USAGE}");
            ExitCode::FAILURE
        }
    }
}

fn plugins_command(args: &[String]) -> Result<(), String> {
    let (positional, db_path) = parse_plugin_options(args)?;
    let Some(command) = positional.first().map(String::as_str) else {
        print_plugin_help();
        return Ok(());
    };

    match command {
        "help" | "-h" | "--help" => {
            print_plugin_help();
            Ok(())
        }
        "list" => {
            expect_plugin_arguments(&positional, 1, "list")?;
            let store = open_plugin_store(&db_path)?;
            let states = store
                .list_plugin_states()
                .map_err(|error| format!("failed to list plugins: {error}"))?;
            if states.is_empty() {
                println!("No plugins registered.");
            }
            for (manifest, enabled) in states {
                println!(
                    "{}\t{}\t{}\t{}",
                    manifest.id,
                    manifest.version,
                    if enabled { "enabled" } else { "disabled" },
                    capability_list(&manifest)
                );
            }
            Ok(())
        }
        "show" => {
            expect_plugin_arguments(&positional, 2, "show")?;
            let id = &positional[1];
            let store = open_plugin_store(&db_path)?;
            let manifest = store
                .load_plugin(id)
                .map_err(|error| format!("failed to load plugin `{id}`: {error}"))?
                .ok_or_else(|| format!("plugin not found: {id}"))?;
            let enabled = store
                .plugin_enabled(id)
                .map_err(|error| format!("failed to read plugin state: {error}"))?
                .unwrap_or(false);
            println!("id: {}", manifest.id);
            println!("name: {}", manifest.name);
            println!("version: {}", manifest.version);
            println!("api_version: {}", manifest.api_version);
            println!("entry_point: {}", manifest.entry_point);
            println!("capabilities: {}", capability_list(&manifest));
            println!("enabled: {enabled}");
            Ok(())
        }
        "enable" | "disable" => {
            expect_plugin_arguments(&positional, 2, command)?;
            let id = &positional[1];
            let enabled = command == "enable";
            let mut store = open_plugin_store(&db_path)?;
            if !store
                .set_plugin_enabled(id, enabled)
                .map_err(|error| format!("failed to update plugin `{id}`: {error}"))?
            {
                return Err(format!("plugin not found: {id}"));
            }
            println!("{} {id}", if enabled { "enabled" } else { "disabled" });
            Ok(())
        }
        "discover" => {
            expect_plugin_arguments(&positional, 2, "discover")?;
            let directory = &positional[1];
            let manifests = plugin::discover_manifests(directory)
                .map_err(|error| format!("failed to discover plugins: {error}"))?;
            let count = manifests.len();
            let mut store = open_plugin_store(&db_path)?;
            for manifest in &manifests {
                store.save_plugin(manifest).map_err(|error| {
                    format!("failed to register plugin `{}`: {error}", manifest.id)
                })?;
            }
            println!("discovered {count} plugin(s) in {}", Path::new(directory).display());
            Ok(())
        }
        other => Err(format!("unknown plugin command: {other}")),
    }
}

fn parse_plugin_options(args: &[String]) -> Result<(Vec<String>, PathBuf), String> {
    let mut positional = Vec::new();
    let mut db_path = PathBuf::from(DEFAULT_DB_PATH);
    let mut index = 0;

    while index < args.len() {
        let argument = &args[index];
        if argument == "--db" {
            index += 1;
            let value = args.get(index).ok_or("--db requires a path")?;
            if value.is_empty() {
                return Err("--db requires a non-empty path".to_string());
            }
            db_path = PathBuf::from(value);
        } else if let Some(value) = argument.strip_prefix("--db=") {
            if value.is_empty() {
                return Err("--db requires a non-empty path".to_string());
            }
            db_path = PathBuf::from(value);
        } else if argument.starts_with('-') && argument != "-h" && argument != "--help" {
            return Err(format!("unknown plugin option: {argument}"));
        } else {
            positional.push(argument.clone());
        }
        index += 1;
    }

    Ok((positional, db_path))
}

fn expect_plugin_arguments(
    positional: &[String],
    expected: usize,
    command: &str,
) -> Result<(), String> {
    if positional.len() != expected {
        return Err(format!(
            "plugins {command} expects {} argument(s)",
            expected.saturating_sub(1)
        ));
    }
    Ok(())
}

fn open_plugin_store(path: &Path) -> Result<store::Store, String> {
    store::Store::open(path)
        .map_err(|error| format!("failed to open plugin database {}: {error}", path.display()))
}

fn capability_list(manifest: &plugin::PluginManifest) -> String {
    manifest
        .capabilities
        .iter()
        .map(|capability| match capability {
            plugin::PluginCapability::Schematic => "schematic",
            plugin::PluginCapability::Pcb => "pcb",
            plugin::PluginCapability::Ai => "ai",
            plugin::PluginCapability::ImportExport => "import-export",
        })
        .collect::<Vec<_>>()
        .join(", ")
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
    let blocked_config = pcb::routing::RouterConfig {
        max_expansions: 20_000,
        allow_vias: false,
        ..router_config.clone()
    };
    ensure(
        pcb::routing::route(&blocked_request, &blocked_design, &blocked_config).is_err(),
        "pcb: router crossed a blocking wall",
    )?;

    let shove_config = pcb::routing::RouterConfig { allow_vias: false, ..router_config.clone() };
    let shoved =
        pcb::routing::route_push_and_shove(&blocked_request, &blocked_design, &shove_config)
            .map_err(|error| format!("pcb: push-and-shove routing failed: {error}"))?;
    ensure(
        !shoved.removed_track_ids.is_empty() && !shoved.rerouted.tracks.is_empty(),
        "pcb: push-and-shove did not rip up and reroute the blocking track",
    )?;
    let mut shoved_design = blocked_design.clone();
    shoved.apply_to(&mut shoved_design);
    let shoved_report = pcb::run_drc(&shoved_design, &shoved_design.rules);
    ensure(shoved_report.is_valid(), "pcb: push-and-shove result fails DRC")?;

    let mut interactive_design = design.clone();
    let mut interactive = pcb::routing::InteractiveRouter::new(
        Uuid::new_v4(),
        core::Point::new(5.0, 30.0),
        0,
        0.25,
        router_config.clone(),
    );
    interactive
        .preview(&interactive_design, core::Point::new(95.0, 50.0))
        .map_err(|error| format!("pcb: interactive routing preview failed: {error}"))?;
    let interactive_route = interactive
        .commit(&mut interactive_design)
        .map_err(|error| format!("pcb: interactive routing commit failed: {error}"))?;
    ensure(!interactive_route.tracks.is_empty(), "pcb: interactive router committed no tracks")?;
    let interactive_report = pcb::run_drc(&interactive_design, &interactive_design.rules);
    ensure(interactive_report.is_valid(), "pcb: interactive route fails DRC")?;

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
        "  pcb: outline {} points, area {:.0} mm^2, {} layer(s), {} copper layer(s), {} footprint(s), DRC clean={} ({} violation(s) before fixing), routed {} point(s), multi-layer {} track(s)/{} via(s), shoved {} track(s), interactive {} track(s)",
        design.outline.points.len(),
        design.outline.area(),
        design.layers.len(),
        design.copper_layer_count(),
        loaded.footprints.len(),
        clean.violations.is_empty(),
        dirty.violations.len(),
        routed[0].path.len(),
        multilayer.tracks.len(),
        multilayer.vias.len(),
        shoved.removed_track_ids.len(),
        interactive_route.tracks.len()
    );
    Ok(())
}

fn check_renderer() -> Result<(), String> {
    let config = renderer::RendererConfig {
        window: renderer::WindowConfig::new("self-check", 640, 480),
        ..renderer::RendererConfig::default()
    };
    ensure(
        config.window.width > 0 && config.window.height > 0,
        "renderer: window size must be non-zero",
    )?;

    let design = pcb::PcbDesign {
        outline: core::PcbOutline::rectangle(100.0, 80.0),
        layers: vec![
            core::PcbLayer::new(0, "F.Cu", core::LayerKind::Copper),
            core::PcbLayer::new(1, "B.Cu", core::LayerKind::Copper),
        ],
        stackup: core::Stackup { layers: vec![0, 1] },
        footprints: vec![pcb::Footprint {
            name: "R_0603".to_string(),
            reference_designator: "R".to_string(),
            position: core::Point::new(50.0, 40.0),
            pad_positions: vec![pcb::PadPosition {
                x: -0.8,
                y: 0.0,
                width: 1.4,
                height: 0.8,
                rotation: 30.0,
                ..pcb::PadPosition::default()
            }],
            ..pcb::Footprint::default()
        }],
        tracks: vec![pcb::Track {
            layer: 0,
            width: 0.4,
            path: vec![core::Point::new(10.0, 20.0), core::Point::new(90.0, 60.0)],
            ..pcb::Track::default()
        }],
        vias: vec![pcb::Via {
            position: core::Point::new(50.0, 40.0),
            layer_start: 0,
            layer_end: 1,
            ..pcb::Via::default()
        }],
        ..pcb::PcbDesign::default()
    };

    let background = [config.background.r, config.background.g, config.background.b];
    let mut renderer = renderer::Renderer::new(config.clone())
        .map_err(|error| format!("renderer: initialization failed: {error}"))?;
    let count_foreground =
        |pixels: &[u8]| pixels.chunks_exact(4).filter(|pixel| pixel[..3] != background).count();

    renderer
        .render_pcb(&design, renderer::ViewMode::TwoD)
        .map_err(|error| format!("renderer: 2D rendering failed: {error}"))?;
    let two_d = renderer.framebuffer().pixels().to_vec();
    let two_d_pixels = count_foreground(&two_d);
    ensure(two_d_pixels > 1_000, "renderer: 2D view drew almost nothing")?;

    renderer
        .render_pcb(&design, renderer::ViewMode::ThreeD)
        .map_err(|error| format!("renderer: 3D rendering failed: {error}"))?;
    let three_d = renderer.framebuffer().pixels();
    let three_d_pixels = count_foreground(three_d);
    ensure(three_d_pixels > 1_000, "renderer: 3D view drew almost nothing")?;
    ensure(two_d != three_d, "renderer: 2D and 3D views produced identical frames")?;

    println!(
        "  renderer: software 2D/3D {}x{}, {} / {} foreground pixels, vsync={}",
        config.window.width, config.window.height, two_d_pixels, three_d_pixels, config.vsync
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

    let alpha = plugin::PluginManifest {
        id: "com.kaicad.self-check.alpha".to_string(),
        name: "Alpha self-check plugin".to_string(),
        version: "0.1.0".to_string(),
        api_version: plugin::SUPPORTED_API_VERSION,
        entry_point: "plugins/alpha.so".to_string(),
        capabilities: vec![plugin::PluginCapability::Schematic],
    };
    let mut beta = plugin::PluginManifest {
        id: "com.kaicad.self-check.beta".to_string(),
        name: "Beta self-check plugin".to_string(),
        version: "0.2.0".to_string(),
        api_version: plugin::SUPPORTED_API_VERSION,
        entry_point: "plugins/beta.so".to_string(),
        capabilities: vec![plugin::PluginCapability::Pcb],
    };
    store
        .save_plugin(&alpha)
        .map_err(|error| format!("store: failed to save alpha plugin: {error}"))?;
    store
        .save_plugin(&beta)
        .map_err(|error| format!("store: failed to save beta plugin: {error}"))?;
    ensure(
        store.list_plugins().map_err(|error| format!("store: failed to list plugins: {error}"))?
            == [alpha.clone(), beta.clone()],
        "store: persisted plugins are not sorted by id",
    )?;
    ensure(
        store
            .set_plugin_enabled(&beta.id, false)
            .map_err(|error| format!("store: failed to disable beta plugin: {error}"))?,
        "store: registered plugin could not be disabled",
    )?;
    beta.name = "Beta self-check plugin updated".to_string();
    store
        .save_plugin(&beta)
        .map_err(|error| format!("store: failed to update beta plugin: {error}"))?;
    ensure(
        store
            .plugin_enabled(&beta.id)
            .map_err(|error| format!("store: failed to read beta state: {error}"))?
            == Some(false),
        "store: manifest upsert reset the disabled state",
    )?;
    ensure(
        store
            .list_enabled_plugins()
            .map_err(|error| format!("store: failed to list enabled plugins: {error}"))?
            .len()
            == 1,
        "store: enabled-plugin filter returned the wrong count",
    )?;
    ensure(
        store
            .list_plugins_with_capability(plugin::PluginCapability::Pcb)
            .map_err(|error| format!("store: failed to filter plugins: {error}"))?
            .len()
            == 1,
        "store: capability filter returned the wrong count",
    )?;
    ensure(
        store
            .set_plugin_enabled(&beta.id, true)
            .map_err(|error| format!("store: failed to enable beta plugin: {error}"))?,
        "store: disabled plugin could not be re-enabled",
    )?;
    let persisted_plugins = store
        .list_plugins()
        .map_err(|error| format!("store: failed to list persisted plugins: {error}"))?;
    let enabled_plugins = store
        .list_enabled_plugins()
        .map_err(|error| format!("store: failed to list enabled plugins: {error}"))?;
    ensure(
        persisted_plugins.len() == 2 && enabled_plugins.len() == 2,
        "store: plugin persistence lost a manifest or its state",
    )?;

    let version = store
        .schema_version()
        .map_err(|error| format!("store: failed to read schema version: {error}"))?;

    println!(
        "  store: created, loaded and deleted 1 project with its PCB design (schema v{version}), {} plugin(s) persisted/{} enabled",
        persisted_plugins.len(),
        enabled_plugins.len()
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

    const PCB_FIXTURE: &str = r#"(kicad_pcb (version 20240108) (generator pcbnew)
      (layers (0 "F.Cu" signal) (31 "B.Cu" signal) (44 "Edge.Cuts" user))
      (net 0 "") (net 1 "GND")
      (gr_line (start 0 0) (end 10 0) (layer "Edge.Cuts") (width 0.05))
      (gr_line (start 10 0) (end 10 10) (layer "Edge.Cuts") (width 0.05))
      (gr_line (start 10 10) (end 0 10) (layer "Edge.Cuts") (width 0.05))
      (gr_line (start 0 10) (end 0 0) (layer "Edge.Cuts") (width 0.05))
      (segment (start 1 1) (end 2 1) (width 0.25) (layer "F.Cu") (net 1))
      (via (at 2 1) (size 0.6) (drill 0.3) (layers "F.Cu" "B.Cu") (net 1))
    )"#;
    let imported = kicad_parser::parse_pcb(PCB_FIXTURE)
        .map_err(|error| format!("kicad-parser: PCB import failed: {error}"))?;
    ensure(imported.design.tracks.len() == 1, "kicad-parser: PCB import lost its track")?;
    ensure(imported.design.vias.len() == 1, "kicad-parser: PCB import lost its via")?;
    ensure(
        imported.design.net_name(imported.design.tracks[0].net_id) == Some("GND"),
        "kicad-parser: PCB import lost its net name",
    )?;
    let exported = kicad_parser::write_pcb(&imported.design)
        .map_err(|error| format!("kicad-parser: PCB export failed: {error}"))?;
    let round_trip = kicad_parser::parse_pcb(&exported)
        .map_err(|error| format!("kicad-parser: exported PCB did not parse: {error}"))?;
    ensure(
        round_trip.design.tracks.len() == 1 && round_trip.design.vias.len() == 1,
        "kicad-parser: exported PCB lost copper objects",
    )?;

    const SCH_FIXTURE: &str = r#"(kicad_sch (version 20231120) (generator eeschema)
      (uuid "00000000-0000-0000-0000-000000000011")
      (paper "A4")
      (lib_symbols (symbol "Device:R"))
      (wire (pts (xy 10 10) (xy 20 10)) (stroke (width 0) (type default)) (uuid "00000000-0000-0000-0000-000000000012"))
      (symbol (lib_id "Device:R") (at 30 20 90) (unit 1)
        (property "Reference" "R1" (at 30 20 0))
        (property "Value" "10k" (at 30 20 0))
        (uuid "00000000-0000-0000-0000-000000000013"))
      (sheet_instances (path "/" (page "1")))
    )"#;
    let schematic = kicad_parser::parse_schematic(SCH_FIXTURE)
        .map_err(|error| format!("kicad-parser: schematic import failed: {error}"))?;
    ensure(
        schematic.data.components.len() == 1,
        "kicad-parser: schematic import lost its component",
    )?;
    let schematic_export = kicad_parser::write_schematic(&schematic.data)
        .map_err(|error| format!("kicad-parser: schematic export failed: {error}"))?;
    let schematic_round_trip = kicad_parser::parse_schematic(&schematic_export)
        .map_err(|error| format!("kicad-parser: exported schematic did not parse: {error}"))?;
    ensure(
        schematic_round_trip.data.components.len() == 1,
        "kicad-parser: exported schematic lost its component",
    )?;

    const MOD_FIXTURE: &str = r#"(footprint "R_0603" (version 20240108) (generator pcbnew)
      (layer "F.Cu")
      (fp_line (start -1 0) (end 1 0) (stroke (width 0.1) (type solid)) (layer "F.SilkS"))
      (pad "1" smd roundrect (at -0.8 0) (size 1.4 0.8) (layers "F.Cu" "F.Paste" "F.Mask") (roundrect_rratio 0.2))
    )"#;
    let footprint = kicad_parser::parse_footprint(MOD_FIXTURE)
        .map_err(|error| format!("kicad-parser: footprint import failed: {error}"))?;
    let footprint_export = kicad_parser::write_footprint(&footprint.footprint)
        .map_err(|error| format!("kicad-parser: footprint export failed: {error}"))?;
    let footprint_round_trip = kicad_parser::parse_footprint(&footprint_export)
        .map_err(|error| format!("kicad-parser: exported footprint did not parse: {error}"))?;
    ensure(
        footprint_round_trip.footprint.pad_positions.len() == 1,
        "kicad-parser: exported footprint lost its pad",
    )?;

    println!(
        "  kicad-parser: loaded {} symbol(s) from {}, PCB/schematic/footprint round trips ok",
        library.symbols.len(),
        path.display()
    );
    Ok(())
}
