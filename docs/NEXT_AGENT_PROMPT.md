# Next Agent Prompt — Week 3: Symbol Management

You are continuing **KaiCad**, a pre-alpha EDA application written in Rust, at
`/home/sneak/vscode/KaiCad`. Week 2 is finished and verified. Your job is
**Week 3 of Phase 1: symbol management** — internal symbol-library data
structures, KiCad `.kicad_sym` loading, and persistence through the engineering
database — without breaking anything that already works.

This document is self-contained. Read it fully before writing code.

---

## 1. Current repository state

- **Branch:** `main`
- **Repository tip:** see `git log -1` (this brief predates the Week 2 close-out
  commit `af3d0a7`)
- **Working tree:** clean at the time this brief was written
- **Tests:** 90 passing, 0 failing
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
| `store` | SQLite engineering database + project CRUD | `core` |
| `kai-cad` | Application binary + cross-crate smoke tests | all of the above |

### Existing symbol support

`core::symbol::{Symbol, Pin, PinDirection, PinPosition, PinType}` already
describes a schematic symbol and its pins. `core::Project::schematic.symbols` is
a `Vec<Symbol>`. What is missing is:

- A first-class **symbol library** concept (a named collection of symbols
  independent of any one project).
- A parser for KiCad `.kicad_sym` symbol-library files.
- A way to load a KiCad library and expose its symbols through the workspace.
- Storage/retrieval of library symbols through the `store` crate.

---

## 2. Non-negotiable project rules

These come from `PRD.md` and are enforced in review:

1. **No KiCad wrapping.** KaiCad implements its own engines. KiCad file formats
   are an import/export target, never the internal representation.
2. **Local-first.** No network access, cloud service or AI model may be required
   for editing or persistence.
3. **Deterministic operations.** No unseeded randomness in geometry, netlisting,
   routing, rule checks — or in library loading.
4. **No silent data loss.** Preserve or report data you cannot represent.
5. **AI output is untrusted.** Do not weaken `ai::SuggestionValidator`.
6. **Zero warnings.** `cargo clippy --workspace --all-targets -- -D warnings`
   and `cargo fmt --all -- --check` must pass.
7. **`core` stays dependency-light.** Do not add `rusqlite` to `core`.
8. **No breaking changes** to existing public APIs. Additions are fine.
9. **Update the docs** listed in §7 in the same change.

---

## 3. Deliverable

Extend the workspace so that KaiCad can load a KiCad symbol library and make its
symbols available for schematic design.

### 3.1 `core::symbol_library`

Add a new public module `core::symbol_library` (and re-export its types from
`core::lib.rs`):

```rust
/// A named symbol library containing zero or more symbols.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SymbolLibrary {
    pub id: Uuid,
    pub name: String,
    pub source: SymbolLibrarySource,
    pub symbols: Vec<Symbol>,
}

/// Where a library came from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SymbolLibrarySource {
    /// Created inside KaiCad.
    Internal,
    /// Imported from a KiCad `.kicad_sym` file.
    KiCad { path: String },
}
```

Rules:

- `SymbolLibrary` must be serializable with the existing `core::serialization`
  helpers.
- Do not remove or rename any existing `core::symbol` fields or types.

### 3.2 KiCad `.kicad_sym` parser

