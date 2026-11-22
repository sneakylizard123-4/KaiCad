# Next Agent Prompt — Week 16: Foundation Release Readiness

You are continuing **KaiCad**, a pre-alpha EDA application written in Rust at
`/home/sneak/vscode/KaiCad`. Weeks 1–15, the Phase 2 PCB/renderer scope and the
first Studio editing increment and the supported `.kicad_pcb` import/export
subset are finished and verified. Your job is **Week 16: harden the CLI,
document the plugin workflow and release surface, add performance baselines,
and complete the foundation release-readiness audit**. Do not add dynamic
plugin execution or new engineering engines.

This brief is release hardening, not the full-KiCad replacement program. The
current Studio shell is no longer read-only (it has track routing), and the CLI
can convert supported `.kicad_pcb`, `.kicad_sch`, `.kicad_mod` and `.kicad_pro`
files, but footprint placement, schematic capture, manufacturing outputs,
recursive project bundles and semantic KiCad compatibility remain future work. The long-range gap analysis and
delivery sequence are in [`KICAD_PARITY.md`](KICAD_PARITY.md). Read this
document fully before writing code.

---

## 1. Current repository state

- **Branch:** `main`
- **Tests:** 363 passing, 0 failing
- **Schema:** version 6 (`plugins` includes persisted activation state)
- **Toolchain:** `stable`; edition 2024; `rust-version` **1.85**
- **License:** GPL-3.0-or-later

### Workspace crates

`core`, `schematic`, `pcb`, `renderer`, `gui`, `ai`, `collab`, `plugin`, `store`,
`kicad-parser`, the `kai-cad` CLI binary and the `kai-cad-gui` desktop binary.

### Weeks 13–15 foundation

- Schema v5 persists validated plugin manifests in the global `plugins` table;
  canonical `manifest` JSON plus scalar/capability projections.
- Schema v6 adds a separate `enabled` column. Upserts preserve activation state.
- Store APIs: `save_plugin`, `load_plugin`, `list_plugins`, `list_plugin_states`,
  `list_enabled_plugins`, `list_plugins_with_capability`,
  `list_enabled_plugins_with_capability`, `set_plugin_enabled`,
  `plugin_enabled` and `delete_plugin`.
- CLI: `kai-cad plugins list|show|enable|disable|discover`, with optional
  `--db <path>` (default `kaicad.db`).
- Plugin metadata is validated at the plugin boundary and again on store load.
  No plugin code is loaded or executed.

### Phase 2 completion

- Rotated pads use oriented copper rectangles in DRC and routing.
- Vias are classified as through, blind or buried; routing can emit
  span-limited vias with `RouterConfig::allow_blind_buried`.
- `route_push_and_shove` performs bounded deterministic rip-up/reroute, and
  `InteractiveRouter` provides preview/commit/cancel sessions.
- `renderer` is a headless software 2D/3D rasterizer with a depth buffer and
  PPM output. It requires no Vulkan loader. The Vulkan GPU backend remains
  deferred.
- A post-Phase-2 `gui` crate adds KaiCad Studio: a native eframe window with
  board JSON open/save, a software-rendered 2D/3D viewport, layer/footprint
  inspection, DRC and undoable click-to-route track editing with DRC preflight.
  Footprint placement and schematic capture are not implemented.
- `kicad-parser` imports/exports the supported `.kicad_pcb` subset, preserving
  layers, named nets, pad numbers, Edge.Cuts outlines, footprints, pads, tracks
  and vias, and warning about skipped or approximated constructs. The CLI
  exposes `pcb import|export`; this is not full KiCad project compatibility.

---

## 2. Non-negotiable project rules

1. **No dynamic loading or execution.** This milestone is release hardening,
   not a plugin runtime.
2. **Local-first.** No network requirement, telemetry or cloud service.
3. **No silent data loss.** Validation and persistence failures are reported.
4. **Deterministic output.** CLI lists are id-ordered; error text is stable.
5. **No timing assertions.** Performance work records baselines; CI must not
   become flaky because a machine is slow.
6. **Never edit an applied migration.** Schema changes are not expected.
7. **Zero warnings.** Clippy with `-D warnings`, rustfmt and rustdoc must pass.
8. **No dependency without justification.** Prefer the standard library.
9. **No breaking API changes.** Extend the CLI and add docs/tests.
10. **Do not commit.** Leave the working tree ready for the repository owner.

---

## 3. Deliverable

### 3.1 CLI hardening

Extend `kai-cad/src/main.rs` without changing existing successful output
formats:

- Add an explicit `--self-check` flag as an alias for running with no arguments.
- Reject missing `--db` values, empty values, unknown options and extra
  positional arguments with exit code 1 and a specific message.
- Accept `--db=<path>` and `--db <path>` consistently.
- Keep `plugins help`, `plugins --help` and top-level `--help` accurate.
- Ensure `plugins discover` validates every manifest before opening the
  database; a failed discovery must not add rows.
- Keep list output as `id\tversion\tenabled|disabled\tcapabilities` with
  capabilities separated by `, ` in manifest order.
- Add tests for: missing DB value, empty DB value, unknown option, extra
  argument, unknown command, `--db=` form, explicit `--self-check`, and CLI
  version output.

### 3.2 User documentation

Add `docs/USER_GUIDE.md` covering:

