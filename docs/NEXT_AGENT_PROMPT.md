# Next Agent Prompt — Week 2: Engineering Database

You are continuing **KaiCad**, a pre-alpha EDA application written in Rust, at
`/home/sneak/vscode/KaiCad`. Week 1 is finished and committed. Your job is
**Week 2 of Phase 1: the engineering database** — a SQLite-backed store with
versioned migrations and project CRUD — without breaking anything that already
works.

This document is self-contained. Read it fully before writing code.

---

## 1. Current repository state

- **Branch:** `main`
- **Latest commit:** `c864fbb` — `feat(workspace): finalize Week 1 deliverables`
- **Working tree:** clean at the time this brief was written
- **Tests:** 71 passing, 0 failing
- **CI:** green on `c864fbb`
  ([run 43965323831](https://github.com/sneakylizard123-4/KaiCad/actions/runs/43965323831))
- **Toolchain:** `stable` channel; edition 2024; MSRV/`rust-version` **1.85**
- **License:** GPL-3.0-or-later

### Workspace crates

| Crate | Role | Workspace dependencies |
|-------|------|------------------------|
| `core` | Shared data model + JSON serialization | none |
| `schematic` | Netlist, connectivity graph, ERC | `core` |
| `pcb` | Board data, layers, footprints, routing config | `core` |
| `renderer` | Vulkan configuration types (`ash`) | none |
| `ai` | kAI suggestions + deterministic validation | `core`, `schematic` |
| `collab` | Sessions, versioning, sharing | `core` |
| `kai-cad` | Application binary + cross-crate smoke tests | all of the above |

`core` is the bottom of the graph. It must not depend on any other workspace
crate. `renderer` intentionally has no workspace dependencies.

### `core` module map (the data you must persist)

| Module | Public API |
|--------|------------|
| `core::geometry` | `Point { x: f64, y: f64 }`, `distance_to` |
| `core::net` | `Net { id, name, pins: Vec<Uuid>, netclass }`, `NetClass` |
| `core::symbol` | `Symbol { id, name, pins: Vec<Pin>, library }`, `Pin`, direction/position/type enums |
| `core::sheet` | `Sheet`, `Wire`, `Label`, position/type enums |
| `core::component` | `Component`, `ComponentProperty` |
| `core::pcb` | `PcbOutline { points }`, `PcbLayer { id, name, kind }`, `LayerKind`, `Stackup { layers }` |
| `core::project` | `Project { id, name, version, schematic, pcb, metadata }`, `SchematicData`, `PcbData`, `ProjectMetadata` |
| `core::proposal` | `ChangeProposal` (shared with `ai` and `collab`) |
| `core::serialization` | `Error`, `Result`, `to_json`, `to_json_pretty`, `from_json`, `save`, `load`; `Project::{to_json, from_json, save, load}` |

`Project` already has `Project::new(name)` and `Project::touch()`.
`SchematicData` contains `sheets`, `symbols`, `nets`, `components`.
`PcbData` contains `outline`, `layers`, `stackup`, `footprint_ids` (the `pcb`
crate owns footprint bodies; `core` stores ids only).

### What exists today for persistence

**JSON files only.** `core::serialization::save`/`load` write and read pretty
JSON and create parent directories. There is **no SQLite dependency, schema,
migration or query layer anywhere in the workspace.** That is the gap you are
closing.

---

## 2. Non-negotiable project rules

These come from `PRD.md` and are enforced in review:

1. **No KiCad wrapping.** KaiCad implements its own engines. KiCad file formats
   are an import/export target, never the internal representation.
2. **Local-first.** No network access, cloud service or AI model may be required
   for editing or persistence.
3. **Deterministic operations.** No unseeded randomness in geometry, netlisting,
   routing, rule checks — or in database writes. Do not generate new UUIDs or
   timestamps while loading a project.
4. **No silent data loss.** Preserve or report data you cannot represent.
5. **AI output is untrusted.** Do not weaken `ai::SuggestionValidator`.
6. **Zero warnings.** `cargo clippy --workspace --all-targets -- -D warnings`
   and `cargo fmt --all -- --check` must pass.
7. **`core` stays dependency-light.** Do not add `rusqlite` to `core`.
8. **No breaking changes** to existing public APIs. Additions are fine.
9. **Update the docs** listed in §7 in the same change.

---

## 3. Deliverable

A new workspace crate **`store`** (directory `store/`, package name `store`,
description "KaiCad engineering database — SQLite persistence and project CRUD")
that depends on `core` and provides a synchronous, local SQLite store.

### 3.1 Dependency choice

Use **`rusqlite`** with the **`bundled`** feature:

```toml
[dependencies]
core = { path = "../core" }
rusqlite = { version = "0.32", features = ["bundled"] }
serde_json = "1"
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }

[dev-dependencies]
tempfile = "3"
```

Notes:

- `bundled` compiles SQLite from source, so CI and Windows/macOS developers need
  no system `libsqlite3`. This is deliberate.
- `tempfile` is for file-backed tests only. If you prefer not to add it, use
  `std::env::temp_dir()` plus a UUID filename and clean up — but `tempfile` is
  acceptable and simpler.
- Do **not** add an async runtime. The store is synchronous for now.
- Add `[lints] workspace = true` and inherit `[workspace.package]` fields
  exactly like the other crates.
- Add `store` to `members` in the root `Cargo.toml`.

### 3.2 Schema (migration `0001_initial.sql`)

Design the schema so the **canonical record is the full project document**, with
queryable projection tables kept in sync in the same transaction. This satisfies
the Phase 1 requirement that the schema define projects, components, symbols,
nets, sheets and footprints without duplicating every field of the Rust model.

Required tables:

```sql
CREATE TABLE schema_migrations (
    version    INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL          -- RFC 3339 / ISO 8601 UTC
);

CREATE TABLE projects (
    id           TEXT PRIMARY KEY,    -- UUID string
    name         TEXT NOT NULL,
    version      TEXT NOT NULL,
    created_at   TEXT NOT NULL,
    modified_at  TEXT NOT NULL,
    author       TEXT NOT NULL,
    description  TEXT NOT NULL,
    document     TEXT NOT NULL        -- pretty JSON of the whole core::Project
);

CREATE INDEX projects_modified_at_idx ON projects (modified_at);

CREATE TABLE sheets (
    id         TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    data       TEXT NOT NULL          -- JSON of the core::Sheet
);

CREATE TABLE symbols (
    id         TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    library    TEXT NOT NULL,
    data       TEXT NOT NULL          -- JSON of the core::Symbol
);

CREATE TABLE components (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    symbol_id   TEXT NOT NULL,
    footprint_id TEXT,
    data        TEXT NOT NULL         -- JSON of the core::Component
);

CREATE TABLE nets (
    id         TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    data       TEXT NOT NULL          -- JSON of the core::Net
);

CREATE TABLE footprints (
    id         TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    data       TEXT NOT NULL          -- JSON; bodies are owned by the pcb crate
);
```

Rules:

- Enable `PRAGMA foreign_keys = ON` on every connection.
- `projects.document` is authoritative. Projection rows are rebuilt from it on
  every create/update inside one transaction. Document this in a module comment.
- Store UUIDs as hyphenated lowercase strings and timestamps as RFC 3339 UTC.
- Migrations are numbered SQL files under `store/migrations/`, embedded with
  `include_str!`, applied in ascending order inside a transaction, and recorded
  in `schema_migrations`. Running migrations twice must be a no-op.
- Never edit an applied migration; add a new numbered file.

### 3.3 Public API

Implement in `store/src/lib.rs` (types may live in submodules and be re-exported):

```rust
pub struct Store { /* rusqlite::Connection */ }

/// Summary row for project listings.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectSummary {
    pub id: uuid::Uuid,
    pub name: String,
    pub version: String,
    pub modified_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug)]
pub enum StoreError { /* Sqlite, Json, Io, NotFound, ... */ }
pub type Result<T> = std::result::Result<T, StoreError>;

impl Store {
    /// Open (creating if needed) a database file, then run migrations.
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self>;
    /// Open a private in-memory database and run migrations.
    pub fn open_in_memory() -> Result<Self>;
    /// Highest applied migration version (0 when none).
    pub fn schema_version(&self) -> Result<u32>;

    pub fn create_project(&mut self, project: &core::Project) -> Result<()>;
    pub fn load_project(&self, id: uuid::Uuid) -> Result<Option<core::Project>>;
    pub fn list_projects(&self) -> Result<Vec<ProjectSummary>>;
    /// Update an existing project; returns false if the id does not exist.
    pub fn update_project(&mut self, project: &core::Project) -> Result<bool>;
    /// Delete a project; returns false if the id does not exist.
    pub fn delete_project(&mut self, id: uuid::Uuid) -> Result<bool>;
}
```

Implementation requirements:

- `open` creates missing parent directories (like `core::serialization::save`).
- `create_project` must reject a duplicate id with a clear `StoreError`
  (map SQLite's constraint violation; do not panic).
- `update_project` writes the document and rebuilds all projection rows in one
  transaction.
- `load_project` parses `projects.document` with `core::serialization::from_json`.
  Do **not** reconstruct the project from projection rows.
- `list_projects` is ordered by `modified_at DESC, name ASC` for determinism.
- Implement `std::fmt::Display` and `std::error::Error` for `StoreError`, with
  `From<rusqlite::Error>`, `From<serde_json::Error>` and `From<std::io::Error>`.
- No `unwrap()`/`expect()` in library code. Tests may use them.

### 3.4 Example

`store/examples/project_crud.rs`:

- Open a database under `std::env::temp_dir()` with a UUID filename.
- Create a `core::Project::new("store-demo")` with one net and one symbol.
- Insert, list, load, modify the name, update, reload, delete, then confirm it is
  gone.
- Print one short line per step and end with `store example: ok`.
- Remove the database file at the end.

It must run with `cargo run -p store --example project_crud`.

### 3.5 Wire the store into the application

Extend the no-argument self-check in `kai-cad/src/main.rs` with a `store` line:

```text
  store: created, loaded and deleted 1 project (schema v1)
```

Use an in-memory store (`Store::open_in_memory`) so the self-check stays
hermetic. Keep `self-check: ok` as the last line and keep `--help`/`--version`
behaviour unchanged. Add `store` to `kai-cad/Cargo.toml` and to the smoke tests
in `kai-cad/tests/smoke.rs` (one CRUD round-trip test is enough).

### 3.6 Tests (target: 10–14 new tests)

Put them in `store/src/tests.rs` (declared from `lib.rs`). Cover at least:

1. `open_in_memory` applies migration 1 and `schema_version() == 1`.
2. Running migrations again is a no-op (open a file DB twice).
3. create → load round trip preserves id, name, version, metadata, nets,
   symbols, sheets, components and PCB data.
4. Duplicate id on create returns an error and leaves the first row intact.
5. `list_projects` returns summaries and is ordered deterministically.
6. `update_project` returns `true` and persists changed fields.
7. `update_project` returns `false` for an unknown id.
8. `delete_project` returns `true` once and `false` afterwards.
9. `load_project` returns `None` for an unknown id.
10. File-backed database survives closing and reopening.
11. Projection tables match the document (counts of nets/symbols/sheets).
12. Deleting a project cascades its projection rows.

Use deterministic fixtures: fixed UUIDs via `uuid::Uuid::parse_str`, fixed
timestamps via `chrono::DateTime::parse_from_rfc3339`. Do not rely on wall-clock
ordering.

---

## 4. Definition of done

All of the following must be true:

- [ ] `store` crate added to the workspace and compiles on stable Rust 1.85.
- [ ] Migrations run automatically on open and are idempotent.
- [ ] `create`/`load`/`list`/`update`/`delete` work and are tested.
- [ ] JSON persistence (`core::serialization`) is unchanged and still tested.
- [ ] No public API of `core`, `schematic`, `pcb`, `renderer`, `ai`, `collab`
      was broken.
- [ ] `kai-cad` self-check includes the store and still ends `self-check: ok`.
- [ ] `store/examples/project_crud.rs` runs successfully.
- [ ] Test count rises from 71 to at least 81, all passing.
- [ ] Clippy, fmt and rustdoc are warning-free.
- [ ] Documentation updated (§7).

## 5. Verification commands

Run from the workspace root and paste the tails in your report:

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo doc --workspace --no-deps
cargo test --workspace
cargo run -p store --example project_crud
cargo run -p kai-cad
cargo build --workspace --release
```

Also prove the database file is real:

```bash
cargo run -q -p store --example project_crud
```

## 6. Suggested implementation order

1. Add the crate, manifest, workspace member and an empty `lib.rs`; confirm
   `cargo check -p store`.
2. Write `store/migrations/0001_initial.sql` and the migration runner; test
   `schema_version()` and idempotency.
3. Implement `StoreError` and `open`/`open_in_memory`.
4. Implement create + load with projection writes; test round trip.
5. Implement list, update, delete; test each.
6. Add file-backed persistence test and cascade test.
7. Add the example, then wire `kai-cad` and its smoke test.
8. Run the full verification suite; fix warnings before moving on.
9. Update documentation and commit with a Conventional Commit message such as
   `feat(store): add SQLite engineering database with project CRUD`.

## 7. Documentation updates (required)

Update these files so they describe the new reality:

- `README.md` — status, test count, crate table/layout, and the new `store`
  crate; keep the Week 2 line current.
- `PLAN.md` — mark Week 2 complete in the Phase 1 progress table; point to the
  next brief.
- `PHASE1_TASKS.md` — tick Tasks 1.4 and 1.5 when genuinely done; update the
  completion criteria and status note.
- `docs/ARCHITECTURE.md` — add `store` to the crate graph, document the schema,
  migration policy and the "document is authoritative, projections are derived"
  rule; update the "Current milestone and next steps" section.
- `docs/DEVELOPMENT.md` — add store commands, the new test count, and any
  SQLite-specific troubleshooting (e.g. `bundled` build time).
- `docs/CONTRIBUTING.md` — note the migration rule (never edit an applied
  migration; add a new numbered file).
- `WEEK1_TASKS.md` — leave as the historical Week 1 record; do not rewrite it.
- `docs/NEXT_AGENT_PROMPT.md` — replace this brief with the Week 3 one (symbol
  management and KiCad symbol library loading) once Week 2 is verified.

## 8. Out of scope for this pass

Do not start these; they are later milestones:

- KiCad `.kicad_sch` / `.kicad_pcb` parsing or writing.
- Symbol library loading.
- Any GUI, windowing or Vulkan object creation.
- Routing algorithms, DRC, or geometry computation.
- Async, networking, collaboration transport.
- Undo/redo transactions above the database layer.

## 9. Reporting format

When finished, report:

1. Files added and changed.
2. The final public API of `store` (signatures).
3. Schema version and table list.
4. Test count per crate and total.
5. Verification output tails for the commands in §5.
6. Anything you deliberately deferred, with the reason.

Do not run `git commit` unless the human asks you to. Do not rewrite unrelated
code, and do not reformat files you did not change.
