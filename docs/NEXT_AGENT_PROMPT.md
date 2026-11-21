# Next Agent Prompt — Week 6: PCB Geometry and DRC

You are continuing **KaiCad**, a pre-alpha EDA application written in Rust at
`/home/sneak/vscode/KaiCad`. Week 5 is finished and verified. Your job is
**Week 6 of Phase 2: deterministic PCB geometry and design-rule checking** —
shared distance primitives, track and via dimensions, and a DRC report — while
keeping every existing API working.

This document is self-contained. Read it fully before writing code.

---

## 1. Current repository state

- **Branch:** `main`
- **Repository tip:** see `git log -1`
- **Tests:** 144 passing, 0 failing
- **Schema:** version 3 (`projects`, projection tables, `symbol_libraries`)
- **Toolchain:** `stable` channel; edition 2024; MSRV/`rust-version` **1.85**
- **License:** GPL-3.0-or-later

### Workspace crates

| Crate | Role | Workspace dependencies |
|-------|------|------------------------|
| `core` | Shared data model, JSON serialization, symbol libraries, PCB primitives | none |
| `schematic` | Netlist generation/export, connectivity graph, ERC | `core` |
| `pcb` | Board data, outline geometry, layers, footprints, routing config | `core` |
| `renderer` | Vulkan configuration types (`ash`) | none |
| `ai` | kAI suggestions + deterministic validation | `core`, `schematic` |
| `collab` | Sessions, versioning, sharing | `core` |
| `store` | SQLite database: project and symbol-library CRUD | `core` |
| `kicad-parser` | `.kicad_sym` import, deterministic S-expressions | `core` |
| `kai-cad` | Application binary + cross-crate smoke tests | all of the above |

### Existing PCB support (built in Week 5)

- `core::Point` with `new` and `distance_to`; `core::BoundingBox { min, max }`.
- `core::PcbOutline` with `new`, `rectangle`, `is_closed`, `signed_area`,
  `area`, `perimeter`, `contains_point` (boundary inclusive, even-odd ray
  casting) and `bounding_box`.
- `pcb::LayerStack` with `new`, `push`, `len`, `is_empty`, `copper_layers`,
  `find`, `validate(&Stackup)` and `copper_layers_in_stackup(&Stackup)`;
  `LayerIssue`/`LayerIssueKind` for `DuplicateId`, `DuplicateName`,
  `EmptyName`, `MissingFromStackup` and `NonCopperInStackup`.
- `pcb::FootprintLibrary` with `sorted`, `find`, `to_json`, `from_json`,
  `save`, `load` and `FootprintLibraryError`; libraries are KaiCad JSON.
- `pcb::PcbDesign { id, outline, layers, stackup, footprints, tracks, vias }`;
  `pcb::Track { id, net_id, layer, path: Vec<core::Point> }`;
  `pcb::Via { id, position, layer_start, layer_end }`.

What is missing for Week 6:

- No distance from a point to a line segment, so clearance checks cannot be
  written yet.
- Tracks have no width and vias have no diameter or drill, so width and
  annular-ring rules have nothing to measure.
- No DRC: nothing checks track layers, clearances, board containment or via
  spans.

---

## 2. Non-negotiable project rules

These come from `PRD.md` and are enforced in review:

1. **No KiCad wrapping.** KaiCad implements its own engines. KiCad file formats
   are an import/export target, never the internal representation.
2. **Local-first.** No network access, cloud service or AI model may be
   required for editing or persistence.
3. **Deterministic operations.** No unseeded randomness in geometry, netlisting,
   routing, rule checks or export. Identical input must produce identical
   output, byte for byte.
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

### 3.1 Shared distance primitives

Add to `core::geometry` (keep `Point` and `distance_to` unchanged):

```rust
impl Point {
    /// Shortest distance from this point to the segment `start`–`end`.
    pub fn distance_to_segment(&self, start: &Point, end: &Point) -> f64;
}

/// Shortest distance between two line segments.
pub fn segment_distance(
    first_start: &Point,
    first_end: &Point,
    second_start: &Point,
    second_end: &Point,
) -> f64;
```

Rules:

- A zero-length segment is a point: `distance_to_segment(p, p, p) ==
  p.distance_to(p)` and `segment_distance` degenerates accordingly.
- The result is symmetric: `segment_distance(a, b, c, d) ==
  segment_distance(c, d, a, b)`.
- Crossing segments return `0.0`; touching endpoints return `0.0`.
- Any non-finite coordinate returns `f64::INFINITY` (document it). Callers must
  not treat that as "far away and therefore fine" without checking.