Create a new workspace crate **`kicad-parser`** (directory `kicad-parser/`,
package name `kicad-parser`, description "KiCad file-format parser for KaiCad
import/export").

Dependencies (suggested; adjust as needed):

```toml
[dependencies]
core = { path = "../core" }

[dev-dependencies]
tempfile = "3"

[lints]
workspace = true
```

Public API (in `kicad-parser/src/lib.rs`):

```rust
#[derive(Debug)]
pub enum ParseError {
    Io(std::io::Error),
    Syntax(String),
    Unsupported(String),
}

impl std::fmt::Display for ParseError { /* ... */ }
impl std::error::Error for ParseError { /* ... */ }

/// Parse a KiCad `.kicad_sym` file and return a `SymbolLibrary`.
pub fn parse_symbol_library(path: impl AsRef<std::path::Path>) -> Result<core::SymbolLibrary, ParseError>;
```

Implementation notes:

- KiCad `.kicad_sym` files are S-expression documents. Implement a minimal
  deterministic S-expression tokenizer and parser. Do not add a generic
  expression-parsing crate unless you can justify it.
- Extract the top-level `(kicad_symbol_lib ...)` container and each
  `(symbol "Name" ...)` inside it.
- Map each KiCad `(symbol ...)` to `core::Symbol`:
  - `name` from the symbol string.
  - `library` from the library name.
  - `id` generated deterministically from `(library name, symbol name)` using
    `uuid::Uuid::new_v5` with `Uuid::NAMESPACE_OID` (add the `v5` feature to
    `kicad-parser`'s `uuid` dependency).
  - Extract pins from `(pin ...)` entries: map `electrical_type` to
    `core::PinType`, map `name`, map `number` to `String`. Position and
    direction may default to zero/unspecified for this pass.
- If a field cannot be represented, preserve it in a `Symbol`'s `pins` or
  ignore it with a documented limitation. Do not silently drop named pins.
- No network access, no external tools, no shelling out to KiCad.
- Return a clear `ParseError` on malformed input; do not panic.

### 3.3 Load symbol libraries through `store`

Add symbol-library CRUD to the `store` crate:

```rust
impl Store {
    /// Persist a new symbol library. Rejects duplicate ids.
    pub fn create_symbol_library(&mut self, library: &core::SymbolLibrary) -> Result<()>;

    /// Load a symbol library by id.
    pub fn load_symbol_library(&self, id: Uuid) -> Result<Option<core::SymbolLibrary>>;

    /// List all symbol libraries ordered by name ASC.
    pub fn list_symbol_libraries(&self) -> Result<Vec<(Uuid, String)>>;

    /// Update an existing symbol library; returns false if missing.
    pub fn update_symbol_library(&mut self, library: &core::SymbolLibrary) -> Result<bool>;

    /// Delete a symbol library; returns false if missing.
    pub fn delete_symbol_library(&mut self, id: Uuid) -> Result<bool>;
}
```

Schema additions (migration `0003_symbol_libraries.sql`):

```sql
CREATE TABLE symbol_libraries (
    id      TEXT PRIMARY KEY,
    name    TEXT NOT NULL,
    source  TEXT NOT NULL,  -- "Internal" or "KiCad:{path}"
    data    TEXT NOT NULL   -- pretty JSON of core::SymbolLibrary
);

CREATE INDEX symbol_libraries_name_idx ON symbol_libraries (name);
```

Rules:

- Apply the migration through the existing migration runner (add a new
  numbered file; do not edit `0001_initial.sql`).
- The canonical record is `symbol_libraries.data`; the `name` and `source`
  columns are queryable projections.
- Map duplicate id errors to `StoreError::Duplicate(id)` just like projects.

### 3.4 Wire symbol loading into the application

Extend the `kai-cad` self-check with a `kicad-parser` line:

```text
  kicad-parser: loaded N symbol(s) from <path>
```

Use a tiny inline `.kicad_sym` string written to a temporary file so the
self-check stays hermetic. The file should contain at least one symbol with at
least two pins. Parse it, verify the library name and symbol count, then clean
up.

Add `kicad-parser` to `kai-cad/Cargo.toml` and a corresponding integration
smoke test in `kai-cad/tests/smoke.rs`.

### 3.5 Example

`kicad-parser/examples/parse_library.rs`:

- Take a path from `std::env::args()` or use a temporary file containing a
  minimal `.kicad_sym` snippet.
- Parse it and print the library name, source and number of symbols/pins.
- End with `parse example: ok`.

It must run with `cargo run -p kicad-parser --example parse_library`.

### 3.6 Tests (target: 10–16 new tests)

Cover at least:

1. `core::SymbolLibrary` JSON round trip.
2. KiCad S-expression tokenizer handles atoms, strings, quoted strings and
   nested lists.
3. `parse_symbol_library` rejects a non-existent file with `ParseError::Io`.
4. `parse_symbol_library` rejects malformed S-expressions with
   `ParseError::Syntax`.
5. A minimal `.kicad_sym` produces a `SymbolLibrary` with the correct name,
   source and one symbol.
6. A library with two symbols produces two distinct symbol ids.
7. Pins are extracted with correct name, number and electrical type.
8. `store::create_symbol_library` persists and `load_symbol_library` returns it.
9. Duplicate library id returns `StoreError::Duplicate`.
10. `list_symbol_libraries` is ordered by name ASC.
11. `update_symbol_library` persists changes.
12. `delete_symbol_library` removes the library.
13. (Optional) Parsing a real small KiCad library sample matches expected
   symbol count.

---

## 4. Definition of done

All of the following must be true:

- [ ] `core::symbol_library` added with `SymbolLibrary` and
      `SymbolLibrarySource`.
- [ ] `kicad-parser` crate added to the workspace and compiles on stable Rust
      1.85.
- [ ] A minimal KiCad `.kicad_sym` parser loads at least name, pins and
      electrical types deterministically.
- [ ] `store` migration `0003_symbol_libraries.sql` applied automatically and
      idempotently.
- [ ] `store` symbol-library CRUD is implemented and tested.
- [ ] No public API of `core`, `schematic`, `pcb`, `renderer`, `ai`, `collab`
      or `store` was broken.
- [ ] `kai-cad` self-check includes the kicad-parser line and still ends
      `self-check: ok`.
- [ ] `kicad-parser/examples/parse_library.rs` runs successfully.
- [ ] Test count rises from 90 to at least 95, all passing.
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
cargo run -p kai-cad
cargo build --workspace --release
```

---

## 6. Suggested implementation order

1. Add `core::symbol_library` types and JSON round-trip tests.
2. Create the `kicad-parser` crate, manifest and workspace member; confirm
   `cargo check -p kicad-parser`.
3. Implement the S-expression tokenizer and parser with unit tests.
4. Implement `parse_symbol_library` and map KiCad symbols/pins to `core::Symbol`.
5. Add `store` migration `0003_symbol_libraries.sql` and symbol-library CRUD.
6. Add the parser example and wire `kai-cad` self-check + smoke test.
7. Run the full verification suite; fix warnings before moving on.
8. Update documentation and commit with a Conventional Commit message such as
   `feat(kicad-parser): add KiCad symbol library parser`.

---

## 7. Documentation updates (required)

Update these files so they describe the new reality:

- `README.md` — status, test count, crate table/layout, and the new
  `kicad-parser` crate; keep the Week 3 line current.
- `docs/PLAN.md` — mark Week 3 in progress/complete in the Phase 1 progress
  table; point to the next brief.
- `docs/PHASE1_TASKS.md` — update Tasks 1.6 and related acceptance criteria.
- `docs/ARCHITECTURE.md` — add `kicad-parser` to the crate graph, document the
  symbol-library model and parser responsibilities; update the
  "Current milestone and next steps" section.
- `docs/DEVELOPMENT.md` — add parser commands, the new test count, and any
  S-expression/KiCad troubleshooting notes.
- `docs/CONTRIBUTING.md` — note that KiCad file format support is import-only
  and must not become the internal representation.
- `docs/NEXT_AGENT_PROMPT.md` — replace this brief with the Week 4 one once
  Week 3 is verified.

---

## 8. Out of scope for this pass

Do not start these; they are later milestones:

- Schematic editing or placement of library symbols on sheets.
- KiCad `.kicad_sch` / `.kicad_pcb` parsing or writing.
- Footprint library loading.
- Any GUI, windowing or Vulkan object creation.
- Routing algorithms, DRC, or geometry computation.
- Async, networking, collaboration transport.
- Undo/redo transactions above the database layer.

---

## 9. Reporting format

When finished, report:

1. Files added and changed.
2. The final public API of `kicad-parser` and the new `store` symbol-library
   methods.
3. Migration version and table list.
4. Test count per crate and total.
5. Verification output tails for the commands in §5.
6. Anything you deliberately deferred, with the reason.

Do not run `git commit` unless the human asks you to. Do not rewrite unrelated
code, and do not reformat files you did not change.
