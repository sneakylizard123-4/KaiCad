//! Tests for the `store` crate.

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{Store, StoreError};

fn fixed_uuid(text: &str) -> Uuid {
    let mut bytes = [0u8; 16];
    for (i, byte) in text.bytes().enumerate().take(16) {
        bytes[i] = byte;
    }
    Uuid::from_bytes(bytes)
}

fn fixed_time(text: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(text)
        .expect("valid RFC 3339 fixture timestamp")
        .with_timezone(&Utc)
}

fn fixture_project() -> core::Project {
    fixture_project_with_namespace("default")
}

fn fixture_project_with_namespace(namespace: &str) -> core::Project {
    let sheet_id = fixed_uuid(&format!("{namespace}:sheet"));
    let symbol_id = fixed_uuid(&format!("{namespace}:symbol"));
    let pin_id_a = fixed_uuid(&format!("{namespace}:pin-a"));
    let pin_id_b = fixed_uuid(&format!("{namespace}:pin-b"));
    let component_id = fixed_uuid(&format!("{namespace}:component"));
    let net_id = fixed_uuid(&format!("{namespace}:net"));
    let footprint_id = fixed_uuid(&format!("{namespace}:footprint"));

    let mut project = core::Project {
        id: fixed_uuid(&format!("{namespace}:project")),
        name: "fixture".to_string(),
        version: "1.0".to_string(),
        schematic: core::SchematicData::default(),
        pcb: core::PcbData::default(),
        metadata: core::ProjectMetadata {
            created_at: fixed_time("2024-01-01T00:00:00Z"),
            modified_at: fixed_time("2024-01-02T12:34:56Z"),
            author: "test author".to_string(),
            description: "fixture project".to_string(),
        },
    };

    project.schematic.sheets.push(core::Sheet {
        id: sheet_id,
        name: "sheet-a".to_string(),
        parent: None,
        children: Vec::new(),
        components: vec![component_id],
        wires: Vec::new(),
        labels: Vec::new(),
    });

    project.schematic.symbols.push(core::Symbol {
        id: symbol_id,
        name: "R".to_string(),
        pins: vec![
            core::Pin {
                id: pin_id_a,
                number: "1".to_string(),
                name: "A".to_string(),
                ..core::Pin::default()
            },
            core::Pin {
                id: pin_id_b,
                number: "2".to_string(),
                name: "B".to_string(),
                ..core::Pin::default()
            },
        ],
        library: "fixture-lib".to_string(),
    });

    project.schematic.components.push(core::Component {
        id: component_id,
        reference_designator: "R1".to_string(),
        symbol_id,
        footprint_id,
        properties: vec![core::ComponentProperty {
            name: "value".to_string(),
            value: "10k".to_string(),
        }],
    });

    project.schematic.nets.push(core::Net {
        id: net_id,
        name: "NET1".to_string(),
        pins: vec![pin_id_a, pin_id_b],
        netclass: core::NetClass::Signal,
    });

    project.pcb.outline = core::PcbOutline::rectangle(100.0, 80.0);
    project.pcb.layers.push(core::PcbLayer::new(0, "F.Cu", core::LayerKind::Copper));
    project.pcb.stackup.layers.push(0);
    project.pcb.footprint_ids.push(footprint_id);

    project
}

fn fixture_symbol_library(namespace: &str, name: &str) -> core::SymbolLibrary {
    core::SymbolLibrary {
        id: fixed_uuid(&format!("{namespace}:library")),
        name: name.to_string(),
        source: core::SymbolLibrarySource::KiCad { path: "/libs/Device.kicad_sym".to_string() },
        symbols: vec![core::Symbol {
            id: fixed_uuid(&format!("{namespace}:library-symbol")),
            name: "Device:R".to_string(),
            pins: vec![core::Pin {
                id: fixed_uuid(&format!("{namespace}:library-pin")),
                number: "1".to_string(),
                name: "~".to_string(),
                ..core::Pin::default()
            }],
            library: name.to_string(),
        }],
    }
}

#[test]
fn open_in_memory_applies_initial_migration() {
    let store = Store::open_in_memory().expect("open in-memory store");
    assert_eq!(store.schema_version().expect("schema version"), 4);
}

