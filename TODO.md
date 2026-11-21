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

### Week 5 — PCB core
- Deterministic outline geometry: `signed_area`, `area`, `perimeter`, `contains_point`, `bounding_box`
- `LayerStack::validate` with `LayerIssue`/`LayerIssueKind`, sorted and deterministic
- `FootprintLibrary` JSON persistence (`to_json`, `from_json`, `save`, `load`) rejecting duplicate names
- Outline, layer and footprint-library checks wired into the `kai-cad` self-check and smoke tests
- CI: Markdown link checker, `cargo-llvm-cov` coverage job and uploaded `lcov.info`
- Documentation updated; total tests: 144

### Week 6 — PCB geometry and DRC
- Shared `Point::distance_to_segment` and `core::geometry::segment_distance`; non-finite input yields infinity
- `Track::width` (0.25 mm) and `Via::{diameter, drill}` (0.6/0.3 mm) with serde defaults for older documents
- `pcb::drc` with `DrcRules`, `DrcSeverity`, `DrcViolation`, `DrcReport` and eight geometry rules plus `drc-rules` limit validation: `track-width`, `track-layer`, `clearance`, `track-outside-board`, `via-outside-board`, `via-span`, `via-drill`, `track-path`
- DRC limits are persisted on `PcbDesign::rules`, validated before use, and cover a dedicated annular-ring limit, a minimum via drill and a same-net clearance exemption
- DRC wired into the `kai-cad` self-check and cross-crate smoke tests
- Documentation updated; total tests: 171

## 🚧 Next — Week 7: Deterministic routing

See [`docs/NEXT_AGENT_PROMPT.md`](docs/NEXT_AGENT_PROMPT.md) for the full brief.

- [ ] Grid/cost configuration and a `RouteRequest` API in `pcb::routing`
- [ ] Deterministic A* single-layer router that avoids existing copper and the board edge
- [ ] Validate routes with `run_drc` and wire them into the `kai-cad` self-check and smoke tests
- [ ] Update docs (README, ARCHITECTURE, DEVELOPMENT, CONTRIBUTING, PLAN, PHASE1_TASKS)
- [ ] Target: ≥187 tests, clippy/fmt/doc/link clean

## ⬜ Later milestones

### Phase 1
- ✅ Complete (Weeks 1–4, basic schematic engine)

### Phase 2 — PCB core (Weeks 5–8)
- ✅ Board outline, layers and stackup (Week 5)
- ✅ Footprint library loading (Week 5)
- ⬜ Push-and-shove routing
- ⬜ Interactive routing
- ✅ Deterministic DRC (Week 6)
- 🚧 Deterministic grid routing (Week 7)
- ⬜ Basic 2D/3D Vulkan renderer

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

- [x] Add a Markdown link checker to CI (`scripts/check_markdown_links.py`)
- [x] Add coverage reporting to CI (`cargo-llvm-cov`, `lcov.info` artifact)
- [ ] Add SPDX license headers to new files as they are substantially rewritten
- [ ] Keep `Cargo.lock` committed
- [ ] Maintain `docs/NEXT_AGENT_PROMPT.md` as the active brief
