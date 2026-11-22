# Next Agent Prompt — Week 8: Multi-Layer Routing with Vias

You are continuing **KaiCad**, a pre-alpha EDA application written in Rust at
`/home/sneak/vscode/KaiCad`. Week 7 is finished and verified. Your job is
**Week 8: extend deterministic routing across copper layers with deterministic
via insertion**, keeping every existing API working and every generated route
DRC-clean.

Read this document fully before writing code.

---

## 1. Current repository state

- **Branch:** `main`
- **Tests:** 188 passing, 0 failing
- **Schema:** version 4 (`projects`, projection tables, `symbol_libraries`,
  `pcb_designs`)
- **Toolchain:** `stable`; edition 2024; `rust-version` **1.85**
- **License:** GPL-3.0-or-later

### Workspace crates

| Crate | Role | Workspace dependencies |
|-------|------|------------------------|
| `core` | Shared data model, JSON, symbol libraries, PCB primitives | none |
| `schematic` | Netlist, graph, ERC | `core` |
| `pcb` | Board data, outline, layers, footprints, DRC, routing | `core` |
| `renderer` | Vulkan configuration types (`ash`) | none |
| `ai` | kAI suggestions and deterministic validation | `core`, `schematic` |
| `collab` | Sessions, versioning, sharing | `core` |
| `store` | SQLite project, symbol-library and PCB-design CRUD | `core`, `pcb` |
| `kicad-parser` | `.kicad_sym` import | `core` |
| `kai-cad` | Binary and cross-crate smoke tests | all of the above |

### Existing PCB support

- `core::Point` with `distance_to`, `distance_to_segment` and
  `core::geometry::segment_distance`; `core::PcbOutline` with area, perimeter,
  containment and bounding box.
- `pcb::PcbDesign { id, outline, layers, stackup, footprints, tracks, vias,
  rules }`; tracks carry `net_id`, `layer`, `width` and a polyline path.
- `pcb::Via { id, position, layer_start, layer_end, diameter, drill }`.
- `pcb::Footprint { name, reference_designator, position, pad_positions,
  outline }` and `pcb::PadPosition { x, y, pad_type, width, height, net_id }`.
  Pads are copper rectangles placed at `footprint.position + (x, y)`;
  through-hole pads cover every copper layer, SMD/tented pads the top layer.
- `pcb::drc::{DrcRules, DrcRulesError, DrcSeverity, DrcViolation, DrcReport,
  run_drc}`: eight geometry rules plus `drc-rules` limit validation. Clearance
  covers track-track, track-via, pad-track, pad-via and pad-pad pairs on shared
  copper layers; same-net copper is exempt.
- `pcb::routing::{Router, RouterConfig, RouteRequest, RouteError, route}`:
  deterministic single-layer A* on an origin-anchored grid, UUIDv5 track ids,
  collinear simplification, obstacle avoidance for tracks, vias and pads, and
  a documented `(f, g, y, x, direction)` tie-break.
- `store` persists complete designs with `save_pcb_design` / `load_pcb_design` /
  `delete_pcb_design` (schema v4, cascade on project delete).

What is missing for Week 8:

- The router never changes layer and never creates a via, so any connection
  that cannot be made on one layer fails even when the board has spare copper.

---

## 2. Non-negotiable project rules

1. **No KiCad wrapping.** KiCad formats are import/export targets only.
2. **Local-first.** No network, cloud service or model may be required.
3. **Deterministic operations.** No unseeded randomness. Identical requests
   must produce byte-identical tracks and vias, including ids.
4. **No silent data loss.** Preserve or report what cannot be represented.
5. **AI output is untrusted.** Do not weaken `ai::SuggestionValidator`.
6. **Zero warnings.** `cargo clippy --workspace --all-targets -- -D warnings`
   and `cargo fmt --all -- --check` must pass.
7. **No breaking changes.** Add APIs and serde-defaulted fields; do not rename
   or remove existing ones. `route` must keep its current signature and
   single-layer behaviour.
8. **Never edit an applied migration.** Add `0005_*.sql` only if truly needed;
   this milestone should not need a schema change.
9. **Update the docs** listed in §7 in the same change.
10. **Do not commit.** Leave the working tree ready for the repository owner.

---

## 3. Deliverable

### 3.1 Configuration

Extend `RouterConfig` with serde-defaulted fields:

```rust
pub struct RouterConfig {
    pub grid: f64,              // existing: 0.5
    pub bend_cost: f64,         // existing: 1.0
    pub max_expansions: usize,  // existing: 200_000
    pub allow_vias: bool,       // new: true
    pub via_cost: f64,          // new: 5.0 (in grid steps)
    pub max_vias: usize,        // new: 64
    pub via_diameter: f64,      // new: 0.6
    pub via_drill: f64,         // new: 0.3
}
```

Validate all new values: `allow_vias` may be false, but `via_cost` must be
finite and non-negative, `max_vias > 0`, and via dimensions finite and
positive with `via_drill < via_diameter`.

### 3.2 Multi-layer search

Add a new entry point and keep `route` intact:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct RouteResult {
    pub tracks: Vec<Track>,
    pub vias: Vec<Via>,
}