- Implement projection clamping with `f64::clamp`; do not allocate.

### 3.2 Track and via dimensions

Extend the PCB data model. Defaults keep existing documents loadable:

```rust
pub struct Track {
    pub id: uuid::Uuid,
    pub net_id: uuid::Uuid,
    pub layer: u16,
    /// Copper width in millimetres.
    #[serde(default = "default_track_width")]
    pub width: f64,
    pub path: Vec<core::Point>,
}

pub struct Via {
    pub id: uuid::Uuid,
    pub position: core::Point,
    pub layer_start: u16,
    pub layer_end: u16,
    /// Outer copper diameter in millimetres.
    #[serde(default = "default_via_diameter")]
    pub diameter: f64,
    /// Finished drill diameter in millimetres.
    #[serde(default = "default_via_drill")]
    pub drill: f64,
}
```

- Defaults: track width `0.25`, via diameter `0.6`, via drill `0.3`.
- `Track` and `Via` must keep working with `..Default::default()`; update any
  struct literal in the workspace that now needs the new fields.
- Defaults must round trip through `serde_json` for documents written before
  this change (test the missing-field case explicitly).

### 3.3 Design-rule checking

Add `pcb::drc`:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct DrcRules {
    /// Minimum acceptable copper width in millimetres.
    pub min_track_width: f64,
    /// Minimum copper-to-copper gap in millimetres.
    pub clearance: f64,
}

