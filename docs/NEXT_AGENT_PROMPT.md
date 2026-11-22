# Next Agent Prompt — Week 10: Collaborative Edit Sessions and Version History

You are continuing **KaiCad**, a pre-alpha EDA application written in Rust at
`/home/sneak/vscode/KaiCad`. Week 9 is finished and verified. Your job is
**Week 10: make edit transactions collaborative — add session membership and
permissions, stage transactions deterministically, and record version history
tied to those transactions**, without adding a network transport.

Read this document fully before writing code.

---

## 1. Current repository state

- **Branch:** `main`
- **Tests:** 225 passing, 0 failing
- **Schema:** version 4 (`projects`, projection tables, `symbol_libraries`,
  `pcb_designs`)
- **Toolchain:** `stable`; edition 2024; `rust-version` **1.85**
- **License:** GPL-3.0-or-later

### Workspace crates

| Crate | Role | Workspace dependencies |
|-------|------|------------------------|
| `core` | Shared data model, JSON, symbol libraries, PCB primitives, atomic edit transactions | none |
| `schematic` | Netlist, graph, ERC | `core` |
| `pcb` | Board data, outline, layers, footprints, DRC, routing | `core` |
| `renderer` | Vulkan configuration types (`ash`) | none |
| `ai` | kAI suggestions, deterministic validation, transaction application | `core`, `schematic` |
| `collab` | Sessions, versioning, sharing, sync staging | `core` |
| `store` | SQLite project, symbol-library and PCB-design CRUD | `core`, `pcb` |
| `kicad-parser` | `.kicad_sym` import | `core` |
| `kai-cad` | Binary and cross-crate smoke tests | all of the above |

### Existing collaboration and edit support

- `core::edit`:
  - `EditTransaction { id, label, changes }`,
    `AppliedChange { target, field, old_value, new_value, object_id }`,
    `EditError::{UnknownTarget, UnknownField, StaleChange, InvalidValue}`.
  - `apply(project, label, changes) -> Result<EditTransaction, EditError>`
    stages on a clone and commits only when every change succeeds.
  - `undo` walks changes in reverse, `redo` forward; both verify current values
    and are atomic on failure.
  - Targets are `project` and `component:<reference_designator>`; fields are
    `value`, `property:<name>`, `reference_designator`, and the project text
    fields. Transaction ids are UUIDv5 values.
- `ai::kAI::apply_suggestion(project, suggestion) -> AppliedSuggestion`
  validates first, applies through `core::edit::apply`, and returns the
  undoable transaction. `kAIError::ApplyFailed` wraps `EditError`.
- `collab`:
  - `CollabManager { sessions, versions }` with `new`, `create_session`,
    `session_count`; the creator is an `Owner` participant.
  - `CollabSession { session_id, project_id, participants, version }`;
    `Participant { user_id, username, role }`;
    `ParticipantRole::{Owner, Editor, Viewer}`.
  - `VersioningManager { sessions, versions }` with `record(author, summary)`
    and `latest`; `VersionEntry { version, timestamp, author, change_summary }`.
  - `SharingManager::share/unshare` over project name strings.
  - `SyncManager { sessions, pending_changes }`, where `pending_changes` is a
    `Vec<core::ChangeProposal>`.
  - `Window::new(title)`.

What is missing for Week 10:

- Sessions cannot add participants, change roles or check edit permissions.
- There is no way to apply an edit transaction through a session; viewers and
  non-members are not rejected.
- Version entries are not linked to edit transactions or project snapshots.
- `SyncManager` cannot queue whole transactions, deduplicate them or drain them
  in a deterministic order.

---

## 2. Non-negotiable project rules

1. **No KiCad wrapping.** KiCad formats are import/export targets only.
2. **Local-first.** No network, cloud service or model may be required.
3. **AI output is untrusted.** Do not weaken validation or bypass
   `kAI::apply_suggestion`.
4. **Deterministic operations.** No unseeded randomness in new production
   paths. Project digests and new transaction-linked identifiers must use
   UUIDv5 over canonical serialized data, never `Uuid::new_v4`.
5. **No silent data loss.** Preserve or report information that cannot be
   represented.
6. **Atomic edits.** A rejected session edit leaves both the project and the
   session version unchanged.
7. **Zero warnings.** `cargo clippy --workspace --all-targets -- -D warnings`
   and `cargo fmt --all -- --check` must pass.
