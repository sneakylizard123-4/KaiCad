# KaiCad — KiCad Parity Program

KaiCad's product target is a **full KiCad replacement**: a native EDA
application that can take a real project from schematic capture through board
layout and manufacturing output without requiring KiCad or a cloud service.
This is the long-range target, not a statement about the current pre-alpha
tree.

Parity means **workflow and file compatibility**, not a pixel-for-pixel clone
of KiCad's interface. A user must be able to open a KiCad project, edit it,
save it without losing data KaiCad does not understand, and produce outputs
that manufacturing tools accept.

## Where the implementation stands

The current tree has real engines and a first interactive editor, but it does
not yet have a complete EDA workflow.

| Area | Implemented now | Missing for parity |
|------|-----------------|--------------------|
| **Project model** | `core::Project` with schematic and board metadata; `PcbDesign` JSON; SQLite project/board persistence; `.kicad_pro` JSON preserved by the CLI | One project document that owns schematic and board together; automatic project/sheet discovery; project manager |
| **Schematic** | Symbols, sheets, nets, wires/labels data types; netlist generation; connectivity graph; five ERC rules; KiCad netlist export; `.kicad_sch` subset read/write with opaque source preservation | Schematic canvas; symbol placement; wire/bus drawing; annotation; symbol chooser; geometry-based connectivity and full ERC |
| **PCB** | Outline geometry; layer stack validation; footprint libraries; pads, tracks and vias; DRC; single/multi-layer A* routing; push-and-shove; blind/buried vias; click-to-route track editor with undo | Footprint placement and editing; pad/via editing; semantic copper zones and keepouts; net classes and impedance rules; differential pairs; length matching; ratsnest; stackup editor; selection and measurement |
| **Manufacturing** | None | Gerber, Excellon drill, BOM, pick-and-place, 3D models and enclosure/manufacturing packages |
| **Libraries** | KiCad `.kicad_sym` import; `.kicad_mod` import/export; KaiCad JSON footprint libraries | Symbol and footprint editors; `sym-lib-table`/`fp-lib-table` handling and versioned libraries |
| **Board compatibility** | `.kicad_pcb` subset import/export; unknown constructs (zones, arcs, graphics, text, models, setup) retained and re-emitted; footprint rotation and side preserved; explicit warnings; `kai-cad pcb import|export` | Semantic models for zones, arcs, graphics and DRC setup; full-fidelity round trip and a broad KiCad 9/10 corpus |
| **User interface** | Native Studio window; 2D/3D software viewport; board open/save; DRC inspector; 2D track routing; undo/redo; typed file paths | Project workspace; schematic editor; footprint placement; native file picker; pan/zoom; copy/paste; preferences; complete editing commands and keyboard workflow |
| **Compatibility** | Export-only KiCad netlist; `.kicad_sym` import; `.kicad_pcb`, `.kicad_sch`, `.kicad_mod` and `.kicad_pro` file-level read/write; unknown-node preservation; four-fixture compatibility corpus | Full semantic `.kicad_sch`/`.kicad_pcb` support, recursive project bundles, library tables and a broad KiCad 9/10 compatibility corpus |
| **Ecosystem** | kAI suggestion validation; collaboration data structures; local plugin metadata registry | Model-backed kAI generation; network sync transport; plugin execution and sandboxing; Python/scripting compatibility; SPICE/simulation integration |
| **Rendering** | Deterministic CPU 2D/3D rasterizer | Vulkan GPU backend and larger-scene performance work |

## Delivery sequence

The sequence is ordered so each stage produces a usable loop rather than
another isolated subsystem.

### P0 — Integrated editable project

- Define one project document that owns the schematic and PCB design.
- Open, edit and save that document from Studio.
- Move board edits behind a shared transaction model with complete undo/redo.
- Add footprint placement, selection and deletion.
- Transfer net and footprint assignments from schematic data to the board.

