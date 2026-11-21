# Next Agent Prompt — Week 4: Schematic Engine

You are continuing **KaiCad**, a pre-alpha EDA application written in Rust, at
`/home/sneak/vscode/KaiCad`. Week 3 is finished and verified. Your job is
**Week 4 of Phase 1: the schematic engine** — deterministic KiCad netlist
export, graph operations, and broader ERC — without breaking anything that
already works.

This document is self-contained. Read it fully before writing code.

---

## 1. Current repository state

- **Branch:** `main`
- **Repository tip:** see `git log -1`
- **Working tree:** clean at the time this brief was written
- **Tests:** 107 passing, 0 failing
- **Schema:** version 3 (`projects`, projection tables, `symbol_libraries`)
- **Toolchain:** `stable` channel; edition 2024; MSRV/`rust-version` **1.85**
- **License:** GPL-3.0-or-later

### Workspace crates

| Crate | Role | Workspace dependencies |
|-------|------|------------------------|
| `core` | Shared data model, JSON serialization, symbol libraries | none |
| `schematic` | Netlist, connectivity graph, ERC | `core` |
| `pcb` | Board data, layers, footprints, routing config | `core` |
| `renderer` | Vulkan configuration types (`ash`) | none |
| `ai` | kAI suggestions + deterministic validation | `core`, `schematic` |
| `collab` | Sessions, versioning, sharing | `core` |
| `store` | SQLite database: project and symbol-library CRUD | `core` |
| `kicad-parser` | `.kicad_sym` import, deterministic S-expressions | `core` |
| `kai-cad` | Application binary + cross-crate smoke tests | all of the above |

### Existing schematic support

- `core::symbol::{Symbol, Pin, PinType, PinDirection, PinPosition}`.
- `schematic::netlist::{Netlist, Connection, PinRef}` and
  `generate_netlist(&symbols, &nets)`. Generation is deterministic: the
  netlist, connection and graph ids are UUIDv5 derivations.
- `schematic::graph::Graph` with `from_netlist`, `node_count` and `edge_count`.
  Nodes represent symbols and nets; edges come from connections.
- `schematic::run_erc(&netlist)` with three rules: `net-single-pin` (warning),
  `duplicate-net-name` (error) and `self-connection` (error).

What is missing for Week 4:

- No netlist serialization/export; the netlist only exists in memory.
- The graph exposes counts but no traversal or mutation helpers.
- ERC does not check pins that appear in no net, or duplicate reference names.

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
7. **`core` stays dependency-light.** Do not add `rusqlite` to `core`.
8. **No breaking changes** to existing public APIs. Additions are fine.
9. **Update the docs** listed in §7 in the same change.

---

## 3. Deliverable

### 3.1 Deterministic KiCad netlist export

Add to `schematic::netlist` (re-export from `schematic::lib.rs`):

```rust
/// Render the netlist as a KiCad-compatible S-expression netlist document.
pub fn to_kicad_netlist(netlist: &Netlist) -> String;
```

Output shape:

```text
(export (version "E")
  (components
    (comp (ref "R1") (value "") (footprint ""))
    ...)
  (nets
    (net (code "1") (name "N1")
      (node (ref "R1") (pin "1"))
      ...)
    ...))
```

Rules:

- **Components** come from `netlist.symbols`, one `comp` per symbol. Use
  `Symbol::name` as the reference, and empty `value`/`footprint` strings for
  now (the netlist does not carry component properties).
- **Nets** come from `netlist.nets`. Emit net codes starting at 1 in the order
  nets are written. Each net lists the pins in `Net::pins` that resolve to a
  known symbol pin, as `(node (ref "<symbol name>") (pin "<pin number>"))`.
- **Ordering:** sort components by `(name, id)` and nets by `(name, id)` so the
  output does not depend on insertion order. Sort nodes by
  `(ref, pin, pin id)`. Skip nets with no resolvable nodes? No — emit the net
  with its name and no nodes so the caller sees it.
