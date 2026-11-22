# Next Agent Prompt — Week 11: Multi-Step Edit History and Redo Branching

You are continuing **KaiCad**, a pre-alpha EDA application written in Rust at
`/home/sneak/vscode/KaiCad`. Week 10 is finished and verified. Your job is
**Week 11: add a bounded, deterministic multi-step undo/redo history over
`core::edit` transactions**, with correct redo-branch clearing and capacity
eviction.

Read this document fully before writing code.

---

## 1. Current repository state

- **Branch:** `main`
- **Tests:** 244 passing, 0 failing
- **Schema:** version 4 (`projects`, projection tables, `symbol_libraries`,
  `pcb_designs`)
- **Toolchain:** `stable`; edition 2024; `rust-version` **1.85**
- **License:** GPL-3.0-or-later

### Workspace crates

| Crate | Role | Workspace dependencies |
|-------|------|------------------------|
| `core` | Shared data model, JSON, symbol libraries, PCB primitives, edit transactions | none |
| `schematic` | Netlist, graph, ERC | `core` |
| `pcb` | Board data, outline, layers, footprints, DRC, routing | `core` |
| `renderer` | Vulkan configuration types (`ash`) | none |
| `ai` | kAI validation and transaction application | `core`, `schematic` |
| `collab` | Sessions, permissions, versioning, sync staging | `core` |
| `store` | SQLite project, symbol-library and PCB-design CRUD | `core`, `pcb` |
| `kicad-parser` | `.kicad_sym` import | `core` |
| `kai-cad` | Binary and cross-crate smoke tests | all of the above |

### Existing edit support

- `core::edit::{apply, undo, redo}` operate on one `EditTransaction` at a time.
  `apply` stages on a project clone and is atomic; `undo` and `redo` verify
  current values and are atomic on failure.
- `EditTransaction { id, label, changes }`; ids are UUIDv5 values over the
  label and ordered changes.
- `AppliedChange` records `target`, `field`, `old_value`, `new_value` and an
  optional component `object_id`.
- `core::edit` targets `project` and `component:<reference_designator>`; fields
  are `value`, `property:<name>`, `reference_designator`, `name`, `version`,
  `author` and `description`.
- `kAI::apply_suggestion` and `CollabManager::apply_edit` both return
  `EditTransaction` values that callers can undo or redo.
- `collab` records versions from transactions but does not keep an undo stack.

What is missing for Week 11:

- A caller can undo only the most recently returned transaction and must track
  the stack itself.
- There is no redo branch policy: a new edit after an undo must discard the
  redo path.
- There is no capacity bound on retained history.

---

## 2. Non-negotiable project rules

1. **No KiCad wrapping.** KiCad formats are import/export targets only.
2. **Local-first.** No network, cloud service or model may be required.
3. **Deterministic operations.** History order and behaviour must depend only
   on the sequence of user edits. No unseeded randomness and no wall-clock
   decisions in stack semantics.
4. **No silent data loss.** Do not discard a redo transaction without the
   documented branch rule, and do not evict undo entries except at the
   configured limit.
5. **Atomic edits.** A failed apply, undo or redo must leave both the project
   and both history stacks exactly as they were.
6. **Zero warnings.** `cargo clippy --workspace --all-targets -- -D warnings`
   and `cargo fmt --all -- --check` must pass.
7. **No breaking changes.** Add types and methods; keep `core::edit`,
   `kAI::apply_suggestion` and `CollabManager::apply_edit` working unchanged.
8. **Never edit an applied migration.** No schema change is expected.
9. **Update the docs** listed in §7 in the same change.
10. **Do not commit.** Leave the working tree ready for the repository owner.

---

## 3. Deliverable

### 3.1 Core history

Add `core::history` (exported as `core::history` and re-exported where useful):

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum HistoryError {
    Edit(core::edit::EditError),
    EmptyUndo,
    EmptyRedo,
    InvalidLimit,
}

pub struct EditHistory {
    undo_stack: Vec<core::edit::EditTransaction>,
    redo_stack: Vec<core::edit::EditTransaction>,
    limit: usize,
}

impl Default for EditHistory { /* limit 100 */ }

