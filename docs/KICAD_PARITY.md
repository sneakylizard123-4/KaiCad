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
| **Project model** | `core::Project` with schematic and board metadata; `PcbDesign` JSON; SQLite project/board persistence; `.kicad_pro` JSON preserved; recursive project-bundle load/save with `sym-lib-table`/`fp-lib-table` and `${KIPRJMOD}` library copying | One project document that owns schematic and board together; project manager and installed-library discovery |
| **Schematic** | Symbols, sheets, nets, wires/labels data types; netlist generation; connectivity graph; five ERC rules; KiCad netlist export; `.kicad_sch` subset read/write with opaque source preservation; Studio schematic canvas with pan/zoom and two-point wire capture | Symbol graphics and placement; bus drawing; annotation; symbol chooser; geometry-based connectivity and full ERC |
| **PCB** | Outline geometry; layer stack validation; footprint libraries; pads, tracks, vias, copper arcs, zones, keepouts and board graphics; shape-aware DRC; single/multi-layer A* routing; push-and-shove; blind/buried vias; click-to-route track editor with undo | Footprint placement and editing; pad/via editing; zone editing and pouring; net classes and impedance rules; differential pairs; length matching; ratsnest; stackup editor; selection and measurement |
| **Manufacturing** | None | Gerber, Excellon drill, BOM, pick-and-place, 3D models and enclosure/manufacturing packages |
| **Libraries** | KiCad `.kicad_sym` import; `.kicad_mod` import/export; KaiCad JSON footprint libraries; `sym-lib-table`/`fp-lib-table` parsing and rewriting | Symbol and footprint editors; global library discovery, versioned libraries and library management UI |
| **Board compatibility** | `.kicad_pcb` subset import/export; semantic zones, keepouts, arcs, graphics, pad shapes and project DRC rules; unknown constructs retained and re-emitted; footprint rotation and side preserved; explicit warnings; `kai-cad pcb import|export` | Full-fidelity round trip for custom pad primitives, dimensions and every setup field; broader third-party project corpus |
| **User interface** | Native Studio window; board 2D/3D software viewport with wheel zoom, drag pan and fit; native `.kicad_pcb` open/save; DRC inspector; 2D track routing; schematic workspace with `.kicad_sch` open/save, pan/zoom and wire capture; undo/redo; typed file paths | Project workspace; schematic symbol editor and placement; footprint placement; native file picker; copy/paste; preferences; complete editing commands and keyboard workflow |
| **Compatibility** | Export-only KiCad netlist; `.kicad_sym` import; `.kicad_pcb`, `.kicad_sch`, `.kicad_mod` and `.kicad_pro` read/write; unknown-node preservation; five-fixture corpus; all 19 installed template boards and 19 project hierarchies with schematics validated through KiCad 10.0.6 | Full semantic `.kicad_sch` support, custom pad primitives, dimensions and a broader third-party project corpus |
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

### P2 — KiCad interoperability (complete)

- [x] Read and write the `.kicad_pcb` subset KaiCad's board model represents.
- [x] Read and write a `.kicad_sch` subset: components, wires, labels,
  junctions, no-connects and sheet symbols, with `lib_symbols` and unmodelled
  constructs retained for export.
- [x] Preserve `.kicad_pro` JSON, including settings KaiCad does not model.
- [x] Read and write `.kicad_mod` footprints with pad shapes, graphics, text and
  3D models retained.
- [x] Preserve unmodelled `.kicad_pcb` constructs (custom pad primitives,
  dimensions, some setup fields, models) and unknown schematic constructs on
  round trip.
- [x] Build a five-fixture compatibility corpus (KiCad 8 and 10 boards,
  schematic, footprint, project) and validate exported files with KiCad 10.0.6
  command-line tools.
- [x] Give zones, arcs, graphics, non-rectangular pad shapes and DRC setup
  semantic models. Copper arcs participate in clearance and width checks,
  zone fills are checked against foreign copper, keepouts reject tracks, vias,
  pads, footprints and other copper fills, and pad clearance uses the actual rect/roundrect/oval/
  circle/trapezoid outline.