- Prerequisites and build commands.
- `kai-cad --help`, `--version` and `--self-check`.
- The plugin manifest format with a complete JSON example.
- Discovery, registration, listing, showing, enabling and disabling plugins.
- The default `kaicad.db` location and how `--db` changes it.
- What "enabled" means: registry state only; no code is executed.
- Current limitations: the GUI supports track routing but not footprint/pad
  editing, pan/zoom or schematic capture; no Vulkan GPU backend (a headless
  software renderer is available); `.kicad_pcb`/`.kicad_sch`/`.kicad_mod`/
  `.kicad_pro` file interchange with unknown-node preservation, but no semantic
  zone/DRC modeling or recursive project bundle; no manufacturing outputs; no
  dynamic plugin loading and no network collaboration.
- A short troubleshooting table for invalid manifests, duplicate ids, unsafe
  entry points and corrupt registry rows.

Link the guide from `README.md`, `docs/DEVELOPMENT.md` and the Markdown docs
index. Keep the link checker passing.

### 3.3 Performance baselines

Add compile-checked Criterion benchmarks, or extend existing ones, for:

1. Compact and pretty serialization of a representative `core::Project`.
2. Single-layer and multi-layer routing on a representative board.
3. Plugin registry save, load, list and capability filtering in an in-memory
   store.

Requirements:

- Benchmarks compile with `cargo bench --workspace --no-run`.
- No benchmark asserts a wall-clock threshold.
- Document how to run them and where results are written.
- Do not add timing tests to `cargo test`.
- Reuse existing fixture construction; do not put benchmark-only APIs in
  production crates.

### 3.4 Release checklist

Add `docs/RELEASE.md` with a beta checklist that records, at minimum:

- Supported platforms and Rust version.
- `cargo fmt`, check, clippy, doc, test, examples, links and release build.
- Manual self-check output and exit code.
- Schema version and migration policy.
- Plugin registry limitations.
- Known deferred features (footprint placement, full PCB and schematic
  editing, manufacturing outputs, Vulkan GPU renderer, dynamic plugins,
  real-time collaboration, full KiCad project/schematic parsing and
  full-fidelity `.kicad_pcb` support).
- Versioning and GPL-3.0-or-later licensing.

The checklist must distinguish verified items from deferred ones; do not mark
future work complete.

### 3.5 Tests (target: 12–16 new tests, at least 354 total)

Cover at least:

1. Missing `--db` value.
2. Empty `--db` value.
3. Unknown top-level option.
4. Unknown plugin option.
5. Missing plugin subcommand argument.
6. Extra plugin positional argument.
7. Unknown plugin command.
8. `--db=<path>` form.
9. Explicit `--self-check` success.
10. CLI `--version` output.
11. Help output contains every plugin command.
12. Help output is stable and exits 0.
13. Invalid discovery leaves the registry empty.
14. Existing CLI lifecycle still passes.

### 3.6 Definition of done

- [ ] CLI edge cases return exit code 1 with specific messages.
- [ ] `--self-check` and `--db=<path>` work.
- [ ] `docs/USER_GUIDE.md` and `docs/RELEASE.md` exist and are linked.
- [ ] Benchmarks compile and are documented.
- [ ] Existing 363 tests still pass.
- [ ] Test count rises to at least 364, all passing.
- [ ] No plugin code is loaded or executed.
- [ ] Clippy, fmt, strict rustdoc, links, examples and release build pass.
- [ ] Documentation updated (§5).

---

## 4. Verification commands

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
cargo test --workspace
cargo bench --workspace --no-run
python3 scripts/check_markdown_links.py
cargo run -p core --example project_roundtrip
cargo run -p kicad-parser --example parse_library
cargo run -p kai-cad -- pcb import board.kicad_pcb board.json
cargo run -p kai-cad -- pcb export board.json board.kicad_pcb
cargo run -p store --example project_crud
cargo run -p pcb --example board_outline
cargo run -p kai-cad
cargo build --workspace --release
```

---

## 5. Documentation updates (required)

- `README.md` — status, test count, user-guide/release links and roadmap.
- `docs/PLAN.md` — mark Week 16 complete and close the 16-week foundation plan.
- `docs/KICAD_PARITY.md` — keep the post-beta parity sequence and exit criteria
  current; do not mark parity complete from Week 16 work.
- `docs/PHASE1_TASKS.md` — update the status note.
- `docs/ARCHITECTURE.md` — CLI behavior, benchmark layout and "Current
  milestone" section.
- `docs/DEVELOPMENT.md` — test table, CLI edge cases, benchmark commands and
  docs links.
- `docs/CONTRIBUTING.md` — release checklist and benchmark expectations.
- `docs/NEXT_AGENT_PROMPT.md` — replace with a post-1.0 roadmap brief or state
  that the 16-week plan is complete.
- `TODO.md` — mark Week 16 done and list post-beta work.
- `PRD.md` — update the implementation-status paragraph.

---

## 6. Out of scope for this pass

- Dynamic plugin loading, execution, sandboxing, signing or installation UI.
- Vulkan renderer implementation or new GUI editing features; only fixes to
  the existing Studio inspection/DRC/track-routing shell are in scope.
- New schematic, PCB or routing features.
- Semantic zone/arc/graphics/DRC modeling, recursive sheet bundles or
  `sym-lib-table`/`fp-lib-table` handling; the existing file-level conversion
  must not regress.
- Network collaboration, telemetry or cloud services.
- Schema changes; migrations 0001–0006 remain immutable.
- Rewriting or reformatting unrelated modules.

---

## 7. Reporting format

Report changed files (do not commit), CLI behavior, benchmark commands, test
counts per crate and total, verification tails, and anything deliberately
deferred.