impl Default for DrcRules {
    // min_track_width: 0.15, clearance: 0.2
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DrcSeverity { Warning, Error }

#[derive(Debug, Clone, PartialEq)]
pub struct DrcViolation {
    pub rule_id: String,
    pub severity: DrcSeverity,
    pub message: String,
    /// Ids of the objects involved (tracks, vias), in a deterministic order.
    pub objects: Vec<uuid::Uuid>,
    /// Board location most representative of the problem, when there is one.
    pub location: Option<core::Point>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DrcReport {
    pub violations: Vec<DrcViolation>,
}

impl DrcReport {
    /// True when there are no `Error` violations.
    pub fn is_valid(&self) -> bool;
    /// Count of violations with the given severity.
    pub fn count(&self, severity: DrcSeverity) -> usize;
}

/// Run every built-in rule on `design`.
pub fn run_drc(design: &pcb::PcbDesign, rules: &DrcRules) -> DrcReport;
```

Built-in rules (all deterministic; one violation per offending object or pair):

| Rule id | Severity | Trigger |
|---------|----------|---------|
| `track-width` | Error if `<= 0`, Warning below `min_track_width` | Track width |
| `track-layer` | Error | Track layer is missing from `design.layers` or is not copper |
| `clearance` | Error | Two tracks on the same copper layer are closer than `rules.clearance`, measured edge to edge (`segment_distance - w1/2 - w2/2`) |
| `track-outside-board` | Error | Any track path point is outside the outline (skip when the outline has fewer than three points) |
| `via-outside-board` | Error | A via centre is outside the outline (same skip rule) |
| `via-span` | Error | `layer_start == layer_end`, the span is not top-to-bottom in stackup order, or either layer is missing/non-copper |
| `via-drill` | Error if `drill <= 0` or `drill >= diameter`, Warning if the annular ring `(diameter - drill) / 2` is below `min_track_width / 2` | Via drill and annular ring |
| `track-path` | Error | A track has fewer than two points, or a non-finite coordinate |

Rules:

- Clearance is checked between every unordered pair on the same layer
  (`for left in 0..n { for right in left + 1..n }`) so each pair is reported
  once. Include each track id once in `objects`, ordered by id string.
- `track-outside-board` names the track once even when several points are
  outside; `location` is the first offending point in path order.
- Violations are sorted by `(rule_id, objects)` and the report is independent
  of `HashMap` iteration. Do not deduplicate distinct pairs.
- `is_valid` ignores warnings. `count` is a straight filter count.
- The report must not mutate the design, allocate per-rule rule ids at run
  time beyond `String::from`, and must never panic on degenerate input
  (empty design, single-point tracks, unknown layers).
- Copper layers are the layers in `design.layers` with `LayerKind::Copper`.
  The stackup may be empty; treat an empty stackup as "layers not ordered"
  and only require that the via's two layers exist and are copper, with
  `layer_start != layer_end`.

### 3.4 Wire DRC into the application

Extend the `kai-cad` PCB self-check to:

1. Build a 100 × 80 mm board with two copper layers (`F.Cu` id 0, `B.Cu` id 1)
   and a stackup `[0, 1]`.
2. Add two parallel tracks on `F.Cu` that violate the default clearance.
3. Run `run_drc` and assert the report contains a `clearance` error and is
   therefore invalid.
4. Widen/move the tracks to a clean configuration and assert the report is
   valid and empty.
5. Print the DRC result in the existing `pcb:` line.

Add a cross-crate smoke test in `kai-cad/tests/smoke.rs` that builds a clean
board (one track, one via inside the outline) and asserts `run_drc` is valid,
then moves the track outside the outline and asserts `track-outside-board`.

### 3.5 Tests (target: 14–18 new tests)

Cover at least:

1. Point-to-segment distance for inside, before-the-start, after-the-end and
   zero-length segments.
2. Point-to-segment distance is symmetric for reversed endpoints.
3. Segment distance for parallel, crossing, touching and disjoint segments.
4. Segment distance is symmetric and non-finite input yields infinity.
5. Track and via serde defaults when the fields are absent.
6. `track-width` for zero, below-minimum and acceptable widths.
7. `track-layer` for unknown and non-copper layers.
8. `clearance` for overlapping, close and far-apart tracks on one layer, and
   for tracks on different layers.
9. `track-outside-board` including the empty-outline skip.
10. `via-outside-board` and `via-span` (same layer, reversed span, unknown
    layer, non-copper layer).
11. `via-drill` for zero drill, drill larger than diameter and a thin ring.
12. `track-path` for a one-point track and a NaN coordinate.
13. Report ordering is deterministic across repeated runs and independent of
    the track vector order.
14. `is_valid`, `count` and an empty design.
15. kai-cad smoke test for clean and outside-board DRC.

---

## 4. Definition of done

All of the following must be true:

- [ ] `Point::distance_to_segment` and `core::geometry::segment_distance` exist,
      are documented and tested.
- [ ] `Track::width`, `Via::diameter` and `Via::drill` exist with serde defaults
      that load old documents.
- [ ] `pcb::drc::{DrcRules, DrcSeverity, DrcViolation, DrcReport, run_drc}` and
      all eight rules exist with deterministic ordering.
- [ ] `kai-cad` self-check exercises DRC and still ends `self-check: ok`.
- [ ] The new smoke test passes.
- [ ] No public API was broken; old JSON documents still load.
- [ ] Test count rises from 144 to at least 158, all passing.
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

1. Add the distance primitives and their tests, checking hand-computed cases
   (3-4-5 triangles, unit squares, parallel offsets).
2. Add the track/via fields and serde-default tests; fix any constructors.
3. Add `pcb::drc` one rule at a time, with a test per rule; finish with the
   ordering and empty-input tests.
4. Wire DRC into `kai-cad` and add the smoke test.
5. Run the full verification suite; fix warnings before moving on.
6. Update documentation and leave the changes uncommitted.

---

## 7. Documentation updates (required)

Update these files so they describe the new reality:

- `README.md` — status, test count, PCB features, roadmap.
- `docs/PLAN.md` — mark Week 6 complete and point to the next brief.
- `docs/PHASE1_TASKS.md` — update the status note (Phase 1 remains closed).
- `docs/ARCHITECTURE.md` — distance primitives, PCB dimensions, the DRC module
  and rule table, and the "Current milestone and next steps" section.
- `docs/DEVELOPMENT.md` — new test count and DRC troubleshooting notes.
- `docs/CONTRIBUTING.md` — note that DRC reports are deterministic and that
  rules must not depend on iteration order.
- `docs/NEXT_AGENT_PROMPT.md` — replace this brief with the Week 7 (routing)
  one once Week 6 is verified.
- `TODO.md` — mark Week 6 done and list Week 7.

---

## 8. Out of scope for this pass

Do not start these; they are later milestones:

- Routing algorithms, push-and-shove, interactive routing or rip-up/reroute.
- Pad copper geometry, pad-to-track clearance or footprint placement.
- Differential pairs, impedance, length matching or thermal analysis.
- KiCad `.kicad_pcb` import/export.
- Interactive editing, GUI, windowing or Vulkan object creation.
- Schema migrations: the DRC report is computed, not stored, this week.
- Async, networking or collaboration transport.

---

## 9. Reporting format

When finished, report:

1. Files added and changed (do not commit).
2. The final public API of the distance helpers, PCB dimensions and DRC.
3. Test count per crate and total.
4. Verification output tails for the commands in §5.
5. Anything you deliberately deferred, with the reason.

Follow the repository's commit conventions when describing the changes, but
leave the commit itself to the repository owner.