- [x] Load and rewrite a project bundle recursively across hierarchical sheet files.
- [x] Handle `sym-lib-table` and `fp-lib-table`, including entry/order and
  unknown-node preservation plus `${KIPRJMOD}` library copying.
- [x] Map project `board.design_settings.rules` (`min_clearance`,
  `min_track_width`, `min_via_annular_width`) onto `pcb::DrcRules`.
- [x] Broaden the corpus and exercise edit/save/reopen on a complete hierarchy.
  The corpus includes a KiCad 10 board and a hierarchy test that edits a child
  sheet, writes tables and local libraries, reloads, and checks the edit survived.

Exit criterion: representative KiCad projects open, edit, save and reopen
without silent loss of design data. **Met for the documented subset:** the
semantic, library-table, hierarchy-edit and corpus checks above pass, and all
19 installed KiCad template project hierarchies with root schematics round-trip
through KaiCad and plot successfully in KiCad 10.0.6. Custom pad primitives,
dimensions, some setup fields and byte-for-byte formatting remain outside the
claim.

### Supported `.kicad_pcb` subset today

The exporter and importer are deliberately honest about their boundary:

- **Semantic models:** board layer table and copper stackup order, named nets
  (including KiCad 10's name-only net references), `Edge.Cuts`
  line/rectangle/polygon outlines, footprint placement/rotation/side, pad
  position/size/rotation/net/type/shape (rect, roundrect, oval, circle,
  trapezoid), tracks, vias, copper arcs, zones with fills and keepouts, and
  top-level line/rectangle/circle/arc/polygon/text graphics.
- **DRC integration:** pad clearance uses the modeled pad outline; copper arcs
  are checked as copper; zone fills are checked against foreign tracks, arcs,
  vias and pads; keepouts reject tracks, vias, pads, footprints and copper
  fills. Project
  `min_clearance`, `min_track_width` and `min_via_annular_width` settings load
  into `DrcRules` when a project bundle is opened.
- **Retained opaquely:** `Edge.Cuts` arcs (the outline model is polygonal),
  custom pad primitives, groups, dimensions, images, targets, setup/title
  blocks, footprint graphics/properties/models and unknown root nodes. They are
  stored in `PcbDesign::source` or `Footprint::preserved` and re-emitted.
- **Approximated:** custom pads use their bounding rectangle for DRC and
  routing still uses each pad's rectangle; dimensions and unsupported setup
  fields round-trip as source rather than becoming editable properties.
- **Not claimed:** byte-for-byte preservation, custom-pad primitive semantics,
  zone pouring/editing, or full-fidelity support for every KiCad construct. The
  internal board id has no KiCad file token, so it is not written to
  `.kicad_pcb`; item UUIDs are preserved instead.

Studio opens and saves `.kicad_pcb` directly. The CLI remains available for conversion and schematic/footprint/project workflows:

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
- `.kicad_pro` is preserved as JSON; board `min_clearance`, `min_track_width`
  and `min_via_annular_width` are additionally applied to the project board.

```bash
cargo run -p kai-cad -- sch import main.kicad_sch main.kaicad.json
cargo run -p kai-cad -- sch export main.kaicad.json main.kicad_sch
cargo run -p kai-cad -- footprint import part.kicad_mod part.json
cargo run -p kai-cad -- footprint export part.json part.kicad_mod
cargo run -p kai-cad -- project import project.kicad_pro project.json
cargo run -p kai-cad -- project export project.json project.kicad_pro
cargo run -p kai-cad -- project bundle project.kicad_pro output-dir
```

`ProjectBundle` recursively discovers child sheet files from the root schematic
and rewrites the root, each child, the sibling board, `sym-lib-table`,
`fp-lib-table` and any `${KIPRJMOD}` libraries referenced by those tables. Use
`kai-cad project bundle <project.kicad_pro|root.kicad_sch> <output-dir>`.
Global/user library tables outside the project directory are not merged; a
project template with no root schematic (the base `kicad.kicad_pro`) is not a
bundle.

### Compatibility corpus and validation

`kicad-parser/tests/fixtures/` contains representative `.kicad_sch`,
`.kicad_pcb`, `.kicad_mod` and `.kicad_pro` files plus a KiCad 10
`compat-v10.kicad_pcb` board with name-only nets. `kicad-parser/tests/compatibility.rs`
round-trips each file and checks that components, pads, zones, arcs, graphics,
models and unknown nodes survive. It also creates a two-file hierarchy with
project local libraries and library tables, edits a child sheet, loads it
through `ProjectBundle`, writes it to a new directory and reloads it to verify
the edit, tables and copied `${KIPRJMOD}` libraries.

KiCad 10.0.6 validation performed on generated files:

- `kicad-cli sch export svg` plotted the exported schematic.
- `kicad-cli fp export svg` plotted the exported footprint.
- `kicad-cli pcb export svg`, `pcb drc`, `pcb export gerbers` and
  `pcb export drill` accepted the Studio demo board and the compatibility
  board. They reported only connectivity, geometry and library warnings, never
  a syntax error.
- `kicad-cli pcb upgrade` converted the generated boards to the KiCad 10
  `20260206` format; KaiCad then imported and re-exported that board, and
  KiCad plotted the result again.
- A board containing the former top-level KaiCad UUID still imports and is
  repaired by re-saving. The repaired document has item UUIDs only.
- `kicad-cli sch erc` and `kicad-cli pcb drc` parsed the exported files and
  reported the expected fixture connectivity/geometry warnings, not syntax
  errors.
- All **19** installed KiCad template `.kicad_pcb` boards imported, exported
  and plotted with KiCad, including boards with user layers, graphics fills,
  dimensions, zones and arcs. The base `kicad.kicad_pro` template has no
  sibling schematic and is not a project bundle; the other **19** template
  project hierarchies bundled, saved, reopened and plotted all **38** rewritten
  schematics and **38** boards.

The corpus is representative of the installed KiCad 10 template set, not of
arbitrary third-party projects. Custom pad primitives, dimensions and some
setup fields still round-trip as preserved source rather than semantic data.

### P3 — Manufacturing and advanced PCB

- Interactive zone editing and copper pouring; net classes and impedance rules.
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

## P2 increment

This change completes the semantic KiCad interoperability milestone:

- `pcb::board` adds typed zones/fills/keepouts, copper arcs, board graphics and
  arc sampling; `PcbDesign` gains `zones`, `arcs` and `graphics` with serde
  defaults for old JSON documents.
- `pcb::footprint::PadShape` and `PadPosition::local_outline` model rectangle,
  roundrect, oval, circle and trapezoid copper; `PadCopper::outline` places that
  outline on the board and DRC uses it for pad-to-copper checks.
- DRC checks copper arcs, zone-fill clearance and keepouts, and reports
  shape-aware pad violations with deterministic objects and locations.
- `kicad_parser::parse_pcb`/`write_pcb` read and write zones, fills, keepouts,
  arcs and line/rectangle/circle/arc/polygon/text graphics semantically, while
  keeping unknown children and unmodelled root nodes source-preserved.
- `kicad_parser::library_table` reads, writes and preserves `sym-lib-table` and
  `fp-lib-table` entries; `ProjectBundle` loads them, applies project DRC rules
  to the sibling board, copies `${KIPRJMOD}` libraries into saved bundles and
  writes the tables back.
- `ProjectFile::drc_rules` maps `min_clearance`, `min_track_width` and
  `min_via_annular_width`; KiCad's non-positive values keep KaiCad's defaults
  rather than becoming invalid DRC limits.
- The corpus adds a KiCad 10 board and a hierarchy edit/save/reopen test with
  local libraries and tables. All 19 installed template boards and 19 project
  hierarchies with schematics validate through KiCad 10.0.6.
- Test count rises from 367 to **384**, with clippy, fmt, rustdoc, Markdown
  links, the CLI self-check and the release build clean.

Full schematic capture, manufacturing outputs, interactive zone editing and a
third-party project corpus remain future work.
