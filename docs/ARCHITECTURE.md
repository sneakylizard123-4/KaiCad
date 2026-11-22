# KaiCad Architecture

> **Status: pre-alpha scaffold.** This document describes the intended
> architecture and the code that exists today. Most engines are type-level
> skeletons whose functions return `NotImplemented` or empty values. Do not
> read this as a description of working EDA functionality.
>
> Weeks 1 (workspace, core data model, CI), 2 (SQLite engineering database),
> 3 (symbol libraries and KiCad `.kicad_sym` import), 4 (schematic engine),
> 5 (PCB core), 6 (PCB geometry and DRC), 7 (deterministic single-layer
> routing) and 8 (multi-layer routing with vias) are complete. The workspace
> has 205 passing tests.

KaiCad is a Rust workspace containing a shared data model, per-domain engines,
a Vulkan renderer skeleton, an AI subsystem (`kAI`), a collaboration layer and
a thin application binary. The long-term goal is a complete EDA application
that is **not** a wrapper around KiCad (see `PRD.md`).

## Crate graph

```mermaid
graph TD
    app["kai-cad<br/>(binary)"]
    core["core<br/>shared data model"]
    schematic["schematic<br/>netlist · graph · ERC"]
    pcb["pcb<br/>footprint · layer · drc · routing"]
    renderer["renderer<br/>Vulkan via ash"]
    ai["ai (kAI)<br/>suggest · validate"]
    collab["collab<br/>sessions · versioning · sharing"]
    store["store<br/>SQLite persistence"]
    kicad_parser["kicad-parser<br/>KiCad symbol import"]

    app --> core
    app --> schematic
    app --> pcb
    app --> renderer
    app --> ai
    app --> collab
    app --> store
    app --> kicad_parser

    kicad_parser --> core
    schematic --> core
    pcb --> core
    ai --> core
    ai --> schematic
    collab --> core
    store --> core
```

Arrows mean "depends on". `core` sits at the bottom of the graph and must not
depend on any other workspace crate. There are no cross-engine edges between
`schematic` and `pcb`: both depend only on `core`. `renderer` has no workspace
dependencies at all (only `ash` and `serde`); it is wired into the application
binary, not into the data model.

## Application layering

```mermaid
flowchart BT
    data["Data model<br/>core"]
    schematic["Schematic engine<br/>schematic"]
    pcb["PCB engine<br/>pcb"]
    render["Renderer<br/>renderer (Vulkan)"]
    ai["AI subsystem<br/>ai (kAI)"]
    collab["Collaboration<br/>collab"]
    store["Engineering database<br/>store"]
    ui["UI / application shell<br/>kai-cad"]

    schematic --> data
    pcb --> data
    ai --> data
    ai --> schematic
    collab --> data
    store --> data
    ui --> data
    ui --> schematic
    ui --> pcb
    ui --> render
    ui --> ai
    ui --> collab
    ui --> store
```

The UI layer is currently only a CLI (`kai-cad/src/main.rs`): it prints a
banner, supports `--help`/`--version`, and runs a deterministic self-check that
calls every subsystem. A custom GUI is planned but not started. The AI and
collaboration layers are optional: KaiCad must remain usable without AI,
network access or external services (`PRD.md` §1.2, §1.3).

## Core data model

`core` is the single source of truth for design data. Dependencies: `serde`,
`serde_json`, `uuid`, `chrono`. No workspace dependencies.

| Module | Public types / functions | Purpose |
|--------|--------------------------|---------|
| `component` | `Component`, `ComponentProperty` | Instance linking a symbol to a footprint |
| `geometry` | `Point`, `segment_distance` | 2D point in millimetres with `distance_to` and `distance_to_segment`; segment-to-segment distance |
| `net` | `Net`, `NetClass` | Named connectivity and design-rule class |
| `pcb` | `PcbOutline`, `BoundingBox`, `PcbLayer`, `LayerKind`, `Stackup` | Board outline with area/perimeter/contains-point/bounds, layers and layer order |
| `project` | `Project`, `SchematicData`, `PcbData`, `ProjectMetadata` | Root document and its sections |
| `proposal` | `ChangeProposal` | Field-level change shared by `ai` and `collab` |
| `serialization` | `Error`, `Result`, `to_json`, `to_json_pretty`, `from_json`, `save`, `load` | JSON encoding and file persistence |
| `sheet` | `Sheet`, `Wire`, `Label` and position/type enums | Hierarchical sheets, wires, labels |
| `symbol` | `Symbol`, `Pin` and direction/position/type enums | Schematic symbol and pins |
| `symbol_library` | `SymbolLibrary`, `SymbolLibrarySource` | Named, project-independent symbol collection and provenance |