#[test]
fn migrations_are_idempotent() {
    let temp = tempfile::tempdir().expect("temp dir");
    let path = temp.path().join("idempotent.db");

    {
        let store = Store::open(&path).expect("first open");
        assert_eq!(store.schema_version().expect("schema version"), 4);
    }
    {
        let store = Store::open(&path).expect("second open");
        assert_eq!(store.schema_version().expect("schema version"), 4);
    }
}

#[test]
fn create_and_load_round_trip_preserves_project() {
    let mut store = Store::open_in_memory().expect("open in-memory store");
    let project = fixture_project();

    store.create_project(&project).expect("create project");
    let loaded = store.load_project(project.id).expect("load project").expect("project exists");

    assert_eq!(loaded.id, project.id);
    assert_eq!(loaded.name, project.name);
    assert_eq!(loaded.version, project.version);
    assert_eq!(loaded.metadata.created_at, project.metadata.created_at);
    assert_eq!(loaded.metadata.modified_at, project.metadata.modified_at);
    assert_eq!(loaded.metadata.author, project.metadata.author);
    assert_eq!(loaded.metadata.description, project.metadata.description);

    assert_eq!(loaded.schematic.sheets.len(), 1);
    assert_eq!(loaded.schematic.sheets[0].id, project.schematic.sheets[0].id);
    assert_eq!(loaded.schematic.sheets[0].name, project.schematic.sheets[0].name);

    assert_eq!(loaded.schematic.symbols.len(), 1);
    assert_eq!(loaded.schematic.symbols[0].id, project.schematic.symbols[0].id);

    assert_eq!(loaded.schematic.components.len(), 1);
    assert_eq!(loaded.schematic.components[0].id, project.schematic.components[0].id);

    assert_eq!(loaded.schematic.nets.len(), 1);
    assert_eq!(loaded.schematic.nets[0].id, project.schematic.nets[0].id);

    assert_eq!(loaded.pcb.outline, project.pcb.outline);
    assert_eq!(loaded.pcb.layers, project.pcb.layers);
    assert_eq!(loaded.pcb.stackup, project.pcb.stackup);
    assert_eq!(loaded.pcb.footprint_ids, project.pcb.footprint_ids);
}

#[test]
fn create_duplicate_id_returns_error_and_leaves_first_row_intact() {
    let mut store = Store::open_in_memory().expect("open in-memory store");
    let project = fixture_project();

    store.create_project(&project).expect("first create");

    let result = store.create_project(&project);
    match result {
        Err(StoreError::Duplicate(id)) => assert_eq!(id, project.id),
        other => panic!("expected Duplicate error, got {other:?}"),
    }

    let loaded =
        store.load_project(project.id).expect("load project").expect("project still exists");
    assert_eq!(loaded.name, project.name);
}

#[test]
fn duplicate_project_reuses_entity_ids_across_projects() {
    let mut store = Store::open_in_memory().expect("open in-memory store");
    let original = fixture_project();

    store.create_project(&original).expect("create original project");

    // Duplicating a project keeps its entity ids; only the project id changes.
    let mut copy = original.clone();
    copy.id = fixed_uuid("copy:project");
    copy.name = "copy".to_string();

    store.create_project(&copy).expect("a copied project may reuse the original's entity ids");

    let loaded = store.load_project(copy.id).expect("load copy").expect("copy exists");
    assert_eq!(loaded.schematic.symbols[0].id, original.schematic.symbols[0].id);
    assert_eq!(loaded.schematic.nets[0].id, original.schematic.nets[0].id);
    assert_eq!(store.list_projects().expect("list projects").len(), 2);

    assert!(store.delete_project(copy.id).expect("delete copy"));
    let counts = projection_counts(&store, original.id).expect("projection counts");
    assert_eq!(counts.symbols, 1);
    assert_eq!(counts.nets, 1);
}

