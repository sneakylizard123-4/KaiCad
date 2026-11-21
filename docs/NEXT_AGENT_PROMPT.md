# Next Agent Prompt — Week 5: PCB Core Foundation

You are continuing **KaiCad**, a pre-alpha EDA application written in Rust, at
`/home/sneak/vscode/KaiCad`. Week 4 is finished and verified. Your job is
**Week 5 of Phase 1: the PCB core foundation** — deterministic board-outline
geometry, layer-stack validation and footprint library loading — without
breaking anything that already works.

This document is self-contained. Read it fully before writing code.

---

## 1. Current repository state

- **Branch:** `main`
- **Repository tip:** see `git log -1`
- **Working tree:** clean at the time this brief was written
- **Tests:** 126 passing, 0 failing
- **Schema:** version 3 (`projects`, projection tables, `symbol_libraries`)
- **Toolchain:** `stable` channel; edition 2024; MSRV/`rust-version` **1.85**
- **License:** GPL-3.0-or-later

### Workspace crates

| Crate | Role | Workspace dependencies |
|-------|------|------------------------|
| `core` | Shared data model, JSON serialization, symbol libraries, PCB primitives | none |
| `schematic` | Netlist generation/export, connectivity graph, ERC | `core` |
| `pcb` | Board data, layers, footprints, routing config | `core` |
| `renderer` | Vulkan configuration types (`ash`) | none |
| `ai` | kAI suggestions + deterministic validation | `core`, `schematic` |
| `collab` | Sessions, versioning, sharing | `core` |
| `store` | SQLite database: project and symbol-library CRUD | `core` |
| `kicad-parser` | `.kicad_sym` import, deterministic S-expressions | `core` |
| `kai-cad` | Application binary + cross-crate smoke tests | all of the above |

### Existing PCB support

- `core::pcb::{PcbOutline, PcbLayer, LayerKind, Stackup}` and `core::Point`.
  `PcbOutline::{new, rectangle, is_closed}` exist; `is_closed` checks for a
  non-zero shoelace area.
- `pcb::PcbDesign { outline, layers, stackup, footprints, tracks, vias }` with
  `copper_layer_count`.
- `pcb::layer::LayerStack` with `new`, `push`, `len`, `is_empty`,
  `copper_layers` and `find`.
- `pcb::footprint::{Footprint, PadPosition, PadType, Outline, OutlineShape}`
  as data types only; there is no library container and no loader.
- `pcb::routing::{Router, RoutingAlgorithm, RoutingOptimization}` as
  configuration types only.

What is missing for Week 5:

- No outline measurements (`area`, point containment, bounding box).
- No layer-stack validation: duplicate ids/names, empty names, stackup ids
  that do not exist, and non-copper entries in a copper stackup all pass.
- No footprint library: footprints are bare structs with no named collection,
  no persistence and no deterministic loading.

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
7. **`core` stays dependency-light.** Do not add `rusqlite` or `serde_json`
   to `core`; it already has `serde`, `serde_json`, `uuid` and `chrono`.
8. **No breaking changes** to existing public APIs. Additions are fine.
9. **Update the docs** listed in §7 in the same change.

---

## 3. Deliverable

### 3.1 Board-outline geometry

Add to `core::pcb::PcbOutline` (re-export anything new from `core::lib.rs`):

```rust
/// Axis-aligned bounding box in millimetres.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BoundingBox {
    pub min: Point,
    pub max: Point,
}

impl PcbOutline {
    /// Signed shoelace area in mm², positive for counter-clockwise corners.
    pub fn signed_area(&self) -> f64;
    /// Absolute enclosed area in mm²; zero for degenerate outlines.
    pub fn area(&self) -> f64;
    /// Whether `point` lies inside or on the outline (even-odd ray casting).
    pub fn contains_point(&self, point: &Point) -> bool;
    /// Bounding box of the corner points, or `None` for an empty outline.
    pub fn bounding_box(&self) -> Option<BoundingBox>;
    /// Total length of the implicit closed perimeter in millimetres.
    pub fn perimeter(&self) -> f64;
}
```

Rules:

- Areas are in square millimetres and lengths in millimetres; no unit
  conversion.
- `contains_point` treats the boundary as inside, works for concave polygons
  and for points exactly on horizontal edges, and returns `false` for fewer
  than three points. Do not panic for NaN input; return `false`.