All public types derive `serde::Serialize`/`Deserialize`.

### Serialization API

`core::serialization` provides generic helpers plus inherent methods on
`Project`:

```rust
use core::Project;
use core::serialization::{from_json, load, save, to_json, to_json_pretty};

let project = Project::new("demo");
let compact = to_json(&project)?;          // String
let pretty = to_json_pretty(&project)?;    // indented String
let decoded: Project = from_json(&pretty)?;
save(&project, "out/demo.json")?;          // creates parent directories
let loaded: Project = load("out/demo.json")?;

// Equivalent convenience methods:
let compact = project.to_json()?;
let decoded = Project::from_json(&compact)?;
project.save("out/demo.json")?;
let loaded = Project::load("out/demo.json")?;
```

`save` creates missing parent directories and writes pretty JSON. `Error`
distinguishes `Json` and `Io` failures and implements `std::error::Error`.

### Consolidation

The Week 1 cleanup removed the duplicated types that used to blur crate
boundaries:

- **One `Netlist`.** `schematic::netlist::Netlist` is the only definition;
  `schematic::lib.rs` re-exports it. `generate_netlist` and `Graph` both use it.
- **One `ChangeProposal`.** `core::ChangeProposal` is shared by `ai` and
  `collab` (`ai` re-exports it; `collab::sync` re-exports it) instead of each
  crate defining its own.
- **One set of PCB primitives.** `core::pcb::{PcbOutline, PcbLayer, Stackup}`
  and `core::Point` are used by `pcb::PcbDesign`, so the project document and
  the PCB engine speak the same types.
- **Versioning lives in `collab`.** The orphaned `pcb/src/versioning.rs` was
  deleted; `collab::VersioningManager` owns version history.
- **No copied modules.** `renderer` no longer contains a copy of the `collab`
  module and declares no workspace dependencies.

## What each crate owns

### `schematic` — schematic engine

Dependencies: `core`, `serde`, `uuid`. Modules:

- `netlist` — `Netlist`, `Connection`, `PinRef`, `generate_netlist` and
  `to_kicad_netlist`. `generate_netlist` indexes every symbol pin by `Pin::id`,
  walks each net's pins in order, and emits a `Connection` for every adjacent
  pair that resolves to a known pin; unknown pin ids are skipped.
  `to_kicad_netlist` renders a deterministic, pretty-printed S-expression
  document: components and nets sorted, net codes from 1, nodes sorted by
  `(reference, pin, id)`, escaped strings and one trailing newline. It is
  export-only; no KiCad netlist is parsed back into the internal model.
- `graph` — hand-rolled connectivity `Graph` of symbol/net `Node`s and `Edge`s.
  `Graph::from_netlist` builds one node per symbol and per net, and one edge per
  connection whose endpoints are known symbols; `node_count`/`edge_count`
  report the sizes. `edges_for`, `neighbours`, `is_connected` (BFS) and
  `connected_components` add deterministic read-only traversal; unknown ids
  yield empty or `false` results rather than panics, and net nodes are isolated
  because edges only join symbol pins.
- `erc` — `run_erc`, `ERCResult`, `Violation`, `ViolationSeverity` and the five
  built-in rules listed below.

### `pcb` — PCB engine

Dependencies: `core`, `serde`, `serde_json`, `uuid`. Modules:

- `footprint` — `Footprint` (with a board `position`), `PadPosition` (with
  copper `width`/`height` and optional `net_id`), `PadType`, `Outline`,
  `OutlineShape::{Rectangle, RoundedRectangle}`, plus `FootprintLibrary`
  (`sorted`, `find`, `to_json`, `from_json`, `save`, `load`) and
  `FootprintLibraryError`. Missing placement and pad-size fields default, so
  older libraries still load. Libraries are KaiCad's own JSON with two-space
  indentation and one trailing newline; duplicate names are rejected on load.
  A KiCad `.kicad_mod` parser does not exist.
