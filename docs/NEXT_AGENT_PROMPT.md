# Next Agent Prompt — Week 13: SQLite-Backed Plugin Registry

You are continuing **KaiCad**, a pre-alpha EDA application written in Rust at
`/home/sneak/vscode/KaiCad`. Week 12 is finished and verified. Your job is
**Week 13: persist validated plugin manifests in the SQLite engineering
database (schema v5) with deterministic CRUD**, without loading or executing
plugin code.

Read this document fully before writing code.

---

## 1. Current repository state

- **Branch:** `main`
- **Tests:** 282 passing, 0 failing
- **Schema:** version 4 (`projects`, projection tables, `symbol_libraries`,
  `pcb_designs`)
- **Toolchain:** `stable`; edition 2024; `rust-version` **1.85**
- **License:** GPL-3.0-or-later

### Workspace crates

| Crate | Role | Workspace dependencies |
|-------|------|------------------------|
| `core` | Shared data model, JSON, edit/history primitives | none |
| `schematic` | Netlist, graph, ERC | `core` |
| `pcb` | Board data, outline, layers, footprints, DRC, routing | `core` |
| `renderer` | Vulkan configuration types (`ash`) | none |
| `ai` | kAI validation and transaction application | `core`, `schematic` |
| `collab` | Sessions, permissions, versioning, sync staging | `core` |
| `plugin` | Manifest validation, sorted registry, local discovery | none |
| `store` | SQLite project, symbol-library and PCB-design CRUD | `core`, `pcb` |
| `kicad-parser` | `.kicad_sym` import | `core` |
| `kai-cad` | Binary and cross-crate smoke tests | all of the above |

### Week 12 foundation

- `plugin::PluginManifest` validates ids, names, versions, API version,
  capabilities and relative entry points.
- `plugin::PluginRegistry` keeps manifests sorted by id and rejects duplicates
  without mutating.
- `plugin::discover_manifests` reads direct `*.json` files deterministically
  and rejects invalid/duplicate manifests.
- `kai-cad` self-check and smoke tests cover discovery, registry order and
  rejection. No plugin code is loaded or executed.

What is missing for Week 13:

- Manifests disappear when the process exits.
- `store` has no plugin dependency, table or CRUD API.
- Schema v5 does not exist.

---

## 2. Non-negotiable project rules

1. **No dynamic loading.** Persist metadata only. Never `dlopen`, spawn or
   execute a plugin, and never open the declared entry point.
2. **Local-first.** No network, marketplace or external service is involved.
3. **Validated at every boundary.** Validate before writing and validate again
   when reading. Corrupt rows are errors, not silent skips.
4. **Deterministic CRUD.** Lists are ordered by plugin id, never by rowid or
   insertion order.
5. **No silent data loss.** Reject malformed or unsupported manifests instead of
   dropping fields or rows.
6. **Never edit an applied migration.** Add `0005_plugins.sql`; leave
   `0001`–`0004` untouched.
7. **Zero warnings.** `cargo clippy --workspace --all-targets -- -D warnings`
   and `cargo fmt --all -- --check` must pass.
8. **No breaking changes.** Add APIs and a `StoreError` variant; do not rename
   or remove existing ones.
9. **Update every schema-version expectation** from 4 to 5.
10. **Do not commit.** Leave the working tree ready for the repository owner.

---

## 3. Deliverable

### 3.1 Schema v5

Add `store/migrations/0005_plugins.sql`:

```sql
CREATE TABLE IF NOT EXISTS plugins (
    id           TEXT PRIMARY KEY NOT NULL,
    name         TEXT NOT NULL,
    version      TEXT NOT NULL,
    api_version  INTEGER NOT NULL,
    entry_point  TEXT NOT NULL,
    capabilities TEXT NOT NULL,         -- compact JSON array
    manifest     TEXT NOT NULL          -- pretty JSON of plugin::PluginManifest
);

CREATE INDEX IF NOT EXISTS idx_plugins_name ON plugins(name);
```

- `manifest` is the canonical record. The scalar columns and `capabilities`
  are queryable projections. Rebuild them from the manifest on every write.
- Keep the table independent of projects: plugin registration is global.
- Register migration `(5, include_str!("../migrations/0005_plugins.sql"))` and
  set `CURRENT_SCHEMA_VERSION = 5`.

### 3.2 Store dependency and errors

Add `plugin = { path = "../plugin" }` to `store/Cargo.toml`.

Add a `StoreError::Plugin(plugin::PluginError)` variant that preserves the
underlying error, implements `Display` with the plugin error text, and returns
it from `source()`.

### 3.3 Store CRUD