- `area` is the absolute value of `signed_area`, so clockwise and
  counter-clockwise outlines report the same area.
- `bounding_box` returns the minimum and maximum corner coordinates; a single
  point is a zero-size box, not `None`.
- `perimeter` includes the closing edge from the last point to the first.
- Do not allocate or sort inside these methods; they must be `O(n)` and
  deterministic.

### 3.2 Layer-stack validation

Extend `pcb::layer::LayerStack` (keep every existing method):

```rust
/// A problem found in a layer stack.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LayerIssue {
    pub layer_id: u16,
    pub kind: LayerIssueKind,
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LayerIssueKind {
    DuplicateId,
    DuplicateName,
    EmptyName,
    MissingFromStackup,
    NonCopperInStackup,
}

impl LayerStack {
    /// Validate the stack and its optional `stackup` ordering.
    /// Returns issues sorted by `(layer_id, kind)`.
    pub fn validate(&self, stackup: &core::Stackup) -> Vec<LayerIssue>;
    /// Copper layers in stackup order, or in stack order when the stackup is empty.
    pub fn copper_layers_in_stackup(&self, stackup: &core::Stackup) -> Vec<&core::PcbLayer>;
}
```

Rules:

- One issue per offending layer and kind. Report each duplicated id against
  every layer that carries it, matching the existing ERC convention of naming
  the offender rather than hiding it.
- `DuplicateName` ignores empty names; `EmptyName` covers those instead.
- When `stackup.layers` is non-empty, a stackup id with no layer is a
  `MissingFromStackup` issue with that id; a stackup id that resolves to a
  non-copper layer is a `NonCopperInStackup` issue against that layer.
- Need a stable `Ord` on `(layer_id, LayerIssueKind)`: derive `PartialOrd`/`Ord`
  on the enum (declaration order is the tie-break).
- Deterministic: sort the final vector; never iterate a `HashMap`.

### 3.3 Footprint library

Add `pcb::footprint::FootprintLibrary`:

```rust
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct FootprintLibrary {
    pub name: String,
    pub footprints: Vec<Footprint>,
}

impl FootprintLibrary {
    /// Footprints sorted by `(name, reference_designator)`.
    pub fn sorted(&self) -> Vec<&Footprint>;
    /// Look up a footprint by exact name.
    pub fn find(&self, name: &str) -> Option<&Footprint>;
    /// Serialize as pretty JSON with a single trailing newline.
    pub fn to_json(&self) -> Result<String, FootprintLibraryError>;
    /// Parse a library document; rejects duplicate footprint names.
    pub fn from_json(json: &str) -> Result<Self, FootprintLibraryError>;
    /// Write the library to `path`, creating parent directories.
    pub fn save(&self, path: impl AsRef<std::path::Path>) -> Result<(), FootprintLibraryError>;
    /// Load a library from `path`.
    pub fn load(path: impl AsRef<std::path::Path>) -> Result<Self, FootprintLibraryError>;
}
```

Rules:

- Footprint library documents are **KaiCad's own JSON**, not KiCad `.kicad_mod`
  files. A KiCad footprint parser is explicitly out of scope.
- `to_json` uses two-space indentation, `\n` line endings and exactly one
  trailing newline so documents are byte-stable across runs.
- `from_json` must return an error, not silently drop a duplicate name, and
  must not panic on malformed input.
- `FootprintLibraryError` implements `Display` and `std::error::Error` and
  distinguishes JSON, I/O and `Duplicate` failures.
- `save` writes deterministically (serialize the same struct), and `load`
  round trips a saved library unchanged.
- `pcb` may gain `serde_json` as a dependency; do not add it to `core`.

### 3.4 Wire the PCB work into the application

Extend the `kai-cad` self-check PCB section to:

1. Compute the rectangular outline's area and assert it equals `width * height`.
2. Assert the outline contains its centre and rejects a point outside it.
3. Validate a two-layer stack and assert the report is empty.
4. Build a small `FootprintLibrary`, save it to a unique temp path, load it,
   assert the footprint count survives, and delete the directory.

Keep the self-check hermetic, deterministic and ending `self-check: ok`.

Add a cross-crate smoke test in `kai-cad/tests/smoke.rs` that builds a
footprint library, round trips it through JSON and asserts the footprint and
pad data survive.

### 3.5 Tests (target: 14–18 new tests)

