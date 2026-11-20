-- KaiCad engineering database — initial schema.
--
-- The canonical record for every project is `projects.document`, a complete
-- JSON serialization of `core::Project`.  The additional tables below are
-- queryable projections that are rebuilt from the document inside the same
-- transaction on every create/update.  They must never be edited independently.

CREATE TABLE IF NOT EXISTS schema_migrations (
    version    INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL          -- RFC 3339 / ISO 8601 UTC
);

CREATE TABLE IF NOT EXISTS projects (
    id           TEXT PRIMARY KEY,    -- UUID string
    name         TEXT NOT NULL,
    version      TEXT NOT NULL,
    created_at   TEXT NOT NULL,
    modified_at  TEXT NOT NULL,
    author       TEXT NOT NULL,
    description  TEXT NOT NULL,
    document     TEXT NOT NULL        -- pretty JSON of the whole core::Project
);

CREATE INDEX IF NOT EXISTS projects_modified_at_idx ON projects (modified_at);

CREATE TABLE IF NOT EXISTS sheets (
    id         TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    data       TEXT NOT NULL          -- JSON of the core::Sheet
);

CREATE TABLE IF NOT EXISTS symbols (
    id         TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    library    TEXT NOT NULL,
    data       TEXT NOT NULL          -- JSON of the core::Symbol
);

CREATE TABLE IF NOT EXISTS components (
    id           TEXT PRIMARY KEY,
    project_id   TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    symbol_id    TEXT NOT NULL,
    footprint_id TEXT,
    data         TEXT NOT NULL        -- JSON of the core::Component
);

CREATE TABLE IF NOT EXISTS nets (
    id         TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    data       TEXT NOT NULL          -- JSON of the core::Net
);

CREATE TABLE IF NOT EXISTS footprints (
    id         TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    data       TEXT NOT NULL          -- JSON placeholder; bodies are owned by the pcb crate
);
