# KaiCad

**KaiCad** is a complete, high-performance, open-source electronic design automation (EDA) application written in Rust.

## Status
- **Architecture:** Decided
- **Implementation:** Pending (pre-alpha)
- **Target:** Rust, Linux (primary), Windows, macOS

## Features
- Custom GUI
- Vulkan-based rendering engine
- Native KiCad 9/10 bidirectional compatibility
- Full schematic and PCB design capabilities
- Integrated design-rule and electrical-rule checking
- Advanced PCB routing and engineering analysis
- Built-in AI engineering assistance (kAI)
- Collaborative project workflows
- Extensible plugin and scripting architecture

## Getting Started

### Prerequisites
- Rust toolchain (stable channel)
- Git
- Vulkan SDK (for rendering)
- Dependencies for your OS:
  - Ubuntu/Debian: `sudo apt-get install pkg-config libssl-dev libudev-dev`
  - Fedora: `sudo dnf install pkgconf-pkg-config openssl-devel systemd-devel`
  - macOS: `brew install pkg-config openssl`

### Building
```bash
# Clone the repository
git clone https://github.com/yourusername/KaiCad.git
cd KaiCad

# Build the workspace
cargo build --workspace

# Run tests
cargo test --workspace

# Run the basic application (currently just outputs version info)
cargo run --bin kai-cad
```

### Development
- Follow the conventional Rust workflow with `cargo check`, `cargo test`, and `cargo fmt`
- All code should pass `cargo clippy` with default settings
- Unit tests are located in `src/tests.rs` files within each crate
- Integration tests can be added to the `tests/` directory at the workspace root

### Current State
This is a pre-alpha implementation focused on establishing the foundational architecture:
- ✅ Workspace structure with 6 crates (core, schematic, pcb, renderer, ai, collab)
- ✅ Successful compilation with `cargo check --workspace`
- ✅ Basic unit tests for core data structures
- ✅ CI/CD pipeline configured via GitHub Actions
- 🚧 Schematic and PCB engines in early development
- 🚧 Vulkan renderer skeleton implemented
- 🚧 kAI and collaboration subsystems outlined

## License
GPL-3.0-or-later