8. **No breaking changes.** Add modules, variants and serde-defaulted fields;
   do not rename or remove existing public APIs. Existing `collab` JSON must
   continue to load.
9. **Never edit an applied migration.** Add `0005_*.sql` only if truly needed;
   this milestone should not need a schema change.
10. **Update the docs** listed in §7 in the same change.
11. **Do not commit.** Leave the working tree ready for the repository owner.

---

## 3. Deliverable

### 3.1 Session membership and permissions

Add to `collab` (module layout is your choice):

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollabError {
    UnknownSession(uuid::Uuid),
    UnknownParticipant(uuid::Uuid),
    DuplicateParticipant(uuid::Uuid),
    ReadOnly,
    Edit(core::edit::EditError),
    History(String),
}

impl CollabManager {
    pub fn add_participant(
        &mut self,
        session_id: uuid::Uuid,
        participant: Participant,
    ) -> Result<(), CollabError>;

    pub fn set_role(
        &mut self,
        session_id: uuid::Uuid,
        user_id: uuid::Uuid,
        role: ParticipantRole,
    ) -> Result<(), CollabError>;

    pub fn can_edit(&self, session_id: uuid::Uuid, user_id: uuid::Uuid) -> bool;

    pub fn apply_edit(
        &mut self,
        session_id: uuid::Uuid,
        user_id: uuid::Uuid,
        project: &mut core::Project,
        label: &str,
        changes: &[core::ChangeProposal],
    ) -> Result<core::edit::EditTransaction, CollabError>;
}
```

Required semantics:

- `add_participant` rejects duplicate `user_id`s within a session and unknown
  session ids; usernames are stored as supplied.
- Only `Owner` and `Editor` may edit. `Viewer`, unknown participants and
  unknown sessions are rejected **before** the project is touched.
- `apply_edit` checks permission, calls `core::edit::apply`, and only after a
  successful edit increments `CollabSession::version` by one. A failed
  `core::edit::apply` leaves the session version unchanged and returns
  `CollabError::Edit`.
- `can_edit` is read-only and returns false for unknown sessions, unknown
  users, and viewers.
- `CollabError` implements `Display` and `std::error::Error`; `Edit` exposes
  the underlying edit error through `source`.
- Session creation keeps its current behaviour and owner seeding. Session ids
  may keep using `Uuid::new_v4` for compatibility; do not make them part of a
  deterministic edit record.

### 3.2 Edit-linked version history

Extend `VersionEntry` with serde-defaulted fields:

```rust
#[serde(default)]
pub transaction_id: Option<uuid::Uuid>,
#[serde(default)]
pub project_digest: Option<uuid::Uuid>,
```

Add:

```rust
impl VersioningManager {
    pub fn record_transaction(
        &mut self,
        author: uuid::Uuid,
        summary: impl Into<String>,
        transaction: &core::edit::EditTransaction,
        project: &core::Project,
    ) -> Result<u64, CollabError>;

    pub fn find_by_transaction(&self, transaction_id: uuid::Uuid) -> Option<&VersionEntry>;
}
```

Required semantics:

- `record_transaction` computes the project digest as a UUIDv5 over the compact
  JSON produced by `core::serialization::to_json`, then stores
  `Some(transaction.id)` and `Some(digest)`. Return
  `CollabError::History` on serialization failure.
- Version numbers continue to increase by one from `versions.len()`.
- `record(author, summary)` keeps working and stores `None` for both new
  fields, so existing callers and JSON documents stay compatible.
- `find_by_transaction` returns the first matching entry.
- A digest must be identical for identical project JSON and different after an
  edit. Do not use `chrono` or wall-clock time in the digest.

### 3.3 Transaction staging

Extend `SyncManager` with a serde-defaulted queue of whole transactions:

```rust
#[serde(default)]
pub staged: Vec<core::edit::EditTransaction>;
```

Add:

```rust
impl SyncManager {
    /// Queue a transaction, returning false if its id is already staged.
    pub fn stage(&mut self, transaction: core::edit::EditTransaction) -> bool;

    /// Remove and return every staged transaction in insertion order.
    pub fn drain(&mut self) -> Vec<core::edit::EditTransaction>;

