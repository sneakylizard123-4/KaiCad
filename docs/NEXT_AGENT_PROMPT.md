# Next Agent Prompt — Week 7: Deterministic PCB Routing

You are continuing **KaiCad**, a pre-alpha EDA application written in Rust at
`/home/sneak/vscode/KaiCad`. Week 6 is finished and verified. Your job is
**Week 7 of Phase 2: deterministic single-layer grid routing** — a configurable
A* router that avoids existing copper and the board edge, returns simplified
tracks, and produces results that pass the design-rule check — while keeping
every existing API working.

This document is self-contained. Read it fully before writing code.

---

## 1. Current repository state

- **Branch:** `main`
- **Repository tip:** see `git log -1`
- **Tests:** 162 passing, 0 failing
- **Schema:** version 3 (`projects`, projection tables, `symbol_libraries`)
- **Toolchain:** `stable` channel; edition 2024; MSRV/`rust-version` **1.85**
- **License:** GPL-3.0-or-later

### Workspace crates

| Crate | Role | Workspace dependencies |
|-------|------|------------------------|
| `core` | Shared data model, JSON serialization, symbol libraries, PCB primitives | none |
| `schematic` | Netlist generation/export, connectivity graph, ERC | `core` |
| `pcb` | Board data, outline geometry, layers, footprints, DRC, routing config | `core` |
| `renderer` | Vulkan configuration types (`ash`) | none |
| `ai` | kAI suggestions + deterministic validation | `core`, `schematic` |
| `collab` | Sessions, versioning, sharing | `core` |
| `store` | SQLite database: project and symbol-library CRUD | `core` |
| `kicad-parser` | `.kicad_sym` import, deterministic S-expressions | `core` |
| `kai-cad` | Application binary + cross-crate smoke tests | all of the above |

### Existing PCB support (built in Weeks 5–6)

- `core::Point` with `new`, `distance_to`, `distance_to_segment`;
  `core::geometry::segment_distance` (symmetric, `0.0` for crossing or touching
  segments, `f64::INFINITY` for non-finite input).
- `core::PcbOutline` with `new`, `rectangle`, `is_closed`, `signed_area`,
  `area`, `perimeter`, `contains_point` (boundary inclusive) and
  `bounding_box`; `core::BoundingBox { min, max }`.
- `pcb::PcbDesign { id, outline, layers, stackup, footprints, tracks, vias }`.
- `pcb::Track { id, net_id, layer, width, path }` — `width` defaults to `0.25`
  mm when absent from JSON.
- `pcb::Via { id, position, layer_start, layer_end, diameter, drill }` —
  `diameter`/`drill` default to `0.6`/`0.3` mm when absent from JSON.
- `pcb::{DrcRules, DrcSeverity, DrcViolation, DrcReport, run_drc}` with eight
  rules: `track-width`, `track-layer`, `clearance`, `track-outside-board`,
  `via-outside-board`, `via-span`, `via-drill`, `track-path`. Reports are
  sorted by `(rule_id, objects)`; `DrcRules::default()` is `0.15` mm minimum
  width and `0.2` mm clearance; `DrcReport::is_valid` ignores warnings.
- `pcb::routing::{Router, RoutingAlgorithm, RoutingOptimization}` — the
  `Router` is configuration only and routes nothing.

What is missing for Week 7:

- No route request or routing cost configuration.
- No search: nothing connects two points with copper while avoiding existing
  tracks, vias and the board edge.
- No test showing that a generated route passes `run_drc`.

---

## 2. Non-negotiable project rules

These come from `PRD.md` and are enforced in review:

1. **No KiCad wrapping.** KaiCad implements its own engines. KiCad file formats
   are an import/export target, never the internal representation.
2. **Local-first.** No network access, cloud service or AI model may be
   required for editing or persistence.
3. **Deterministic operations.** No unseeded randomness in geometry, netlisting,
   routing, rule checks or export. Identical input must produce identical
   output, byte for byte. This includes generated track ids.
4. **No silent data loss.** Preserve or report data you cannot represent.
5. **AI output is untrusted.** Do not weaken `ai::SuggestionValidator`.
6. **Zero warnings.** `cargo clippy --workspace --all-targets -- -D warnings`
   and `cargo fmt --all -- --check` must pass.
7. **`core` stays dependency-light.** It already has `serde`, `serde_json`,
   `uuid` and `chrono`; add nothing new.
8. **No breaking changes** to existing public APIs. Adding fields with serde
   defaults and new methods is fine; renaming or removing is not.
9. **Update the docs** listed in §7 in the same change.
10. **Do not commit.** Leave the changes in the working tree and report what
    changed; the repository owner commits.

---

## 3. Deliverable