#[test]
fn list_projects_returns_ordered_summaries() {
    let mut store = Store::open_in_memory().expect("open in-memory store");

    let mut older = fixture_project_with_namespace("older");
    older.name = "alpha".to_string();
    older.metadata.modified_at = fixed_time("2024-01-01T00:00:00Z");

    let mut newer = fixture_project_with_namespace("newer");
    newer.name = "beta".to_string();
    newer.metadata.modified_at = fixed_time("2024-01-03T00:00:00Z");

    let mut same_time = fixture_project_with_namespace("same-time");
    same_time.name = "gamma".to_string();
    same_time.metadata.modified_at = newer.metadata.modified_at;

    store.create_project(&older).expect("create older");
    store.create_project(&newer).expect("create newer");
    store.create_project(&same_time).expect("create same-time");

    let summaries = store.list_projects().expect("list projects");
    assert_eq!(summaries.len(), 3);

    let names: Vec<&str> = summaries.iter().map(|summary| summary.name.as_str()).collect();
    assert_eq!(names, vec!["beta", "gamma", "alpha"]);

    let ids: Vec<Uuid> = summaries.iter().map(|summary| summary.id).collect();
    let unique: HashSet<_> = ids.iter().copied().collect();
    assert_eq!(unique.len(), ids.len());
}

#[test]
fn update_project_persists_changes() {
    let mut store = Store::open_in_memory().expect("open in-memory store");
    let mut project = fixture_project();

    store.create_project(&project).expect("create project");

    project.name = "renamed".to_string();
    project.version = "2.0".to_string();
    project.metadata.author = "updated author".to_string();
    project.schematic.nets.clear();

    let updated = store.update_project(&project).expect("update project");
    assert!(updated);

    let loaded = store.load_project(project.id).expect("load project").expect("project exists");
    assert_eq!(loaded.name, "renamed");
    assert_eq!(loaded.version, "2.0");
    assert_eq!(loaded.metadata.author, "updated author");
    assert!(loaded.schematic.nets.is_empty());
}

#[test]
fn update_unknown_project_returns_false() {
    let mut store = Store::open_in_memory().expect("open in-memory store");
    let project = fixture_project();

    let updated = store.update_project(&project).expect("update project");
    assert!(!updated);
}

#[test]
fn delete_project_returns_true_once_then_false() {
    let mut store = Store::open_in_memory().expect("open in-memory store");
    let project = fixture_project();

    store.create_project(&project).expect("create project");

    let first = store.delete_project(project.id).expect("delete first");
    assert!(first);

    let second = store.delete_project(project.id).expect("delete second");
    assert!(!second);

    assert!(store.load_project(project.id).expect("load").is_none());
}

#[test]
fn load_unknown_project_returns_none() {
    let store = Store::open_in_memory().expect("open in-memory store");
    let id = fixed_uuid("missing-project");

    assert!(store.load_project(id).expect("load project").is_none());
}

#[test]
fn file_backed_database_survives_reopen() {
    let temp = tempfile::tempdir().expect("temp dir");
    let path = temp.path().join("persistent.db");

    {
        let mut store = Store::open(&path).expect("open store");
        store.create_project(&fixture_project()).expect("create project");
    }

    {
        let store = Store::open(&path).expect("reopen store");
        let loaded = store
            .load_project(fixture_project().id)
            .expect("load project")
            .expect("project exists");
        assert_eq!(loaded.name, fixture_project().name);
    }
}

#[test]
fn projection_tables_match_document_counts() {
    let mut store = Store::open_in_memory().expect("open in-memory store");
    let project = fixture_project();

    store.create_project(&project).expect("create project");

    let counts = projection_counts(&store, project.id).expect("projection counts");
    assert_eq!(counts.sheets, project.schematic.sheets.len());
    assert_eq!(counts.symbols, project.schematic.symbols.len());
    assert_eq!(counts.components, project.schematic.components.len());
    assert_eq!(counts.nets, project.schematic.nets.len());
    assert_eq!(counts.footprints, project.pcb.footprint_ids.len());
}

#[test]
fn deleting_project_cascades_projection_rows() {
    let mut store = Store::open_in_memory().expect("open in-memory store");
    let project = fixture_project();

    store.create_project(&project).expect("create project");
    store.delete_project(project.id).expect("delete project");

    let counts = projection_counts(&store, project.id).expect("projection counts");
    assert_eq!(counts.sheets, 0);
    assert_eq!(counts.symbols, 0);
    assert_eq!(counts.components, 0);
    assert_eq!(counts.nets, 0);
    assert_eq!(counts.footprints, 0);
}

#[test]
fn list_project_summary_parses_metadata() {
    let mut store = Store::open_in_memory().expect("open in-memory store");
    let project = fixture_project();

    store.create_project(&project).expect("create project");

    let summaries = store.list_projects().expect("list projects");
    assert_eq!(summaries.len(), 1);

    let summary = &summaries[0];
    assert_eq!(summary.id, project.id);
    assert_eq!(summary.name, project.name);
    assert_eq!(summary.version, project.version);
    assert_eq!(summary.modified_at, project.metadata.modified_at);
}

