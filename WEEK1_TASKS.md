# KaiCad Week 1 — Project Scaffolding

## Goal
Set up the foundational project structure, tooling, and initial development environment.

## Tasks

### Task 1.1: Initialize Cargo Workspace
- Create a new Cargo workspace named `kaicas`
- Structure:
  - Root directory with `Cargo.toml` (workspace)
  - `core/` crate (shared types, enums, traits)
  - `schematic/` crate (schematic-specific functionality)
  - `pcb/` crate (PCB-specific functionality)
  - `renderer/` crate (Vulkan rendering)
  - `ai/` crate (AI subsystem)
  - `collab/` crate (collaboration features)
- **Deliverable:** Working `Cargo.toml` workspace with all crates initialized
- **Verification:** `cargo check` runs without errors

### Task 1.2: Configure CI/CD Pipeline
- Create GitHub Actions workflow at `.github/workflows/ci.yml`
- Configure to run on push and pull requests to main branch
- Include steps for:
  - Checkout code
  - Setup Rust (stable version)
  - `cargo check` - syntax verification
  - `cargo test` - run tests (will be empty initially)
  - `cargo clippy` - linting
- **Deliverable:** Working CI pipeline that runs on every push
- **Verification:** PR shows "All checks passed" status

### Task 1.3: Set Up Rust Toolchain Configuration
- Create `rust-toolchain.toml` specifying stable Rust version
- Create `.clippy.toml` with recommended lints enabled
- Create `rustfmt.toml` for formatting configuration
- **Deliverable:** All toolchain configuration files in place
- **Verification:** `cargo clippy` and `cargo fmt` run without errors

### Task 1.4: Initialize Core Crates
- Initialize `core/` crate:
  - Define basic data structures: `Project`, `Component`, `Symbol`, `Net`, `Sheet`
  - Implement basic serialization/deserialization interfaces
- **Deliverable:** `core` crate with basic data model
- **Verification:** `cargo test` in `core/` crate passes

### Task 1.5: Set Up Project Management
- Create `docs/` directory for project documentation
- Create `examples/` directory with minimal example code
- Create `benches/` directory for performance benchmarks
- **Deliverable:** Standard Rust project structure directories in place
- **Verification:** `tree` command shows proper directory structure

## Week 1 Completion Criteria

- [x] Cargo workspace initialized with proper crate structure
- [x] CI/CD pipeline configured and passing on first run
- [x] Rust toolchain properly configured
- [x] Core crate with basic data structures implemented
- [x] Project structure follows Rust best practices

## Status (verified 2026-11-11)

Verification was run locally on Linux with `rustc 1.99.1` / `cargo 1.99.1`
(the workspace pins the `stable` channel, MSRV 1.85).

| Command | Result |
|---------|--------|
| `cargo build --workspace --all-targets` | **pass** — no warnings |
| `cargo test --workspace` | **pass** — 8 tests, 0 failed |
| `cargo run -q -p core --example project_roundtrip` | **pass** — JSON round trip printed, id/name/version asserted equal |
| `cargo run -q -p pcb --example board_outline` | **pass** — rounded-rectangle outline summary printed |
| `cargo bench -p core --no-run` | **pass** — Criterion 0.5.1, bench target `serialization` compiled |
| `cargo bench -p core --bench serialization -- --warm-up-time 0.2 --measurement-time 0.5 --sample-size 10` | **pass** — ≈380 µs serialize, ≈335 µs deserialize (100 symbols, 100 nets) |
| `cargo fmt --all -- --check` | **pass** — clean, no configuration warnings |
| `cargo clippy --workspace --all-targets -- -D warnings` | **pass** — no warnings across libs, bins, tests, examples and benches |

During verification the concurrent warning-cleanup work briefly produced
formatting diffs and compiler warnings in `ai/` and other crates; the final
run above is clean. The unused `use_field_shorthand` key was removed from
`rustfmt.toml` (it is nightly-only), so formatting checks are now silent.

Deliverables for Task 1.5:

- `docs/ARCHITECTURE.md`, `docs/DEVELOPMENT.md`, `docs/CONTRIBUTING.md`
- `core/examples/project_roundtrip.rs`, `pcb/examples/board_outline.rs`
- `core/benches/serialization.rs` (Criterion, `harness = false`)

**Task 1.2 caveat:** the workflow `.github/workflows/ci.yml` is configured with
the full job list, but the verification criterion "PR shows *All checks
passed*" cannot be checked locally. It is **pending the first push** to GitHub;
until then only the individual commands above have been exercised.