- Escape `"` and `\` in strings (names, references, numbers). Do not emit
  newlines inside quoted strings.
- Output is pretty-printed with two-space indentation, uses `\n` line endings,
  and ends with a single trailing newline.
- Do not add a KiCad netlist parser in this pass; export only.

### 3.2 Graph operations

Extend `schematic::graph::Graph` with read-only traversal helpers (keep the
existing struct fields and constructors):

```rust
impl Graph {
    /// Edges attached to `node_id`.
    pub fn edges_for(&self, node_id: Uuid) -> Vec<&Edge>;
    /// Node ids adjacent to `node_id` through at least one edge.
    pub fn neighbours(&self, node_id: Uuid) -> Vec<Uuid>;
    /// Whether two node ids are in the same connected component (BFS).
    pub fn is_connected(&self, a: Uuid, b: Uuid) -> bool;
    /// Connected components as sorted node-id lists, themselves sorted.
    pub fn connected_components(&self) -> Vec<Vec<Uuid>>;
}
```

Rules:

- Traversal must be deterministic: sort neighbour lists and components by UUID.
- `is_connected(a, a)` is true when `a` is a node in the graph, false when the
  id is unknown.
- Unknown ids produce empty results, never a panic.
- Do not change node/edge ids or `from_netlist` semantics.

### 3.3 Broader ERC

Add two rules to `schematic::run_erc`:

| Rule id | Severity | Trigger |
|---------|----------|---------|
| `unconnected-pin` | Warning | A symbol pin id appears in no net's `pins` |
| `duplicate-symbol-name` | Error | Two symbols share the same non-empty `name` |

Rules:

- Report one `unconnected-pin` violation per orphaned pin, with the pin number
  in the message and a stable rule id.
- Report one `duplicate-symbol-name` violation per duplicated name (or per
  symbol after the first — document which; prefer one per name).
- Existing rule ids, severities and `ERCResult` semantics must not change.
- Keep it deterministic: iterate symbols/pins in their existing order or sort.

### 3.4 Wire the export into the application

Extend the `kai-cad` self-check schematic section (or add a line) to render the
netlist with `to_kicad_netlist`, assert it starts with `(export`, and print its
byte length. The self-check must stay hermetic and still end `self-check: ok`.

Add a cross-crate smoke test in `kai-cad/tests/smoke.rs` that generates a
netlist, exports it, and asserts the document contains the component reference
and net name.

### 3.5 Tests (target: 10–14 new tests)

Cover at least:

1. Export of a two-symbol/two-net fixture contains components and nets.
2. Export is deterministic across repeated calls (byte equality).
3. Export sorts components and nets independent of input order.
4. Export escapes quotes/backslashes in names.
5. Export ends with exactly one newline.
6. `edges_for` returns edges touching a node.
7. `neighbours` returns distinct adjacent ids.
8. `is_connected` handles connected, disconnected and unknown ids.
9. `connected_components` is sorted and groups the expected nodes.
10. ERC reports `unconnected-pin` for an orphaned pin.
11. ERC accepts a fully connected schematic (no new warnings).
12. ERC reports `duplicate-symbol-name` and leaves the first symbol alone.

---

## 4. Definition of done

All of the following must be true:

- [ ] `to_kicad_netlist` is public, documented, deterministic and tested.
- [ ] `Graph::{edges_for, neighbours, is_connected, connected_components}` exist
      and are deterministic.
- [ ] ERC has `unconnected-pin` and `duplicate-symbol-name` rules with tests.
- [ ] `kai-cad` self-check renders a netlist and still ends `self-check: ok`.
- [ ] The new smoke test passes.
- [ ] No public API of any crate was broken.
- [ ] Test count rises from 107 to at least 117, all passing.
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
cargo run -p kai-cad
cargo build --workspace --release
```

---

## 6. Suggested implementation order

1. Add `to_kicad_netlist` plus a fixture-based unit test; confirm byte
   determinism.
2. Add graph traversal helpers and their tests.
3. Add the two ERC rules and tests, keeping existing rules untouched.
4. Wire export into `kai-cad` and add the smoke test.
5. Run the full verification suite; fix warnings before moving on.
6. Update documentation and commit with a Conventional Commit message such as
   `feat(schematic): add deterministic KiCad netlist export`.

---

## 7. Documentation updates (required)

Update these files so they describe the new reality:

- `README.md` — status, test count, crate features, examples, roadmap.
- `docs/PLAN.md` — mark Week 4 complete in the Phase 1 progress table; point to
  the next brief.
- `docs/PHASE1_TASKS.md` — update Tasks 1.7, 1.8, 1.9 and acceptance criteria.
- `docs/ARCHITECTURE.md` — document netlist export, graph traversal helpers, the
  new ERC rules and the "Current milestone and next steps" section.
- `docs/DEVELOPMENT.md` — commands (if any change), the new test count, and
  netlist-export troubleshooting notes.
- `docs/CONTRIBUTING.md` — note that KiCad netlist output is export-only and
  must not become the internal representation.
- `docs/NEXT_AGENT_PROMPT.md` — replace this brief with the Week 5 (PCB core)
  one once Week 4 is verified.

---

## 8. Out of scope for this pass

Do not start these; they are later milestones:

- KiCad netlist **import**, `.kicad_sch` or `.kicad_pcb` parsing/writing.
- Schematic editing UI, symbol placement, wire/junction geometry.
- Footprint library loading, routing, DRC or PCB geometry.
- Any GUI, windowing or Vulkan object creation.
- Async, networking or collaboration transport.
- Undo/redo transactions above the database layer.

---

## 9. Reporting format

When finished, report:

1. Files added and changed.
2. The final public API of netlist export and graph helpers, and the ERC rule
   list with severities.
3. Test count per crate and total.
4. Verification output tails for the commands in §5.
5. Anything you deliberately deferred, with the reason.

Follow the repository's commit conventions. Do not rewrite unrelated code, and
do not reformat files you did not change.
