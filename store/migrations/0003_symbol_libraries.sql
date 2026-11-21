-- KaiCad engineering database — symbol libraries.
--
-- A symbol library is a named collection of symbols independent of any one
-- project. The canonical record is `data`, a complete JSON serialization of
-- `core::SymbolLibrary`; `name` and `source` are queryable projections.

CREATE TABLE IF NOT EXISTS symbol_libraries (
    id     TEXT PRIMARY KEY,          -- UUID string
    name   TEXT NOT NULL,
    source TEXT NOT NULL,             -- "Internal" or "KiCad:{path}"
    data   TEXT NOT NULL              -- pretty JSON of the core::SymbolLibrary
);

CREATE INDEX IF NOT EXISTS symbol_libraries_name_idx ON symbol_libraries (name);
