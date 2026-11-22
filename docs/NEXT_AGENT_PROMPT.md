# Next Agent Prompt — Week 9: Transactional kAI Suggestion Application

You are continuing **KaiCad**, a pre-alpha EDA application written in Rust at
`/home/sneak/vscode/KaiCad`. Week 8 is finished and verified. Your job is
**Week 9: apply validated kAI suggestions through deterministic, atomic and
reversible edits**, without adding a network or model dependency.

Read this document fully before writing code.

---

## 1. Current repository state

- **Branch:** `main`
- **Tests:** 205 passing, 0 failing
- **Schema:** version 4 (`projects`, projection tables, `symbol_libraries`,
  `pcb_designs`)
- **Toolchain:** `stable`; edition 2024; `rust-version` **1.85**
- **License:** GPL-3.0-or-later

### Workspace crates

| Crate | Role | Workspace dependencies |
|-------|------|------------------------|
| `core` | Shared data model, JSON, symbol libraries, PCB primitives, change proposals | none |
| `schematic` | Netlist, graph, ERC | `core` |
| `pcb` | Board data, outline, layers, footprints, DRC, routing | `core` |
| `renderer` | Vulkan configuration types (`ash`) | none |
| `ai` | kAI suggestions and deterministic validation | `core`, `schematic` |
| `collab` | Sessions, versioning, sharing | `core` |
| `store` | SQLite project, symbol-library and PCB-design CRUD | `core`, `pcb` |
| `kicad-parser` | `.kicad_sym` import | `core` |
| `kai-cad` | Binary and cross-crate smoke tests | all of the above |

### Existing AI and editing support

- `core::proposal::ChangeProposal { id, target, field, old_value, new_value }`
  is a field-level change with JSON values. `ChangeProposal::new` currently
  creates a random v4 id.
- `ai::kAI { config }` with `kAIConfig`, `OptimizationContext`, `Constraint`,
  `ConstraintType`, `OptimizationSuggestion`, `ValidationResult`,
  `ValidationError`, `kAIError` and `SuggestionValidator`.
- `OptimizationSuggestion { id, description, changes, confidence_score }` and
  `is_applicable` (non-empty changes and confidence in `0.0..=1.0`).
- `kAI::validate_suggestion` runs `SuggestionValidator` with no constraints.
  `kAI::suggest_optimization` still returns `kAIError::NotImplemented`.
- `core::Project { id, name, version, schematic, pcb, metadata }`;
  `SchematicData { sheets, symbols, nets, components }`;
  `Component { id, reference_designator, symbol_id, footprint_id, properties }`;
  `ComponentProperty { name, value }`.

What is missing for Week 9:

- A validated suggestion cannot be applied to a project.
- There is no atomic edit operation: a partially applied multi-change
  suggestion would leave the project inconsistent.
- There is no undo/redo, no stale-value detection, and no stable edit record.

---

## 2. Non-negotiable project rules

1. **No KiCad wrapping.** KiCad formats are import/export targets only.
2. **Local-first.** No network, cloud service or model may be required.
3. **AI output is untrusted.** Never apply a suggestion that has not passed
   deterministic validation, and never weaken `SuggestionValidator`.
4. **Deterministic operations.** No unseeded randomness. Applying the same
   transaction twice to the same starting project must produce byte-identical
   JSON, including transaction ids. Do not use `Uuid::new_v4` for new
   transaction identifiers.
5. **No silent data loss.** Report or preserve anything that cannot be
   represented, and fail cleanly instead of dropping changes.
6. **Atomicity.** A failed multi-change edit leaves the project exactly as it
   was before the call.
7. **Zero warnings.** `cargo clippy --workspace --all-targets -- -D warnings`
   and `cargo fmt --all -- --check` must pass.
8. **No breaking changes.** Add modules, variants and serde-defaulted fields;
   do not rename or remove existing public APIs.
9. **Never edit an applied migration.** Add `0005_*.sql` only if truly needed;
   this milestone should not need a schema change.
10. **Update the docs** listed in §7 in the same change.
11. **Do not commit.** Leave the working tree ready for the repository owner.

---

## 3. Deliverable

### 3.1 Core edit transactions

Add `core::edit` (exported as `core::edit` and re-exported where useful):

```rust
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EditTransaction {
    /// Stable UUIDv5 id derived from the label and applied changes.
    pub id: uuid::Uuid,
    pub label: String,
    pub changes: Vec<AppliedChange>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AppliedChange {
    pub target: String,
    pub field: String,
    pub old_value: serde_json::Value,
    pub new_value: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditError {
    UnknownTarget(String),
    UnknownField { target: String, field: String },
    StaleChange { target: String, field: String },
    InvalidValue { target: String, field: String, message: String },
}

pub fn apply(
    project: &mut core::Project,
    label: &str,
    changes: &[core::ChangeProposal],
) -> Result<EditTransaction, EditError>;

pub fn undo(project: &mut core::Project, transaction: &EditTransaction) -> Result<(), EditError>;

pub fn redo(project: &mut core::Project, transaction: &EditTransaction) -> Result<(), EditError>;
```

Required semantics:

- Validate every change against a **clone** of the project and only replace the
  caller's project after all changes succeed. No partial edits.
- Supported targets:
  - `component:<reference_designator>` — exact match on a component's
    reference designator.
  - `project` — project-level fields.
- Supported fields:
  - `component:<ref>` + `value` — the `ComponentProperty` named `Value`.
    If it is absent, `old_value` must be `null`; applying creates it, and undo
    removes it again.
  - `component:<ref>` + `property:<name>` — an arbitrary existing
    `ComponentProperty`. Creating a missing property is allowed only with
    `old_value: null`, and undo removes it.
  - `component:<ref>` + `reference_designator` — non-empty string.
  - `project` + `name`, `version`, `author`, `description` — strings.
