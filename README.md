# KaiCad

[![CI](https://github.com/sneakylizard123-4/KaiCad/actions/workflows/ci.yml/badge.svg)](https://github.com/sneakylizard123-4/KaiCad/actions/workflows/ci.yml)

**KaiCad** is a complete, high-performance, open-source electronic design automation (EDA) application written in Rust.

## Status
- **Architecture:** Decided
- **Implementation:** Phase 2 (PCB core + software renderer), board inspection, the first track-routing editor and `.kicad_pcb`/`.kicad_sch`/`.kicad_mod`/`.kicad_pro` file interchange complete (pre-alpha); full KiCad parity remains a multi-phase program
- **Target:** Rust, Linux (primary), Windows, macOS

This repository currently contains the workspace structure, the shared data
model, a SQLite engineering database, engine skeletons, CI configuration,
documentation, examples and a Criterion benchmark. It is **not** a usable EDA
tool yet.

## Features
- Native Studio GUI for board viewing, DRC and undoable track routing (placement and schematic editing planned)
- Deterministic software 2D/3D rendering (Vulkan GPU backend planned)
- KiCad `.kicad_sym` symbol import and `.kicad_pcb`, `.kicad_sch`, `.kicad_mod` and `.kicad_pro` file interchange through `kai-cad` (integrated schematic/board project document not implemented)
- Full schematic and PCB design capabilities target (not complete)
- Integrated design-rule and electrical-rule checking
- Advanced PCB routing and engineering analysis
- Built-in AI engineering assistance (kAI)
- Collaborative project workflows
- Extensible plugin and scripting architecture

## Getting Started

### Prerequisites
- Rust toolchain (stable channel, 1.85+; `rust-toolchain.toml` selects it)
- Git
- Vulkan SDK / loader (optional; only the future GPU backend needs it)
- Dependencies for your OS:
  - Ubuntu/Debian: `sudo apt-get install pkg-config libssl-dev libudev-dev libvulkan-dev vulkan-tools`
  - Fedora: `sudo dnf install pkgconf-pkg-config openssl-devel systemd-devel vulkan-loader-devel vulkan-tools`
  - macOS: `brew install pkg-config openssl`

The Studio GUI needs an X11 or Wayland desktop session, an OpenGL-capable
driver and `libxkbcommon` at runtime. The headless renderer and all tests need
no display or Vulkan loader.

See [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md) for the full setup guide.

### Building
```bash
# Clone the repository
git clone https://github.com/sneakylizard123-4/KaiCad.git
cd KaiCad

# Build the workspace
cargo build --workspace

# Run tests (384 tests across the workspace)
cargo test --workspace

# Run the desktop GUI with the built-in demo board
cargo run -p gui

# Open a KiCad board directly in the GUI
cargo run -p gui -- path/to/board.kicad_pcb

# Import a supported KiCad PCB into KaiCad's JSON board format
cargo run -p kai-cad -- pcb import path/to/board.kicad_pcb path/to/board.kaicad.json

# Export a KaiCad board as a KiCad PCB
cargo run -p kai-cad -- pcb export path/to/board.kaicad.json path/to/board.kicad_pcb

# Convert a KiCad schematic, footprint or project JSON
cargo run -p kai-cad -- sch import path/to/main.kicad_sch path/to/main.json
cargo run -p kai-cad -- sch export path/to/main.json path/to/main.kicad_sch
cargo run -p kai-cad -- footprint import path/to/part.kicad_mod path/to/part.json
cargo run -p kai-cad -- footprint export path/to/part.json path/to/part.kicad_mod

# Recursively round-trip a project, its child sheets and its board
cargo run -p kai-cad -- project bundle path/to/project.kicad_pro path/to/output-dir

# Run the CLI application (banner + subsystem self-check)
cargo run -p kai-cad

# Print usage or the version without running the self-check
cargo run -p kai-cad -- --help
cargo run -p kai-cad -- --version
```

### Examples and benchmarks
```bash
# Build a Project, serialize it to JSON and deserialize it again
cargo run -p core --example project_roundtrip

# Construct a PCB outline shape and print a summary
cargo run -p pcb --example board_outline

# Create, list, load, update and delete a project in a SQLite database
cargo run -p store --example project_crud

# Parse a KiCad symbol library and print a summary
cargo run -p kicad-parser --example parse_library

# Compile the serialization benchmark without running it
cargo bench -p core --no-run
```

### Development
- Follow the conventional Rust workflow with `cargo check`, `cargo test`, and `cargo fmt`
- All code must pass `cargo clippy --workspace --all-targets -- -D warnings`
- Unit tests live in `src/tests.rs` or inline `#[cfg(test)] mod tests` blocks
- Cross-crate integration tests live in `kai-cad/tests/smoke.rs`; the workspace
  root is a virtual manifest and has no test target of its own
- Benchmarks live in `<crate>/benches/`, examples in `<crate>/examples/`

## Project Layout

```text
KaiCad/
├── Cargo.toml                  # virtual workspace manifest (no [package])
├── Cargo.lock                  # committed lockfile (workspace binaries are shipped)
├── rust-toolchain.toml         # stable channel + rustfmt/clippy
├── clippy.toml                 # Clippy behaviour (msrv = 1.85)
├── rustfmt.toml                # formatting rules (edition 2024, width 100)
├── .github/workflows/ci.yml    # "Check" and "Coverage" CI jobs
├── scripts/
│   └── check_markdown_links.py # relative-link checker used by CI
├── docs/
│   ├── ARCHITECTURE.md         # crate graph, layering, data flow, design rules
│   ├── DEVELOPMENT.md          # setup, commands, examples, benchmarks, CI
│   ├── CONTRIBUTING.md         # branches, commits, review checklist, license
│   └── NEXT_AGENT_PROMPT.md    # self-contained brief for the next implementation pass
├── core/                       # shared data model (Project, Symbol, Net, Sheet)
│   ├── src/
│   ├── examples/
│   │   └── project_roundtrip.rs
│   └── benches/
│       └── serialization.rs
├── schematic/                  # netlist, connectivity graph, ERC
├── pcb/                        # footprints, layers, routing
│   └── examples/
│       └── board_outline.rs
├── renderer/                   # software 2D/3D renderer + Vulkan config skeleton
├── gui/                        # KaiCad Studio desktop shell (eframe)
│   └── src/
│       ├── app.rs              # StudioState, viewport and eframe panels
│       ├── demo.rs             # built-in DRC-clean demo board
│       ├── document.rs         # native .kicad_pcb open/save (JSON optional)
│       ├── lib.rs
│       └── main.rs             # kai-cad-gui binary
├── ai/                         # kAI subsystem
├── collab/                     # collaboration layer
├── plugin/                     # plugin manifests, registry and discovery
├── store/                      # SQLite engineering database + project CRUD
│   ├── src/
│   │   └── lib.rs
│   ├── migrations/
│   │   ├── 0001_initial.sql
│   │   ├── 0002_projection_primary_keys.sql
│   │   ├── 0003_symbol_libraries.sql
│   │   ├── 0004_pcb_designs.sql
│   │   ├── 0005_plugins.sql
│   │   └── 0006_plugin_enabled.sql
│   └── examples/
│       └── project_crud.rs
├── kicad-parser/               # KiCad symbol, PCB, schematic, footprint and project I/O
│   ├── src/
│   │   ├── lib.rs              # `.kicad_sym` parser
│   │   ├── pcb.rs              # `.kicad_pcb` and `.kicad_mod` I/O with preservation
│   │   └── schematic.rs        # `.kicad_sch` and `.kicad_pro` preservation
│   └── tests/                  # compatibility corpus and structural round trips
│   └── examples/
│       └── parse_library.rs
├── kai-cad/                    # application binary (kai-cad)
│   ├── src/
│   │   └── main.rs
│   └── tests/
│       └── smoke.rs            # cross-crate integration smoke tests
├── PRD.md                      # product requirements
├── docs/PLAN.md                # implementation plan
├── docs/PHASE1_TASKS.md        # Phase 1 task breakdown
└── docs/WEEK1_TASKS.md         # Week 1 deliverables and status
```

## Documentation

- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — crate graph, layering, core data model, validation surfaces and design rules
- [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md) — prerequisites, toolchain, commands, examples, benchmarks, CI and troubleshooting
- [`docs/CONTRIBUTING.md`](docs/CONTRIBUTING.md) — branch/commit conventions, no-warnings policy, tests, review checklist, license headers
- [`docs/NEXT_AGENT_PROMPT.md`](docs/NEXT_AGENT_PROMPT.md) — self-contained brief for the next implementation pass (Week 16)
- [`PRD.md`](PRD.md) — master product requirements document
- [`docs/PLAN.md`](docs/PLAN.md) — implementation roadmap and technical approach
- [`docs/KICAD_PARITY.md`](docs/KICAD_PARITY.md) — full-KiCad gap analysis, parity milestones, current editor scope and the KiCad file-interchange boundary
- [`docs/PHASE1_TASKS.md`](docs/PHASE1_TASKS.md) — Phase 1 (weeks 1–4) task breakdown
- [`docs/WEEK1_TASKS.md`](docs/WEEK1_TASKS.md) — Week 1 deliverables and verification status

## Current State

Weeks 1–15, the Phase 2 PCB/renderer scope, the first Studio editing
increment and a KiCad file-interchange layer are complete and verified:
**384 tests pass**, `cargo clippy --workspace --all-targets -- -D warnings`
and `cargo fmt --all -- --check` are clean, and the release build succeeds.
Phase 2 added pad rotation, blind/buried vias, deterministic push-and-shove
routing, interactive route previews, and a headless software 2D/3D renderer
with a depth buffer and PPM output. Weeks 13–15 added SQLite-backed plugin
persistence (schema v6), activation state and deterministic capability
filters, plus `kai-cad plugins list|show|enable|disable|discover`. Manifests
never load or execute plugin code.

The post-Phase-2 `gui` crate adds **KaiCad Studio**, a native eframe/egui
shell. It opens `.kicad_pcb` boards directly, renders them in a 2D or 3D
viewport through the deterministic CPU rasterizer, shows the layer stack and
placed footprints, runs DRC, and saves back to `.kicad_pcb`; `.json` remains an
explicit interchange format. The 2D view has an undoable track-routing tool:
click a pad or free point, click a destination, and DRC-checked copper is added.
The CLI can also convert supported `.kicad_pcb`, `.kicad_sch`, `.kicad_mod` and
`.kicad_pro` files with explicit warnings for constructs retained opaquely.
Zones, keepouts, copper arcs, graphics, standard pad shapes and project DRC
rules now have semantic board models; `project bundle` recursively rewrites the
hierarchy, library tables and `${KIPRJMOD}` libraries. Custom pad primitives,
dimensions and some setup fields remain source-preserved.
Footprint/pad placement, pan/zoom, native file pickers and schematic capture
remain unimplemented.

The Vulkan GPU backend remains deferred. Board rendering is deterministic CPU
rasterization; eframe presents the framebuffer in a native window. Week 16
release hardening is still outlined in
[`docs/NEXT_AGENT_PROMPT.md`](docs/NEXT_AGENT_PROMPT.md); the product path to a
full KiCad replacement is in [`docs/KICAD_PARITY.md`](docs/KICAD_PARITY.md),
not in the 16-week foundation plan alone.

What each crate provides today:

- ✅ **core** — `Project`/`SchematicData`/`PcbData`/`ProjectMetadata`, symbols,
  nets, sheets, components, PCB outline/layer/stackup types (with outline area,
  perimeter, contains-point and bounding-box queries), `Point`, the shared
  `ChangeProposal`, named symbol libraries, atomic `edit` transactions
  (`apply`, `undo`, `redo`, stale-value checks, UUIDv5 transaction ids) and
  bounded `history` stacks (`EditHistory`, redo-branch clearing, capacity
  limits). Real JSON serialization: `to_json`, `to_json_pretty`, `from_json`,
  `save`, `load` plus `Project::{to_json, from_json, save, load}`.
- ✅ **schematic** — real netlist generation (`generate_netlist` maps pin ids to
  `PinRef`s and emits connections for adjacent pins), deterministic KiCad
  netlist export (`to_kicad_netlist`), a connectivity `Graph` with
  neighbour/connectivity/component queries, and deterministic ERC with five
  rules: `net-single-pin` and `unconnected-pin` (warnings),
  `duplicate-net-name`, `duplicate-symbol-name` and `self-connection` (errors).
- ✅ **pcb (Phase 2 scope)** — `PcbDesign` built on `core` types, board-outline
  geometry, `LayerStack` with validation, `FootprintLibrary` JSON persistence,
  track and via dimensions, placed footprints with shape-aware rotated pad
  copper, copper arcs, zones/keepouts and board graphics, and a deterministic
  DRC (`run_drc`) covering tracks, arcs, vias, pads, zone fills and keepouts.
  Routing includes
  deterministic single-layer and multi-layer A* (`route`, `route_multilayer`),
  optional blind/buried vias, bounded push-and-shove rip-up/reroute, and an
  `InteractiveRouter` preview/commit/cancel session. Through vias remain the
  default; blind/buried vias are opt-in via `RouterConfig::allow_blind_buried`.
  `net_ids` and `copper_layer_ids` expose the assigned nets and stackup order
  needed by an editor.
- ✅ **renderer (basic)** — deterministic CPU rasterizer for `PcbDesign` values.
  `ViewMode::TwoD` draws the board, copper, pads and vias; `ViewMode::ThreeD`
  extrudes the board and renders it through a fitted perspective camera with a
  depth buffer and flat shading. It is headless, needs no Vulkan loader and
  writes binary PPM via `Framebuffer::save_ppm`. `BoardTransform` exposes the
  exact 2D board-to-pixel fit for viewport hit-testing. A Vulkan GPU backend is
  not implemented.
- 🚧 **gui (KaiCad Studio)** — native eframe shell with headless-testable
  `StudioState`, native `.kicad_pcb` open/save with optional JSON, a
  software-rendered 2D/3D viewport (capped at 2048 px per side), layer-stack
  and footprint inspection, DRC counts and violation details, status feedback,
  keyboard shortcuts and drag-and-drop board opening. The 2D viewport has an undoable click-to-route track
  editor with DRC preflight. Footprint/pad editing and schematic capture remain
  deferred.
- 🚧 **ai (kAI)** — configuration, deterministic suggestion validation
  (confidence range, empty changes, no-op changes, negative constraints) and
  atomic `apply_suggestion` with undoable `core::edit` transactions. Every
  application is validated first and leaves the project untouched on failure.
  `kAI::suggest_optimization` returns `NotImplemented`.
- 🚧 **collab** — sessions with owner/editor/viewer roles, permission-checked
  atomic edits, transaction-linked version history with UUIDv5 project digests,
  deterministic staged-transaction queues, sharing and windows. No transport,
  merge or real-time sync.
- ✅ **plugin** — validated local manifest metadata (`PluginManifest`,
  `PluginCapability`), a deterministic id-sorted `PluginRegistry`, and
  deterministic discovery of direct `.json` files. Entry points are path-checked
  but never opened; no plugin code is loaded or executed.
- ✅ **store** — SQLite engineering database (schema v6) with versioned
  migrations, project CRUD (`create`, `load`, `list`, `update`, `delete`),
  symbol-library CRUD, PCB-design CRUD (`save`, `load`, `delete`) storing
  tracks, vias, footprints and DRC rules as JSON, and validated plugin CRUD
  with persisted enable/disable state and capability filters.
- ✅ **kicad-parser** — deterministic S-expression reader, `.kicad_sym`
  loader mapping symbols and pins into `core::SymbolLibrary` with UUIDv5 ids,
  plus `.kicad_pcb`, `.kicad_sch`, `.kicad_mod` and `.kicad_pro` file
  interchange. `ProjectBundle` follows child sheet files recursively and
  carries `sym-lib-table`, `fp-lib-table` and project-local libraries. Unknown
  board/schematic nodes are retained for export; zones, keepouts, arcs,
  graphics, standard pad shapes and project DRC rules are semantically modeled.
- ✅ **kai-cad** — binary with a banner, `--help`/`--version`, and a
  deterministic self-check that calls every subsystem, exports a KiCad netlist,
  round trips a footprint library, routes a track and verifies it is DRC-clean,
  crosses a full-width top-layer wall using deterministic through vias, rejects
  a blocked route, applies a validated suggestion and undoes/redoes it, drives
  a bounded multi-step undo/redo history, discovers and registers validated
  plugin manifests, manages the persisted plugin registry through `plugins
  list|show|enable|disable|discover`, converts supported KiCad PCBs through
  `pcb import|export`, creates a session and enforces roles,
  records a versioned edit, stages/drains it, stores and reloads a PCB design,
  shoves and interactively routes copper, and renders the board in 2D and 3D.

## Roadmap

| Phase | Weeks | Focus | Status |
|-------|-------|-------|--------|
| 1 | 1–4 | Foundation: workspace, CI, engineering database, symbols, basic schematic engine | ✅ Complete |
| 2 | 5–8 | PCB core: stackups, footprints, routing, DRC, basic renderer | ✅ Complete (basic software renderer; Vulkan GPU backend deferred) |
| GUI | — | KaiCad Studio desktop editor: open/save, 2D/3D, DRC, track routing | 🚧 Viewer + track routing; footprint placement next |
| 3 | 9–12 | kAI, collaboration, plugin architecture, performance tests | 🚧 Week 12 complete; kAI generation, real-time sync and plugin execution deferred |
| 4 | 13–16 | Plugin persistence and management, polish, beta and foundation-level KiCad compatibility work | 🚧 Weeks 13–15 complete; `.kicad_pcb` subset landed; release readiness next |
| Parity | post-beta | Full KiCad workflow and project-file compatibility | ⬜ Multi-phase; see the parity plan |

See [`docs/PLAN.md`](docs/PLAN.md) for the foundation roadmap,
[`docs/KICAD_PARITY.md`](docs/KICAD_PARITY.md) for the full replacement
program, and [`docs/PHASE1_TASKS.md`](docs/PHASE1_TASKS.md) for the Phase 1
breakdown.

## License
GPL-3.0-or-later
