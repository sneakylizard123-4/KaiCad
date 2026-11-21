# KaiCad TODO

A living list of completed work and upcoming tasks. For the full roadmap see
[`docs/PLAN.md`](docs/PLAN.md); for the immediate next pass see
[`docs/NEXT_AGENT_PROMPT.md`](docs/NEXT_AGENT_PROMPT.md).

## ✅ Done

### Week 1 — Project scaffolding
- Cargo workspace with `core`, `schematic`, `pcb`, `renderer`, `ai`, `collab`, `kai-cad`
- Shared data model (`Project`, `Symbol`, `Net`, `Sheet`, `Component`, PCB types)
- JSON serialization and file persistence (`core::serialization`)
- Deterministic netlist generation and three-rule ERC
- CI, formatting, clippy, docs and release build
- 71 tests passing

### Week 2 — Engineering database
- New `store` crate with `rusqlite` (`bundled`)
- Schema v1: `projects`, `sheets`, `symbols`, `components`, `nets`, `footprints`, `schema_migrations`
- Idempotent migration runner
- Synchronous SQLite project CRUD (`create`, `load`, `list`, `update`, `delete`)
- Projection tables rebuilt from the authoritative `projects.document` JSON
- `store/examples/project_crud.rs`
- Wired store into `kai-cad` self-check and smoke tests
- Documentation updated; total tests: 85
- Pushed to GitHub as `af3d0a7`

### Week 3 — Symbol management
- `core::symbol_library::{SymbolLibrary, SymbolLibrarySource}` with JSON round trip
- New `kicad-parser` crate: deterministic S-expression tokenizer/parser
- `.kicad_sym` → `core::SymbolLibrary` mapping with UUIDv5 symbol, pin and library ids
- KiCad pins mapped to name, number, electrical type, position and direction; unknown types become `PinType::Unspecified`
- Schema v3 migration `0003_symbol_libraries.sql` with `(name)` index
- Store symbol-library CRUD (`create`, `load`, `list`, `update`, `delete`)
- `kicad-parser/examples/parse_library.rs`
- Parser wired into the `kai-cad` self-check and cross-crate smoke tests
- Documentation and the next-agent brief updated; total tests: 110

### Week 4 — Schematic engine
- Deterministic KiCad netlist export (`schematic::to_kicad_netlist`)
- Graph traversal: `edges_for`, `neighbours`, `is_connected`, `connected_components`
- ERC rules `unconnected-pin` (warning) and `duplicate-symbol-name` (error)
- Netlist export wired into the `kai-cad` self-check and smoke tests
- Documentation updated; total tests: 126

## 🚧 Next — Week 5: PCB core

See [`docs/NEXT_AGENT_PROMPT.md`](docs/NEXT_AGENT_PROMPT.md) for the full brief.

- [ ] Deterministic board-outline geometry: area, contains-point, bounding box
- [ ] Layer-stack validation and query helpers
- [ ] Footprint library model and loading
- [ ] Wire the new PCB work into the `kai-cad` self-check and smoke tests
- [ ] Update docs (README, ARCHITECTURE, DEVELOPMENT, CONTRIBUTING, PLAN, PHASE1_TASKS)
- [ ] Target: ≥140 tests, clippy/fmt/doc clean

## ⬜ Later milestones

### Phase 1
- ✅ Complete (Weeks 1–4, basic schematic engine)

### Phase 2 — PCB core (Weeks 5–8)
- Board outline, layers, stackup
- Footprint library loading
- Push-and-shove routing
- Interactive routing
- DRC
- Basic 2D/3D Vulkan renderer

### Custom GUI (not yet scheduled)
- No dedicated GUI week exists in the 16-week plan; the application shell is currently the `kai-cad` CLI.
- The basic Vulkan renderer (Phase 2, Weeks 5–8) is a prerequisite for any graphical editor.
- The custom GUI should be scoped and scheduled after basic schematic/PCB rendering is available.
- [ ] Decide GUI scope and slot it into the roadmap (likely after Phase 2 or as a new phase)

### Phase 3 — kAI & collaboration (Weeks 9–12)
- Circuit optimization suggestions
- Deterministic validation integration
- Project sharing and versioning
- Real-time sync transport
- Plugin architecture

### Phase 4 — Polish & release (Weeks 13–16)
- Performance optimization
- User guides and API reference
- KiCad 9/10 import/export QA
- Beta release

## 🏠 Housekeeping

- [ ] Add SPDX license headers to new files as they are substantially rewritten
- [ ] Keep `Cargo.lock` committed
- [ ] Maintain `docs/NEXT_AGENT_PROMPT.md` as the active brief
- [ ] Add a Markdown link checker to CI