- Values use JSON strings for text fields. UUID-valued component fields are
  out of scope for this pass unless you add explicit, documented support.
- `old_value` must equal the current value exactly; otherwise return
  `StaleChange` without mutating anything.
- Apply changes in the order given; undo in reverse order. `redo` applies the
  recorded `new_value`s again and must also reject stale state.
- Derive the transaction id with UUIDv5 over the label and the ordered
  `(target, field, old_value, new_value)` tuples. Repeated edits on the same
  starting state must produce the same id.
- `EditError` implements `Display` and `std::error::Error`.

### 3.2 kAI application

Extend `ai` without changing existing signatures:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct AppliedSuggestion {
    pub suggestion_id: uuid::Uuid,
    pub transaction: core::edit::EditTransaction,
}

impl kAI {
    pub fn apply_suggestion(
        &self,
        project: &mut core::Project,
        suggestion: &OptimizationSuggestion,
    ) -> Result<AppliedSuggestion, kAIError>;
}
```

Required semantics:

- Run deterministic validation first. Invalid suggestions return
  `kAIError::ValidationFailed` and must not mutate the project.
- Reject a suggestion failing `is_applicable` (empty changes or out-of-range
  confidence) before touching the project.
- Apply all changes through `core::edit::apply`; on any `EditError`, return an
  error that preserves the details and leaves the project unchanged. Add an
  additive `kAIError::ApplyFailed(core::edit::EditError)` variant with
  `Display` and `source` support.
- The returned `AppliedSuggestion` carries the suggestion id and the exact
  transaction that can be undone or redone.
- No model call, no network access and no change to `SuggestionValidator`.
- Suggestion generation (`suggest_optimization`) remains out of scope.

### 3.3 Application wiring

Extend the `kai-cad` self-check and cross-crate smoke tests to:

1. Build a project with at least two components and `Value` properties.
2. Apply a valid suggestion (for example, change `R1`'s value), verify the
   property changed, and assert the returned transaction has stable ids.
3. Undo and assert the project's serialized JSON is byte-identical to the
   starting project; redo and assert the change returns.
4. Attempt a stale suggestion and an invalid-confidence suggestion; both must
   fail without changing the project.
5. Print the number of applied changes, the undo result and the transaction id
   length or a short prefix in the `ai:` line.

Add a cross-crate smoke test that covers apply → undo → redo, failure atomicity
and deterministic transaction ids.

### 3.4 Tests (target: 16–20 new tests)

Cover at least:

1. Apply a `component:<ref>` `value` change and update the property.
2. Create a missing property from `null` and remove it again on undo.
3. Update an arbitrary `property:<name>`.
4. Update project-level `name` and `version`.
5. Unknown target is rejected.
6. Unknown field is rejected.
7. Stale `old_value` is rejected.
8. Wrong JSON type is rejected.
9. A multi-change transaction with one failing change leaves the project
   byte-identical.
10. Undo restores the exact starting JSON.
11. Redo after undo restores the edited JSON.
12. Undo/redo reject a project that changed underneath them.
13. Transaction ids are UUIDv5, non-nil and repeatable for the same input.
14. kAI applies a valid suggestion and returns its id and transaction.
15. kAI rejects invalid confidence without mutation.
16. kAI rejects stale changes without mutation.
17. kAI rejects empty changes and no-op changes.
18. `kai-cad` smoke test for apply, undo, redo and atomic failure.

### 3.5 Definition of done

- [ ] `core::edit` provides atomic apply, undo and redo with documented targets.
- [ ] `kAI::apply_suggestion` validates first and never mutates on failure.
- [ ] Transaction and applied-suggestion ids are deterministic UUIDv5 values.
- [ ] The project JSON after apply → undo is byte-identical to the start.
- [ ] `kai-cad` self-check and smoke tests cover apply/undo/redo and failures.
- [ ] No public API was broken; existing 205 tests still pass.
- [ ] Test count rises from 205 to at least 221, all passing.
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

1. Add `core::edit` types and strict target/field resolution.
2. Implement atomic apply on a project clone, then undo/redo and UUIDv5 ids.
3. Add `kAI::apply_suggestion`, the `ApplyFailed` variant and error mapping.
4. Add unit tests for core edits and kAI application.
5. Wire the self-check and smoke test, then run the full verification suite.
6. Update documentation and leave the changes uncommitted.

---

## 6. Documentation updates (required)

- `README.md` — status, test count, AI features and roadmap.
- `docs/PLAN.md` — mark Week 9 complete and point to the Week 10 brief.
- `docs/PHASE1_TASKS.md` — update the status note.
- `docs/ARCHITECTURE.md` — core edit transactions, kAI application flow and the
  "Current milestone" section.
- `docs/DEVELOPMENT.md` — test table, self-check sample, apply/undo
  troubleshooting.
- `docs/CONTRIBUTING.md` — deterministic transaction ids and atomicity rules.
- `docs/NEXT_AGENT_PROMPT.md` — replace with the Week 10 brief.
- `TODO.md` — mark Week 9 done and list Week 10.
- `PRD.md` — update the implementation-status paragraph.

---

## 7. Out of scope for this pass

- LLM or model invocation, network transport, prompt construction and
  `suggest_optimization` generation.
- Multi-step undo history, branching history, persistence of transactions in
  SQLite or collaboration merge.
- GUI, windowing or Vulkan object creation.
- PCB-level suggestions that edit tracks, vias or footprints.
- Automatic symbol-to-footprint binding, impedance/length rules and
  push-and-shove routing.
- Schema changes.

---

## 8. Reporting format

Report changed files (do not commit), the final edit/kAI API, test counts per
crate and total, verification tails, and anything deliberately deferred.
