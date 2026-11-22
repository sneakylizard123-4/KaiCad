# Next Agent Prompt — Week 12: Plugin Manifest Registry and Deterministic Discovery

You are continuing **KaiCad**, a pre-alpha EDA application written in Rust at
`/home/sneak/vscode/KaiCad`. Week 11 is finished and verified. Your job is
**Week 12: introduce a validated, local plugin manifest format and a
deterministic registry with local directory discovery**, without dynamic code
loading or network access.

Read this document fully before writing code.

---

## 1. Current repository state

- **Branch:** `main`
- **Tests:** 263 passing, 0 failing
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
| `store` | SQLite project, symbol-library and PCB-design CRUD | `core`, `pcb` |
| `kicad-parser` | `.kicad_sym` import | `core` |
| `kai-cad` | Binary and cross-crate smoke tests | all of the above |

### Existing foundations

- `core::serialization` provides compact and pretty JSON plus `save`/`load`.
- `core::edit` and `core::history` provide atomic reversible edits and bounded
  multi-step undo/redo.
- `collab` exposes sessions, roles, version history and sync staging.
- The workspace has no plugin crate, manifest format or registry. The roadmap
  calls for a plugin architecture but dynamic execution is not implemented.

What is missing for Week 12:

- A stable, validated description of a plugin's identity and capabilities.
- Duplicate and unsupported-API rejection.
- Deterministic local discovery and sorted registry views.

---

## 2. Non-negotiable project rules

1. **No KiCad wrapping.** KiCad formats are import/export targets only.
2. **Local-first.** No network, marketplace, cloud service or model may be
   required. Discovery reads local files only.
3. **No dynamic loading in this pass.** Do not `dlopen`, spawn plugin code,
   evaluate scripts or add an `unsafe` execution path.
4. **Deterministic operations.** Registry order and discovery results must not
   depend on filesystem iteration order. Sort by plugin id and then path.
5. **No silent data loss.** Reject malformed or ambiguous manifests with a
   specific error; do not silently skip a discovered file.
6. **Security by validation.** Reject absolute paths, parent traversal and
   non-relative entry points. Never resolve or execute an entry point.
7. **Zero warnings.** `cargo clippy --workspace --all-targets -- -D warnings`
   and `cargo fmt --all -- --check` must pass.
8. **No breaking changes.** Add a workspace crate and public APIs; do not
   rename or remove existing ones.
9. **Never edit an applied migration.** No schema change is expected.
10. **Update the docs** listed in §7 in the same change.
11. **Do not commit.** Leave the working tree ready for the repository owner.

---

## 3. Deliverable

### 3.1 New `plugin` crate

Add a workspace member named `plugin` with dependencies `serde` and
`serde_json` only. The crate is local metadata and discovery; it does not
depend on `core` in this pass.

```rust
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api_version: u32,
    pub entry_point: String,
    pub capabilities: Vec<PluginCapability>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PluginCapability {
    Schematic,
    Pcb,
    Ai,
    ImportExport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginError {
    InvalidManifest { field: String, message: String },
    UnsupportedApiVersion { found: u32, supported: u32 },
    DuplicateId(String),
    UnsafeEntryPoint(String),
    Json(String),
    Io(String),
}
```

Required manifest semantics:

- `id` is non-empty and limited to ASCII lowercase letters, digits, `.`, `_`
  and `-`; it must start with a letter or digit. This is the stable identity.
- `name` is non-empty after trimming.
- `version` is non-empty; semantic-version parsing is not required.
- `api_version` must equal `SUPPORTED_API_VERSION == 1`. Other values return
  `UnsupportedApiVersion`.
- `entry_point` is a non-empty relative path. Reject absolute paths, drive
  prefixes, `..` components and empty components. Do not check whether the file
  exists and never open it.
- `capabilities` must contain at least one entry and no duplicates.
- Implement `PluginManifest::validate()`, `from_json`, `to_json`, `save` and
  `load`. JSON is compact for `to_json`; `save` may use pretty JSON with one
  trailing newline. Malformed JSON maps to `PluginError::Json`; I/O maps to
  `PluginError::Io`.
- `PluginError` implements `Display` and `std::error::Error`.

### 3.2 Registry

```rust
#[derive(Debug, Clone, Default)]
pub struct PluginRegistry {
    plugins: Vec<PluginManifest>,
}

impl PluginRegistry {
    pub fn new() -> Self;
    pub fn register(&mut self, manifest: PluginManifest) -> Result<(), PluginError>;
    pub fn remove(&mut self, id: &str) -> bool;
    pub fn find(&self, id: &str) -> Option<&PluginManifest>;
    pub fn list(&self) -> Vec<&PluginManifest>;
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
}
```

