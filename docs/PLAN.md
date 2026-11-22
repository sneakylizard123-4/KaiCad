# KaiCad — Implementation Plan

## 1. Project Overview

**KaiCad** is a complete, high-performance, open-source EDA application written in Rust, designed as a modern alternative to KiCad. The project targets Linux (primary), with Windows and macOS support, and must maintain native compatibility with KiCad 9 and KiCad 10.

**Key Requirements from PRD:**
- Rust-first architecture
- Vulkan-based rendering engine
- Full schematic and PCB design capabilities
- Integrated design-rule and electrical-rule checking
- Advanced PCB routing and engineering analysis
- Built-in AI assistance via kAI subsystem
- Collaborative workflows and extensible plugin/scripting architecture
- Local-first project model with persistence
- Deterministic engineering operations
- No mandatory cloud dependencies

## 2. Architecture

### 2.1 Core Subsystems

| Subsystem | Responsibility | Technology |
|-----------|----------------|------------|
| **Engineering Database** | Project metadata, component definitions, design data | Rust + SQLite/PostgreSQL |
| **Geometry Engine** | Schematic and PCB geometry, footprints, layers | Rust + custom geometry libs |
| **Schematic Engine** | Netlist generation, rule checking, cross-probing | Rust |
| **PCB Engine** | Multi-layer board design, stackups, routing | Rust + custom routing algorithm |
| **Renderer** | Vulkan-based 3D/2D visualization | Rust + Vulkan API |
| **AI Subsystem (kAI)** | Circuit optimization, design suggestions | Rust + LLM integration |
| **Collaboration Layer** | Version control, sharing, real-time sync | Rust + WebSocket |
| **Plugin/Scripting** | Extensible architecture | Rust + Cargo workspace |

### 2.2 Modular Design
- Each subsystem is a separate crate in a Cargo workspace
- Shared types and interfaces via `src/lib.rs`
- Plugin system using `cargo` entry points and dynamic loading
- Transactional editing for data integrity

## 3. Implementation Roadmap

### Phase 1: Foundation (Weeks 1-4)
- **Set up project structure** — Cargo workspace, CI/CD, testing infrastructure
- **Engineering Database** — Core schema, project CRUD operations
- **Basic Schematic Engine** — Netlist creation, symbol management
- **Rust toolchain** — Dependency management, linting, formatting

### Phase 2: PCB Core (Weeks 5-8)
- **PCB Engine** — Board outline, layers, stackup, footprints
- **Routing** — Push-and-shove algorithm, interactive routing
- **Electrical Rule Checking** — Continuous validation during design
- **Vulkan Renderer** — Basic 2D/3D visualization of schematics and PCBs

### Phase 3: AI & Collaboration (Weeks 9-12)
- **kAI Subsystem** — Circuit optimization, design suggestions
- **Collaboration Layer** — Project sharing, versioning, real-time sync
- **Plugin Architecture** — Extensible module system
- **Testing** — Unit tests, integration tests, performance benchmarks

### Phase 4: Polish & Release (Weeks 13-16)
- **Performance optimization** — Vulkan rendering, incremental computation
- **Documentation** — User guides, API reference
- **Beta release** — Internal testing with target users
- **Final QA** — Compatibility with KiCad 9/10, bug fixes

### Implementation status

**Weeks 1–12 are complete and verified.** The Cargo workspace, CI pipeline,
toolchain configuration, shared data model, SQLite engineering database, named
symbol libraries, KiCad `.kicad_sym` import, the basic schematic engine
(netlist generation and export, graph traversal, ERC), the PCB core (outline
geometry, layer validation, footprint libraries), PCB geometry/DRC (distance
primitives, track and via dimensions, pad copper, validated rules),
deterministic single-layer and multi-layer grid routing with through vias,
atomic core edit transactions with kAI suggestion application,
permission-checked collaborative edit sessions with version history, bounded
multi-step undo/redo history, and a validated local plugin manifest registry
are in place. PCB designs persist with their tracks, vias and DRC rules in
SQLite. The workspace has **282 passing tests**, clean `cargo clippy
--workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`, and a
successful release build. See [`WEEK1_TASKS.md`](WEEK1_TASKS.md) for Week 1
evidence and [`NEXT_AGENT_PROMPT.md`](NEXT_AGENT_PROMPT.md) for the Week 13
brief.

