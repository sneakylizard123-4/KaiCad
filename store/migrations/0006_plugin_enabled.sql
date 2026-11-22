-- KaiCad engineering database — plugin activation state.
--
-- Activation is registry metadata, not part of the manifest. It defaults to
-- enabled and survives manifest upserts. The CHECK constraint keeps external
-- writes to the same two states the API exposes.

ALTER TABLE plugins ADD COLUMN enabled INTEGER NOT NULL DEFAULT 1
    CHECK (enabled IN (0, 1));

CREATE INDEX IF NOT EXISTS plugins_enabled_idx ON plugins (enabled);
