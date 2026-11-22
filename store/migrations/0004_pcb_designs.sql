-- KaiCad engineering database — PCB designs.
--
-- The canonical board for a project is `data`, a complete JSON serialization of
-- `pcb::PcbDesign` including tracks, vias, placed footprints and DRC rules.
-- The row is keyed by project and removed with the project.

CREATE TABLE IF NOT EXISTS pcb_designs (
    project_id TEXT PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
    data       TEXT NOT NULL,         -- pretty JSON of the pcb::PcbDesign
    updated_at TEXT NOT NULL          -- RFC 3339 / ISO 8601 UTC
);
