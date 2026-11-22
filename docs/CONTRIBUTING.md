# Contributing to KaiCad

KaiCad is a pre-alpha Rust project. The core data model, schematic and PCB
engines, routing, software renderer, persistence, collaboration primitives and
a desktop GUI editor/inspection shell are implemented and tested, along with
the supported `.kicad_pcb` import/export subset, while full board and schematic
editing, full KiCad project-file compatibility, the Vulkan GPU backend and some
advanced engines remain incomplete. Contributions that move
the architecture forward — or that honestly document what is missing — are
welcome.

Read [`ARCHITECTURE.md`](ARCHITECTURE.md) for the crate layout and design
rules, and [`DEVELOPMENT.md`](DEVELOPMENT.md) for build, test and tooling
instructions. The current release brief — Week 16, beta readiness — is in
[`NEXT_AGENT_PROMPT.md`](NEXT_AGENT_PROMPT.md). The full-KiCad gap analysis and
delivery sequence are in [`KICAD_PARITY.md`](KICAD_PARITY.md).

## Ground rules

- **No KiCad wrapping.** KaiCad implements its own engines. KiCad file formats
  are an import/export target only: `kicad-parser` maps `.kicad_sym` and the
  supported `.kicad_pcb` subset into `core`/`pcb` types, and those types remain
  the internal representation. KiCad netlist output from
  `schematic::to_kicad_netlist` is likewise export-only and must never become
  the internal representation or gain a parser in the same layer.
  `pcb::FootprintLibrary` persists KaiCad's own JSON; a KiCad `.kicad_mod`
  reader is separate import work and must not become the storage format.
  Board import must return explicit warnings for skipped constructs rather
  than claim a lossless round trip.
- **Local-first.** No change may make a network connection, cloud service or
  AI model mandatory for normal editing.
- **AI output is untrusted.** Any AI-proposed change must pass deterministic
  validation before it is applied.
- **Plugin metadata is untrusted.** Manifests are validated before registration,
  discovery or persistence, ids are sorted deterministically, and entry points
  must remain relative. Discovery must never load or execute plugin code.
  Activation is separate registry state, not a manifest field.
- **GUI state is headless.** `gui::StudioState` owns board, path, view and DRC
  state so it can be tested without a window; `StudioApp` is only an eframe
  adapter. Do not duplicate PCB geometry or rasterization in the UI. The
  viewport must keep using `renderer::Renderer`. Board edits must go through
  the PCB/routing engines, preflight DRC, and preserve an undoable headless
  history; project edits use `core::edit` transactions. Do not mutate an
  unvalidated private copy of the design.
- **Deterministic operations.** Do not introduce unseeded randomness into
  geometry, netlisting, routing or rule checks. DRC violations are sorted by
  `(rule_id, objects)`, each unordered track pair on different nets is checked
  once, and rule results must not depend on track-vector order or `HashMap`
  iteration. Limits live on the design, are validated before use, and same-net
  copper is exempt from clearance because it is an intentional connection.
  Routing breaks A* ties by `(f, g, y, x, direction, layer, vias)` and derives
track and via ids with UUIDv5, never `Uuid::new_v4`; do not reintroduce heap  or `HashMap` iteration order into the result. Through vias are deterministic
  for the same stackup, node and config; blind/buried via spans and the
  push-and-shove candidate order must also be deterministic, and generated
  vias record their net so same-net clearance checks stay exempt. Interactive
  routing must keep previews out of the design until commit. Edit transactions
  derive UUIDv5 ids over their label and ordered changes, verify `old_value`
  before writing, and
  apply atomically on a clone — do not add partial-write paths. Version
  digests are UUIDv5 over compact project JSON, and sync staging deduplicates
  by transaction id while preserving insertion order. `EditHistory` clears
  redo on every new edit, evicts only the oldest undo entry at the limit, and
  changes stacks only after the underlying edit succeeds. `PluginRegistry`
  keeps manifests sorted by id and rejects duplicates without mutating, and
  `discover_manifests` reads only direct `.json` files in file-name order.
  `store` persists the canonical manifest with scalar projections; plugin
  lists are ordered by id, loads revalidate, and manifest upserts preserve the
  separate `enabled` column.
- **No silent data loss.** Report or preserve data you cannot represent.
- **Never edit an applied migration.** Migrations live in
  `store/migrations/<version>_<name>.sql` and are embedded with `include_str!`.
  Once a migration has been applied, add a new numbered file rather than
  changing an existing one.

## Branches

`main` must always build, format and pass tests.