- `layer` — `LayerStack` (`new`, `push`, `len`, `is_empty`, `copper_layers`,
  `find`) over `core::PcbLayer`, plus `validate(&Stackup)` and
  `copper_layers_in_stackup(&Stackup)`. Validation reports `LayerIssue`s of
  kind `DuplicateId`, `DuplicateName`, `EmptyName`, `MissingFromStackup` and
  `NonCopperInStackup`, sorted by `(layer_id, kind)`.
- `drc` — `DrcRules` (validated, serde-friendly, stored on the design),
  `DrcRulesError`, `DrcSeverity`, `DrcViolation`, `DrcReport` and `run_drc`, a
  pure function implementing the eight geometry rules and the rule-set
  validation listed below.
- `routing` — `Router`, `RoutingAlgorithm::{PushAndShove, LengthMatching,
  Hybrid}`, `RoutingOptimization`, `RouterConfig` (grid, bend cost, expansion
  limit, `allow_vias`, via cost/limit/dimensions), `RouteRequest`, `RouteError`,
  `Route`, `RouteResult` and `route_multilayer`. A* state is `(node, layer,
  direction)` plus the via count; in-layer moves avoid tracks, vias and pads
  with the design's clearance, and a via transition stays on the node, costs
  `via_cost` and moves to another copper layer. Candidate through vias must
  clear every spanned layer and satisfy `via-drill` and annular-ring limits.
  Tracks are split at layer changes and simplified per layer; ids are UUIDv5
  values over the request and segment. `route` disables via insertion and
  returns the tracks only.
- `lib.rs` — `PcbDesign` (`outline`, `layers`, `stackup`, `footprints`,
  `tracks`, `vias`, `rules`) built on `core::PcbOutline`/`core::PcbLayer`/
  `core::Stackup`/`core::Point`, plus `Track` (copper `width`, default
  `0.25` mm) and `Via` (optional `net_id`, `diameter`/`drill`, defaults
  `0.6`/`0.3` mm). Missing fields load with those defaults, so older documents
  still open.

`PcbDesign::copper_layer_count` is implemented and honours the stackup.
Outline geometry lives on `core::PcbOutline`: `signed_area`, `area`,
`perimeter`, `contains_point` (even-odd ray casting with boundary-inclusive
edges) and `bounding_box`. Design-rule checking lives in `pcb::drc`; single-
layer and multi-layer grid routing live in `pcb::routing`. Interactive routing,
blind/buried vias and pad rotation do not exist yet.

### `renderer` — Vulkan rendering engine

Dependencies: `ash` (Vulkan bindings) and `serde` only. Modules: `window`,
`device`, `swapchain`, `render_pass`, `pipeline`.

- `RendererConfig { window: WindowConfig, vsync }`.
- `WindowConfig { title, width, height, resizable, fullscreen }` with
  `aspect_ratio()`.
- `Renderer::new()` and `Renderer::render()` return
  `Err(RendererError::NotImplemented)`; **no Vulkan objects are created yet.**

`ash` loads the Vulkan loader at runtime, so the workspace builds on machines
without a Vulkan SDK.

### `ai` — kAI subsystem

Dependencies: `core`, `schematic`, `serde`, `serde_json`, `uuid`. Modules:
`optimizer`, `suggester`, `validator`.

- `kAI`, `kAIConfig`, `kAIError` (the brand spelling is kept with
  `#[allow(non_camel_case_types)]`).
- `OptimizationContext { netlist: schematic::Netlist, current_design_hash,
  constraints }`.
- `SuggestionValidator::validate` implements the deterministic rules listed
  below.
- `kAI::suggest_optimization` still returns `NotImplemented`;
  `kAI::validate_suggestion` runs the validator locally (no network, no model).

The default endpoint is `http://localhost:11434` (a local model server).
Deterministic validation is mandatory and always on; `kAIConfig` exposes no
switch to disable it.

### `collab` — collaboration layer

Dependencies: `core`, `serde`, `serde_json`, `uuid`, `chrono`. Modules:
`versioning`, `sync`, `sharing`, `window`.

- `CollabManager::{new, create_session, session_count}`. `create_session` uses
  the passed `project_id` and seeds an `Owner` participant.
- `VersioningManager::{new, record, latest}`.
- `SharingManager::{share, unshare}`.
- `Window::new`.

No transport, merge algorithm or real-time sync exists yet.

### `store` — engineering database

Dependencies: `core`, `pcb`, `rusqlite` (with the `bundled` feature),
`serde_json`, `uuid`, `chrono`. Modules: `lib.rs` plus embedded `migrations/`.

