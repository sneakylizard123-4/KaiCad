# KaiCad Phase 1 — Foundation Tasks (Weeks 1-4)

> **Status legend:** ✅ complete · 🚧 partial (some acceptance criteria met) ·
> ⬜ not started. Phase 1 (Weeks 1–4) is complete and verified, and Phase 2
> began with Week 5 (PCB core). See [`WEEK1_TASKS.md`](WEEK1_TASKS.md) for
> Week 1 evidence and [`NEXT_AGENT_PROMPT.md`](NEXT_AGENT_PROMPT.md) for the
> Week 13 brief.

## Project Setup & Infrastructure

### Task 1.1: Initialize Cargo Workspace — ✅ complete
- **Owner:** Lead Developer
- **Deliverable:** `Cargo.toml` workspace root with workspace members
- **Dependencies:** None
- **Acceptance Criteria:**
  - The workspace root `Cargo.toml` declares the ten members: `core`,
    `schematic`, `pcb`, `renderer`, `ai`, `collab`, `plugin`, `store`,
    `kicad-parser` and the `kai-cad` binary
  - `cargo check` passes with no warnings
  - `cargo test` runs (initially no tests, but infrastructure in place)

### Task 1.2: Configure CI/CD Pipeline — ✅ complete
- **Owner:** DevOps Lead
- **Deliverable:** `.github/workflows/ci.yml` (or equivalent)
- **Dependencies:** Task 1.1
- **Acceptance Criteria:**
  - GitHub Actions workflow runs on push and pull requests
  - Workflow runs `cargo check`, `cargo test`, `cargo clippy`
  - Workflow posts clippy lint results as comments
  - Basic badge showing build status
- **Note:** the workflow runs fmt, check, clippy (`-D warnings`), docs, tests and
  a release build; the README carries the status badge. Clippy results appear in
  the job log rather than as a PR comment.

### Task 1.3: Set Up Rust Toolchain & Formatting — ✅ complete
- **Owner:** Lead Developer
- **Deliverable:** `rust-toolchain.toml`, `.clippy.toml`, `rustfmt.toml`
- **Dependencies:** Task 1.1
- **Acceptance Criteria:**
  - `rust-toolchain.toml` specifies stable Rust version
  - `.clippy.toml` enables recommended lints
  - `cargo fmt` and `cargo clippy` pass without errors

## Core Engineering Database

### Task 1.4: Design Database Schema — ✅ complete
- **Owner:** Lead Architect
- **Deliverable:** SQL schema or ORM model definition
- **Dependencies:** None
- **Acceptance Criteria:**
  - ✅ Schema defines: projects, components, symbols, nets, sheets, footprints
  - ✅ Migration scripts for schema evolution
  - Schema supports KiCad 9/10 bidirectional compatibility (deferred to import/export work)
- **Done:** `store/migrations/0001_initial.sql` creates `projects`, `sheets`,
  `symbols`, `components`, `nets`, `footprints` and `schema_migrations` tables;
  `0002` scopes projection keys per project and `0003` adds
  `symbol_libraries`. Foreign-key cascades keep projections in sync with
  project deletion.

### Task 1.5: Implement Project Model — ✅ complete
- **Owner:** Lead Developer
- **Deliverable:** Rust structs + persistence layer (SQLite recommended)
- **Dependencies:** Task 1.4
- **Acceptance Criteria:**
  - ✅ `Project` struct with: id, name, version, schematic, PCB, metadata
  - ✅ Basic CRUD operations working
  - Can serialize/deserialize project to/from KiCad .kicad_sch and .kicad_pcb format (deferred)
- **Done:** the `Project` struct and JSON persistence (`core::serialization`) keep
  working. The new `store` crate provides synchronous SQLite-backed CRUD
  (`Store::open`, `open_in_memory`, `create_project`, `load_project`,
  `list_projects`, `update_project`, `delete_project`) with idempotent
  migrations and projection tables rebuilt in the same transaction.

### Task 1.6: Basic Symbol Management — ✅ complete
- **Owner:** Schematic Lead
- **Deliverable:** Symbol data structures, library loading
- **Dependencies:** Task 1.5
- **Acceptance Criteria:**
  - ✅ Symbol struct with pins; reference designator and footprint association
    live on `core::Component`
  - ✅ Can load basic KiCad symbol library format
  - ✅ Can create/save symbols in internal format
- **Done:** `core::symbol_library::{SymbolLibrary, SymbolLibrarySource}` define
  internal libraries; the `kicad-parser` crate parses `.kicad_sym` files into
  symbols and pins with deterministic UUIDv5 ids; `store` persists libraries
  through migration `0003` and `create/load/list/update/delete`
  `symbol_library` methods. Pin positions, angles and directions are imported;
  known KiCad types map exactly (including `free` and `no_connect`) and only
  unknown types become `PinType::Unspecified` rather than being dropped.

## Basic Schematic Engine

### Task 1.7: Netlist Generation — ✅ complete
- **Owner:** Schematic Lead
- **Deliverable:** Netlist data structure + generation from schematic
- **Dependencies:** Tasks 1.5, 1.6
- **Acceptance Criteria:**
  - `Netlist` struct with: nets, components, connections
  - Can generate netlist from a basic schematic graph
  - Can export netlist to/from KiCad format
- **Done:** `schematic::netlist::{Netlist, Connection, PinRef}` and a working
  `generate_netlist` for symbol/net data, plus deterministic KiCad netlist
  export (`to_kicad_netlist`): sorted components and nets, codes from 1,
  escaped strings and a single trailing newline.