#[test]
fn symbol_library_round_trip() {
    let mut store = Store::open_in_memory().expect("open in-memory store");
    let library = fixture_symbol_library("round-trip", "Device");

    store.create_symbol_library(&library).expect("create library");
    let loaded =
        store.load_symbol_library(library.id).expect("load library").expect("library exists");

    assert_eq!(loaded.id, library.id);
    assert_eq!(loaded.name, library.name);
    assert_eq!(loaded.source, library.source);
    assert_eq!(loaded.symbols.len(), 1);
    assert_eq!(loaded.symbols[0].id, library.symbols[0].id);
    assert_eq!(loaded.symbols[0].pins[0].number, "1");
}

#[test]
fn duplicate_symbol_library_id_returns_error() {
    let mut store = Store::open_in_memory().expect("open in-memory store");
    let library = fixture_symbol_library("duplicate", "Device");

    store.create_symbol_library(&library).expect("first create");

    let result = store.create_symbol_library(&library);
    match result {
        Err(StoreError::Duplicate(id)) => assert_eq!(id, library.id),
        other => panic!("expected Duplicate error, got {other:?}"),
    }

    assert!(store.load_symbol_library(library.id).expect("load library").is_some());
}

#[test]
fn list_symbol_libraries_is_ordered_by_name() {
    let mut store = Store::open_in_memory().expect("open in-memory store");
    let zeta = fixture_symbol_library("zeta", "zeta");
    let alpha = fixture_symbol_library("alpha", "alpha");
    let mid = fixture_symbol_library("mid", "mid");

    store.create_symbol_library(&zeta).expect("create zeta");
    store.create_symbol_library(&alpha).expect("create alpha");
    store.create_symbol_library(&mid).expect("create mid");

    let libraries = store.list_symbol_libraries().expect("list libraries");
    let names: Vec<&str> = libraries.iter().map(|(_, name)| name.as_str()).collect();

    assert_eq!(names, vec!["alpha", "mid", "zeta"]);
}

#[test]
fn update_symbol_library_persists_changes() {
    let mut store = Store::open_in_memory().expect("open in-memory store");
    let mut library = fixture_symbol_library("update", "Device");

    store.create_symbol_library(&library).expect("create library");

    library.name = "Device-renamed".to_string();
    library.source = core::SymbolLibrarySource::Internal;
    library.symbols[0].pins.clear();

    assert!(store.update_symbol_library(&library).expect("update library"));

    let loaded =
        store.load_symbol_library(library.id).expect("load library").expect("library exists");
    assert_eq!(loaded.name, "Device-renamed");
    assert_eq!(loaded.source, core::SymbolLibrarySource::Internal);
    assert!(loaded.symbols[0].pins.is_empty());

    let mut missing = fixture_symbol_library("missing", "Missing");
    missing.id = fixed_uuid("missing:library");
    assert!(!store.update_symbol_library(&missing).expect("update missing"));
}

#[test]
fn delete_symbol_library_returns_true_once_then_false() {
    let mut store = Store::open_in_memory().expect("open in-memory store");
    let library = fixture_symbol_library("delete", "Device");

    store.create_symbol_library(&library).expect("create library");

    assert!(store.delete_symbol_library(library.id).expect("delete first"));
    assert!(!store.delete_symbol_library(library.id).expect("delete second"));
    assert!(store.load_symbol_library(library.id).expect("load").is_none());
}

fn fixture_pcb_design() -> pcb::PcbDesign {
    pcb::PcbDesign {
        outline: core::PcbOutline::rectangle(100.0, 80.0),
        layers: vec![core::PcbLayer::new(0, "F.Cu", core::LayerKind::Copper)],
        stackup: core::Stackup { layers: vec![0] },
        tracks: vec![pcb::Track {
            id: fixed_uuid("design:track"),
            net_id: fixed_uuid("design:net"),
            layer: 0,
            width: 0.3,
            path: vec![core::Point::new(10.0, 10.0), core::Point::new(40.0, 10.0)],
        }],
        rules: pcb::DrcRules { clearance: 0.25, ..pcb::DrcRules::default() },
        ..pcb::PcbDesign::default()
    }
}

