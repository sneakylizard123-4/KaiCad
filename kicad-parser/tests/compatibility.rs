//! Compatibility corpus round trips for the file formats KaiCad supports.

use kicad_parser::{
    ProjectBundle, ProjectFile, parse_footprint, parse_pcb, parse_schematic, write_footprint,
    write_pcb, write_schematic,
};

#[test]
fn schematic_corpus_round_trip_keeps_components_library_and_unknown_text() {
    let imported = parse_schematic(include_str!("fixtures/compat.kicad_sch"))
        .expect("schematic fixture imports");
    assert_eq!(imported.data.components.len(), 1);
    assert_eq!(imported.data.components[0].reference_designator, "R1");
    assert_eq!(imported.data.components[0].library_id, "Device:R");
    assert_eq!(imported.data.sheets[0].wires.len(), 2);
    assert_eq!(imported.data.sheets[0].labels.len(), 2);

    let document = write_schematic(&imported.data).expect("schematic exports");
    assert!(document.contains("(lib_symbols"));
    assert!(document.contains("(text \"KaiCad compatibility fixture\""));
    assert!(document.contains("(symbol (lib_id \"Device:R\")"));

    let restored = parse_schematic(&document).expect("exported schematic reimports");
    assert_eq!(restored.data.components.len(), 1);
    assert_eq!(restored.data.sheets[0].wires.len(), 2);
    assert_eq!(
        restored.data.components[0]
            .properties
            .iter()
            .find(|property| property.name == "Value")
            .map(|property| property.value.as_str()),
        Some("10k")
    );
}

#[test]
fn board_corpus_round_trip_keeps_zones_arcs_graphics_and_models() {
    let imported =
        parse_pcb(include_str!("fixtures/compat.kicad_pcb")).expect("PCB fixture imports");
    assert_eq!(imported.design.footprints.len(), 2);
    assert_eq!(imported.design.tracks.len(), 1);
    assert_eq!(imported.design.vias.len(), 1);
    assert_eq!(imported.design.zones.len(), 1);
    assert_eq!(imported.design.arcs.len(), 1);
    assert_eq!(imported.design.graphics.len(), 1);
    assert!(
        imported
            .design
            .graphics
            .iter()
            .any(|graphic| { matches!(graphic.kind, pcb::BoardGraphicKind::Text { .. }) }),
        "missing typed board text"
    );
    assert!(
        imported.design.source.as_ref().expect("source metadata").contains("gr_arc"),
        "missing preserved Edge.Cuts arc"
    );
    assert!(
        imported.design.footprints[0].preserved.iter().any(|node| node.head == "model"),
        "missing preserved footprint model"
    );

    let document = write_pcb(&imported.design).expect("PCB exports");
    assert!(document.contains("(zone "));
    assert!(document.contains("(gr_arc "));
    assert!(document.contains("(model "));
    assert!(document.contains("(at 10 10 90)"));

    let restored = parse_pcb(&document).expect("exported PCB reimports");
    assert_eq!(restored.design.footprints.len(), 2);
    assert_eq!(restored.design.footprints[0].rotation, 90.0);
    assert_eq!(restored.design.footprints[1].layer, "B.Cu");
    assert_eq!(restored.design.footprints[0].pad_positions[0].shape, "roundrect");
    assert_eq!(restored.design.zones.len(), 1);
    assert_eq!(restored.design.arcs.len(), 1);
    assert_eq!(restored.design.graphics.len(), 1);
}

