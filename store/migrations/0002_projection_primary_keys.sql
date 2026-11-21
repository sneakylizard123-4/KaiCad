-- KaiCad engineering database — scope projection keys per project.
--
-- Entity ids are only unique within a project: duplicating a project (for
-- example "Save As") keeps the symbol, sheet, net, component and footprint ids
-- under a new project id. The original schema keyed each projection table by
-- `id` alone, so the copy failed with `UNIQUE constraint failed: symbols.id`.
--
-- Rebuild every projection table with a composite primary key of
-- (project_id, id) and copy the existing rows across. The tables remain
-- queryable projections of `projects.document`; this migration does not change
-- their contents.

CREATE TABLE sheets_v2 (
    id         TEXT NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    data       TEXT NOT NULL,         -- JSON of the core::Sheet
    PRIMARY KEY (project_id, id)
);

INSERT INTO sheets_v2 (id, project_id, name, data)
    SELECT id, project_id, name, data FROM sheets;
DROP TABLE sheets;
ALTER TABLE sheets_v2 RENAME TO sheets;

CREATE TABLE symbols_v2 (
    id         TEXT NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    library    TEXT NOT NULL,
    data       TEXT NOT NULL,         -- JSON of the core::Symbol
    PRIMARY KEY (project_id, id)
);

INSERT INTO symbols_v2 (id, project_id, name, library, data)
    SELECT id, project_id, name, library, data FROM symbols;
DROP TABLE symbols;
ALTER TABLE symbols_v2 RENAME TO symbols;

CREATE TABLE components_v2 (
    id           TEXT NOT NULL,
    project_id   TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    symbol_id    TEXT NOT NULL,
    footprint_id TEXT,
    data         TEXT NOT NULL,       -- JSON of the core::Component
    PRIMARY KEY (project_id, id)
);

INSERT INTO components_v2 (id, project_id, symbol_id, footprint_id, data)
    SELECT id, project_id, symbol_id, footprint_id, data FROM components;
DROP TABLE components;
ALTER TABLE components_v2 RENAME TO components;

CREATE TABLE nets_v2 (
    id         TEXT NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    data       TEXT NOT NULL,         -- JSON of the core::Net
    PRIMARY KEY (project_id, id)
);

INSERT INTO nets_v2 (id, project_id, name, data)
    SELECT id, project_id, name, data FROM nets;
DROP TABLE nets;
ALTER TABLE nets_v2 RENAME TO nets;

CREATE TABLE footprints_v2 (
    id         TEXT NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    data       TEXT NOT NULL,         -- JSON placeholder; bodies are owned by the pcb crate
    PRIMARY KEY (project_id, id)
);

INSERT INTO footprints_v2 (id, project_id, data)
    SELECT id, project_id, data FROM footprints;
DROP TABLE footprints;
ALTER TABLE footprints_v2 RENAME TO footprints;
