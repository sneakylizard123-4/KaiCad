-- KaiCad engineering database — plugin registry.
--
-- The canonical record is `manifest`, a complete pretty JSON serialization of
-- `plugin::PluginManifest`. Scalar columns and `capabilities` are queryable
-- projections rebuilt from the manifest on every save. Plugin registration is
-- global: rows are not tied to a project and do not cascade with projects.

CREATE TABLE IF NOT EXISTS plugins (
    id           TEXT PRIMARY KEY NOT NULL,
    name         TEXT NOT NULL,
    version      TEXT NOT NULL,
    api_version  INTEGER NOT NULL,
    entry_point  TEXT NOT NULL,
    capabilities TEXT NOT NULL,         -- compact JSON array
    manifest     TEXT NOT NULL          -- pretty JSON of the plugin manifest
);

CREATE INDEX IF NOT EXISTS plugins_name_idx ON plugins (name);