Phase 1 progress:

| Week | Focus | Status |
|------|-------|--------|
| 1 | Project scaffolding: workspace, CI, toolchain, core data model | ✅ Complete |
| 2 | Engineering database: schema, migrations, SQLite-backed project CRUD | ✅ Complete |
| 3 | Symbol management: symbol libraries and KiCad symbol loading | ✅ Complete |
| 4 | Schematic engine: netlist export, graph operations, broader ERC | ✅ Complete |

Phase 2 progress:

| Week | Focus | Status |
|------|-------|--------|
| 5 | PCB core: outline geometry, layer-stack validation, footprint libraries | ✅ Complete |
| 6 | PCB geometry and design-rule checking | ✅ Complete |
| 7 | Deterministic single-layer routing | ✅ Complete |
| 8 | Multi-layer routing, vias and renderer integration | ✅ Complete (routing; renderer deferred) |
| 9 | kAI suggestion application and transactional edits | ✅ Complete |
| 10 | Collaborative edit sessions, permissions and version history | ✅ Complete |
| 11 | Multi-step edit history and redo branching | ✅ Complete |
| 12 | Plugin manifest registry and deterministic discovery | ✅ Complete |
| 13 | SQLite-backed plugin registry (schema v5) | ⬜ Not started |

The detailed brief for the next implementation pass — including API
requirements, acceptance criteria and verification commands — is in
[`NEXT_AGENT_PROMPT.md`](NEXT_AGENT_PROMPT.md). It currently covers Week 13:
persisting validated plugin manifests with deterministic CRUD.

## 4. Key Milestones

| Milestone | Target | Deliverable |
|-----------|--------|-------------|
| **M1** | End of Week 4 | Working engineering database + basic schematic editor |
| **M2** | End of Week 8 | Full PCB design capabilities + basic renderer |
| **M3** | End of Week 12 | AI optimization features + collaboration layer |
| **M4** | End of Week 16 | Beta release, documentation, ready for open-source launch |

## 5. Technical Approach

### 5.1 Rust Best Practices
- Use `clippy` for linting, `cargo fmt` for formatting
- Implement transactional editing with undo/redo
- Ensure deterministic operations (no non-deterministic randomness)
- Write comprehensive unit and integration tests

### 5.2 Performance
- Vulkan-based rendering for GPU acceleration
- Incremental computation for fast design updates
- Transactional editing for data consistency
- Caching of geometry and rule-check results

### 5.3 Compatibility
- Maintain bidirectional sync with KiCad 9/10 project files
- Use KiCad's file format as a reference for import/export
- Abstract EDA operations behind stable APIs

## 6. Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Complexity of full EDA stack | High | Start with core schematic + PCB, add features iteratively |
| Performance bottlenecks | High | Early Vulkan integration, profiling, incremental computation |
| AI reliability | Medium | Deterministic validation of AI outputs, human-in-the-loop review |
| Cross-platform consistency | Medium | Test on Linux, Windows, macOS early; use cross-platform libraries |

## 7. Success Criteria

- [ ] KaiCad can import/export KiCad 9/10 projects
- [ ] Full schematic design (nets, symbols, rules) works
- [x] Multi-layer PCB design with routing is functional
- [ ] Vulkan renderer produces accurate 2D/3D visualizations
- [ ] kAI provides meaningful circuit optimization suggestions
- [ ] Collaborative features (sharing, versioning) are operational
- [ ] Code quality meets Rust best practices (clippy pass, test coverage)

---

*Plan created based on PRD.md (KaiCad Master PRD)*