pub fn route_multilayer(
    request: &RouteRequest,
    design: &PcbDesign,
    config: &RouterConfig,
) -> Result<RouteResult, RouteError>;
```

- `RouteRequest` keeps `start`, `end`, `layer`, `net_id` and `width`: both
  endpoints are on `request.layer`, but the path may leave that layer and
  return.
- The A* state becomes `(node, layer, direction)`. Four in-layer moves cost
  `1.0` plus `bend_cost` on a direction change, as today.
- A via transition stays on the same grid node and costs `via_cost`; it moves
  to another copper layer in the design's stackup. Prefer deterministic
  ordering (for example the next layer in stackup order, wrapping) and cap the
  total via count at `max_vias`.
- A candidate via is usable only when a through via of `config.via_diameter`
  and `config.via_drill`, centred on the node, would pass the clearance checks
  on **every** copper layer it spans and would satisfy `via-span` and
  `via-drill` with the design's rules. It must also leave the node itself
  unblocked on both the old and new layer.
- Tracks are split at layer changes: one `Track` per contiguous run on a layer,
  each simplified by merging collinear points. All runs use `request.net_id`
  and `request.width`.
- Vias are through vias spanning the first to the last copper layer in stackup
  order, with the configured diameter/drill.
- Ids must be deterministic UUIDv5 values derived from the request and the
  transition's node/layer, never `Uuid::new_v4`.
- With `allow_vias == false`, `route_multilayer` behaves like `route` (one
  track, no vias; `NoPath` when a single layer cannot connect).
- `route` itself must delegate to the same search with vias disabled and
  return only its tracks, preserving current tests and output.

### 3.3 Application wiring

Extend the `kai-cad` PCB self-check to:

1. Keep the existing single-layer route and its DRC-clean assertion.
2. Build a board where a full-width wall on `F.Cu` can only be crossed by
   changing layer, route across it with `route_multilayer`, and assert at
   least one via and two or more tracks are returned.
3. Add the tracks and vias to a clone of the design and assert `run_drc` is
   valid.
4. Print the track and via counts in the `pcb:` line.

Add a cross-crate smoke test for a two-layer crossing: a wall on the top layer,
a clean route with at least one via, DRC-clean output, and a deterministic
repeat of the same request.

### 3.4 Tests (target: 16–20 new tests)

Cover at least:

1. New `RouterConfig` defaults and JSON round trip, including legacy JSON.
2. Config validation: negative/NaN `via_cost`, zero `max_vias`, invalid via
   dimensions.
3. A route crosses a full-width top-layer wall using a via.
4. `allow_vias: false` returns `NoPath` for the same board.
5. The via sits at a snapped node, spans the stackup, and its diameter/drill
   match the config.
6. Tracks are split at the transition, each on the correct layer, with no
   duplicate or zero-length runs.
7. Via transitions respect clearance on all spanned layers.
8. `max_vias` is enforced.
9. A via whose drill/diameter is invalid for the rules is not used.
10. Multi-layer routing is byte-identical across repeated calls (including via
    ids).
11. The input design is not mutated.
12. Adding the result to the design passes `run_drc` with no errors.
13. A 4-layer board can use an inner layer, with a through via spanning top to
    bottom.
14. Single-layer `route` keeps its previous results for a board with no need
    for vias.
15. kai-cad smoke test for a two-layer crossing and a blocked route.

---

## 4. Definition of done

- [ ] `RouteResult`, `route_multilayer` and the new `RouterConfig` fields exist
      with the documented semantics.
- [ ] `route` is unchanged externally and still passes every Week 7 test.
- [ ] Multi-layer routes are deterministic (UUIDv5 ids) and DRC-clean.
- [ ] Vias respect clearance, span and drill rules on every layer they span.
- [ ] `kai-cad` self-check routes across a wall using a via and ends
      `self-check: ok`.
- [ ] No public API was broken.
- [ ] Test count rises from 188 to at least 204, all passing.
- [ ] Clippy, fmt, rustdoc and Markdown links are clean.
- [ ] Documentation updated (§7).

---

## 5. Verification commands

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo doc --workspace --no-deps
cargo test --workspace
python3 scripts/check_markdown_links.py
cargo run -p core --example project_roundtrip
cargo run -p kicad-parser --example parse_library
cargo run -p store --example project_crud
cargo run -p pcb --example board_outline
cargo run -p kai-cad
cargo build --workspace --release
```

---

## 6. Suggested implementation order

1. Add and validate the config fields; add `RouteResult`/`route_multilayer`
   stubs that delegate to the single-layer search.
2. Generalise the A* state and queue item to include the layer; keep the
   exact tie-break shape so single-layer output stays byte-identical.
3. Add via transitions and via validity checks, then split and simplify the
   path into per-layer tracks; add the via records.
4. Add determinism, clearance and max-via tests.
5. Wire the self-check and smoke test, then run the full verification suite.
6. Update documentation and leave the changes uncommitted.

---

## 7. Documentation updates (required)

- `README.md` — status, test count, PCB features and roadmap.
- `docs/PLAN.md` — mark Week 8 complete and point to the Week 9 brief.
- `docs/PHASE1_TASKS.md` — update the status note.
- `docs/ARCHITECTURE.md` — routing state, via insertion, new outputs and the
  "Current milestone" section.
- `docs/DEVELOPMENT.md` — test table, self-check sample and routing
  troubleshooting.
- `docs/CONTRIBUTING.md` — determinism rules for via ids and tie-breaks.
- `docs/NEXT_AGENT_PROMPT.md` — replace with the Week 9 brief.
- `TODO.md` — mark Week 8 done and list Week 9.

---

## 8. Out of scope for this pass

- Blind, buried or micro vias; back-drilling; via-in-pad.
- Push-and-shove, rip-up/reroute, interactive routing or length matching.
- Differential pairs, impedance, planes or thermal relief.
- Pad rotation and full pad shapes (rectangles only today).
- KiCad `.kicad_pcb` import/export.
- GUI, windowing or Vulkan object creation.
- Schema changes or database projections for tracks/vias.
- Async, networking or collaboration transport.

---

## 9. Reporting format

Report changed files (do not commit), the final routing API, test counts per
crate and total, verification tails, and anything deliberately deferred.
