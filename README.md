# KaiCad

[![CI](https://github.com/sneakylizard123-4/KaiCad/actions/workflows/ci.yml/badge.svg)](https://github.com/sneakylizard123-4/KaiCad/actions/workflows/ci.yml)

**KaiCad** is a complete, high-performance, open-source electronic design automation (EDA) application written in Rust.

## Status
- **Architecture:** Decided
- **Implementation:** Week 1 scaffold complete (pre-alpha)
- **Target:** Rust, Linux (primary), Windows, macOS

This repository currently contains the workspace structure, the shared data
model, engine skeletons, CI configuration, documentation, one example per
engine package and a Criterion benchmark. It is **not** a usable EDA tool yet.

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

# Run tests (71 tests across the workspace)
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
├── .github/workflows/ci.yml    # single "Check" CI job
├── docs/
│   ├── ARCHITECTURE.md         # crate graph, layering, data flow, design rules
│   ├── DEVELOPMENT.md          # setup, commands, examples, benchmarks, CI
│   └── CONTRIBUTING.md         # branches, commits, review checklist, license
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
├── kai-cad/                    # application binary (kai-cad)
│   ├── src/
│   │   └── main.rs
│   └── tests/
│       └── smoke.rs            # cross-crate integration smoke tests
├── PRD.md                      # product requirements
├── PLAN.md                     # implementation plan
├── PHASE1_TASKS.md             # Phase 1 task breakdown
└── WEEK1_TASKS.md              # Week 1 deliverables and status
```

## Documentation

- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — crate graph, layering, core data model, validation surfaces and design rules
- [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md) — prerequisites, toolchain, commands, examples, benchmarks, CI and troubleshooting
- [`docs/CONTRIBUTING.md`](docs/CONTRIBUTING.md) — branch/commit conventions, no-warnings policy, tests, review checklist, license headers
- [`PRD.md`](PRD.md) — master product requirements document
- [`PLAN.md`](PLAN.md) — implementation roadmap and technical approach
- [`PHASE1_TASKS.md`](PHASE1_TASKS.md) — Phase 1 (weeks 1–4) task breakdown
- [`WEEK1_TASKS.md`](WEEK1_TASKS.md) — Week 1 deliverables and verification status

## Current State

Week 1 scaffolding is complete and verified: **71 tests pass**, `cargo clippy
--workspace --all-targets -- -D warnings` and `cargo fmt --all -- --check` are
clean, and CI is green
([run 43071969392](https://github.com/sneakylizard123-4/KaiCad/actions/runs/43071969392),
commit `ecfbf17`).

What each crate provides today:

- ✅ **core** — `Project`/`SchematicData`/`PcbData`/`ProjectMetadata`, symbols,
  nets, sheets, components, PCB outline/layer/stackup types, `Point`, and the
  shared `ChangeProposal`. Real JSON serialization: `to_json`, `to_json_pretty`,
  `from_json`, `save`, `load` plus `Project::{to_json, from_json, save, load}`.
- ✅ **schematic** — real netlist generation (`generate_netlist` maps pin ids to
  `PinRef`s and emits connections for adjacent pins), a connectivity `Graph`,
  and deterministic ERC with three rules: `net-single-pin` (warning),
  `duplicate-net-name` (error) and `self-connection` (error).
- 🚧 **pcb** — `PcbDesign` built on `core` types, `LayerStack`, `Router`
  configuration and `Footprint`/`OutlineShape` types. No routing or geometry
  computation yet.
- 🚧 **renderer** — `RendererConfig` and `WindowConfig` only. `Renderer::new`
  and `Renderer::render` return `NotImplemented`; no Vulkan objects are created.
- 🚧 **ai (kAI)** — configuration plus deterministic suggestion validation
  (confidence range, empty changes, no-op changes, negative constraints).
  `kAI::suggest_optimization` returns `NotImplemented`.
- 🚧 **collab** — `CollabManager` session creation, `VersioningManager`,
  `SharingManager` and `Window`. No transport, merge or real-time sync.
- ✅ **kai-cad** — binary with a banner, `--help`/`--version`, and a
  deterministic self-check that calls every subsystem.

## License
GPL-3.0-or-later