impl EditHistory {
    pub fn new(limit: usize) -> Result<Self, HistoryError>;
    pub fn apply(
        &mut self,
        project: &mut core::Project,
        label: &str,
        changes: &[core::ChangeProposal],
    ) -> Result<core::edit::EditTransaction, HistoryError>;
    pub fn undo(&mut self, project: &mut core::Project) -> Result<(), HistoryError>;
    pub fn redo(&mut self, project: &mut core::Project) -> Result<(), HistoryError>;
    pub fn can_undo(&self) -> bool;
    pub fn can_redo(&self) -> bool;
    pub fn undo_label(&self) -> Option<&str>;
    pub fn redo_label(&self) -> Option<&str>;
    pub fn undo_len(&self) -> usize;
    pub fn redo_len(&self) -> usize;
    pub fn clear(&mut self);
}
```

Required semantics:

- `new(limit)` rejects `limit == 0` with `InvalidLimit`; `Default` uses 100.
- `apply` calls `core::edit::apply`; only on success does it push the returned
  transaction onto the undo stack, clear the redo stack, and enforce the limit.
  On failure, both stacks are unchanged.
- Capacity applies to the undo stack: when it exceeds `limit`, remove the
  oldest entry. `undo_len` never exceeds the limit.
- `undo` pops only after `core::edit::undo` succeeds, then pushes the same
  transaction onto the redo stack. `EmptyUndo` is returned when there is
  nothing to undo. A `core::edit` failure leaves both stacks unchanged.
- `redo` pops only after `core::edit::redo` succeeds, then pushes the
  transaction back onto the undo stack. `EmptyRedo` is returned when there is
  nothing to redo; a failure leaves both stacks unchanged.
- `undo_label`/`redo_label` return the label of the next transaction without
  modifying the stacks.
- `clear` empties both stacks.
- `HistoryError` implements `Display` and `std::error::Error`; `Edit` exposes
  its source.
- Do not derive serde for `EditHistory` in this pass; persistence of history is
  out of scope.

### 3.2 Application wiring

Extend the `kai-cad` self-check and cross-crate smoke tests to:

1. Build a project with one component and make three value edits through
   `EditHistory`.
2. Undo twice and assert the value is back to the first edited state.
3. Redo once and assert the value advances by one edit.
4. Make a new edit and assert the redo stack is cleared.
5. Configure a small limit (for example 2), make enough edits to exceed it, and
   assert the oldest transaction is evicted and only the newest are undoable.
6. Assert `undo_label`, `redo_label`, `can_undo` and `can_redo` match the stack.
7. Print undo/redo counts in the `core:` or a dedicated `history:` line.

Add a smoke test covering apply/undo/redo, branch clearing, capacity eviction
and failure atomicity.

### 3.3 Tests (target: 16–20 new tests)

Cover at least:

1. `new(0)` is rejected and default limit is 100.
2. A single apply advances undo and leaves redo empty.
3. Undo restores the previous project JSON and moves the transaction to redo.
4. Redo restores the edited JSON and moves it back to undo.
5. Undo on an empty history returns `EmptyUndo`.
6. Redo on an empty history returns `EmptyRedo`.
7. A new apply after undo clears the redo stack.
8. Multiple undos run in reverse order.
9. Multiple redos run in forward order.
10. `undo_label` and `redo_label` identify the next transaction.
11. `can_undo` and `can_redo` match the stack state.
12. `clear` empties both stacks.
13. The limit evicts the oldest undo entry.
14. A failed apply leaves both stacks unchanged.
15. A failed undo leaves both stacks unchanged.
16. A failed redo leaves both stacks unchanged.
17. `undo_len` never exceeds the configured limit.
18. `HistoryError` exposes the underlying edit error.
19. `kai-cad` smoke test for multi-step history and branching.

### 3.4 Definition of done

- [ ] `core::history::EditHistory` provides bounded apply/undo/redo.
- [ ] New edits clear redo; capacity evicts the oldest undo entry.
- [ ] Failed operations leave the project and both stacks unchanged.
- [ ] Labels and predicates reflect the next available transactions.
- [ ] `kai-cad` self-check and smoke tests cover branching and eviction.
- [ ] No public API was broken; existing 244 tests still pass.
- [ ] Test count rises from 244 to at least 260, all passing.
- [ ] Clippy, fmt, rustdoc and Markdown links are clean.
- [ ] Documentation updated (§7).

---

## 4. Verification commands

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

## 5. Suggested implementation order

1. Add `HistoryError` and the `EditHistory` stack fields/accessors.
2. Implement apply with redo clearing and capacity eviction.
3. Implement undo/redo with stack-preserving failure paths.
4. Add unit tests for branching, limits and atomic failures.
5. Wire the self-check and smoke test, then run the full verification suite.
6. Update documentation and leave the changes uncommitted.

---

## 6. Documentation updates (required)

- `README.md` — status, test count, history features and roadmap.
- `docs/PLAN.md` — mark Week 11 complete and point to the Week 12 brief.
- `docs/PHASE1_TASKS.md` — update the status note.
- `docs/ARCHITECTURE.md` — history module, stack semantics and the
  "Current milestone" section.
- `docs/DEVELOPMENT.md` — test table, self-check sample and history
  troubleshooting.
- `docs/CONTRIBUTING.md` — deterministic stack order, branch clearing and
  capacity rules.
- `docs/NEXT_AGENT_PROMPT.md` — replace with the Week 12 brief.
- `TODO.md` — mark Week 11 done and list Week 12.
- `PRD.md` — update the implementation-status paragraph.

---

## 7. Out of scope for this pass

- Persisting history in SQLite, project files or collaboration sessions.
- Multi-user undo, merge/conflict resolution, remote history or transport.
- GUI history panels, keyboard shortcuts or interactive editing.
- Grouping several transactions into one undo step.
- History for PCB objects, kAI generation or collaboration-specific changes.
- Plugin architecture, renderer integration and schema changes.

---

## 8. Reporting format

Report changed files (do not commit), the final history API, test counts per
crate and total, verification tails, and anything deliberately deferred.
