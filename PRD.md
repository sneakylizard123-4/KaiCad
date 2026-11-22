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
**Rendering:** Vulkan target; deterministic software 2D/3D renderer implemented\
**Status:** Architecture decisions finalized; Phase 1, Weeks 9–15, Phase 2 functional scope and the first KaiCad Studio GUI shell implemented; pre-alpha

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
- Mechanical layers and documentation layers.
- Mounting holes and board cutouts.
- 3D component models and enclosure export.
- Manufacturing outputs: Gerber, drill files, pick-and-place and BOM.

## 3.2 Document scope

This revision covers the product vision, target users and the functional scope
that every later design decision must serve. Detailed subsystem design,
milestones and task breakdowns are maintained separately:

- [`docs/PLAN.md`](docs/PLAN.md) — architecture, 16-week roadmap and success criteria.
- [`docs/PHASE1_TASKS.md`](docs/PHASE1_TASKS.md) — Weeks 1–4 task breakdown and status.
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — crate graph, data model and
  design rules as implemented.
- [`docs/NEXT_AGENT_PROMPT.md`](docs/NEXT_AGENT_PROMPT.md) — the current
  implementation brief.

Implementation status: Weeks 1–15 and the Phase 2 PCB/renderer scope are
complete — scaffolding, the shared data model, the SQLite engineering database,
symbol management, the schematic engine, PCB geometry and DRC, rotated pads,
deterministic single-layer and multi-layer routing with through/blind/buried
vias, push-and-shove and interactive routing, PCB-design persistence, a
headless software 2D/3D renderer, atomic kAI suggestion application with
undo/redo, permission-checked collaborative edit sessions with version history
and sync staging, bounded multi-step edit history, and a validated local
plugin registry with SQLite persistence, activation state and CLI management.
A native eframe-based Studio shell now opens and saves board documents, renders
2D/3D views and reports DRC, stackup and footprint information. The Vulkan GPU
renderer, full GUI editing and a usable end-to-end editing workflow remain
deferred. Week 16 (beta readiness) is next. KaiCad is pre-alpha.
