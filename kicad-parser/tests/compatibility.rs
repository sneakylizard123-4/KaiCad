//! Compatibility corpus round trips for the file formats KaiCad supports.

use kicad_parser::{
    ProjectFile, parse_footprint, parse_pcb, parse_schematic, write_footprint, write_pcb,
    write_schematic,
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
    let source = imported.design.source.as_ref().expect("source metadata");
    for head in ["zone", "arc", "gr_arc", "gr_text"] {
        assert!(source.contains(head), "missing preserved {head}");
    }
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
    assert!(restored.design.source.as_ref().expect("source").contains("zone"));
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