- `register` validates the manifest, rejects duplicate ids and keeps a stable
  sorted-by-id order. Duplicate rejection does not mutate the registry.
- `remove` returns whether an id was present.
- `list` returns manifests sorted by id regardless of registration order.
- Existing entries stay valid; a failed registration leaves the registry
  unchanged.

### 3.3 Deterministic discovery

```rust
pub fn discover_manifests(directory: impl AsRef<std::path::Path>)
    -> Result<Vec<PluginManifest>, PluginError>;
```

- Read only regular files directly inside `directory` whose extension is
  `.json`; do not recurse and do not follow directory symlink trees.
- Sort paths by file name before reading, so errors and results are stable.
- Parse and validate every manifest. Reject duplicate ids across files.
- On any parse, validation or I/O error, return the error. Do not skip a bad
  file silently.
- An empty or missing directory is an I/O error (`Io`), not an empty success.
  Document this choice.
- Do not execute, copy or modify any discovered file.

### 3.4 Application wiring

Extend the `kai-cad` self-check and cross-crate smoke tests to:

1. Write two valid manifests to a temporary directory.
2. Discover them, build a registry and assert sorted ids and capabilities.
3. Reject a duplicate id and an unsupported `api_version`.
4. Reject unsafe entry points such as `../plugin` and absolute paths.
5. Remove a plugin and verify `find`/`len`/`is_empty`.
6. Clean up the temporary directory.
7. Print the discovered/registered plugin count in a `plugin:` line.

Add a smoke test covering manifest validation, deterministic discovery,
duplicate rejection and registry ordering.

### 3.5 Tests (target: 16–20 new tests)

Cover at least:

1. Valid manifest round-trips through JSON.
2. Invalid id characters and empty ids are rejected.
3. Empty name/version are rejected.
4. Unsupported API version is rejected.
5. Entry point accepts a normal relative path.
6. Entry point rejects `..`, absolute paths and empty components.
7. Empty and duplicate capabilities are rejected.
8. Manifest JSON parse errors and I/O errors map correctly.
9. Register sorts by id.
10. Duplicate registration leaves the registry unchanged.
11. Remove/find/len/is_empty behave correctly.
12. Discovery sorts manifest files deterministically.
13. Discovery returns all valid manifests.
14. Discovery rejects duplicate ids across files.
15. Discovery rejects an invalid file rather than skipping it.
16. Missing directory is an I/O error.
17. Non-JSON files are ignored.
18. `kai-cad` smoke test for discovery and registry behaviour.

### 3.6 Definition of done

- [ ] New `plugin` crate builds and is part of the workspace.
- [ ] `PluginManifest` validates identity, API version, capabilities and entry
      point safety.
- [ ] `PluginRegistry` is sorted, duplicate-free and failure-atomic.
- [ ] `discover_manifests` is deterministic and rejects bad files.
- [ ] `kai-cad` self-check and smoke tests cover discovery and rejection.
- [ ] No dynamic loading or network access was added.
- [ ] No public API was broken; existing 263 tests still pass.
- [ ] Test count rises from 263 to at least 279, all passing.
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

1. Create the `plugin` workspace crate and manifest types.
2. Implement validation, JSON and file helpers.
3. Implement the sorted, failure-atomic registry.
4. Implement deterministic directory discovery.
5. Add unit tests, then wire the self-check and smoke test.
6. Update documentation and leave the changes uncommitted.

---

## 6. Documentation updates (required)

- `README.md` — status, test count, plugin features and roadmap.
- `docs/PLAN.md` — mark Week 12 complete and point to the Week 13 brief.
- `docs/PHASE1_TASKS.md` — update the status note.
- `docs/ARCHITECTURE.md` — plugin crate, manifest validation, registry and
  discovery semantics, plus the "Current milestone" section.
- `docs/DEVELOPMENT.md` — test table, self-check sample and plugin
  troubleshooting.
- `docs/CONTRIBUTING.md` — deterministic discovery and manifest-safety rules.
- `docs/NEXT_AGENT_PROMPT.md` — replace with the Week 13 brief.
- `TODO.md` — mark Week 12 done and list Week 13.
- `PRD.md` — update the implementation-status paragraph.

---

## 7. Out of scope for this pass

- Dynamic library loading, process spawning, scripting runtimes or sandboxing.
- Plugin installation, updates, signing, permissions or a marketplace.
- Network access, remote registries or telemetry.
- Executing entry points, resolving dependencies or checking file existence.
- GUI plugin management or renderer integration.
- Persisting the registry in SQLite; schema changes are not needed.
- Plugin APIs for schematic/PCB mutation.

---

## 8. Reporting format

Report changed files (do not commit), the final plugin API, test counts per
crate and total, verification tails, and anything deliberately deferred.