Exit criterion: a user can open a project, place footprints, route tracks,
run DRC and save the result.

### P1 — Schematic capture

- Draw and edit symbols, wires, junctions, labels, power symbols and buses.
- Derive connectivity from schematic geometry, not only explicit net lists.
- Add hierarchical sheets, sheet pins and annotation.
- Extend ERC to cover pin conflicts and connectivity semantics.
- Add schematic-to-PCB cross-probing and ratsnest update.

Exit criterion: a small circuit can be captured, ERC-checked and transferred
to a routed board entirely inside KaiCad.

### P2 — KiCad interoperability (file layer landed; exit criterion open)

- [x] Read and write the `.kicad_pcb` subset KaiCad's board model represents.
- [x] Read and write a `.kicad_sch` subset: components, wires, labels,
  junctions, no-connects and sheet symbols, with `lib_symbols` and unmodelled
  constructs retained for export.
- [x] Preserve `.kicad_pro` JSON, including settings KaiCad does not model.
- [x] Read and write `.kicad_mod` footprints with pad shapes, graphics, text and
  3D models retained.
- [x] Preserve unknown `.kicad_pcb` constructs (zones, arcs, drawings, text,
  models, setup) and unknown schematic constructs on round trip.
- [x] Build a small compatibility corpus (board, schematic, footprint, project)
  and validate exported files with KiCad 10.0.6 command-line tools.
- [ ] Give zones, arcs, graphics, non-rectangular pad shapes and DRC setup
  semantic models; today they round-trip but DRC still treats pads as
  rectangles and ignores zone copper.
- [ ] Load a whole project bundle recursively across hierarchical sheet files.
- [ ] Handle `sym-lib-table` and `fp-lib-table`.
- [ ] Broaden the corpus and exercise edit/save/reopen on a complete hierarchy.

Exit criterion: representative KiCad projects open, edit, save and reopen
without silent loss of design data. **This is not yet met.** The file layer
now survives structural round trips, but the semantic and hierarchy gaps above
mean P2 remains open.

### Supported `.kicad_pcb` subset today

The exporter and importer are deliberately honest about their boundary:

- **Preserved:** board layer table and copper stackup order, named nets, pad
  numbers, `Edge.Cuts` line/rectangle/polygon outlines, footprint placement,
  rotation and side, pad position/size/rotation/net/type/shape, track geometry,
  width, layer and net, and via position/size/drill/span/net.
- **Retained opaquely:** zones and fills, copper/graphical arcs, top-level
  drawings and text, groups, dimensions, images, targets, rule areas, footprint
  graphics, properties, UUIDs, 3D models, setup/title blocks and unknown root
  nodes. They are stored in `PcbDesign::source` or `Footprint::preserved` and
  re-emitted.
- **Approximated for checking only:** non-rectangular pads retain their source
  shape, but DRC and routing still use rectangular copper.
- **Not claimed:** semantic editing of zones/arcs/graphics, DRC-rule import or
  byte-for-byte preservation.

Use the CLI for conversion; Studio currently opens KaiCad board JSON:

```bash
cargo run -p kai-cad -- pcb import board.kicad_pcb board.kaicad.json
cargo run -p kai-cad -- pcb export board.kaicad.json board.kicad_pcb
```

Import warnings are printed to stderr and counted in the summary. The library
API is `kicad_parser::{load_pcb, parse_pcb, save_pcb, write_pcb}`; `parse_pcb`
returns `PcbImport { design, warnings }`.

### Supported `.kicad_sch`, `.kicad_mod` and `.kicad_pro` subset

- Schematic components map to `core::Component` with placement, unit, library
  id and properties. Junctions, no-connects, wires and the four label types map
  to the sheet model. Child sheet symbols record their file name and placement.
- `lib_symbols`, property effects, pin uuid blocks, instance paths, buses, text,
  images and unknown nodes are retained in `SchematicData::source` or on the
  owning component/sheet.
