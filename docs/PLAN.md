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
| **Renderer** | Deterministic software 3D/2D visualization; Vulkan backend planned | Rust (Vulkan API planned) |
| **GUI** | Native board viewing, DRC and track routing; more editing planned | Rust + eframe/egui |
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
- **Renderer** — Basic 2D/3D visualization of PCBs (software rasterizer complete; Vulkan GPU backend deferred)

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

**Weeks 1–15 and the Phase 2 PCB/renderer scope are complete and verified.**
The Cargo workspace, CI pipeline, toolchain configuration, shared data model,
SQLite engineering database, named symbol libraries, KiCad `.kicad_sym` import,
the basic schematic engine (netlist generation and export, graph traversal,
ERC), the PCB core (outline geometry, layer validation, footprint libraries),
PCB geometry/DRC (distance primitives, track and via dimensions, rotated pad
copper, validated rules), deterministic single-layer and multi-layer grid
routing with through/blind/buried vias, deterministic push-and-shove and
interactive routing, atomic core edit transactions with kAI suggestion
application, permission-checked collaborative edit sessions with version
history, bounded multi-step undo/redo history, and a validated plugin manifest
registry with SQLite persistence, activation state and CLI management are in
place. A deterministic headless software renderer produces framed 2D top views
and depth-tested 3D board views; the Vulkan GPU backend remains deferred. A
post-Phase-2 eframe-based KaiCad Studio shell opens and saves `.kicad_pcb` board
documents natively (JSON is optional), presents the software-rendered 2D/3D
viewport with 2D zoom and pan, runs DRC, inspects the stackup
and footprints, and adds undoable click-to-route track editing with DRC
preflight. `kicad-parser` also provides `.kicad_pcb`, `.kicad_sch`, `.kicad_mod` and
`.kicad_pro` file interchange through `kai-cad pcb|sch|footprint|project`,
retaining unmodelled constructs instead of silently dropping them. The P2 KiCad
interoperability milestone is complete: zones, keepouts, arcs, graphics,
standard pad shapes and project DRC rules are modeled semantically, and
`ProjectBundle` handles library tables and `${KIPRJMOD}` libraries. Footprint
placement, schematic capture, interactive zone editing and full KiCad project
compatibility are deferred. PCB designs persist with their tracks, vias and DRC
rules in SQLite.
The workspace has **389 passing tests**, clean `cargo clippy --workspace
--all-targets -- -D warnings`, `cargo fmt --all -- --check`, and a successful
release build. See
[`WEEK1_TASKS.md`](WEEK1_TASKS.md) for Week 1 evidence,
[`NEXT_AGENT_PROMPT.md`](NEXT_AGENT_PROMPT.md) for the Week 16 release brief,
and [`KICAD_PARITY.md`](KICAD_PARITY.md) for the full-replacement sequence.

Phase 1 progress:

| Week | Focus | Status |
|------|-------|--------|
| 1 | Project scaffolding: workspace, CI, toolchain, core data model | ✅ Complete |
| 2 | Engineering database: schema, migrations, SQLite-backed project CRUD | ✅ Complete |
| 3 | Symbol management: symbol libraries and KiCad symbol loading | ✅ Complete |
| 4 | Schematic engine: netlist export, graph operations, broader ERC | ✅ Complete |

Foundation progress (Phases 2–4):

| Week | Focus | Status |
|------|-------|--------|
| 5 | PCB core: outline geometry, layer-stack validation, footprint libraries | ✅ Complete |
| 6 | PCB geometry and design-rule checking | ✅ Complete |
| 7 | Deterministic single-layer routing | ✅ Complete |
| 8 | Multi-layer routing, vias and renderer integration | ✅ Complete (software renderer; Vulkan GPU backend deferred) |
| 9 | kAI suggestion application and transactional edits | ✅ Complete |
| 10 | Collaborative edit sessions, permissions and version history | ✅ Complete |
| 11 | Multi-step edit history and redo branching | ✅ Complete |
| 12 | Plugin manifest registry and deterministic discovery | ✅ Complete |
| 13 | SQLite-backed plugin registry (schema v5) | ✅ Complete |
| 14 | Plugin activation state and capability queries (schema v6) | ✅ Complete |
| 15 | Plugin management CLI and end-to-end tests | ✅ Complete |
| 16 | Beta readiness: CLI hardening, docs, performance baselines, release checklist | ⬜ Not started |

The detailed brief for the next release-hardening pass — including API
requirements, acceptance criteria and verification commands — is in
[`NEXT_AGENT_PROMPT.md`](NEXT_AGENT_PROMPT.md). It covers Week 16: release
validation, not full KiCad parity.

The 16-week plan is the foundation program. The long-range full-KiCad target,
capability gaps and delivery sequence are in
[`KICAD_PARITY.md`](KICAD_PARITY.md); do not mark parity complete from the
16-week milestones alone.

## 4. Key Milestones

| Milestone | Target | Deliverable |
|-----------|--------|-------------|
| **M1** | End of Week 4 | Working engineering database + basic schematic editor |
| **M2** | End of Week 8 | Phase 2 PCB scope + basic renderer — ✅ complete (software renderer) |
| **M3** | End of Week 12 | AI optimization features + collaboration layer |
| **M4** | End of Week 16 | Foundation beta, documentation and release validation; KiCad parity follows separately |

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
- Maintain bidirectional sync with KiCad 9/10 project files — full project
  compatibility remains the target
- Support `.kicad_pcb`, `.kicad_sch`, `.kicad_mod` and `.kicad_pro` file
  interchange through `kicad-parser` and the `kai-cad pcb|sch|footprint|project`
  CLI, retaining unknown constructs for export
- Use KiCad's file format as a reference for import/export and keep semantic
  limitations explicit
- Abstract EDA operations behind stable APIs

## 6. Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Complexity of full EDA stack | High | Start with core schematic + PCB, add features iteratively |
| Performance bottlenecks | High | Early Vulkan integration, profiling, incremental computation |
| AI reliability | Medium | Deterministic validation of AI outputs, human-in-the-loop review |
| Cross-platform consistency | Medium | Test on Linux, Windows, macOS early; use cross-platform libraries |

## 7. Success Criteria

- 🚧 KaiCad can import/export KiCad board, schematic, footprint and project
  files with semantic board objects, DRC rules and library tables; the single
  integrated project document and schematic capture are not implemented
- [ ] Full schematic design (nets, symbols, rules) works
- [x] Multi-layer PCB design with routing is functional
- [x] Software renderer produces deterministic 2D/3D board visualizations
- [x] Native GUI shell opens/saves boards, renders navigable 2D/3D views and reports DRC
- [x] GUI supports basic interactive track routing with undo/redo and DRC preflight
- [ ] GUI supports footprint placement and complete PCB editing
- [ ] GUI supports schematic editing and schematic/PCB cross-probing
- [ ] Vulkan GPU renderer produces accelerated 2D/3D visualizations
- [ ] kAI provides meaningful circuit optimization suggestions
- [ ] Collaborative features (sharing, versioning) are operational
- [ ] Code quality meets Rust best practices (clippy pass, test coverage)

---

*Plan created based on PRD.md (KaiCad Master PRD)*
