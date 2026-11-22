# Development Guide

Practical instructions for building, testing and hacking on KaiCad.

> KaiCad is a **pre-alpha scaffold**. Crates compile and the data model is
> exercised by tests, but most engine functions are stubs. See
> [`ARCHITECTURE.md`](ARCHITECTURE.md) for what actually exists.
>
> **Current state:** Weeks 1–8 are complete (205 tests, clippy/fmt/doc/release
> all clean). Week 9 — transactional kAI suggestion application — is next; its
> work order is in [`NEXT_AGENT_PROMPT.md`](NEXT_AGENT_PROMPT.md).

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
the renderer and future OS integration will need them. The current workspace
itself is pure Rust.

### macOS

```bash
brew install pkg-config openssl
```

Install the Vulkan SDK (which provides MoltenVK) from
<https://vulkan.lunarg.com/> if you intend to work on the renderer.

### Windows

- Visual Studio Build Tools with the "Desktop development with C++" workload.
- The Vulkan SDK from <https://vulkan.lunarg.com/> for renderer work.
- Rustup using the `stable-x86_64-pc-windows-msvc` toolchain.

### Vulkan runtime

`renderer` depends on `ash`, which loads the Vulkan loader (`libvulkan.so.1`,
`vulkan-1.dll`, `libvulkan.dylib`) **at runtime**. Building the workspace does
not require a Vulkan SDK; running renderer code eventually will. Install a
loader plus an ICD (Mesa, NVIDIA, AMD or MoltenVK) and verify with
`vulkaninfo --summary`.

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
| Print usage | `cargo run -p kai-cad -- --help` |
| Print the version | `cargo run -p kai-cad -- --version` |
| Build a release binary | `cargo build --workspace --release` |
| Run the store CRUD example | `cargo run -p store --example project_crud` |
| Parse a KiCad symbol library | `cargo run -p kicad-parser --example parse_library` |
| Check relative Markdown links | `python3 scripts/check_markdown_links.py` |
| Measure test coverage locally (after `cargo install cargo-llvm-cov`) | `cargo llvm-cov --workspace --all-targets --summary-only` |

Note that `--all-targets` is required to lint/check tests, examples and
benchmarks; a bare `cargo check` misses them.

## Tests

`cargo test --workspace` currently runs **205 tests**, all passing:

| Suite | Tests |
|-------|------:|
| `ai` (`src/lib.rs`) | 7 |
| `collab` (`src/lib.rs`) | 9 |
| `core` (`src/tests.rs` and inline modules) | 31 |
| `kicad-parser` (`src/lib.rs` and inline modules) | 12 |
| `pcb` (`src/lib.rs`, `drc.rs`, `routing.rs` and modules) | 72 |
| `renderer` (`src/lib.rs` and modules) | 5 |
| `schematic` (`src/lib.rs` and modules) | 33 |
| `store` (`src/tests.rs`) | 23 |
| `kai-cad/tests/smoke.rs` (cross-crate integration) | 13 |

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
  core: project JSON round trip ok (… bytes)
  schematic: 1 net(s), 1 connection(s), ERC valid=true, 0 violation(s), export 208 bytes
  pcb: outline 4 points, area 8000 mm^2, 2 layer(s), 2 copper layer(s), 1 footprint(s), DRC clean=true (1 violation(s) before fixing), routed 5 point(s), multi-layer 2 track(s)/2 via(s)
  renderer: window 1920x1080, vsync=true
  ai: endpoint=http://localhost:11434, deterministic validation=always
  collab: 1 session(s)
  store: created, loaded and deleted 1 project with its PCB design (schema v4)
  kicad-parser: loaded 1 symbol(s) from <temp path>.kicad_sym
self-check: ok
```

`--help`/`-h` prints usage and exits; `--version`/`-V` prints `kai-cad
<version>` and exits. Both return exit code 0 without running the self-check.

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

Weeks 1–8 are finished; Week 9 adds transactional kAI suggestion application
with deterministic undo/redo. Before starting, read
[`NEXT_AGENT_PROMPT.md`](NEXT_AGENT_PROMPT.md), which contains the API
requirements, acceptance criteria and verification commands. The existing JSON
persistence in `core::serialization` and the footprint-library format must keep
working while new persistence layers are added.

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

Symptoms include `vulkaninfo` failing, or (once the renderer is implemented)
`ash` failing to load `libvulkan.so.1` / `vulkan-1.dll` / `libvulkan.dylib`.
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

### KiCad symbol parsing looks wrong

`kicad-parser` reads only `.kicad_sym` symbol libraries, not `.kicad_sch`
schematics or `.kicad_pcb` boards. Known limits of the Week 3 import:

- The library name is the file stem; KiCad files do not record a library name.
- Pin positions are stored exactly as KiCad's symbol-space `(at x y angle)` in
  millimetres; no axis conversion is applied. `PinDirection` is derived from
  the electrical type (`input` → `In`, `output` → `Out`, `bidirectional` and
  `tri_state` → `InOut`, otherwise `Unspecified`).
- Every KiCad electrical type maps to a `PinType`, including `free` and
  `no_connect`. Only genuinely unknown non-standard types become
  `PinType::Unspecified`, so a pin is never silently dropped.
- Symbol and pin ids are UUIDv5 derivations, so re-parsing the same library
  yields the same ids. If ids differ, check that the file content changed.

Malformed S-expressions return `ParseError::Syntax`; unreadable files return
`ParseError::Io`. Neither path panics.

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
  subtract the via radius, and pads are axis-aligned copper rectangles offset
  by the footprint position. Tracks on other layers, non-copper layers and the
  **same net** are skipped, because same-net copper is an intentional
  connection. Pad-to-track and pad-to-via clearance is skipped when the nets
  match, and track-to-via clearance is skipped when `via.net_id` matches the
  track's net. Pads in one footprint are not compared with each other.
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
  `pad-outside-board` checks all four pad corners. Through-hole pads occupy
  every copper layer in the stackup; surface-mount and tented pads occupy the
  top copper layer only. Pad copper defaults to 1 mm square and net `None`.

If the report changes between identical runs, look for code that consumes
`HashMap` iteration order or track insertion order instead of the sorted
report.

### Routing results look wrong

`pcb::routing::route` snaps both endpoints to an origin-anchored grid (half-grid
values round away from zero), runs four-neighbour A* and returns simplified
tracks; `route_multilayer` uses the same search and may insert through vias.
Check these semantics before changing the router:

- The start and end nodes must be inside the outline and not blocked by
  existing copper. A blocked endpoint is an `InvalidRequest`, not `NoPath`.
- Nodes and each moved segment are tested against tracks, vias and pads on the
  requested layer, using the design's clearance plus half of the route width.
  Same-net pads are ignored so a route can connect to its own pad.
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
  `min_via_drill` and `min_annular_ring`. It is a through via from the first to
  the last stackup layer; `max_vias` caps transitions. Generated vias carry the
  requested net so their own tracks are not reported as clearance violations.
- Returned points are the snapped lattice coordinates, and collinear points are
  merged within each layer run. Tracks are split at every layer change, so each
  track is single-layer and at least two points long.

### Footprint libraries behave unexpectedly

Footprint libraries are KaiCad's own JSON, not KiCad `.kicad_mod` files.
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
  `width`/`height`, an optional `net_id`, and is placed by
  `Footprint::position`. Outlines are documentation shapes and are not checked
  as copper. There is no rotation support yet, so pads are axis-aligned.

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