- `Store` wraps a `rusqlite::Connection`, enables `PRAGMA foreign_keys = ON`,
  and applies migrations automatically on open.
- `Store::{open, open_in_memory}` create file-backed or in-memory databases.
- `Store::{create_project, load_project, list_projects, update_project,
  delete_project}` implement project CRUD.
- `Store::{create_symbol_library, load_symbol_library, list_symbol_libraries,
  update_symbol_library, delete_symbol_library}` implement symbol-library CRUD;
  listing is ordered by `name ASC`.
- `Store::{save_pcb_design, load_pcb_design, delete_pcb_design}` persist a
  complete `pcb::PcbDesign` (tracks, vias, footprints, pad copper and DRC
  rules) in `pcb_designs`, keyed by project. Saving is an upsert and requires
  the project to exist; deleting a project cascades to its design.
- `ProjectSummary` is returned by `list_projects` and ordered by
  `modified_at DESC, name ASC`.
- `StoreError` covers SQLite, JSON, I/O, `Duplicate`, `MissingProject` and
  migration failures.

**Schema rule:** the canonical record is `projects.document`, a complete JSON
serialization of `core::Project`. The tables `sheets`, `symbols`, `components`,
`nets` and `footprints` are queryable projections rebuilt from the document
inside the same transaction on every create/update. They must never be edited
independently of the authoritative document. Their primary key is
`(project_id, id)`: entity ids are unique within a project rather than
globally, so a duplicated project may reuse the same entity ids. Symbol
libraries use the same pattern in `symbol_libraries`: canonical `data` JSON with
queryable `name` and `source` projections. PCB designs use `pcb_designs`, keyed
by `project_id` with the complete design JSON in `data`; they cascade with the
project. Migrations are numbered SQL files
embedded with `include_str!`; once applied they are immutable — add a new
numbered file rather than editing an old one.

### `kicad-parser` — KiCad symbol import

Dependencies: `core`, `uuid` (with the `v5` feature). The crate is import-only;
KiCad's file format never becomes the internal representation.

- `parse_symbol_library(path) -> Result<core::SymbolLibrary, ParseError>` reads
a `.kicad_sym` file and maps each top-level `(symbol ...)` to
  `core::Symbol`, including pins nested inside unit sub-symbols.
- `ParseError::{Io, Syntax, Unsupported}` implements `Display` and
  `std::error::Error`; malformed input never panics.
- The parser implements a minimal deterministic S-expression tokenizer/parser
  (atoms, quoted strings, nested lists). No generic expression crate is used.
- Library and symbol names become UUIDv5 ids with `Uuid::NAMESPACE_OID`, so
  parsing the same content twice yields identical ids. The library name is the
  file stem because KiCad libraries do not record one.
- Pins map name, number, electrical type, position (`at` x/y/angle in KiCad
  symbol space, no axis conversion) and direction derived from the electrical
  type. Known types include `free` and `no_connect`; only genuinely unknown
  types become `PinType::Unspecified`.

### `kai-cad` — application binary

Depends on every library crate. `src/main.rs` prints a banner, supports
`--help`/`--version`, and with no arguments runs a deterministic self-check:
it serializes a `Project`, generates, ERC-checks and exports a KiCad netlist,
builds a PCB design (outline measurements, layer validation, a footprint
library round trip, clean/dirty DRC runs, a DRC-clean single-layer route, a
multi-layer route across a full-width wall with through vias, and a blocked-
route check), reads the default renderer, AI and collaboration configuration,
exercises the SQLite store with an in-memory database including a PCB-design
round trip, and parses a hermetic `.kicad_sym` fixture with `kicad-parser`. Integration tests live
in `kai-cad/tests/smoke.rs`; the workspace root is a virtual manifest, so a
root `tests/` directory would not be built.

## Data flow: schematic netlist to PCB

The intended pipeline (steps 2, 3, 5 (single- and multi-layer), 6 and 7
(persistence) have real implementations; the rest are planned):

```mermaid
flowchart LR
    subgraph editing["Schematic editing (planned)"]
        sheets["Sheets<br/>wires · labels"]
        symbols["Symbols<br/>pins"]
    end

    netlist["schematic::Netlist<br/>nets · symbols · connections"]
    graph["schematic::Graph<br/>symbol/net nodes"]
    components["core::Component<br/>symbol_id ↔ footprint_id"]
    pcb["pcb::PcbDesign<br/>footprints · tracks · vias"]
    erc["ERC<br/>deterministic rules"]
    render["renderer<br/>(planned)"]

    sheets --> netlist
    symbols --> netlist
    netlist --> graph
    netlist --> components
    components --> pcb
    netlist --> erc
    pcb --> render
```

