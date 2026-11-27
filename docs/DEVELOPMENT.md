# Development Guide

Practical instructions for building, testing and hacking on KaiCad.

> KaiCad is **pre-alpha**. The data model, schematic and PCB engines, routing,
> software renderer, persistence, collaboration primitives and a desktop GUI
> editor/inspection shell are implemented and tested, together with a supported
> `.kicad_pcb`, `.kicad_sch`, `.kicad_mod` and `.kicad_pro` file interchange.
> Full semantic KiCad project compatibility, full board/schematic editing, the
> Vulkan GPU backend, AI generation and real-time transport are not. See [`ARCHITECTURE.md`](ARCHITECTURE.md) for
> details.
>
> **Current state:** Weeks 1–15, the Phase 2 PCB/renderer scope, the first
> Studio editing increment and the P2 KiCad interoperability milestone are
> complete (401 tests, clippy/fmt/doc/release all clean). Week 16 release
> hardening is described in [`NEXT_AGENT_PROMPT.md`](NEXT_AGENT_PROMPT.md);
> full KiCad parity is tracked separately in
> [`KICAD_PARITY.md`](KICAD_PARITY.md).

## Prerequisites

### All platforms

- [Rustup](https://rustup.rs/) — `rust-toolchain.toml` pins the `stable`
  channel with the `rustfmt` and `clippy` components, so rustup installs the
  right toolchain automatically.
- Git.
- A C linker (`cc`/`clang` or MSVC `link.exe`).

### Linux

Ubuntu/Debian:

```bash
sudo apt-get update
sudo apt-get install build-essential pkg-config libssl-dev libudev-dev \
    libvulkan-dev vulkan-tools
```

Fedora:

```bash
sudo dnf install gcc pkgconf-pkg-config openssl-devel systemd-devel \
    vulkan-loader-devel vulkan-tools
```

Arch:

```bash
sudo pacman -S base-devel pkgconf openssl systemd-libs \
    vulkan-icd-loader vulkan-tools
```

`libssl`/`libudev` are not linked by any crate today; they are listed because
future OS integration and the Vulkan GPU backend will need them. The current
software renderer is pure Rust and needs no Vulkan runtime.

### macOS

```bash
brew install pkg-config openssl
```

Install the Vulkan SDK (which provides MoltenVK) from
<https://vulkan.lunarg.com/> only if you intend to work on the future GPU
backend; the current software renderer does not need it.

### Windows

- Visual Studio Build Tools with the "Desktop development with C++" workload.
- The Vulkan SDK from <https://vulkan.lunarg.com/> only for future GPU-backend work.
- Rustup using the `stable-x86_64-pc-windows-msvc` toolchain.

### Vulkan runtime

`renderer` keeps `ash` for the future Vulkan backend, but its current software
rendering path never loads the loader. Building and running the renderer need
no Vulkan SDK. For future GPU work, install a loader plus an ICD (Mesa, NVIDIA,
AMD or MoltenVK) and verify with `vulkaninfo --summary`.

### Desktop GUI runtime

The `gui` crate builds the native KaiCad Studio window with eframe and glow.
Running it needs an X11 or Wayland session, an OpenGL-capable driver and
`libxkbcommon` at runtime. It does not need a Vulkan loader. Unit tests,
`cargo check`, and the headless renderer run without a display.

## Toolchain

| File | Setting |
|------|---------|
| `rust-toolchain.toml` | `channel = "stable"`, components `rustfmt`, `clippy` |
| `rustfmt.toml` | edition 2024, max width 100, 4 spaces, Unix newlines |
| `clippy.toml` | `msrv = "1.85"` |
| root `Cargo.toml` | `resolver = "3"`, `rust-version = "1.85"`, workspace lints, dev/release profiles |

**MSRV:** edition 2024 requires Rust **1.85** or newer; `[workspace.package]
rust-version = "1.85"` records this for the whole workspace and `clippy.toml`
sets `msrv = "1.85"` so lints do not suggest newer APIs. Distro-packaged Rust
is often too old — use rustup.

**Lockfile:** `Cargo.lock` is committed to version control. The workspace
ships binaries, so the lockfile is part of the reproducible build. Do not add
it to `.gitignore`.

**Packaging:** `cargo package` refuses to run on a dirty working tree. When
checking packaging locally, use `cargo package --allow-dirty` (or commit
first).

## Commands

Run these from the workspace root (`/path/to/KaiCad`).

| Task | Command |
|------|---------|
| Type-check everything, including tests/benches/examples | `cargo check --workspace --all-targets` |
| Build everything | `cargo build --workspace --all-targets` |
| Run all tests | `cargo test --workspace` |
| Lint | `cargo clippy --workspace --all-targets` |
| Lint and fail on warnings (what CI does) | `cargo clippy --workspace --all-targets -- -D warnings` |
| Format | `cargo fmt --all` |
| Verify formatting without writing | `cargo fmt --all -- --check` |
| Build API documentation | `cargo doc --workspace --no-deps` |
| Run the application (banner + self-check) | `cargo run -p kai-cad` |
| Run the desktop GUI with the demo board | `cargo run -p gui` |
| Open a board in the GUI | `cargo run -p gui -- path/to/board.kicad_pcb` |
| Import a supported KiCad PCB | `cargo run -p kai-cad -- pcb import board.kicad_pcb board.json` |
| Export a board as KiCad PCB | `cargo run -p kai-cad -- pcb export board.json board.kicad_pcb` |
| Import/export a KiCad schematic | `cargo run -p kai-cad -- sch import main.kicad_sch main.json` |
| Import/export a KiCad footprint | `cargo run -p kai-cad -- footprint import part.kicad_mod part.json` |
| Preserve a KiCad project file | `cargo run -p kai-cad -- project import project.kicad_pro project.json` |
| Round-trip a schematic hierarchy | `cargo run -p kai-cad -- project bundle project.kicad_pro output-dir` |
| Print CLI usage | `cargo run -p kai-cad -- --help` |
| Print the version | `cargo run -p kai-cad -- --version` |
| Build a release binary | `cargo build --workspace --release` |
| Run the store CRUD example | `cargo run -p store --example project_crud` |
| Parse a KiCad symbol library | `cargo run -p kicad-parser --example parse_library` |
| Check relative Markdown links | `python3 scripts/check_markdown_links.py` |
| Measure test coverage locally (after `cargo install cargo-llvm-cov`) | `cargo llvm-cov --workspace --all-targets --summary-only` |

Note that `--all-targets` is required to lint/check tests, examples and
benchmarks; a bare `cargo check` misses them.

## Tests

`cargo test --workspace` currently runs **401 tests**, all passing:

| Suite | Tests |
|-------|------:|
| `ai` (`src/lib.rs`) | 12 |
| `collab` (`src/lib.rs`) | 27 |
| `core` (`src/tests.rs`, `edit.rs`, `history.rs` and inline modules) | 63 |
| `gui` (`src/app.rs`, `editor.rs`, `schematic.rs`, `demo.rs`, `document.rs`) | 34 |
| `kicad-parser` (`src/lib.rs`, `pcb.rs`, `schematic.rs`, `project.rs`, `library_table.rs`, corpus tests) | 43 |
| `pcb` (`src/lib.rs`, `drc.rs`, `routing.rs`, `board.rs` and modules) | 92 |
| `plugin` (`src/lib.rs` and inline modules) | 18 |
| `renderer` (`src/lib.rs`, `framebuffer.rs` and modules) | 19 |
| `schematic` (`src/lib.rs` and modules) | 33 |
| `store` (`src/tests.rs`) | 38 |
| `kai-cad/tests/smoke.rs` (cross-crate integration) | 22 |

Unit tests live next to the code (inline `#[cfg(test)] mod tests`, or
`src/tests.rs` for `core`). Cross-crate integration tests live in
`kai-cad/tests/smoke.rs`: the workspace root is a virtual manifest, so a root
`tests/` directory would not be built.

## Running the binary

With no arguments, `kai-cad` prints a banner and runs a deterministic
self-check that calls every subsystem:

```bash
cargo run -p kai-cad
```

```text
KaiCad — Modern EDA Application in Rust
Version: 0.1.0 (pre-alpha)
License: GPL-3.0-or-later

subsystem self-check:
  core: project JSON round trip ok (500 bytes), history 3 undo / 0 redo, limit 2
  plugin: discovered 2, registered 2, deterministic order and validation ok
  schematic: 1 net(s), 1 connection(s), ERC valid=true, 0 violation(s), export 208 bytes
  pcb: outline 4 points, area 8000 mm^2, 2 layer(s), 2 copper layer(s), 1 footprint(s), DRC clean=true (1 violation(s) before fixing), routed 5 point(s), multi-layer 2 track(s)/2 via(s), shoved 1 track(s), interactive 1 track(s)
  renderer: software 2D/3D 640x480, 204020 / 71789 foreground pixels, vsync=true
  ai: endpoint=http://localhost:11434, deterministic validation=always, applied 1 change(s), undo/redo ok (tx 97f81c80)
  collab: 1 session(s), 3 participant(s), session version 1, history 1 entry, staged 1 -> 0
  store: created, loaded and deleted 1 project with its PCB design (schema v6), 2 plugin(s) persisted/2 enabled
  kicad-parser: loaded 1 symbol(s) from <temp path>.kicad_sym, PCB/schematic/footprint round trips ok
self-check: ok
```

`--help`/`-h` prints usage and exits; `--version`/`-V` prints `kai-cad
<version>` and exits. Both return exit code 0 without running the self-check.

The plugin registry is managed through `plugins` subcommands. `--db <path>`
selects the SQLite database and defaults to `kaicad.db`:

```bash
cargo run -p kai-cad -- plugins list
cargo run -p kai-cad -- plugins discover ./plugins --db my-project.db
cargo run -p kai-cad -- plugins show com.example.alpha --db my-project.db
cargo run -p kai-cad -- plugins disable com.example.alpha --db my-project.db
cargo run -p kai-cad -- plugins enable com.example.alpha --db my-project.db
```

`discover` validates every manifest before opening the database. `list` prints
`id`, `version`, state and comma-separated capabilities, one plugin per line.

## Running the desktop GUI

```bash
# Built-in demo board
cargo run -p gui

# Open a KiCad board at startup
cargo run -p gui -- path/to/board.kicad_pcb
```

The `kai-cad-gui` binary opens **KaiCad Studio**, an eframe/egui window. The
left panel owns the document path, view selector, 2D zoom controls, routing
controls, board summary, design rules and DRC. The right panel lists the copper
layer stack and placed footprints. The centre panel is a live texture rendered
by the deterministic software rasterizer. The 2D view starts fitted; the wheel
zooms at the pointer, middle-drag pans, primary-drag pans in Inspect, and Fit
(or `0`) restores the fitted view. Boards open from the Document path field,
the File menu, a dropped file, or a command-line path. `.kicad_pcb` is the
native format: Save writes it through `kicad-parser`, preserving constructs the
editor does not model. `.json` is accepted when interoperability or debugging
needs an explicit KaiCad board document.

**Schematic workspace.** File → New/Open schematic switches Studio to the
schematic document. The canvas draws the supported `.kicad_sch` model — wires,
junctions, local/global labels, no-connects, child-sheet symbols and labeled
component bodies — with the same wheel zoom, drag panning and `+`/`-`/`0`
controls as the board. Click once to set a wire start and once to finish it;
endpoints snap to a 1.27 mm grid, and `Ctrl+Z`/`Ctrl+Y` undo and redo wire
edits. `.kicad_sch` is saved through `kicad-parser`, retaining unmodelled
nodes. Symbol graphics are placeholder bodies; symbol/label placement and
cross-probing are not implemented.

In the 2D board view, select **Route**, a copper layer, a net and a width, then
click a start point and a destination. A click within 2 mm of a pad snaps to its
centre and adopts its net. The grid router adds copper immediately, runs DRC
before applying it, and stores a board snapshot for undo. A route that adds a
new DRC finding is rejected without changing the board. This is a first editor
increment: there is no route preview, footprint placement, pad/via editing,
symbol placement inside schematics or project-wide file navigation yet.

| Input | Action |
|-------|--------|
| `Ctrl+O` | open the active document's path dialog |
| `Ctrl+S` | save the active document, or open Save As |
| Mouse wheel | zoom the 2D board view or schematic at the pointer |
| Middle-drag, or primary-drag in Inspect | pan the 2D view |
| `+` / `-` | zoom in / out |
| `0` | fit the board or schematic |
| Click in the schematic | set a wire start / finish a wire |
| Drag in the schematic | pan the schematic canvas |
| `Ctrl+Z` | undo the last routed track edit |
| `Ctrl+Y` | redo the last track edit |
| `F5` | run DRC |
| `R` | toggle Inspect / Route |
| `Esc` | cancel a pending route or close the file dialog |
| Drop a file on the window | open that board |

Current limits: footprint, pad and via editing are not implemented; the 3D
view auto-fits the board while the 2D views support zoom and pan; schematic
symbol graphics and symbol/label placement are not modeled; cross-probing is
not implemented; open/save paths are typed rather than chosen from a native
file picker; `.kicad_pcb` and `.kicad_sch` are the native Studio formats, while
board `.json` is optional. The board viewport is capped at 2048 pixels on its
longest side. The window uses OpenGL through eframe; board pixels still come
from the CPU renderer, and no Vulkan loader is required.

## Examples

Examples live in each package under `<crate>/examples/`. The workspace root is
a virtual manifest (no `[package]`), so a root-level `examples/` directory
would **not** be built by Cargo.

```bash
# Build a Project, serialize it to pretty JSON, deserialize it and verify it
cargo run -p core --example project_roundtrip

# Construct a board outline, layer stack, track and DRC report
cargo run -p pcb --example board_outline

# Create, list, load, update and delete a project in a SQLite database
cargo run -p store --example project_crud

# Parse a KiCad symbol library and print a summary
cargo run -p kicad-parser --example parse_library

# Convert a supported KiCad PCB through the CLI
cargo run -p kai-cad -- pcb import board.kicad_pcb board.json
cargo run -p kai-cad -- pcb export board.json board.kicad_pcb
cargo run -p kai-cad -- project bundle project.kicad_pro output-dir
```

List the examples of a package with:

```bash
cargo run -p core --example
```

## Benchmarks

Benchmarks live in `<crate>/benches/` and use
[Criterion](https://bheisler.github.io/criterion.rs/book/) with
`harness = false` (declared in `core/Cargo.toml`).

```bash
# Compile the benchmark without running it
cargo bench -p core --no-run

# Run the serialization benchmarks (Criterion default: ~15-30 s total)
cargo bench -p core

# Run only one benchmark target, with a short measurement window
cargo bench -p core --bench serialization -- \
    --warm-up-time 0.2 --measurement-time 0.5 --sample-size 10
```

Criterion writes reports to `target/criterion/` (already ignored by
`.gitignore` via `target/`; `criterion/` is also ignored in case a custom
output directory is configured). Open `target/criterion/report/index.html` in a
browser after a run.

## Continuous integration

`.github/workflows/ci.yml` runs on every push and pull request targeting
`main`. It has a `Check` job and a `Coverage` job, both on `ubuntu-latest`.

The `Check` job runs:

1. `actions/checkout@v4`
2. `dtolnay/rust-toolchain@stable` with `rustfmt, clippy`
3. `Swatinem/rust-cache@v2`
4. `cargo fmt --all -- --check`
5. `python3 scripts/check_markdown_links.py` ("Check Markdown links")
6. `cargo check --workspace --all-targets`
7. `cargo clippy --workspace --all-targets -- -D warnings`
8. `cargo doc --workspace --no-deps` ("Build docs")
9. `cargo test --workspace`
10. `cargo build --workspace --release`

The `Coverage` job installs `llvm-tools-preview` and `cargo-llvm-cov`, runs
`cargo llvm-cov --workspace --all-targets --lcov --output-path lcov.info`,
prints the summary and uploads `lcov.info` as a `coverage-lcov` artifact.

`CARGO_TERM_COLOR` is set to `always` for readable logs. `-D warnings` is
passed to Clippy only, not through a global `RUSTFLAGS`, because global flags
also apply to dependencies and build scripts where warnings are not ours to
fix.

## Working on the next milestone

Weeks 1–15, the Phase 2 PCB/renderer scope, the first Studio editing
increment and the P2 KiCad interoperability milestone are finished; Week 16
hardens the CLI, adds user documentation and
performance baselines, and completes the release checklist. Before starting,
read [`NEXT_AGENT_PROMPT.md`](NEXT_AGENT_PROMPT.md), which contains the API
requirements, acceptance criteria and verification commands. The existing JSON
persistence in `core::serialization` and the footprint-library format must keep
working while new persistence layers are added. Footprint placement, schematic
capture, full project-file compatibility and manufacturing outputs are parity
work, not Week 16 work; see [`KICAD_PARITY.md`](KICAD_PARITY.md). Keep the
current shell compiling and tested.

When a milestone changes the crate graph, commands or task status, update
`docs/ARCHITECTURE.md`, this guide, `PLAN.md` and `PHASE1_TASKS.md` in the same
change.

## Troubleshooting

### `cargo` reports "Blocking waiting for file lock on build directory"

Two Cargo invocations are sharing the same `target/` directory (for example an
IDE running `cargo check` while you run `cargo test`). Wait for the other
process, or give one of them a private target directory:

```bash
CARGO_TARGET_DIR=target/my-task cargo test --workspace
```

The same applies to the package-cache lock (`Blocking waiting for file lock on
package cache`) while Cargo downloads dependencies.

### Vulkan loader or driver not found

Symptoms include `vulkaninfo` failing, or (once the Vulkan GPU backend is
implemented) `ash` failing to load `libvulkan.so.1` / `vulkan-1.dll` /
`libvulkan.dylib`. The current software renderer does not use the loader.
Install a Vulkan loader and an ICD, then check:

```bash
vulkaninfo --summary
VK_LOADER_DEBUG=all cargo run -p kai-cad   # verbose loader output
```

On headless machines, use a software ICD such as Mesa's `lavapipe`
(`mesa-vulkan-drivers` on Debian/Ubuntu).

### Tests and `panic = "abort"`

The root `Cargo.toml` sets `panic = "abort"` for the `dev` and `release`
profiles. Cargo ignores that setting for the `test` and `bench` profiles (the
test harness needs unwinding), so `cargo test --workspace` works normally.
Consequences:

- Do not add a custom profile that inherits `panic = "abort"` and then expect
  `#[should_panic]` tests to work.
- Release builds (`lto = true`, `opt-level = "z"`, `codegen-units = 1`) are
  slow to link; use dev builds while iterating.

### SQLite `bundled` build is slow the first time

`store` depends on `rusqlite` with the `bundled` feature, which compiles SQLite
from source. The first `cargo check -p store` or `cargo build --workspace` can
take a minute or two while `libsqlite3-sys` builds; later builds are
incremental. This feature avoids requiring a system `libsqlite3` and keeps CI
and Windows/macOS builds self-contained.

### KiCad import or export looks wrong

`kicad-parser` reads `.kicad_sym` symbol libraries, `.kicad_pcb` boards,
`.kicad_sch` schematics, `.kicad_mod` footprints and `.kicad_pro` project JSON.
It preserves many constructs it does not model, but it is not a full-fidelity
converter. `ProjectBundle` recursively follows child sheet files, library tables
and `${KIPRJMOD}` libraries. `sym-lib-table` and `fp-lib-table` entries are
parsed, rewritten and preserved with unknown nodes; global/user tables outside
the project directory are not merged. Convert files through:

```bash
cargo run -p kai-cad -- pcb import board.kicad_pcb board.json
cargo run -p kai-cad -- pcb export board.json board.kicad_pcb
```

Import warnings on stderr are part of the result, not noise. Known limits:

- Zones, fills, keepouts, copper arcs, top-level line/rectangle/circle/arc/
  polygon/text graphics, standard pad shapes and project DRC rules are modeled
  semantically. DRC checks copper arcs, zone fills and keepouts and applies
  project `min_clearance`, `min_track_width` and `min_via_annular_width`.
- Custom pad primitives, dimensions, groups, images, targets, some setup fields,
  footprint graphics, properties and 3D models are retained in the source
  document and re-emitted. DRC uses a custom pad's bounding rectangle, and
  routing still uses each pad's rectangle.
- Pad numbers, net names, layer tables and track/via UUIDs are preserved where
  KiCad supplies them. Net UUIDs are derived from net names because KiCad's net
  table has no UUID. KiCad 9 integer net codes and KiCad 10 name-only net
  references such as `(net "GND")` are both accepted; exports follow the source
  board's version.
- KiCad board files have no board-level UUID. KaiCad's internal design id is
  kept in its JSON format and is not written as a top-level `(uuid ...)` node,
  which KiCad rejects as an unknown board token.
- Symbol libraries use the file stem as the library name because KiCad files do
  not record one. Pin positions remain in KiCad symbol space; no axis
  conversion is applied.
- Symbol and pin ids are UUIDv5 derivations, so re-parsing the same library
  yields the same ids. If ids differ, check that the file content changed.

Malformed S-expressions return `ParseError::Syntax`; unreadable files return
`ParseError::Io`; valid but unrepresentable data is warned about through
`PcbImport::warnings`. None of these paths panics.

The file layer has been validated with KiCad 10.0.6: `kicad-cli sch export svg`
and `kicad-cli fp export svg` plotted generated files, and `sch erc` / `pcb drc`
parsed the generated schematic and board. For boards, `pcb export svg`,
`pcb export gerbers` and `pcb export drill` accepted the Studio demo and a
KiCad-upgraded `20260206` board before and after a KaiCad re-export. All 19
installed template boards and all 19 template project hierarchies with root
schematics were imported, exported/bundled, reopened and plotted by KiCad
10.0.6. The five-fixture corpus plus the template pass is evidence for the
documented subset, not a byte-for-byte or third-party-project guarantee;
custom pad primitives and dimensions remain source-preserved.

### KiCad netlist export looks wrong

`to_kicad_netlist` is export only; KaiCad never reads a KiCad netlist back.
Known limits:

- `value` and `footprint` are always empty because a `Netlist` carries no
  component properties yet.
- Components use `Symbol::name` as their reference. Net codes start at 1 and
  follow sorted `(name, id)` order, so insertion order never changes the bytes.
- A net whose pins resolve to no known symbol pin is still emitted, without
  nodes, so the caller can see every net in the netlist.
- Quotes, backslashes and line breaks are escaped, the document ends with
  exactly one `\n`, and repeated calls on the same netlist are byte identical.

If the output changes between runs, check that no upstream code reintroduced
`HashMap` iteration order into symbol or net collections.

### DRC results look wrong

`pcb::run_drc` is pure and deterministic: the same design and rules always
produce the same report, sorted by `(rule_id, objects)`. If a result surprises
you, check these semantics before changing a rule:

- Clearance is measured **edge to edge** on shared copper layers:
  tracks use `segment_distance - width_a / 2 - width_b / 2`, via-track pairs
  subtract the via radius, pads use their shape-aware local outline offset by
  the footprint position and rotated by `PadPosition::rotation`, and copper
  arcs are sampled as short segments. Tracks on other layers, non-copper
  layers and the **same net** are skipped, because same-net copper is an
  intentional connection. Pad-to-track and pad-to-via clearance is skipped
  when the nets match, and track-to-via clearance is skipped when
  `via.net_id` matches the track's net. Pads in one footprint are not compared
  with each other. Zone fills are checked against foreign tracks, arcs, vias
  and pads; keepouts reject covered tracks, arcs, vias, pads, footprints and
  copper fills.
- Limits live on `PcbDesign::rules` and serialize with the design. Every limit
  must be finite and greater than zero; `DrcRules::validate` checks this and
  `run_drc` reports an invalid set as a `drc-rules` error, then checks with the
  defaults so the rest of the report stays useful.
- `track-outside-board` and `via-outside-board` are disabled when the outline
  has fewer than three points, so an unnamed or empty board does not report
  every object as outside.
- An empty stackup means layer order is unknown, not that no layer is valid: a
  via only needs two distinct, existing copper layers. With a non-empty
  stackup, a via must span the first to the last copper layer in stackup order.
- `track-width` reports a non-finite or `<= 0` width as an error and widths
  below `DrcRules::min_track_width` as a warning. `via-drill` reports a
  non-finite or non-positive drill or diameter, a drill that is not smaller
  than its diameter, or a drill below `DrcRules::min_via_drill` as an error;
  a ring below `DrcRules::min_annular_ring` is only a warning. The ring limit
  is independent of `min_track_width`.
- Non-finite coordinates are reported by `track-path`; the distance helpers
  return `f64::INFINITY` for them, and clearance ignores non-finite distances
  or widths rather than calling them clear.
- `pad-size` reports a non-finite or non-positive pad width/height, and
  `pad-outside-board` checks every modeled outline point. Through-hole pads
  occupy every copper layer in the stackup; surface-mount and tented pads
  occupy the top copper layer only. Pad copper defaults to 1 mm square and net
  `None`. `zone-clearance` and `zone-keepout` use polygon distances; keepout
  footprints are reported at the footprint placement.

If the report changes between identical runs, look for code that consumes
`HashMap` iteration order or track insertion order instead of the sorted
report.

### Plugin manifests look wrong

`plugin` is metadata-only. Check these rules before changing it:

- `PluginManifest::validate` rejects invalid ids, empty names/versions,
  unsupported API versions, empty or duplicate capabilities, and unsafe entry
  points (absolute paths, `..`, empty path components, drive prefixes or
  backslashes). It does not check whether the entry point exists.
- `PluginManifest::{from_json, to_json, save, load}` validate automatically.
  `from_json` returns `Json` for malformed input and the validation error for a
  well-formed but invalid manifest.
- `PluginRegistry::register` validates first, rejects duplicate ids, and keeps
  the registry sorted by id. A rejected manifest leaves the registry unchanged.
- `discover_manifests` reads only direct `*.json` regular files in file-name
  order, rejects duplicate ids and invalid files, and returns manifests sorted
  by `(id, entry_point)`. Missing directories and directories with no JSON
  manifests return `Io`.
- Discovery never opens, resolves, copies or executes an entry point.
- `store` persists the canonical manifest JSON in the global `plugins` table,
  validates it again on load, and keeps activation in a separate `enabled`
  column. Re-saving a manifest never resets its state.
- `plugins list` uses `list_plugin_states` (id order); `enable`/`disable`
  report a missing id as an error; `discover` imports all valid manifests but
  fails before opening the database if any manifest is invalid.

### Edit history looks wrong

`core::history::EditHistory` wraps `core::edit`; check these rules before
changing it:

- `apply` pushes onto undo only after the underlying edit succeeds, clears the
  redo stack, and evicts the oldest undo entry when the configured limit is
  exceeded. History starts at 100 by default; `new(0)` is rejected.
- `undo` and `redo` move a transaction between stacks only after the underlying
  edit succeeds. Empty operations return `EmptyUndo`/`EmptyRedo`.
- New edits always discard the redo branch. Undo and redo preserve their
  relative order (newest undo first, newest redo first).
- Labels and `can_undo`/`can_redo` are read-only views of the stack tops.
- History is in memory only; it is not serialized, persisted or shared across
  collaborators.

### Collaborative edits look wrong

Collaboration builds on `core::edit`; check these rules before changing it:

- `apply_edit` checks the session, participant and role before it touches the
  project. Viewers, unknown users and unknown sessions leave both the project
  and the session version unchanged.
- The session version increments only after `core::edit::apply` succeeds. A
  stale or failed edit returns `CollabError::Edit` and does not bump it.
- `record_transaction` stores the transaction id and a UUIDv5 digest of the
  compact project JSON. Identical JSON produces the same digest; an edit
  changes it. `record` keeps storing `None` for both fields.
- `SyncManager::stage` deduplicates by transaction id, and `drain` returns
  transactions in insertion order without applying them.
- No transport, merge or real-time sync exists; these APIs only prepare and
  describe local edits.

### Applied edits or undo look wrong

`core::edit` is strict by design: `apply` stages every change on a project
clone and only swaps it in when all changes succeed. Check these semantics
before changing it:

- Targets are `project` and `component:<reference_designator>`. Component
  fields are `value` (the property named `Value`), `property:<name>` and
  `reference_designator`; project fields are `name`, `version`, `author` and
  `description`. Text fields use JSON strings.
- `old_value` must match the current value exactly. A mismatch is a
  `StaleChange`, never an overwrite. Creating a missing property requires
  `old_value: null`; undo removes it again.
- Undo walks the recorded changes in reverse and redo walks them forward. Both
  verify the expected current value first and are atomic on failure.
- Transaction ids are UUIDv5 values over the label and ordered
  `(target, field, old_value, new_value)` tuples. Identical edits on identical
  starting projects must produce the same id.
- `kAI::apply_suggestion` validates before calling `core::edit::apply`, maps
  edit failures to `kAIError::ApplyFailed`, and never mutates on failure.
- Only component properties and project text fields are editable today. PCB
  objects, multi-step history and collaborative merge are not.

### Routing results look wrong

`pcb::routing::route` snaps both endpoints to an origin-anchored grid (half-grid
values round away from zero), runs four-neighbour A* and returns simplified
tracks; `route_multilayer` uses the same search and may insert through vias,
or blind/buried vias when `allow_blind_buried` is enabled.
Check these semantics before changing the router:

- The start and end nodes must be inside the outline and not blocked by
  copper on another net. A blocked endpoint is an `InvalidRequest`, not
  `NoPath`.
- Nodes and each moved segment are tested against tracks, vias and pads on the
  requested layer, using the design's clearance plus half of the route width.
  Same-net pads, tracks and vias are ignored, so a route can start on its own
  pad, existing trace or via and form a T-junction.
- Search ties are broken by `(f, g, y, x, direction, layer, vias)`, and track
  and via ids are UUIDv5 values over the request and path segment. Routing the
  same request twice must produce identical JSON.
- `NoPath` means the reachable search space was exhausted; `ExpansionLimit`
  means `max_expansions` was reached first. A larger limit can turn the latter
  into a path or into `NoPath`, never into a different successful path for the
  same limit.
- `route` always disables vias and keeps Week 7 behaviour. `route_multilayer`
  adds a via transition at the same snapped node, costs `via_cost` and moves to
  another layer in stackup order; `allow_vias: false` reproduces `route`.
- A candidate via must clear every copper layer it spans and satisfy
  `min_via_drill` and `min_annular_ring`. By default it is a through via from
  the first to the last stackup layer; with `allow_blind_buried: true` it spans
  only the two layers of the transition. `max_vias` caps transitions. Generated
  vias carry the requested net so their own tracks are not reported as
  clearance violations.
- Returned points are the snapped lattice coordinates, and collinear points are
  merged within each layer run. Tracks are split at every layer change, so each
  track is single-layer and at least two points long.

### Footprint libraries behave unexpectedly

Footprint libraries are KaiCad's own JSON, not KiCad `.kicad_mod` files;
standalone `.kicad_mod` import/export is a separate `kicad-parser` path.
Known limits:

- `FootprintLibrary::from_json` rejects duplicate footprint names with
  `FootprintLibraryError::Duplicate` rather than keeping the first or last
  copy. Rename the footprint or remove the duplicate.
- `to_json` and `save` always write two-space indentation and one trailing
  newline, so a saved file is byte-stable. Do not hand-edit it into compact
  JSON and expect `save` to preserve the layout.
- `save` creates missing parent directories; `load` reads the file as UTF-8
  and reports I/O and JSON errors through `FootprintLibraryError`.
- `sorted` orders by `(name, reference_designator)` for display only; the
  library keeps insertion order, so `to_json` output follows the vector.
- Pad copper is separate from the footprint outline: each `PadPosition` has
  `width`/`height`, a counter-clockwise `rotation` in degrees, an optional
  `net_id`, and is placed by `Footprint::position`. Outlines are documentation
  shapes and are not checked as copper. DRC and routing use the rotated copper
  rectangle, not an axis-aligned approximation.

### Build is slow the first time

Criterion and the Vulkan bindings pull in a large dependency tree. The first
`cargo check --workspace --all-targets` and `cargo bench --no-run` can take
minutes; later builds are incremental.

### Formatting check fails on a file you did not touch

`cargo fmt --all` formats every target in every crate. If a file fails the
check, format the workspace (or just your crate) rather than the individual
file, so line-length decisions stay consistent:

```bash
cargo fmt -p core
```

### Rust version is too old

If Cargo complains about `edition2024` or `resolver = "3"`, the compiler is
older than 1.85. Install rustup and let `rust-toolchain.toml` select `stable`.
