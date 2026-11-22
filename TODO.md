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

### Week 7 — Deterministic routing and PCB persistence
- `RouterConfig` (`grid`, `bend_cost`, `max_expansions`), `RouteRequest`, `RouteError` and deterministic A* `route` in `pcb::routing`
- Grid snapping, board containment, track/via/pad obstacle avoidance, collinear simplification and UUIDv5 track ids
- DRC now covers pad copper (`pad-size`, `pad-outside-board`, pad clearance) and via-to-track clearance
- `PcbDesign` stores placed footprints with pad size, net and position, with serde defaults for older documents
- Schema v4 `pcb_designs` table and `Store::{save_pcb_design, load_pcb_design, delete_pcb_design}`
- Routing, pad/via DRC and design persistence wired into the `kai-cad` self-check and smoke tests
- Documentation updated; total tests: 188

### Week 8 — Multi-layer routing with vias
- `RouterConfig` fields for `allow_vias`, `via_cost`, `max_vias`, `via_diameter` and `via_drill`, with serde defaults for older JSON
- `RouteResult { tracks, vias }` and `route_multilayer`; `route` stays single-layer and delegates to the same search
- A* state now tracks node, layer, direction and via count; deterministic `(f, g, y, x, direction, layer, vias)` tie-break
- Through-via insertion checked against clearance on every spanned layer, `via-span`, `min_via_drill` and `min_annular_ring`
- Tracks split at layer changes, simplified per layer, with UUIDv5 track and via ids
- `Via::net_id` (serde-defaulted) lets DRC exempt a via from its own same-net tracks and pads
- Two-layer wall crossing and blocked-route checks wired into the `kai-cad` self-check and smoke tests
- Documentation updated; total tests: 205

### Week 9 — Transactional kAI suggestion application
- `core::edit` with `EditTransaction`, `AppliedChange`, `EditError`, atomic `apply`, `undo` and `redo`
- Stale-value detection, project/component targets, property creation/removal and rename-safe undo by component id
- Deterministic UUIDv5 transaction ids over the label and ordered changes
- `kAI::apply_suggestion` validates first, applies atomically and returns an undoable transaction; `kAIError::ApplyFailed` preserves edit errors
- Apply/undo/redo and failure atomicity wired into the `kai-cad` self-check and smoke tests
- Documentation updated; total tests: 225

## 🚧 Next — Week 10: Collaborative edit sessions and version history

See [`docs/NEXT_AGENT_PROMPT.md`](docs/NEXT_AGENT_PROMPT.md) for the full brief.

- [ ] Add participant membership, roles and edit permission checks
- [ ] Record versions linked to edit transactions and deterministic project digests
- [ ] Stage transactions deterministically for sync and drain them in order
- [ ] Wire collaboration sessions into the `kai-cad` self-check and smoke tests
- [ ] Update docs (README, ARCHITECTURE, DEVELOPMENT, CONTRIBUTING, PLAN, PHASE1_TASKS)
- [ ] Target: ≥241 tests, clippy/fmt/doc/link clean

## ⬜ Later milestones

### Phase 1
- ✅ Complete (Weeks 1–4, basic schematic engine)

### Phase 2 — PCB core (Weeks 5–8)
- ✅ Board outline, layers and stackup (Week 5)
- ✅ Footprint library loading (Week 5)
- ⬜ Push-and-shove routing
- ⬜ Interactive routing
- ✅ Deterministic DRC (Week 6)
- ✅ Deterministic single-layer grid routing (Week 7)
- ✅ Multi-layer routing with vias (Week 8)
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
