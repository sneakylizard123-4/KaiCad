# KaiCad

[![CI](https://github.com/sneakylizard123-4/KaiCad/actions/workflows/ci.yml/badge.svg)](https://github.com/sneakylizard123-4/KaiCad/actions/workflows/ci.yml)

**KaiCad** is a complete, high-performance, open-source electronic design automation (EDA) application written in Rust.

## Status
- **Architecture:** Decided
- **Implementation:** Week 8 multi-layer routing complete (pre-alpha); Week 9 (kAI suggestion application) is next
- **Target:** Rust, Linux (primary), Windows, macOS

This repository currently contains the workspace structure, the shared data
model, a SQLite engineering database, engine skeletons, CI configuration,
documentation, examples and a Criterion benchmark. It is **not** a usable EDA
tool yet.

## Features
- Custom GUI
- Vulkan-based rendering engine
- Native KiCad 9/10 bidirectional compatibility
- Full schematic and PCB design capabilities
- Integrated design-rule and electrical-rule checking
- Advanced PCB routing and engineering analysis
- Built-in AI engineering assistance (kAI)
- Collaborative project workflows
- Extensible plugin and scripting architecture

## Getting Started

### Prerequisites
- Rust toolchain (stable channel, 1.85+; `rust-toolchain.toml` selects it)
- Git
- Vulkan SDK / loader (for rendering)
- Dependencies for your OS:
  - Ubuntu/Debian: `sudo apt-get install pkg-config libssl-dev libudev-dev libvulkan-dev vulkan-tools`
  - Fedora: `sudo dnf install pkgconf-pkg-config openssl-devel systemd-devel vulkan-loader-devel vulkan-tools`
  - macOS: `brew install pkg-config openssl`

See [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md) for the full setup guide.

### Building
```bash
# Clone the repository
git clone https://github.com/sneakylizard123-4/KaiCad.git
cd KaiCad

# Build the workspace
cargo build --workspace

# Run tests (205 tests across the workspace)
cargo test --workspace

# Run the application (banner + subsystem self-check)
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
├── renderer/                   # Vulkan renderer skeleton (ash)
├── ai/                         # kAI subsystem
├── collab/                     # collaboration layer
├── store/                      # SQLite engineering database + project CRUD
│   ├── src/
│   │   └── lib.rs
│   ├── migrations/
│   │   ├── 0001_initial.sql
│   │   ├── 0002_projection_primary_keys.sql
│   │   ├── 0003_symbol_libraries.sql
│   │   └── 0004_pcb_designs.sql
│   └── examples/
│       └── project_crud.rs
├── kicad-parser/               # KiCad symbol-library import (S-expression)
│   ├── src/
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
- [`docs/NEXT_AGENT_PROMPT.md`](docs/NEXT_AGENT_PROMPT.md) — self-contained brief for the next implementation pass (Week 9)
- [`PRD.md`](PRD.md) — master product requirements document
- [`docs/PLAN.md`](docs/PLAN.md) — implementation roadmap and technical approach
- [`docs/PHASE1_TASKS.md`](docs/PHASE1_TASKS.md) — Phase 1 (weeks 1–4) task breakdown
- [`docs/WEEK1_TASKS.md`](docs/WEEK1_TASKS.md) — Week 1 deliverables and verification status

## Current State

Weeks 1–8 are complete and verified: **205 tests pass**, `cargo clippy
--workspace --all-targets -- -D warnings` and `cargo fmt --all -- --check` are
clean, and the release build succeeds. Week 8 added deterministic multi-layer
A* routing (`RouteResult`, `route_multilayer`), through-via insertion with
clearance checks on every spanned copper layer, per-layer track splitting and
via-aware DRC.

The next milestone is **Week 9 — kAI suggestion application with undo/redo**:
apply validated suggestions through deterministic, reversible edits and wire
the flow into the self-check. The full brief is in
[`docs/NEXT_AGENT_PROMPT.md`](docs/NEXT_AGENT_PROMPT.md).

What each crate provides today:

- ✅ **core** — `Project`/`SchematicData`/`PcbData`/`ProjectMetadata`, symbols,
  nets, sheets, components, PCB outline/layer/stackup types (with outline area,
  perimeter, contains-point and bounding-box queries), `Point`, the shared
  `ChangeProposal`, and named symbol libraries. Real JSON serialization: `to_json`, `to_json_pretty`,
  `from_json`, `save`, `load` plus `Project::{to_json, from_json, save, load}`.
- ✅ **schematic** — real netlist generation (`generate_netlist` maps pin ids to
  `PinRef`s and emits connections for adjacent pins), deterministic KiCad
  netlist export (`to_kicad_netlist`), a connectivity `Graph` with
  neighbour/connectivity/component queries, and deterministic ERC with five
  rules: `net-single-pin` and `unconnected-pin` (warnings),
  `duplicate-net-name`, `duplicate-symbol-name` and `self-connection` (errors).
- 🚧 **pcb** — `PcbDesign` built on `core` types, board-outline geometry,
  `LayerStack` with validation, `FootprintLibrary` JSON persistence, track and
  via dimensions, placed footprints with pad copper, a deterministic DRC
  (`run_drc`) covering tracks, vias and pads with validated per-design limits,
  and deterministic single-layer and multi-layer A* routing (`route`,
  `route_multilayer`) with grid/cost configuration, through-via insertion and
  via-aware clearance checks. Pad rotation, interactive routing and blind or
  buried vias are not implemented.
- 🚧 **renderer** — `RendererConfig` and `WindowConfig` only. `Renderer::new`
  and `Renderer::render` return `NotImplemented`; no Vulkan objects are created.
- 🚧 **ai (kAI)** — configuration plus deterministic suggestion validation
  (confidence range, empty changes, no-op changes, negative constraints).
  `kAI::suggest_optimization` returns `NotImplemented`.
- 🚧 **collab** — `CollabManager` session creation, `VersioningManager`,
  `SharingManager` and `Window`. No transport, merge or real-time sync.
- ✅ **store** — SQLite engineering database (schema v4) with versioned
  migrations, project CRUD (`create`, `load`, `list`, `update`, `delete`),
  symbol-library CRUD, and PCB-design CRUD (`save`, `load`, `delete`) storing
  tracks, vias, footprints and DRC rules as JSON.
- ✅ **kicad-parser** — deterministic S-expression reader and `.kicad_sym`
  loader mapping symbols and pins into `core::SymbolLibrary` with UUIDv5 ids.
- ✅ **kai-cad** — binary with a banner, `--help`/`--version`, and a
  deterministic self-check that calls every subsystem, exports a KiCad netlist,
  round trips a footprint library, routes a track and verifies it is DRC-clean,
  crosses a full-width top-layer wall using deterministic through vias, rejects
  a blocked route, and stores and reloads a PCB design.

## Roadmap

| Phase | Weeks | Focus | Status |
|-------|-------|-------|--------|
| 1 | 1–4 | Foundation: workspace, CI, engineering database, symbols, basic schematic engine | ✅ Complete |
| 2 | 5–8 | PCB core: stackups, footprints, routing, DRC, basic renderer | 🚧 PCB core complete; renderer deferred |
| 3 | 9–12 | kAI, collaboration, plugin architecture, performance tests | ⬜ Not started |
| 4 | 13–16 | Polish, documentation, beta and KiCad 9/10 compatibility QA | ⬜ Not started |

See [`docs/PLAN.md`](docs/PLAN.md) for the full roadmap and
[`docs/PHASE1_TASKS.md`](docs/PHASE1_TASKS.md) for the Phase 1 breakdown.

## License
GPL-3.0-or-later