| Change | Branch name |
|--------|-------------|
| Feature | `feat/<short-slug>` |
| Bug fix | `fix/<short-slug>` |
| Documentation | `docs/<short-slug>` |
| Refactor | `refactor/<short-slug>` |
| Tests / benchmarks | `test/<short-slug>` |
| CI / tooling | `ci/<short-slug>` |
| Chores | `chore/<short-slug>` |

Keep branches short-lived and rebase on `main` rather than merging `main` into
your branch. Never force-push `main`.

## Commits

Use [Conventional Commits](https://www.conventionalcommits.org/):

```text
<type>(<scope>): <imperative summary>

<why the change is needed>

Refs: #123
```

- Types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`,
  `ci`, `chore`, `revert`.
- Scope is usually a crate name: `feat(core): add net class lookup`.
- Imperative mood, no trailing period, subject at most 72 characters.
- One logical change per commit. Do not mix formatting-only churn with
  behaviour changes.
- Use the body to explain *why*; the diff already shows *what*.
- Mark breaking changes with `BREAKING CHANGE:` in the footer.
- Do not commit build output (`target/`), editor state or generated
  documentation.

## No-warnings policy

The workspace denies Clippy correctness lints and warns on `clippy::all` and
`unsafe_code` (see `[workspace.lints]` in the root `Cargo.toml`). CI turns all
warnings into errors: it runs `cargo clippy --workspace --all-targets -- -D
warnings`, so a warning that passes locally without the flag will still fail
the build. Before opening a pull request, run:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Rules:

- Fix warnings instead of silencing them. If an `#[allow(...)]` is genuinely
  required, add a comment explaining why, scoped as narrowly as possible.
- `unsafe` code needs a `// SAFETY:` comment describing the invariants and
  should be called out in the pull request.
- Keep `--all-targets` in lint commands so examples, benches and tests are
  covered too.

## Where tests, examples and benchmarks live

| Kind | Location | Notes |
|------|----------|-------|
| Unit tests | `<crate>/src/tests.rs` or inline `#[cfg(test)] mod tests` | `core` keeps its tests in `src/tests.rs`; the other crates use inline test modules. |
| Integration tests | `kai-cad/tests/smoke.rs` | Cross-crate smoke tests that exercise the public API of every library together. The root manifest is virtual, so there is no workspace-level `tests/` target; per-crate integration tests may be added under `<crate>/tests/`. |
| Benchmarks | `core/benches/*.rs` | Criterion, `harness = false`, registered with `[[bench]]` in `core/Cargo.toml`. |
| Examples | `core/examples/*.rs`, `pcb/examples/*.rs`, `store/examples/*.rs`, `kicad-parser/examples/*.rs` | Each example needs a `//!` header with the command used to run it. |

Examples must run and benchmarks must at least compile in CI:

```bash
cargo run -p core --example project_roundtrip
cargo run -p pcb --example board_outline
cargo run -p store --example project_crud
cargo run -p kicad-parser --example parse_library
cargo bench -p core --no-run
```

## Pull request review checklist

Author and reviewer should both confirm:

- [ ] The change is scoped to one concern and the commit messages follow the
      convention above.
- [ ] `cargo fmt --all -- --check` passes and Markdown links resolve
      (`python3 scripts/check_markdown_links.py`).
- [ ] `cargo check --workspace --all-targets` passes.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes.
- [ ] `cargo test --workspace` passes.
- [ ] New behaviour has tests; bug fixes include a regression test.
- [ ] Public APIs have doc comments; user-facing behaviour is documented.
- [ ] `docs/ARCHITECTURE.md` is updated if the crate graph, module layout or
      data flow changed.
- [ ] `docs/DEVELOPMENT.md` is updated if commands, toolchain or CI changed.
- [ ] `PLAN.md`, `PHASE1_TASKS.md` and `WEEK1_TASKS.md` reflect any change in
      milestone status; do not leave a completed task marked open.
- [ ] No new dependency without a justification (prefer the standard library
      and existing crates).
- [ ] Determinism, local-first operation and AI validation rules are respected.
- [ ] No unrelated reformatting, dead code or commented-out code is included.

CI must be green before merge. If CI cannot run (for example before the first
push to a new repository), state which commands you ran locally in the pull
request description.

## License and headers

KaiCad is licensed under **GPL-3.0-or-later**. By contributing you agree that
your contribution is licensed under the same terms.

New Rust source files should start with an SPDX header:

```rust
// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 KaiCad contributors
```

Most files written during the initial scaffold do not carry a header yet.
Add the header when you create a file or substantially rewrite one; do not
reformat unrelated files just to add it.