Cover at least:

1. Rectangle area and perimeter (9×5 → 45 mm², 28 mm).
2. Clockwise and counter-clockwise outlines report the same absolute area, and
   signed areas have opposite signs.
3. `contains_point` handles inside, outside, boundary and concave polygons.
4. `contains_point` returns `false` for degenerate outlines and NaN input.
5. `bounding_box` of a rectangle, of a single point, and `None` when empty.
6. `perimeter` includes the closing edge.
7. `validate` accepts a consistent stack.
8. `validate` reports duplicate ids, duplicate names and empty names, sorted.
9. `validate` reports stackup ids that are missing or non-copper.
10. `copper_layers_in_stackup` follows stackup order and falls back to stack order.
11. `FootprintLibrary::find` and `sorted` behave deterministically.
12. JSON round trip preserves footprints, pads and outlines.
13. `from_json` rejects duplicate names and malformed input without panicking.
14. `save`/`load` round trip through a temp directory.
15. Empty library serializes and round trips.
16. kai-cad smoke test covers the footprint-library round trip.

---

## 4. Definition of done

All of the following must be true:

- [ ] `PcbOutline::{signed_area, area, contains_point, bounding_box, perimeter}`
      exist, are documented and tested.
- [ ] `LayerStack::{validate, copper_layers_in_stackup}` and the issue types
      exist and are deterministic.
- [ ] `FootprintLibrary` with JSON persistence exists and rejects duplicates.
- [ ] `kai-cad` self-check exercises outline geometry, layer validation and a
      footprint-library round trip, and still ends `self-check: ok`.
- [ ] The new smoke test passes.
- [ ] No public API of any crate was broken.
- [ ] Test count rises from 126 to at least 140, all passing.
- [ ] Clippy, fmt and rustdoc are warning-free.
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
cargo run -p kicad-parser --example parse_library
cargo run -p store --example project_crud
cargo run -p pcb --example board_outline
cargo run -p kai-cad
cargo build --workspace --release
```

---

## 6. Suggested implementation order

1. Add the outline geometry methods and their tests; property-check areas and
   containment against hand-computed fixtures.
2. Add `LayerIssue`, validation and `copper_layers_in_stackup` with tests.
3. Add `FootprintLibrary` and its JSON/file round trips with tests.
4. Wire everything into `kai-cad` and add the smoke test.
5. Run the full verification suite; fix warnings before moving on.
6. Update documentation and commit with a Conventional Commit message such as
   `feat(pcb): add board outline geometry and footprint libraries`.

---

## 7. Documentation updates (required)

Update these files so they describe the new reality:

- `README.md` — status, test count, PCB crate features, examples, roadmap.
- `docs/PLAN.md` — mark Week 5 complete in the progress table; point to the
  next brief.
- `docs/PHASE1_TASKS.md` — record Phase 1 as complete and note where Week 5
  sits relative to the Phase 2 breakdown.
- `docs/ARCHITECTURE.md` — document outline geometry, layer validation, the
  footprint library and the "Current milestone and next steps" section.
- `docs/DEVELOPMENT.md` — new test count and footprint-library troubleshooting
  notes.
- `docs/CONTRIBUTING.md` — note that footprint libraries are KaiCad JSON and
  that KiCad `.kicad_mod` support is import work, not the internal format.
- `docs/NEXT_AGENT_PROMPT.md` — replace this brief with the Week 6 (PCB layout
  and routing) one once Week 5 is verified.
- `TODO.md` — mark Week 5 done and list Week 6.

---

## 8. Out of scope for this pass

Do not start these; they are later milestones:

- KiCad `.kicad_mod` footprint parsing or `.kicad_pcb` writing.
- Footprint placement on a board, pad-to-net assignment or ratsnest.
- Routing, push-and-shove, DRC, geometry collision or clearance checks.
- Interactive editing, GUI, windowing or Vulkan object creation.
- Any change to netlist export or ERC semantics beyond keeping them working.
- Async, networking or collaboration transport.

---

## 9. Reporting format

When finished, report:

1. Files added and changed.
2. The final public API of the outline geometry, layer validation and footprint
   library.
3. Test count per crate and total.
4. Verification output tails for the commands in §5.
5. Anything you deliberately deferred, with the reason.

Follow the repository's commit conventions. Do not rewrite unrelated code, and
do not reformat files you did not change.
