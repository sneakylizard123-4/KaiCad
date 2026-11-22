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
| **Project model** | `core::Project` with schematic and board metadata; `PcbDesign` JSON; SQLite project/board persistence | One project document that owns schematic and board together; project manager; KiCad `.kicad_pro` compatibility |
| **Schematic** | Symbols, sheets, nets, wires/labels data types; netlist generation; connectivity graph; five ERC rules; KiCad netlist export | Schematic canvas; symbol placement; wire/bus drawing; junctions; hierarchy and sheet pins; annotation; symbol chooser; geometry-based connectivity and full ERC |
| **PCB** | Outline geometry; layer stack validation; footprint libraries; pads, tracks and vias; DRC; single/multi-layer A* routing; push-and-shove; blind/buried vias; click-to-route track editor with undo | Footprint placement and editing; pad/via editing; copper zones and keepouts; net classes and impedance rules; differential pairs; length matching; ratsnest; stackup editor; selection and measurement |
| **Manufacturing** | None | Gerber, Excellon drill, BOM, pick-and-place, 3D models and enclosure/manufacturing packages |
| **Libraries** | KiCad `.kicad_sym` import; KaiCad JSON footprint libraries | KiCad `.kicad_mod` import; symbol and footprint editors; library tables and versioned libraries |
| **Board compatibility** | `.kicad_pcb` supported-subset import and export, including layer tables, named nets, Edge.Cuts outlines, footprints/pads, tracks and vias; explicit warnings for skipped constructs; `kai-cad pcb import|export` | Full-fidelity `.kicad_pcb` round trip, preservation of unknown fields, zones, arcs, graphics, 3D models, DRC setup and a KiCad 9/10 compatibility corpus |
| **User interface** | Native Studio window; 2D/3D software viewport; board open/save; DRC inspector; 2D track routing; undo/redo; typed file paths | Project workspace; schematic editor; footprint placement; native file picker; pan/zoom; copy/paste; preferences; complete editing commands and keyboard workflow |
| **Compatibility** | Export-only KiCad netlist; one-way `.kicad_sym` import; `.kicad_pcb` supported-subset read/write | Full `.kicad_sch` and `.kicad_pcb` read/write; `.kicad_pro`; preservation of unknown fields; round-trip compatibility corpus for KiCad 9 and 10 |
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

### P2 — KiCad interoperability (partially started)

- [x] Read and write the `.kicad_pcb` subset KaiCad's board model represents.
- [ ] Read and write `.kicad_pro` and `.kicad_sch` for KiCad 9 and 10.
- [ ] Extend `.kicad_pcb` support to zones, arcs, pad shapes, graphics, 3D
  models and DRC setup.
- [ ] Preserve unknown/unsupported constructs on round trip.
- [ ] Build a corpus of representative projects and compare structural output.
- [ ] Add library-table and `.kicad_mod` handling.

Exit criterion: representative KiCad projects open, edit, save and reopen
without silent loss of design data.

### Supported `.kicad_pcb` subset today

The exporter and importer are deliberately honest about their boundary:

- **Preserved:** board layer table and copper stackup order, named nets, pad
  numbers, `Edge.Cuts` line/rectangle/polygon outlines, footprint placement and
  reference designators, pad position/size/rotation/net/type, track geometry,
  width, layer and net, and via position/size/drill/span/net.
- **Approximated:** non-rectangular pads use their bounding rectangle; a
  rotated footprint is flattened into rotated pad coordinates because
  `Footprint` has no placement-angle field; back-side SMD copper is warned
  because the current pad model only places SMD copper on the top layer.
- **Skipped with a warning:** copper zones and fills, copper/graphical arcs,
  top-level drawings and text, groups, dimensions, images, targets, rule areas,
  footprint graphics, non-reference footprint properties and 3D models. DRC
  setup is not imported and KaiCad's default rules are used.
- **Not claimed:** byte-for-byte preservation, unknown-field retention or
  opening arbitrary KiCad projects without structural changes.

Use the CLI for conversion; Studio currently opens KaiCad board JSON:

```bash
cargo run -p kai-cad -- pcb import board.kicad_pcb board.kaicad.json
cargo run -p kai-cad -- pcb export board.kaicad.json board.kicad_pcb
```

Import warnings are printed to stderr and counted in the summary. The library
API is `kicad_parser::{load_pcb, parse_pcb, save_pcb, write_pcb}`; `parse_pcb`
returns `PcbImport { design, warnings }` so callers cannot mistake a partial
import for a lossless one.

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

This change adds a real, bounded KiCad PCB compatibility path:

- `kicad_parser::parse_pcb`/`load_pcb` import supported `.kicad_pcb` data into
  `pcb::PcbDesign` and return deterministic warnings for skipped constructs.
- `kicad_parser::write_pcb`/`save_pcb` export the same subset in a KiCad-openable
  S-expression document.
- `PcbDesign` now carries named `PcbNet` entries, and `PadPosition` carries its
  footprint pad number; both fields default for older JSON documents.
- The CLI adds `kai-cad pcb import` and `kai-cad pcb export`, with warning and
  object-count summaries.
- The self-check imports and exports a small PCB fixture, including a named net,
  a segment and a via.
- The exporter was smoke-validated with KiCad 10.0.6: `kicad-cli pcb export
  svg` parsed and plotted the generated board. KiCad's DRC still reports the
  expected simplified-footprint and test-geometry warnings; this is not a
  compatibility-corpus or DRC-clean guarantee.
- The previous increment's `gui::TrackEditor` and `renderer::BoardTransform`
  remain in place.

Full `.kicad_pcb` fidelity, `.kicad_sch`, `.kicad_pro`, schematic capture,
manufacturing outputs and the compatibility corpus remain future work. The
implementation is validated by unit tests and fixtures, not by a full KiCad
compatibility corpus. Do not mark the P2 exit criterion complete until that
corpus passes.