- **Remaining (deferred):** component-instance properties (`value`,
  `footprint` are empty in export), KiCad netlist import.

### Task 1.8: Schematic Graph Operations — 🚧 partial
- **Owner:** Schematic Lead
- **Deliverable:** Graph data structures for wires, junctions, labels
- **Dependencies:** Task 1.6
- **Acceptance Criteria:**
  - Can add/remove wires and junctions
  - Can create/hierarchical labels
  - Can navigate between schematic sheets
- **Done:** a connectivity `Graph` with `from_netlist`, node/edge counts, and
  deterministic read-only traversal: `edges_for`, `neighbours`, `is_connected`
  (BFS) and `connected_components`.
- **Remaining:** editing operations (add/remove wires and junctions), label
  semantics, hierarchical sheet navigation.

### Task 1.9: Basic Electrical Rule Checking — 🚧 partial
- **Owner:** ERC Lead
- **Deliverable:** Simple ERC rules implementation
- **Dependencies:** Tasks 1.7, 1.8
- **Acceptance Criteria:**
  - Can detect obvious errors: unconnected pins, power/ground nets
  - Can report ERC results in structured format
  - Can integrate with basic schematic operations
- **Done:** `run_erc` with structured `ERCResult`/`Violation` output and five
  deterministic rules: `net-single-pin`, `unconnected-pin` (warnings) and
  `duplicate-net-name`, `duplicate-symbol-name`, `self-connection` (errors).
- **Remaining:** power/ground-net rules, integration with editing operations.

## Testing & QA Infrastructure

### Task 1.10: Unit Test Framework — ✅ complete
- **Owner:** QA Lead
- **Deliverable:** Test directory structure, example tests
- **Dependencies:** Task 1.1
- **Acceptance Criteria:**
  - `cargo test` runs successfully
  - At least 3 baseline unit tests written for core data structures
  - Test coverage report generated
- **Note:** 282 tests pass across the workspace. The `Coverage` CI job runs
  `cargo llvm-cov` and uploads `lcov.info` as a build artifact.

### Task 1.11: Integration Test Harness — 🚧 partial
- **Owner:** QA Lead
- **Deliverable:** Test fixtures for project load/save cycles
- **Dependencies:** Task 1.5
- **Acceptance Criteria:**
  - Can load a minimal KiCad project and save it back
  - Round-trip preserves basic structure
  - Can detect data loss during import/export
- **Done:** `kai-cad/tests/smoke.rs` exercises cross-crate round trips,
  including JSON project save/load.
- **Remaining:** KiCad-format fixtures and data-loss detection.

## Phase 1 Completion Criteria

- [x] `cargo check` passes across all workspace crates
- [x] `cargo clippy` passes with no warnings
- [x] Basic project CRUD operations work (create, load, save) — JSON and SQLite
- [x] Basic schematic symbol management works — libraries parse and persist
- [x] Netlist generation/export works for simple circuits — generation and
      deterministic KiCad export
- [x] ERC catches basic rule violations — five rules incl. unconnected pins and
      duplicate names
- [x] Unit and integration test infrastructure in place
- [x] CI/CD pipeline running on GitHub Actions

## Week-by-Week Breakdown

| Week | Focus | Key Deliverables |
|------|-------|------------------|
| **1** | Project scaffolding | Cargo workspace, CI/CD, toolchain config |
| **2** | Engineering database | Schema design, Project model, basic CRUD |
| **3** | Symbol management | Symbol data structures, library loading |
| **4** | Schematic engine | Netlist generation, graph operations, basic ERC |

## Status note

Weeks 1–4 (Phase 1) are complete and verified. Week 4 added deterministic
KiCad netlist export, graph traversal helpers, two ERC rules and 16 new tests,
bringing the total from 110 to 126. Week 5 began Phase 2 with board-outline
geometry, layer-stack validation and JSON footprint libraries, adding 18 tests.
Week 6 added distance primitives, track/via dimensions and an eight-rule DRC,
adding another 18 tests. A follow-up limit-hardening pass added validated and
persisted rules, dedicated via limits and a same-net clearance exemption, with
9 more tests. Week 7 added deterministic single-layer routing, pad copper,
via-to-track clearance and SQLite PCB-design persistence, with 17 more tests,
for a total of **188 passing tests**. Week 8 added deterministic multi-layer
routing, through-via insertion with per-layer clearance checks, and a
via-network field so generated copper is exempt from its own clearance checks,
with 17 more tests for a total of **205 passing tests**. Week 9 added atomic
project edit transactions, validated kAI suggestion application and undo/redo,
with 20 more tests for a total of **225 passing tests**. Week 10 added
permission-checked collaborative edits, transaction-linked version digests and
deterministic sync staging, with 19 more tests for a total of **244 passing
tests**. Week 11 added bounded multi-step undo/redo history with redo-branch
clearing and capacity eviction, with 19 more tests for a total of **263
passing tests**. Week 12 added validated local plugin manifests, deterministic
registry ordering and duplicate rejection, and deterministic directory
discovery, with 19 more tests for a total of **282 passing tests**. `cargo check`,
`cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --
--check`, `cargo doc --workspace --no-deps`,
`python3 scripts/check_markdown_links.py` and `cargo build --workspace
--release` all succeed. See [`WEEK1_TASKS.md`](WEEK1_TASKS.md) for Week 1
evidence and [`NEXT_AGENT_PROMPT.md`](NEXT_AGENT_PROMPT.md) for the Week 13
(SQLite-backed plugin registry) work order.