#[test]
fn kicad10_board_corpus_round_trips_name_only_nets_and_semantic_objects() {
    let imported = parse_pcb(include_str!("fixtures/compat-v10.kicad_pcb"))
        .expect("KiCad 10 board fixture imports");
    assert_eq!(imported.design.zones.len(), 1);
    assert_eq!(imported.design.arcs.len(), 1);
    assert_eq!(imported.design.graphics.len(), 1);
    assert_eq!(imported.design.footprints.len(), 2);
    assert!(imported.design.nets.iter().any(|net| net.name == "GND"));

    let document = write_pcb(&imported.design).expect("KiCad 10 board exports");
    assert!(document.contains("(version 20260206)"));
    assert!(!document.lines().any(|line| line.starts_with("  (uuid ")));
    assert!(document.contains("(zone (net \"GND\")"));
    assert!(document.contains("(arc (start"));
    assert!(document.contains("(gr_text \"KaiCad\""));

    let restored = parse_pcb(&document).expect("exported KiCad 10 board reimports");
    assert_eq!(restored.design.zones.len(), 1);
    assert_eq!(restored.design.arcs.len(), 1);
    assert_eq!(restored.design.graphics.len(), 1);
    assert_eq!(restored.design.footprints.len(), 2);
}

#[test]
fn footprint_corpus_round_trip_keeps_graphics_text_and_pad_shapes() {
    let imported = parse_footprint(include_str!("fixtures/compat.kicad_mod"))
        .expect("footprint fixture imports");
    assert_eq!(imported.footprint.pad_positions.len(), 2);
    assert_eq!(imported.footprint.pad_positions[0].shape, "roundrect");
    assert!(imported.footprint.preserved.iter().any(|node| node.head == "fp_line"));
    assert!(imported.footprint.preserved.iter().any(|node| node.head == "model"));

    let document = write_footprint(&imported.footprint).expect("footprint exports");
    assert!(document.contains("(fp_line "));
    assert!(document.contains("(fp_text "));
    assert!(document.contains("(model "));
    assert!(document.contains("smd roundrect"));

    let restored = parse_footprint(&document).expect("exported footprint reimports");
    assert_eq!(restored.footprint.pad_positions.len(), 2);
    assert_eq!(restored.footprint.reference_designator, "REF**");
}
#[test]
fn project_json_round_trip_keeps_unknown_settings() {
    let project = ProjectFile::parse(include_str!("fixtures/compat.kicad_pro"))
        .expect("project fixture parses");
    let json = project.to_json().expect("project serializes");
    let restored = ProjectFile::parse(&json).expect("project reimports");
    assert_eq!(restored.value, project.value);
    assert_eq!(restored.value["text_variables"]["KICAD8_3DMODEL_DIR"], "/usr/share/kicad/3dmodels");
}