#[test]
fn pcb_design_round_trip() {
    let mut store = Store::open_in_memory().expect("open in-memory store");
    let project = fixture_project();
    let design = fixture_pcb_design();

    store.create_project(&project).expect("create project");
    store.save_pcb_design(project.id, &design).expect("save design");

    let loaded = store.load_pcb_design(project.id).expect("load design").expect("design exists");

    assert_eq!(loaded.id, design.id);
    assert_eq!(loaded.outline, design.outline);
    assert_eq!(loaded.rules, design.rules);
    assert_eq!(loaded.tracks.len(), 1);
    assert_eq!(loaded.tracks[0].id, design.tracks[0].id);
    assert_eq!(loaded.tracks[0].width, 0.3);
    assert_eq!(loaded.tracks[0].path, design.tracks[0].path);
}

#[test]
fn pcb_design_upsert_replaces_existing_data() {
    let mut store = Store::open_in_memory().expect("open in-memory store");
    let project = fixture_project();
    let mut design = fixture_pcb_design();

    store.create_project(&project).expect("create project");
    store.save_pcb_design(project.id, &design).expect("save design");

    design.tracks[0].width = 0.5;
    design.rules.min_track_width = 0.2;
    store.save_pcb_design(project.id, &design).expect("save updated design");

    let loaded = store.load_pcb_design(project.id).expect("load design").expect("design exists");
    assert_eq!(loaded.tracks.len(), 1);
    assert_eq!(loaded.tracks[0].width, 0.5);
    assert_eq!(loaded.rules.min_track_width, 0.2);
}

#[test]
fn pcb_design_delete_and_project_cascade() {
    let mut store = Store::open_in_memory().expect("open in-memory store");
    let project = fixture_project();

    store.create_project(&project).expect("create project");
    store.save_pcb_design(project.id, &fixture_pcb_design()).expect("save design");

    assert!(store.delete_pcb_design(project.id).expect("delete design"));
    assert!(!store.delete_pcb_design(project.id).expect("delete design again"));
    assert!(store.load_pcb_design(project.id).expect("load design").is_none());

    store.save_pcb_design(project.id, &fixture_pcb_design()).expect("save design again");
    store.delete_project(project.id).expect("delete project");
    assert!(store.load_pcb_design(project.id).expect("load after cascade").is_none());
}

#[test]
fn saving_pcb_design_for_unknown_project_fails() {
    let mut store = Store::open_in_memory().expect("open in-memory store");
    let missing = fixed_uuid("missing:project");

    match store.save_pcb_design(missing, &fixture_pcb_design()) {
        Err(StoreError::MissingProject(id)) => assert_eq!(id, missing),
        other => panic!("expected MissingProject, got {other:?}"),
    }
}

struct ProjectionCounts {
    sheets: usize,
    symbols: usize,
    components: usize,
    nets: usize,
    footprints: usize,
}

fn projection_counts(store: &Store, project_id: Uuid) -> crate::Result<ProjectionCounts> {
    let id = project_id.to_string();
    let sheets: usize = store
        .connection
        .query_row("SELECT COUNT(*) FROM sheets WHERE project_id = ?1", [&id], |row| {
            row.get::<_, i64>(0)
        })?
        .try_into()
        .expect("non-negative count");
    let symbols: usize = store
        .connection
        .query_row("SELECT COUNT(*) FROM symbols WHERE project_id = ?1", [&id], |row| {
            row.get::<_, i64>(0)
        })?
        .try_into()
        .expect("non-negative count");
    let components: usize = store
        .connection
        .query_row("SELECT COUNT(*) FROM components WHERE project_id = ?1", [&id], |row| {
            row.get::<_, i64>(0)
        })?
        .try_into()
        .expect("non-negative count");
    let nets: usize = store
        .connection
        .query_row("SELECT COUNT(*) FROM nets WHERE project_id = ?1", [&id], |row| {
            row.get::<_, i64>(0)
        })?
        .try_into()
        .expect("non-negative count");
    let footprints: usize = store
        .connection
        .query_row("SELECT COUNT(*) FROM footprints WHERE project_id = ?1", [&id], |row| {
            row.get::<_, i64>(0)
        })?
        .try_into()
        .expect("non-negative count");

    Ok(ProjectionCounts { sheets, symbols, components, nets, footprints })
}