### 3.1 Routing configuration

Extend `pcb::routing` (keep `Router`, `RoutingAlgorithm` and
`RoutingOptimization` working):

```rust
/// Costs and limits for the grid router.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RouterConfig {
    /// Grid spacing in millimetres. Must be finite and positive.
    pub grid: f64,
    /// Extra path cost, in grid steps, for each direction change.
    pub bend_cost: f64,
    /// Maximum number of grid nodes the search may expand.
    pub max_expansions: usize,
}

impl Default for RouterConfig {
    // grid: 0.5, bend_cost: 1.0, max_expansions: 200_000
}

/// Add `pub config: RouterConfig` to `Router` with `#[serde(default)]`.
```

### 3.2 Route request and result

```rust
/// A single-net, single-layer routing request, in millimetres.
#[derive(Debug, Clone, PartialEq)]
pub struct RouteRequest {
    pub net_id: uuid::Uuid,
    pub start: core::Point,
    pub end: core::Point,
    pub layer: u16,
    /// Copper width in millimetres; defaults to 0.25.
    pub width: f64,
}

impl Default for RouteRequest { /* width: 0.25, everything else default */ }

#[derive(Debug, Clone, PartialEq)]
pub enum RouteError {
    /// The request or configuration is unusable. The message names the field.
    InvalidRequest(String),
    /// No path exists within the search bounds.
    NoPath,
    /// The search hit `RouterConfig::max_expansions` before finding a path.
    ExpansionLimit,
}

impl std::fmt::Display for RouteError { /* ... */ }
impl std::error::Error for RouteError {}

/// Route one net on one layer, returning simplified tracks.
pub fn route(
    request: &RouteRequest,
    design: &pcb::PcbDesign,
    config: &RouterConfig,
) -> Result<Vec<pcb::Track>, RouteError>;
```

Rules:

- Validate `config.grid` (finite, `> 0`), `request.width` (finite, `> 0`),
  both endpoints (finite) and `request.layer` (defined and copper). Validate
  that the outline has at least three points. Any failure returns
  `InvalidRequest` naming the field; never panic.
- Snap `start` and `end` to the nearest node of an origin-anchored grid
  (`(value / grid).round() * grid`, so exact half-grid ties round away from
  zero) and document that returned endpoints are the snapped points. If both
  snap to the same node, return `InvalidRequest`.
- The search is 4-neighbour A* over grid nodes inside the outline. A node is
  unusable when it is outside the outline, when it is closer than
  `clearance + request.width / 2 + other.width / 2` to any existing track on
  the same copper layer, or when it is closer than
  `clearance + request.width / 2 + via.diameter / 2` to a via that spans the
  request layer. Use `DrcRules::default().clearance`.
- A via blocks the request layer when its start or end equals that layer, or
  when the layer lies between them in stackup order. With an empty stackup,
  only the two endpoint layer ids block.
- Move cost is `1.0` per step plus `bend_cost` when the move direction differs
  from the previous one. The heuristic is Manhattan distance in grid steps.
  Break all ties deterministically, for example by expanding the lowest
  `(f, g, y, x)`; the result must not depend on heap insertion order.
- Return `NoPath` when the open set empties, and `ExpansionLimit` when
  `max_expansions` nodes have been expanded.
- Simplify the chosen node path by merging collinear points; keep the first and
  last node, the bends and the exact snapped coordinates. Return one `Track`
  today (`Vec` leaves room for via-split multi-layer output later) with
  `net_id` and `width` from the request and `layer` set to the request layer.
- Track ids must be deterministic: derive them with `uuid::Uuid::new_v5` from a
  fixed namespace and a name built from the net id, layer, width and snapped
  endpoints. Never use `Uuid::new_v4` in routing.
- Routing must not mutate `design`.
- The returned tracks, added to a clone of the design, must produce no `Error`
  violations from `run_drc` with default rules: inside the outline, on the
  requested copper layer, wide enough, two or more points, and clear of
  existing copper.

### 3.3 Wire routing into the application

Extend the `kai-cad` PCB self-check to:

1. Keep the existing 100 × 80 mm, two-copper-layer board and its DRC checks.
2. Route a new net between two points on an otherwise empty area and assert
   `route` succeeds.
3. Add the returned tracks to a clone of the design and assert `run_drc` is
   valid with no violations.
4. Block the path with a wall of existing tracks and assert the same request
   returns `RouteError::NoPath` (or `ExpansionLimit` if the wall forces the
   search over the cap).
5. Print the routed point count in the existing `pcb:` line.

Add a cross-crate smoke test in `kai-cad/tests/smoke.rs` that routes a clean
point-to-point connection, asserts the result is non-empty and DRC-clean, then
places a blocking wall and asserts the route fails with `NoPath`.

### 3.4 Tests (target: 16–18 new tests)

Cover at least:

1. `RouterConfig` defaults and JSON round trip.
2. A straight route between aligned points returns one track whose simplified
   path has no redundant collinear points.
3. The route's first and last points are the snapped endpoints.
4. Off-grid endpoints snap to the nearest grid node.
5. A route around a blocking track keeps clearance and stays in the outline.
6. A route through a gap narrower than the clearance fails rather than
   overlapping copper.
7. A wall of tracks from one board edge to the other returns `NoPath`.
8. A small `max_expansions` returns `ExpansionLimit`.
9. Invalid requests: non-finite start, zero width, zero grid, unknown layer,
   non-copper layer and identical snapped endpoints.
10. Vias block the layers they span and do not block unrelated layers.
11. The router never leaves the board outline.
12. Adding the route to the design passes `run_drc` with no violations.
13. Repeated routing of the same request is byte-identical, including track ids
    (serialize the tracks as JSON and compare strings).
14. Routing does not mutate the input design.
15. kai-cad smoke test for a clean route and a blocked route.

---

## 4. Definition of done

All of the following must be true:

- [ ] `RouterConfig`, `RouteRequest`, `RouteError` and `route` exist with the
      documented semantics and doc comments.
- [ ] `Router::config` round trips through serde and old `Router` JSON loads.
- [ ] Routing is deterministic, including ids; repeated calls are identical.
- [ ] A returned route passes `run_drc` and stays inside the outline.
- [ ] `kai-cad` self-check routes and blocks a path and still ends
      `self-check: ok`.
- [ ] The new smoke test passes.
- [ ] No public API was broken.
- [ ] Test count rises from 162 to at least 178, all passing.
- [ ] Clippy, fmt, rustdoc and Markdown links are clean.
- [ ] Documentation updated (§7).

---

## 5. Verification commands

Run from the workspace root and paste the tails in your report:

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo doc --workspace --no-deps
cargo test --workspace
python3 scripts/check_markdown_links.py
cargo run -p kicad-parser --example parse_library
cargo run -p store --example project_crud
cargo run -p pcb --example board_outline
cargo run -p kai-cad
cargo build --workspace --release
```