1. **Capture.** The user places symbols, wires and labels on sheets. These live
   in `Project.schematic` (`sheets`, `symbols`, `nets`).
2. **Netlist extraction.** `schematic::generate_netlist(&symbols, &nets)`
   produces a `Netlist` of nets and pin-to-pin `Connection`s. This is
   implemented: adjacent resolvable pins within each net become connections.
3. **Graph construction.** `Graph::from_netlist(&netlist)` builds the
   symbol/net connectivity graph. This is implemented.
4. **Symbol ↔ footprint binding.** Each `core::Component` references a
   `symbol_id` and a `footprint_id`, which is how schematic instances are
   matched to physical footprints. Binding logic is not written yet.
5. **Board construction.** The PCB engine takes the netlist plus footprint
   assignments and populates `PcbDesign` (`footprints`, `tracks`, `vias`).
   Tracks reference their net by `net_id`; vias record the layers they span and
   optionally their net. `route` and `route_multilayer` can add tracks and
   through vias, but automatic symbol-to-footprint binding is still manual.
6. **Validation and storage.** ERC runs on the schematic netlist and DRC runs on
   the PCB geometry, both with deterministic rules (see below). `store` can
   save and reload the complete PCB design with its rules.
7. **Cross-probing and rendering.** The renderer draws schematic/PCB data, and
   selections are intended to synchronise between views. Not implemented.

There is no code path yet that executes steps 4–7 end to end from a netlist.

## Deterministic validation surfaces

### Electrical rule check (ERC)

`schematic::run_erc(&netlist)` is deterministic and independent of iteration
order. It reports:

| Rule id | Severity | Trigger |
|---------|----------|---------|
| `net-single-pin` | Warning | A net has fewer than two pins |
| `duplicate-net-name` | Error | Two nets share the same non-empty name |
| `self-connection` | Error | A connection's source and target pin are identical |
| `unconnected-pin` | Warning | A symbol pin appears in no net (one per pin) |
| `duplicate-symbol-name` | Error | Two symbols share the same non-empty name (one per name) |

`ERCResult::valid` is true only when there are no `Error` or `Critical`
violations. `ERCResult::is_valid()` is the accessor.

### Design-rule check (DRC)

`pcb::run_drc(&design, &rules)` is a pure, deterministic function. Violations
are sorted by `(rule_id, objects)`, each unordered copper pair on different nets
is reported once, and the report never depends on track order or `HashMap`
iteration.

| Rule id | Severity | Trigger |
|---------|----------|---------|
| `track-width` | Error for a non-finite or `<= 0` width, Warning below `min_track_width` | Track copper width |
| `track-layer` | Error | Track layer missing or not copper |
| `clearance` | Error | Tracks, vias and pads on a shared copper layer with different nets are closer than `clearance`, edge to edge |
| `track-outside-board` | Error | A track path point lies outside the outline |
| `via-outside-board` | Error | A via centre lies outside the outline |
| `pad-outside-board` | Error | A pad corner lies outside the outline |
| `pad-size` | Error | A pad width or height is non-finite or `<= 0` |
| `via-span` | Error | Via ends on the same layer, leaves the top-to-bottom stackup span, or uses a missing/non-copper layer |
| `via-drill` | Error for a non-finite or non-positive drill/diameter, drill `>=` diameter, or drill below `min_via_drill`; Warning for a ring below `min_annular_ring` | Via drill, diameter and annular ring |
| `track-path` | Error | A track has fewer than two points or a non-finite coordinate |
| `drc-rules` | Error | A stored limit is non-finite or `<= 0`; checks then run with the defaults |

An outline with fewer than three points disables the board-containment rules.
An empty stackup means layer order is unknown, so `via-span` only
requires two distinct, existing copper layers. Pad copper is an axis-aligned
rectangle centred on `Footprint::position + PadPosition::{x, y}`: through-hole
pads occupy every copper layer, surface-mount and tented pads the top copper
layer, and pads in the same footprint are not compared with each other.
Pad-to-track and track-to-via clearance is skipped when the shared net matches;
generated vias carry `Some(net_id)`, while older documents load `None` and are
compared as different nets. Limits live on
`PcbDesign::rules` and serialize with the design; `DrcRules` defaults to a
`0.15` mm minimum width, `0.2` mm clearance, `0.05` mm minimum annular ring
and `0.2` mm minimum via drill. `DrcRules::validate` rejects non-finite or
non-positive limits, and `run_drc` reports them through `drc-rules` without
silently changing a valid rule set. `DrcReport::is_valid` ignores warnings,
and `DrcReport::count` filters by severity. A non-finite distance from
`core::geometry` is never treated as clearance.

