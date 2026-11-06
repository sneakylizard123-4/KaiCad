# KaiCad — Master Product Requirements Document

**Project:** KaiCad  
**Codename:** KaiCad  
**AI subsystem:** kAI  
**Document type:** Master PRD + Technical Implementation Specification  
**Target implementation:** Rust  
**License:** GPL-3.0-or-later  
**Primary platform:** Linux  
**Additional platforms:** Windows and macOS  
**Primary EDA compatibility:** KiCad 10  
**Secondary EDA compatibility:** KiCad 9  
**Rendering:** Vulkan  
**Status:** Architecture decisions finalized; implementation pending

---

# 1. Product vision

## 1.1 Overview

KaiCad is a complete, high-performance, open-source electronic design automation (EDA) application written primarily in Rust.

Its objective is to provide a modern alternative to KiCad with:

- A completely custom GUI.
- A high-performance Vulkan-based rendering engine.
- Native, bidirectional KiCad 9 and KiCad 10 project compatibility.
- Full schematic and PCB design capabilities.
- Integrated design-rule and electrical-rule checking.
- Advanced PCB routing and engineering analysis.
- Built-in AI engineering assistance through kAI.
- Collaborative project workflows.
- An extensible plugin and scripting architecture.
- A robust native project model and persistence system.

KaiCad must not be a superficial KiCad frontend or a wrapper around the KiCad application.

It must implement its own engineering database, geometry engine, schematic engine, PCB engine, renderer, user interface and AI subsystem.

## 1.2 Core philosophy

**Performance without sacrificing correctness.**

**Compatibility without requiring KiCad internally.**

**AI assistance without trusting AI-generated engineering results blindly.**

**Collaboration without requiring a proprietary cloud service.**

**A complete EDA application that remains useful without AI, network connectivity or external services.**

## 1.3 Product principles

1. Rust-first architecture.
2. Deterministic engineering operations.
3. Native KiCad interoperability.
4. Local-first project ownership.
5. GPU-accelerated rendering.
6. Incremental computation.
7. Transactional editing.
8. Explicit data preservation.
9. Modular and testable subsystems.
10. No silent data loss.
11. No mandatory cloud dependencies.
12. AI-generated changes must undergo deterministic validation.

---

# 2. Target users

### Primary users

- Electronics hobbyists.
- PCB designers.
- Embedded systems engineers.
- Hardware developers.
- Open-source hardware developers.
- Students and researchers.

### Secondary users

- Professional electrical engineers.
- Hardware development teams.
- PCB manufacturing engineers.
- AI-assisted hardware design researchers.

### Main use cases

1. Design a complete electronic circuit from scratch.
2. Import and continue editing an existing KiCad project.
3. Design complex multi-layer PCBs.
4. Review and debug schematics.
5. Generate and optimize circuits using kAI.
6. Collaborate with other KaiCad users.
7. Exchange projects with regular KiCad users.
8. Produce manufacturing-ready outputs.

---

# 3. Scope and requirements

## 3.1 Full EDA functionality

KaiCad must ultimately support:

### Schematic design

- Multiple schematic sheets.
- Hierarchical designs.
- Symbols and symbol libraries.
- Wires, junctions and buses.
- Global, local and hierarchical labels.
- Power symbols.
- Net naming.
- Pin configuration.
- Component properties.
- Electrical rule checking.
- Cross-probing between schematic and PCB.
- Sheet navigation.
- Symbol editing.
- Custom symbol libraries.

### PCB design

- Arbitrary board outlines.
- Multi-layer PCB designs.
- Custom stackups.
- Footprints and footprint libraries.
- Pads, tracks, vias and copper zones.
- Through-hole, SMD and mixed-technology boards.
- Keepouts.
- Copper clearances.
- Differential pairs.
- Length matching.
- Net classes.
- Impedance constraints.
- Push-and-shove routing.
- Interactive routing.
- Ratsnest visualization.
- Board annotations.
- PCB measurements.
- 3D board visualization.
- Mechanical