```rust
impl Store {
    pub fn save_plugin(&mut self, manifest: &plugin::PluginManifest)
        -> Result<()>;
    pub fn load_plugin(&self, id: &str)
        -> Result<Option<plugin::PluginManifest>>;
    pub fn list_plugins(&self) -> Result<Vec<plugin::PluginManifest>>;
    pub fn delete_plugin(&mut self, id: &str) -> Result<bool>;
}
```

- `save_plugin` calls `manifest.validate()` first and upserts by id. A rejected
  manifest leaves the database unchanged. Store pretty manifest JSON with one
  trailing newline, compact capability JSON, and the current UTC timestamp only
  if you also add a timestamp column (not required).
- `load_plugin` returns `None` for an unknown id. It parses the canonical
  `manifest` column through `PluginManifest::from_json`, so invalid stored
  metadata becomes `StoreError::Plugin` rather than an unchecked value.
- `list_plugins` returns every manifest in ascending id order and validates each
  row. An empty table is `Ok(vec![])`.
- `delete_plugin` returns whether a row existed and does not affect projects.
- Do not expose raw SQL rows or skip invalid manifests.

### 3.4 Application wiring

Extend the `kai-cad` self-check and cross-crate smoke tests to:

1. Open an in-memory store and assert schema v5.
2. Save two manifests with different ids.
3. Load one by id and verify all fields and capabilities.
4. List plugins and assert ascending id order.
5. Upsert an existing id (changed name/version) and verify one row remains.
6. Delete a plugin and verify `false` on a second delete and `None` on load.
7. Feed an invalid manifest to `save_plugin` and verify it is rejected without
   changing stored rows.
8. Keep the existing project/PCB-design store checks working.
9. Print the schema version and plugin row count in the `store:` line.

### 3.5 Tests (target: 16–18 new tests)

Cover at least:

1. Schema v5 is reported by a new in-memory store.
2. Existing project, symbol-library and PCB-design CRUD still pass.
3. Save/load round trips every manifest field and capability.
4. Saving two plugins stores both.
5. List is sorted by id regardless of insertion order.
6. Empty list returns no rows.
7. Load of an unknown id returns `None`.
8. Delete removes only the requested plugin.
9. Deleting twice returns `false`.
10. Upsert by id updates fields without duplicating.
11. Invalid manifests are rejected atomically.
12. Duplicate capabilities are rejected by validation before SQL.
13. A corrupt stored manifest is reported as `StoreError::Plugin`.
14. Schema migration preserves existing v4 data (projects and PCB designs).
15. Plugin rows survive close/reopen of a file-backed database.
16. `kai-cad` smoke test for plugin persistence.
17. Schema-version assertions throughout `store` are updated to 5.

### 3.6 Definition of done

- [ ] Migration `0005_plugins.sql` exists and is registered.
- [ ] `CURRENT_SCHEMA_VERSION` is 5.
- [ ] Plugin CRUD is validated, deterministic and failure-atomic.
- [ ] Existing project/library/PCB CRUD and migrations still pass.
- [ ] Self-check and smoke tests cover plugin persistence.
- [ ] No plugin code is loaded or executed.
- [ ] Existing 282 tests still pass.
- [ ] Test count rises from 282 to at least 298, all passing.
- [ ] Clippy, fmt, rustdoc and Markdown links are clean.
- [ ] Documentation updated (§6).

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

1. Add the migration and bump the schema version.
2. Add the `store` dependency, error variant and projection helpers.
3. Implement save/load/list/delete with validation.
4. Add unit tests, then wire the self-check and smoke test.
5. Update schema-version assertions and documentation.
6. Leave the changes uncommitted.

---

## 6. Documentation updates (required)

- `README.md` — status, test count, store/plugin features and roadmap.
- `docs/PLAN.md` — mark Week 13 complete and point to the Week 14 brief.
- `docs/PHASE1_TASKS.md` — update the status note.
- `docs/ARCHITECTURE.md` — schema v5, plugin table, `store -> plugin` edge,
  CRUD semantics and the "Current milestone" section.
- `docs/DEVELOPMENT.md` — test table, self-check sample and plugin persistence
  troubleshooting.
- `docs/CONTRIBUTING.md` — schema and validation rules.
- `docs/NEXT_AGENT_PROMPT.md` — replace with the Week 14 brief.
- `TODO.md` — mark Week 13 done and list Week 14.
- `PRD.md` — update the implementation-status paragraph.

---

## 7. Out of scope for this pass

- Dynamic library loading, process spawning, scripting runtimes or sandboxing.
- Plugin installation UI, marketplaces, signing, permissions or updates.
- Executing entry points, resolving dependencies or checking file existence.
- Storing project-specific plugin enable/disable state.
- Transport, collaboration or AI use of plugin metadata.
- Editing migrations `0001`–`0004` or changing existing table shapes.

---

## 8. Reporting format

Report changed files (do not commit), the final store/plugin API, test counts
per crate and total, verification tails, and anything deliberately deferred.