### AI suggestion validation

`SuggestionValidator::validate(&suggestion, &constraints)` performs no network
access and no model invocation. It reports:

| Check | Field | Outcome |
|-------|-------|---------|
| `confidence_score` outside `0.0..=1.0` | `confidence_score` | Error |
| Empty `changes` list | `changes` | Error |
| `old_value == new_value` | `changes[<index>]` | Error (no-op) |
| Constraint with a negative `value` | constraint name | Warning |

`ValidationResult::valid` is true iff no errors were found.
`kAI::validate_suggestion` wraps this validator.

**AI output must pass these deterministic checks before it is applied to a
design.** This is a PRD requirement (`PRD.md` §1.3.12): kAI may propose
changes, but a human or downstream engine applies them only after validation.

## Key design rules

These come from `PRD.md` and `PLAN.md` and constrain every future change.

1. **No KiCad wrapping.** KaiCad implements its own engineering database,
   geometry engine, schematic engine, PCB engine, renderer and UI. KiCad 9/10
   file formats are used only as an import/export reference, never as the
   internal representation (`PRD.md` §1.1, `PLAN.md` §5.3).
2. **No mandatory cloud.** Projects are local-first. Collaboration, AI and
   network services are optional; the application must remain useful offline
   (`PRD.md` §1.2, §1.3.11).
3. **AI output must pass deterministic validation.** kAI may propose changes,
   but nothing is applied until `SuggestionValidator::validate` (and later
   ERC/DRC) accepts it (`PRD.md` §1.3.12, `ai/src/validator.rs`).
4. **Deterministic engineering operations.** No unseeded randomness in geometry,
   routing, netlisting or rule checks (`PLAN.md` §5.1).
5. **No silent data loss.** Unknown or unsupported data encountered on import
   must be preserved or reported, never dropped silently (`PRD.md` §1.3.10).
6. **Transactional editing.** Design mutations should be grouped into
   transactions that support undo/redo (`PRD.md` §1.3.7). Not implemented yet.
7. **GPU-accelerated rendering.** Vulkan is the target rendering API on all
   platforms (`PRD.md` §1.1).
8. **Modular, testable subsystems.** Each subsystem is a crate with a narrow
   public API; `core` owns shared types (`PLAN.md` §2.2).

## Compatibility targets

- **Primary:** KiCad 10 project files.
- **Secondary:** KiCad 9 project files.
- `kicad-parser` imports `.kicad_sym` symbol libraries and `schematic` exports
  a KiCad netlist. No `.kicad_sch` or `.kicad_pcb` parser or writer and no
  netlist importer exist yet.

## Current milestone and next steps

**Complete (Weeks 1–8, starting from commit `c864fbb`):** workspace and crate
graph, shared `core` data model with JSON persistence, SQLite engineering
database (`store`, schema v4) with migrations plus project, symbol-library and
PCB-design CRUD, named symbol libraries and a deterministic `.kicad_sym` parser
(`kicad-parser`), deterministic netlist generation and KiCad export, graph
traversal, five-rule ERC, board-outline geometry, layer-stack validation,
JSON footprint libraries, distance primitives, track/via dimensions, pad
copper, nine-rule DRC (eight geometry rules plus limit validation),
deterministic single-layer and multi-layer grid routing with through vias and
UUIDv5 ids, renderer/AI/collab configuration types, the `kai-cad` self-check
binary, CI (including a Markdown link check and coverage job), and
documentation.

**Next (Week 9):** transactional kAI suggestion application. The planned work
is:

1. Add atomic, reversible core edits with stale-value detection.
2. Let `kAI` apply only validated suggestions and return the edit transaction.
3. Wire apply/undo/redo into the self-check and smoke tests.

The full work order is in [`NEXT_AGENT_PROMPT.md`](NEXT_AGENT_PROMPT.md).

## Licensing

GPL-3.0-or-later. See `README.md` and `docs/CONTRIBUTING.md` for the
contribution and header conventions.