- Standalone footprints map to `pcb::Footprint`, including layer, pad shape/kind,
  graphics, text and models.
- `.kicad_pro` is treated as preserved JSON; no project settings are
  interpreted yet.

```bash
cargo run -p kai-cad -- sch import main.kicad_sch main.kaicad.json
cargo run -p kai-cad -- sch export main.kaicad.json main.kicad_sch
cargo run -p kai-cad -- footprint import part.kicad_mod part.json
cargo run -p kai-cad -- footprint export part.json part.kicad_mod
cargo run -p kai-cad -- project import project.kicad_pro project.json
cargo run -p kai-cad -- project export project.json project.kicad_pro
```

The schematic writer only emits one sheet file. It does not recursively discover
or rewrite child sheet files.

### Compatibility corpus and validation

`kicad-parser/tests/fixtures/` contains one representative `.kicad_sch`,
`.kicad_pcb`, `.kicad_mod` and `.kicad_pro`. `kicad-parser/tests/compatibility.rs`
round-trips each file and checks that components, pads, zones, arcs, graphics,
models and unknown nodes survive.

KiCad 10.0.6 validation performed on generated files:

- `kicad-cli sch export svg` plotted the exported schematic.
- `kicad-cli fp export svg` plotted the exported footprint.
- `kicad-cli sch erc` and `kicad-cli pcb drc` parsed the exported files and
  reported the expected fixture connectivity/geometry warnings, not syntax
  errors.

The corpus is intentionally small. It is evidence for the structural round trip,
not for arbitrary KiCad projects.

### P3 — Manufacturing and advanced PCB

- Copper zones, keepouts, copper pours and net classes.
- Differential pairs, length matching and impedance constraints.
- Gerber, drill, BOM and pick-and-place outputs.
- 3D models, board cutouts and mechanical documentation.
- Stackup editing and manufacturing-rule checks.

Exit criterion: a board can be fabricated from outputs generated by KaiCad.

### P4 — Ecosystem and scale

- Symbol and footprint editors.
- Scripting/plugin execution with a sandbox boundary.
- Network collaboration and version history transport.
- Model-backed kAI suggestions with deterministic validation.
- Simulation integration and advanced analysis.

### P5 — Release quality

- Full undo coverage and crash recovery.
- Performance budgets for large boards.
- Accessibility, native file dialogs and platform packaging.
- Linux, Windows and macOS testing.
- Stable project schema and migration policy.

## Current increment

This change adds a real, bounded KiCad file-interchange layer:

- `kicad_parser::parse_pcb`/`load_pcb` import supported `.kicad_pcb` data into
  `pcb::PcbDesign` and retain unmodelled constructs in `PcbDesign::source`.
- `kicad_parser::write_pcb`/`save_pcb` re-emit the supported data and the
  preserved constructs. Footprint rotation, side and original pad kind/shape
  survive the round trip.
- `kicad_parser::parse_schematic`/`write_schematic` cover components, wires,
  labels, junctions, no-connects and sheet symbols, with opaque preservation of
  library symbols and unmodelled nodes.
- `kicad_parser::parse_footprint`/`write_footprint` cover standalone
  `.kicad_mod` files.
- `ProjectFile` preserves `.kicad_pro` JSON.
- The CLI adds `pcb`, `sch`, `footprint` and `project` import/export commands,
  with warning and object-count summaries. The self-check round-trips board,
  schematic and footprint fixtures.
- The corpus fixtures and structural round-trip tests live under
  `kicad-parser/tests/`.
- KiCad 10.0.6's `sch export svg` and `fp export svg` plotted generated files;
  `sch erc` and `pcb drc` parsed them and reported only fixture-level warnings.

Full semantic PCB, recursive project/sheet loading, library tables, schematic
capture, manufacturing outputs and a broad compatibility corpus remain future
work. Do not mark the P2 exit criterion complete until those are in place and a
representative hierarchy passes edit/save/reopen under KiCad.
