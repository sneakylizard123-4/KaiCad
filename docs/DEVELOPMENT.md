# Development Guide

Practical instructions for building, testing and hacking on KaiCad.

> KaiCad is a **pre-alpha scaffold**. Crates compile and the data model is
> exercised by tests, but most engine functions are stubs. See
> [`ARCHITECTURE.md`](ARCHITECTURE.md) for what actually exists.

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
| root `Cargo.toml` | `resolver = "3"`, workspace lints, dev/release profiles |

**MSRV:** edition 2024 requires Rust **1.85** or newer; `clippy.toml` records
`msrv = "1.85"` so lints do not suggest newer APIs. Distro-packaged Rust is
often too old — use rustup.

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
| Run the application | `cargo run -p kai-cad` |
| Build a release binary | `cargo build --workspace --release` |

Note that `--all-targets` is required to lint/check tests, examples and
benchmarks; a bare `cargo check` misses them.

## Examples

Examples live in each package under `<crate>/examples/`. The workspace root is
a virtual manifest (no `[package]`), so a root-level `examples/` directory
would **not** be built by Cargo.

```bash
# Build a Project, serialize it to pretty JSON, deserialize it and verify it
cargo run -p core --example project_roundtrip

# Construct an OutlineShape and print a summary
cargo run -p pcb --example board_outline
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
`main`. It is a single `Check` job on `ubuntu-latest`:

1. `actions/checkout@v4`
2. `dtolnay/rust-toolchain@stable` with `rustfmt, clippy`
3. `Swatinem/rust-cache@v2`
4. `cargo fmt --all -- --check`
5. `cargo check --workspace --all-targets`
6. `cargo clippy --workspace --all-targets -- -D warnings`
7. `cargo test --workspace`
8. `cargo build --workspace --release`

`CARGO_TERM_COLOR` is set to `always` for readable logs. `-D warnings` is
passed to Clippy only, not through a global `RUSTFLAGS`, because global flags
also apply to dependencies and build scripts where warnings are not ours to
fix.

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