---

## 6. Suggested implementation order

1. Add `RouterConfig`, `RouteRequest`, `RouteError` and their defaults; update
   the `Router` field and its tests.
2. Implement grid snapping, node validity and the A* search with a small,
   deterministic test board; check straight and L-shaped routes first.
3. Add obstacle handling for tracks and vias, then simplification and id
   derivation; add the determinism and DRC-clean tests.
4. Wire routing into `kai-cad` and add the smoke test.
5. Run the full verification suite; fix warnings before moving on.
6. Update documentation and leave the changes uncommitted.

---

## 7. Documentation updates (required)

Update these files so they describe the new reality:

- `README.md` — status, test count, PCB features, roadmap.
- `docs/PLAN.md` — mark Week 7 complete and point to the Week 8 brief.
- `docs/PHASE1_TASKS.md` — update the status note (Phase 1 remains closed).
- `docs/ARCHITECTURE.md` — routing types, router algorithm and determinism
  guarantees, and the "Current milestone and next steps" section.
- `docs/DEVELOPMENT.md` — new test count, router example/self-check output and
  routing troubleshooting notes.
- `docs/CONTRIBUTING.md` — note that routing is deterministic, ids are derived
  with UUIDv5, and search ties must not depend on heap or map iteration order.
- `docs/NEXT_AGENT_PROMPT.md` — replace this brief with the Week 8 one once
  Week 7 is verified.
- `TODO.md` — mark Week 7 done and list Week 8.

---

## 8. Out of scope for this pass

Do not start these; they are later milestones:

- Multi-layer routing, via insertion, blind/buried vias or via optimisation.
- Push-and-shove, rip-up/reroute, interactive routing or length matching.
- Pad copper geometry, pad-to-track clearance or footprint placement.
- Differential pairs, impedance, thermal analysis or plane generation.
- KiCad `.kicad_pcb` import/export.
- Interactive editing, GUI, windowing or Vulkan object creation.
- Persisting routes in SQLite: tracks remain part of the in-memory design.
- Async, networking or collaboration transport.

---

## 9. Reporting format

When finished, report:

1. Files added and changed (do not commit).
2. The final public API of the routing configuration, request, error and
   `route` function.
3. Test count per crate and total.
4. Verification output tails for the commands in §5.
5. Anything you deliberately deferred, with the reason.

Follow the repository's commit conventions when describing the changes, but
leave the commit itself to the repository owner.