    pub fn staged_count(&self) -> usize;
}
```

- Deduplicate by transaction id; staging an existing id is a no-op and returns
  `false`.
- `drain` preserves insertion order and leaves the queue empty.
- Staging never applies the transaction to a project. `pending_changes` stays
  available and unchanged for older callers.
- `SyncManager` keeps its existing serde round trip.

### 3.4 Application wiring

Extend the `kai-cad` self-check and cross-crate smoke tests to:

1. Create a session, add an editor and a viewer, and check `can_edit`.
2. Apply an edit as the owner and as an editor; both bump the session version.
3. Reject the viewer, an unknown participant and an unknown session without
   changing the project or version.
4. Record a version for the transaction and assert its transaction id and
   non-nil digest.
5. Stage the transaction, reject a duplicate, and drain it back in order.
6. Undo the edit and assert the project JSON matches the starting state.
7. Print session participant count, version and staged-history status in the
   `collab:` line.

Add a cross-crate smoke test for permissions, version recording, staging and
atomic rejection.

### 3.5 Tests (target: 16–20 new tests)

Cover at least:

1. Add owner/editor/viewer participants and reject duplicates.
2. `can_edit` allows owners and editors, rejects viewers and unknown users.
3. Unknown session is rejected by `add_participant`, `set_role` and
   `apply_edit`.
4. `set_role` changes permissions.
5. Viewer edit is rejected without project or version change.
6. Editor and owner edits apply and bump the session version.
7. Failed stale edit leaves project and version unchanged.
8. `apply_edit` returns the same transaction as direct `core::edit::apply`.
9. `record_transaction` stores transaction id and a non-nil digest.
10. Digest is stable for identical JSON and changes after an edit.
11. `record` remains compatible and stores `None`.
12. `find_by_transaction` finds the recorded version.
13. Version numbers continue sequentially.
14. `stage` deduplicates by transaction id.
15. `drain` preserves insertion order and empties the queue.
16. Staging does not mutate the project.
17. SyncManager JSON round-trips with staged transactions.
18. `kai-cad` smoke test for permissions, versions and staging.

### 3.6 Definition of done

- [ ] Sessions support participants, roles and permission checks.
- [ ] `apply_edit` is atomic and bumps the version only on success.
- [ ] Version entries link transactions and deterministic project digests.
- [ ] `SyncManager` stages and drains transactions deterministically.
- [ ] `kai-cad` self-check and smoke tests cover permissions and staging.
- [ ] No public API was broken; existing 225 tests still pass.
- [ ] Test count rises from 225 to at least 241, all passing.
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

1. Add `CollabError`, membership methods and permission checks.
2. Implement `apply_edit` through `core::edit`, then test atomic rejection.
3. Extend `VersionEntry` and add deterministic `record_transaction`.
4. Extend `SyncManager` with staging and draining.
5. Wire the self-check and smoke test, then run the full verification suite.
6. Update documentation and leave the changes uncommitted.

---

## 6. Documentation updates (required)

- `README.md` — status, test count, collaboration features and roadmap.
- `docs/PLAN.md` — mark Week 10 complete and point to the Week 11 brief.
- `docs/PHASE1_TASKS.md` — update the status note.
- `docs/ARCHITECTURE.md` — session permissions, versions, staging and the
  "Current milestone" section.
- `docs/DEVELOPMENT.md` — test table, self-check sample and collaboration
  troubleshooting.
- `docs/CONTRIBUTING.md` — deterministic digests and atomic session edits.
- `docs/NEXT_AGENT_PROMPT.md` — replace with the Week 11 brief.
- `TODO.md` — mark Week 10 done and list Week 11.
- `PRD.md` — update the implementation-status paragraph.

---

## 7. Out of scope for this pass

- WebSocket or any network transport, remote peers, presence and reconnect.
- Merge algorithms, conflict resolution, operational transforms or CRDTs.
- Persisting sessions, versions or staged transactions in SQLite; schema
  changes are not needed.
- GUI, windowing or Vulkan object creation.
- Multi-step undo history beyond one transaction, branching history and undo
  across collaborators.
- kAI suggestion generation, model calls or PCB-object edit targets.
- Plugin architecture and real-time collaboration.
- Sharing by project id instead of the existing name strings.

---

## 8. Reporting format

Report changed files (do not commit), the final collaboration API, test counts
per crate and total, verification tails, and anything deliberately deferred.
