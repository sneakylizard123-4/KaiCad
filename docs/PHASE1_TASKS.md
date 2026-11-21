# KaiCad Phase 1 — Foundation Tasks (Weeks 1-4)

> **Status legend:** ✅ complete · 🚧 partial (some acceptance criteria met) ·
> ⬜ not started. Weeks 1 and 2 are complete and verified; see
> [`WEEK1_TASKS.md`](WEEK1_TASKS.md) for Week 1 evidence and
> [`NEXT_AGENT_PROMPT.md`](NEXT_AGENT_PROMPT.md) for the Week 3 brief.

## Project Setup & Infrastructure

### Task 1.1: Initialize Cargo Workspace — ✅ complete
- **Owner:** Lead Developer
- **Deliverable:** `Cargo.toml` workspace root with workspace members
- **Dependencies:** None
- **Acceptance Criteria:**
  - The workspace root `Cargo.toml` declares the seven members: `core`,
    `schematic`, `pcb`, `renderer`, `ai`, `collab` and the `kai-cad` binary
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
  `symbols`, `components`, `nets`, `footprints` and `schema_migrations` tables.
  Foreign-key cascades keep projections in sync with project deletion.

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

### Task 1.6: Basic Symbol Management — 🚧 partial
- **Owner:** Schematic Lead
- **Deliverable:** Symbol data structures, library loading
- **Dependencies:** Task 1.5
- **Acceptance Criteria:**
  - Symbol struct with: reference designator, pins, footprint association
  - Can load basic KiCad symbol library format
  - Can create/save symbols in internal format
- **Done:** `core::symbol::{Symbol, Pin, ...}` and JSON serialization exist.
- **Remaining:** reference-designator/footprint association rules, KiCad symbol
  library parsing and an internal symbol library format.

## Basic Schematic Engine

### Task 1.7: Netlist Generation — 🚧 partial
- **Owner:** Schematic Lead
- **Deliverable:** Netlist data structure + generation from schematic
- **Dependencies:** Tasks 1.5, 1.6
- **Acceptance Criteria:**
  - `Netlist` struct with: nets, components, connections
  - Can generate netlist from a basic schematic graph
  - Can export netlist to/from KiCad format
- **Done:** `schematic::netlist::{Netlist, Connection, PinRef}` and a working
  `generate_netlist` for symbol/net data.
- **Remaining:** component-instance resolution, KiCad netlist import/export.

### Task 1.8: Schematic Graph Operations — 🚧 partial
- **Owner:** Schematic Lead
- **Deliverable:** Graph data structures for wires, junctions, labels
- **Dependencies:** Task 1.6
- **Acceptance Criteria:**
  - Can add/remove wires and junctions
  - Can create/hierarchical labels
  - Can navigate between schematic sheets
- **Done:** a connectivity `Graph` with `from_netlist`, node/edge counts.
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
- **Done:** `run_erc` with structured `ERCResult`/`Violation` output and three
  deterministic rules (`net-single-pin`, `duplicate-net-name`,
  `self-connection`).
- **Remaining:** unconnected-pin and power/ground-net rules, integration with
  editing operations.

## Testing & QA Infrastructure

### Task 1.10: Unit Test Framework — ✅ complete
- **Owner:** QA Lead
- **Deliverable:** Test directory structure, example tests
- **Dependencies:** Task 1.1
- **Acceptance Criteria:**
  - `cargo test` runs successfully
  - At least 3 baseline unit tests written for core data structures
  - Test coverage report generated
- **Note:** 85 tests pass across the workspace. Coverage reporting is not wired
  up yet; that remains a CI follow-up.

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
- [ ] Basic schematic symbol management works — data types only
- [ ] Netlist generation/export works for simple circuits — generation works;
      KiCad export does not
- [ ] ERC catches basic rule violations — three starter rules only
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

Week 1 (project scaffolding) and Week 2 (SQLite engineering database) are
complete and verified. The workspace now contains a `store` crate with
migrations, `Store` CRUD, 13 new unit tests and one new cross-crate integration
test, bringing the total from 71 to **85 passing tests**. `cargo check`,
`cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --
--check`, `cargo doc --workspace --no-deps` and `cargo build --workspace
--release` all succeed. See [`WEEK1_TASKS.md`](WEEK1_TASKS.md) for Week 1
evidence and [`NEXT_AGENT_PROMPT.md`](NEXT_AGENT_PROMPT.md) for the
Week 3 (symbol management) work order.
