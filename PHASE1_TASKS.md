# KaiCad Phase 1 — Foundation Tasks (Weeks 1-4)

## Project Setup & Infrastructure

### Task 1.1: Initialize Cargo Workspace
- **Owner:** Lead Developer
- **Deliverable:** `Cargo.toml` workspace root with workspace members
- **Dependencies:** None
- **Acceptance Criteria:**
  - `cargo new --name kaicas --workspace` creates the workspace
  - Workspace includes at minimum: `core`, `schematic`, `pcb`, `renderer`, `ai`, `collab` crates
  - `cargo check` passes with no warnings
  - `cargo test` runs (initially no tests, but infrastructure in place)

### Task 1.2: Configure CI/CD Pipeline
- **Owner:** DevOps Lead
- **Deliverable:** `.github/workflows/ci.yml` (or equivalent)
- **Dependencies:** Task 1.1
- **Acceptance Criteria:**
  - GitHub Actions workflow runs on push and pull requests
  - Workflow runs `cargo check`, `cargo test`, `cargo clippy`
  - Workflow posts clippy lint results as comments
  - Basic badge showing build status

### Task 1.3: Set Up Rust Toolchain & Formatting
- **Owner:** Lead Developer
- **Deliverable:** `rust-toolchain.toml`, `.clippy.toml`, `rustfmt.toml`
- **Dependencies:** Task 1.1
- **Acceptance Criteria:**
  - `rust-toolchain.toml` specifies stable Rust version
  - `.clippy.toml` enables recommended lints
  - `cargo fmt` and `cargo clippy` pass without errors

## Core Engineering Database

### Task 1.4: Design Database Schema
- **Owner:** Lead Architect
- **Deliverable:** SQL schema or ORM model definition
- **Dependencies:** None
- **Acceptance Criteria:**
  - Schema defines: projects, components, symbols, nets, sheets, footprints
  - Schema supports KiCad 9/10 bidirectional compatibility
  - Migration scripts for schema evolution

### Task 1.5: Implement Project Model
- **Owner:** Lead Developer
- **Deliverable:** Rust structs + persistence layer (SQLite recommended)
- **Dependencies:** Task 1.4
- **Acceptance Criteria:**
  - `Project` struct with: id, name, version, schematic, PCB, metadata
  - Basic CRUD operations working
  - Can serialize/deserialize project to/from KiCad .kicad_sch and .kicad_pcb format (initial basic import/export)

### Task 1.6: Basic Symbol Management
- **Owner:** Schematic Lead
- **Deliverable:** Symbol data structures, library loading
- **Dependencies:** Task 1.5
- **Acceptance Criteria:**
  - Symbol struct with: reference designator, pins, footprint association
  - Can load basic KiCad symbol library format
  - Can create/save symbols in internal format

## Basic Schematic Engine

### Task 1.7: Netlist Generation
- **Owner:** Schematic Lead
- **Deliverable:** Netlist data structure + generation from schematic
- **Dependencies:** Tasks 1.5, 1.6
- **Acceptance Criteria:**
  - `Netlist` struct with: nets, components, connections
  - Can generate netlist from a basic schematic graph
  - Can export netlist to/from KiCad format

### Task 1.8: Schematic Graph Operations
- **Owner:** Schematic Lead
- **Deliverable:** Graph data structures for wires, junctions, labels
- **Dependencies:** Task 1.6
- **Acceptance Criteria:**
  - Can add/remove wires and junctions
  - Can create/hierarchical labels
  - Can navigate between schematic sheets

### Task 1.9: Basic Electrical Rule Checking
- **Owner:** ERC Lead
- **Deliverable:** Simple ERC rules implementation
- **Dependencies:** Tasks 1.7, 1.8
- **Acceptance Criteria:**
  - Can detect obvious errors: unconnected pins, power/ground nets
  - Can report ERC results in structured format
  - Can integrate with basic schematic operations

## Testing & QA Infrastructure

### Task 1.10: Unit Test Framework
- **Owner:** QA Lead
- **Deliverable:** Test directory structure, example tests
- **Dependencies:** Task 1.1
- **Acceptance Criteria:**
  - `cargo test` runs successfully
  - At least 3 baseline unit tests written for core data structures
  - Test coverage report generated

### Task 1.11: Integration Test Harness
- **Owner:** QA Lead
- **Deliverable:** Test fixtures for project load/save cycles
- **Dependencies:** Task 1.5
- **Acceptance Criteria:**
  - Can load a minimal KiCad project and save it back
  - Round-trip preserves basic structure
  - Can detect data loss during import/export

## Phase 1 Completion Criteria

- [ ] `cargo check` passes across all workspace crates
- [ ] `cargo clippy` passes with no warnings
- [ ] Basic project CRUD operations work (create, load, save)
- [ ] Basic schematic symbol management works
- [ ] Netlist generation/export works for simple circuits
- [ ] ERC catches basic rule violations
- [ ] Unit and integration test infrastructure in place
- [ ] CI/CD pipeline running on GitHub Actions

## Week-by-Week Breakdown

| Week | Focus | Key Deliverables |
|------|-------|------------------|
| **1** | Project scaffolding | Cargo workspace, CI/CD, toolchain config |
| **2** | Engineering database | Schema design, Project model, basic CRUD |
| **3** | Symbol management | Symbol data structures, library loading |
| **4** | Schematic engine | Netlist generation, graph operations, basic ERC |