#[test]
fn project_bundle_loads_and_rewrites_a_child_sheet() {
    let directory = tempfile::tempdir().expect("temp project directory");
    let root = directory.path().join("bundle.kicad_sch");
    let child = directory.path().join("child.kicad_sch");
    let project = directory.path().join("bundle.kicad_pro");
    std::fs::write(
        &root,
        r#"(kicad_sch (version 20231120) (generator eeschema)
          (uuid "aaaaaaaa-0000-0000-0000-000000000001")
          (paper "A4")
          (lib_symbols)
          (sheet (at 10 10) (size 20 10)
            (property "Sheetname" "Child" (at 10 10 0))
            (property "Sheetfile" "child.kicad_sch" (at 10 10 0))
            (uuid "aaaaaaaa-0000-0000-0000-000000000002"))
          (sheet_instances (path "/" (page "1")))
        )"#,
    )
    .expect("write root schematic");
    std::fs::write(
        &child,
        r#"(kicad_sch (version 20231120) (generator eeschema)
          (uuid "bbbbbbbb-0000-0000-0000-000000000001")
          (paper "A4")
          (lib_symbols)
          (wire (pts (xy 5 5) (xy 15 5)) (stroke (width 0) (type default))
            (uuid "bbbbbbbb-0000-0000-0000-000000000002"))
          (sheet_instances (path "/" (page "1")))
        )"#,
    )
    .expect("write child schematic");
    std::fs::write(
        &project,
        r#"{"meta":{"filename":"bundle.kicad_pro","version":3},"sheets":[],"board":{"design_settings":{"rules":{"min_clearance":0.3,"min_track_width":0.25,"min_via_annular_width":0.08}}}}"#,
    )
    .expect("write project file");
    std::fs::write(
        directory.path().join("bundle.kicad_pcb"),
        include_str!("fixtures/compat.kicad_pcb"),
    )
    .expect("write board");
    std::fs::create_dir_all(directory.path().join("libs")).expect("create library directory");
    std::fs::write(
        directory.path().join("libs/Device.kicad_sym"),
        "(kicad_symbol_lib (version 20231120) (symbol \"Device:R\"))\n",
    )
    .expect("write local symbol library");
    std::fs::create_dir_all(directory.path().join("footprints.pretty"))
        .expect("create footprint directory");
    std::fs::write(
        directory.path().join("footprints.pretty/Part.kicad_mod"),
        include_str!("fixtures/compat.kicad_mod"),
    )
    .expect("write local footprint");
    std::fs::write(
        directory.path().join("sym-lib-table"),
        r#"(sym_lib_table (version 7)
          (lib (name "Device")(type "KiCad")(uri "${KIPRJMOD}/libs/Device.kicad_sym")(options "")(descr "Local symbols")))"#,
    )
    .expect("write symbol table");
    std::fs::write(
        directory.path().join("fp-lib-table"),
        r#"(fp_lib_table (version 7)
          (lib (name "Local")(type "KiCad")(uri "${KIPRJMOD}/footprints.pretty")(options "")(descr "")))"#,
    )
    .expect("write footprint table");

    let mut bundle = ProjectBundle::load(&project).expect("bundle loads");
    assert_eq!(bundle.sheet_file_count(), 2);
    assert_eq!(bundle.symbol_library_table.as_ref().expect("symbol table").libraries.len(), 1);
    assert_eq!(
        bundle.footprint_library_table.as_ref().expect("footprint table").libraries[0].name,
        "Local"
    );
    assert!(!bundle.warnings.iter().any(|warning| warning.contains("missing project library")));
    let rules = &bundle.board.as_ref().expect("sibling board").rules;
    assert!((rules.clearance - 0.3).abs() < 1e-9);
    assert!((rules.min_track_width - 0.25).abs() < 1e-9);
    assert!((rules.min_annular_ring - 0.08).abs() < 1e-9);

    let child_sheet = bundle
        .schematic
        .sheets
        .iter_mut()
        .find(|sheet| sheet.file_name == "child.kicad_sch")
        .expect("child sheet was merged");
    assert_eq!(child_sheet.wires.len(), 1);
    assert!(child_sheet.source.is_some());
    let mut added_wire = child_sheet.wires[0].clone();
    added_wire.id = uuid::Uuid::from_u128(42);
    for point in &mut added_wire.points {
        point.y += 2.0;
    }
    child_sheet.wires.push(added_wire);

    let output = directory.path().join("round-trip");
    bundle.save(&output).expect("bundle saves");
    assert!(output.join("bundle.kicad_pro").exists());
    assert!(output.join("bundle.kicad_sch").exists());
    assert!(output.join("child.kicad_sch").exists());
    assert!(output.join("bundle.kicad_pcb").exists());
    assert!(output.join("sym-lib-table").exists());
    assert!(output.join("fp-lib-table").exists());
    assert!(output.join("libs/Device.kicad_sym").exists());
    assert!(output.join("footprints.pretty/Part.kicad_mod").exists());

    let restored =
        ProjectBundle::load(output.join("bundle.kicad_pro")).expect("rewritten bundle reloads");
    assert_eq!(restored.schematic.sheets.len(), 2);
    assert_eq!(restored.symbol_library_table.expect("symbol table reloads").libraries.len(), 1);
    assert_eq!(restored.board.as_ref().expect("board reloads").rules.min_track_width, 0.25);
    assert_eq!(
        restored
            .schematic
            .sheets
            .iter()
            .find(|sheet| sheet.file_name == "child.kicad_sch")
            .expect("rewritten child sheet")
            .wires
            .len(),
        2
    );
}
