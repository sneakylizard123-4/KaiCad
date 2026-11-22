//! KaiCad engineering database — SQLite persistence and project CRUD.
//!
//! The `store` crate provides a synchronous, local SQLite store for
//! `core::Project` documents.  The canonical record for every project is the
//! `projects.document` column, which stores a complete pretty-printed JSON
//! serialization of the project.  All other entity tables (`sheets`,
//! `symbols`, `components`, `nets`, `footprints`) are queryable projections
//! rebuilt from the document inside the same transaction on every
//! [`Store::create_project`] and [`Store::update_project`].  They must never be
//! edited independently of the authoritative document.

use std::fmt;
use std::path::Path;

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, Transaction};

#[cfg(test)]
mod tests;

/// Highest migration version shipped by this crate.
const CURRENT_SCHEMA_VERSION: u32 = 4;

/// Migrations are numbered SQL files embedded with `include_str!`.  Never edit
/// an applied migration; add a new numbered entry at the end.
const MIGRATIONS: &[(u32, &str)] = &[
    (1, include_str!("../migrations/0001_initial.sql")),
    (2, include_str!("../migrations/0002_projection_primary_keys.sql")),
    (3, include_str!("../migrations/0003_symbol_libraries.sql")),
    (4, include_str!("../migrations/0004_pcb_designs.sql")),
];

/// A synchronous SQLite-backed engineering database.
#[derive(Debug)]
pub struct Store {
    connection: Connection,
}

/// Summary row returned by [`Store::list_projects`].
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectSummary {
    pub id: uuid::Uuid,
    pub name: String,
    pub version: String,
    pub modified_at: DateTime<Utc>,
}

/// Errors that can occur while using a [`Store`].
#[derive(Debug)]
pub enum StoreError {
    /// An underlying SQLite error.
    Sqlite(rusqlite::Error),
    /// A JSON encoding or decoding error.
    Json(serde_json::Error),
    /// An I/O error while creating the database directory or file.
    Io(std::io::Error),
    /// A project with the same id already exists.
    Duplicate(uuid::Uuid),
    /// A PCB design was saved for a project that does not exist.
    MissingProject(uuid::Uuid),
    /// The migration runner failed.
    MigrationFailed(String),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sqlite(error) => write!(f, "sqlite error: {error}"),
            Self::Json(error) => write!(f, "json error: {error}"),
            Self::Io(error) => write!(f, "io error: {error}"),
            Self::Duplicate(id) => write!(f, "project already exists: {id}"),
            Self::MissingProject(id) => write!(f, "project does not exist: {id}"),
            Self::MigrationFailed(message) => write!(f, "migration failed: {message}"),
        }
    }
}

impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Sqlite(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Io(error) => Some(error),
            Self::Duplicate(_) | Self::MissingProject(_) | Self::MigrationFailed(_) => None,
        }
    }
}

impl From<rusqlite::Error> for StoreError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error)
    }
}

impl From<serde_json::Error> for StoreError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<std::io::Error> for StoreError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

fn map_serialization_error(error: core::serialization::Error) -> StoreError {
    match error {
        core::serialization::Error::Json(json_error) => StoreError::Json(json_error),
        core::serialization::Error::Io(io_error) => StoreError::Io(io_error),
    }
}

/// Result type returned by [`Store`] operations.
pub type Result<T> = std::result::Result<T, StoreError>;

impl Store {
    /// Open (creating if needed) a database file, then run pending migrations.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let connection = Connection::open(path)?;
        Self::configure(&connection)?;
        let mut store = Self { connection };
        store.run_migrations()?;
        Ok(store)
    }

    /// Open a private in-memory database and run pending migrations.
    pub fn open_in_memory() -> Result<Self> {
        let connection = Connection::open_in_memory()?;
        Self::configure(&connection)?;
        let mut store = Self { connection };
        store.run_migrations()?;
        Ok(store)
    }

    /// Highest applied migration version (0 when none).
    pub fn schema_version(&self) -> Result<u32> {
        let version: Option<u32> = self
            .connection
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| row.get(0))
            .optional()
            .map_err(StoreError::Sqlite)?
            .flatten();
        Ok(version.unwrap_or(0))
    }

    /// Persist a new project and build its projection rows in one transaction.
    ///
    /// Returns [`StoreError::Duplicate`] if a project with the same id already
    /// exists.
    pub fn create_project(&mut self, project: &core::Project) -> Result<()> {
        let document =
            core::serialization::to_json_pretty(project).map_err(map_serialization_error)?;
        let tx = self.connection.transaction()?;

        if let Err(error) = insert_project_row(&tx, project, &document) {
            return Err(map_constraint_violation(error, project.id));
        }

        insert_projection_rows(&tx, project)?;
        tx.commit()?;
        Ok(())
    }

    /// Load the authoritative project document for `id`.
    ///
    /// Returns `Ok(None)` when no project with that id exists.
    pub fn load_project(&self, id: uuid::Uuid) -> Result<Option<core::Project>> {
        let mut statement =
            self.connection.prepare("SELECT document FROM projects WHERE id = ?1")?;
        let mut rows = statement.query([id.to_string()])?;

        match rows.next()? {
            Some(row) => {
                let document: String = row.get(0)?;
                let project =
                    core::serialization::from_json(&document).map_err(map_serialization_error)?;
                Ok(Some(project))
            }
            None => Ok(None),
        }
    }

    /// List all projects, ordered by `modified_at DESC, name ASC`.
    pub fn list_projects(&self) -> Result<Vec<ProjectSummary>> {
        let mut statement = self.connection.prepare(
            "SELECT id, name, version, modified_at FROM projects ORDER BY modified_at DESC, name ASC",
        )?;

        let rows = statement.query_map([], |row| {
            let id_text: String = row.get(0)?;
            let id = uuid::Uuid::parse_str(&id_text).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
            let modified_at_text: String = row.get(3)?;
            let modified_at = DateTime::parse_from_rfc3339(&modified_at_text)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        3,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?;

            Ok(ProjectSummary { id, name: row.get(1)?, version: row.get(2)?, modified_at })
        })?;

        let mut summaries = Vec::new();
        for row in rows {
            summaries.push(row?);
        }
        Ok(summaries)
    }

    /// Update an existing project; returns `false` if the id does not exist.
    ///
    /// The document and all projection rows are replaced inside one transaction.
    pub fn update_project(&mut self, project: &core::Project) -> Result<bool> {
        let document =
            core::serialization::to_json_pretty(project).map_err(map_serialization_error)?;
        let tx = self.connection.transaction()?;

        let rows_changed = update_project_row(&tx, project, &document)?;
        if rows_changed == 0 {
            return Ok(false);
        }

        delete_projection_rows(&tx, project.id)?;
        insert_projection_rows(&tx, project)?;
        tx.commit()?;
        Ok(true)
    }

    /// Delete a project; returns `false` if the id does not exist.
    ///
    /// Projection rows are removed by `ON DELETE CASCADE`.
    pub fn delete_project(&mut self, id: uuid::Uuid) -> Result<bool> {
        let rows_changed =
            self.connection.execute("DELETE FROM projects WHERE id = ?1", [id.to_string()])?;
        Ok(rows_changed > 0)
    }

    /// Persist a new symbol library.
    ///
    /// Returns [`StoreError::Duplicate`] if a library with the same id exists.
    pub fn create_symbol_library(&mut self, library: &core::SymbolLibrary) -> Result<()> {
        let data = serde_json::to_string_pretty(library)?;
        let result = self.connection.execute(
            "INSERT INTO symbol_libraries (id, name, source, data) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                library.id.to_string(),
                library.name,
                symbol_library_source_text(&library.source),
                data,
            ],
        );

        match result {
            Ok(_) => Ok(()),
            Err(error) => Err(map_constraint_violation(error, library.id)),
        }
    }

    /// Load a symbol library by id; returns `Ok(None)` when it does not exist.
    pub fn load_symbol_library(&self, id: uuid::Uuid) -> Result<Option<core::SymbolLibrary>> {
        let mut statement =
            self.connection.prepare("SELECT data FROM symbol_libraries WHERE id = ?1")?;
        let mut rows = statement.query([id.to_string()])?;

        match rows.next()? {
            Some(row) => {
                let data: String = row.get(0)?;
                Ok(Some(serde_json::from_str(&data)?))
            }
            None => Ok(None),
        }
    }

    /// List symbol libraries as `(id, name)` pairs, ordered by name ASC.
    pub fn list_symbol_libraries(&self) -> Result<Vec<(uuid::Uuid, String)>> {
        let mut statement = self
            .connection
            .prepare("SELECT id, name FROM symbol_libraries ORDER BY name ASC, id ASC")?;

        let rows = statement.query_map([], |row| {
            let id_text: String = row.get(0)?;
            let id = uuid::Uuid::parse_str(&id_text).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
            Ok((id, row.get(1)?))
        })?;

        let mut libraries = Vec::new();
        for row in rows {
            libraries.push(row?);
        }
        Ok(libraries)
    }

    /// Update an existing symbol library; returns `false` if the id is absent.
    pub fn update_symbol_library(&mut self, library: &core::SymbolLibrary) -> Result<bool> {
        let data = serde_json::to_string_pretty(library)?;
        let rows_changed = self.connection.execute(
            "UPDATE symbol_libraries SET name = ?2, source = ?3, data = ?4 WHERE id = ?1",
            rusqlite::params![
                library.id.to_string(),
                library.name,
                symbol_library_source_text(&library.source),
                data,
            ],
        )?;
        Ok(rows_changed > 0)
    }

    /// Delete a symbol library; returns `false` if the id is absent.
    pub fn delete_symbol_library(&mut self, id: uuid::Uuid) -> Result<bool> {
        let rows_changed = self
            .connection
            .execute("DELETE FROM symbol_libraries WHERE id = ?1", [id.to_string()])?;
        Ok(rows_changed > 0)
    }

    /// Insert or replace the PCB design for `project_id`.
    ///
    /// Returns [`StoreError::MissingProject`] when the project does not exist.
    pub fn save_pcb_design(
        &mut self,
        project_id: uuid::Uuid,
        design: &pcb::PcbDesign,
    ) -> Result<()> {
        let project = project_id.to_string();
        let exists = self
            .connection
            .query_row("SELECT 1 FROM projects WHERE id = ?1", [&project], |_| Ok(()))
            .optional()?;
        if exists.is_none() {
            return Err(StoreError::MissingProject(project_id));
        }

        let data = serde_json::to_string_pretty(design)?;
        self.connection.execute(
            "INSERT INTO pcb_designs (project_id, data, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(project_id) DO UPDATE
             SET data = excluded.data, updated_at = excluded.updated_at",
            rusqlite::params![project, data, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    /// Load the PCB design for `project_id`; `Ok(None)` when none is stored.
    pub fn load_pcb_design(&self, project_id: uuid::Uuid) -> Result<Option<pcb::PcbDesign>> {
        let mut statement =
            self.connection.prepare("SELECT data FROM pcb_designs WHERE project_id = ?1")?;
        let mut rows = statement.query([project_id.to_string()])?;

        match rows.next()? {
            Some(row) => {
                let data: String = row.get(0)?;
                Ok(Some(serde_json::from_str(&data)?))
            }
            None => Ok(None),
        }
    }

    /// Delete a project's PCB design; returns `false` when none was stored.
    pub fn delete_pcb_design(&mut self, project_id: uuid::Uuid) -> Result<bool> {
        let rows_changed = self
            .connection
            .execute("DELETE FROM pcb_designs WHERE project_id = ?1", [project_id.to_string()])?;
        Ok(rows_changed > 0)
    }

    fn configure(connection: &Connection) -> Result<()> {
        connection.execute_batch("PRAGMA foreign_keys = ON;")?;
        Ok(())
    }

    fn run_migrations(&mut self) -> Result<()> {
        self.connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version    INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL
            );",
        )?;

        let applied: std::collections::HashSet<u32> = {
            let mut statement = self.connection.prepare("SELECT version FROM schema_migrations")?;
            let rows = statement.query_map([], |row| row.get::<_, u32>(0))?;
            rows.collect::<std::result::Result<_, _>>()?
        };

        for (version, sql) in MIGRATIONS {
            if applied.contains(version) {
                continue;
            }

            let tx = self.connection.transaction()?;
            tx.execute_batch(sql).map_err(|error| {
                StoreError::MigrationFailed(format!("migration {version} failed: {error}"))
            })?;
            tx.execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
                rusqlite::params![version, Utc::now().to_rfc3339()],
            )
            .map_err(|error| {
                StoreError::MigrationFailed(format!("migration {version} record failed: {error}"))
            })?;
            tx.commit().map_err(|error| {
                StoreError::MigrationFailed(format!("migration {version} commit failed: {error}"))
            })?;
        }

        if self.schema_version()? != CURRENT_SCHEMA_VERSION {
            return Err(StoreError::MigrationFailed(format!(
                "schema version mismatch: expected {CURRENT_SCHEMA_VERSION}, got {}",
                self.schema_version()?
            )));
        }

        Ok(())
    }
}

fn symbol_library_source_text(source: &core::SymbolLibrarySource) -> String {
    match source {
        core::SymbolLibrarySource::Internal => "Internal".to_string(),
        core::SymbolLibrarySource::KiCad { path } => format!("KiCad:{path}"),
    }
}

fn map_constraint_violation(error: rusqlite::Error, id: uuid::Uuid) -> StoreError {
    match &error {
        rusqlite::Error::SqliteFailure(sqlite_error, _) => {
            const SQLITE_CONSTRAINT_PRIMARYKEY: i32 = 1555;
            if sqlite_error.extended_code == SQLITE_CONSTRAINT_PRIMARYKEY {
                return StoreError::Duplicate(id);
            }
            StoreError::Sqlite(error)
        }
        _ => StoreError::Sqlite(error),
    }
}

fn insert_project_row(
    tx: &Transaction<'_>,
    project: &core::Project,
    document: &str,
) -> std::result::Result<usize, rusqlite::Error> {
    tx.execute(
        "INSERT INTO projects (id, name, version, created_at, modified_at, author, description, document)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            project.id.to_string(),
            project.name,
            project.version,
            project.metadata.created_at.to_rfc3339(),
            project.metadata.modified_at.to_rfc3339(),
            project.metadata.author,
            project.metadata.description,
            document,
        ],
    )
}

fn update_project_row(
    tx: &Transaction<'_>,
    project: &core::Project,
    document: &str,
) -> std::result::Result<usize, rusqlite::Error> {
    tx.execute(
        "UPDATE projects
         SET name = ?2, version = ?3, created_at = ?4, modified_at = ?5,
             author = ?6, description = ?7, document = ?8
         WHERE id = ?1",
        rusqlite::params![
            project.id.to_string(),
            project.name,
            project.version,
            project.metadata.created_at.to_rfc3339(),
            project.metadata.modified_at.to_rfc3339(),
            project.metadata.author,
            project.metadata.description,
            document,
        ],
    )
}

fn delete_projection_rows(tx: &Transaction<'_>, project_id: uuid::Uuid) -> Result<()> {
    let id = project_id.to_string();
    tx.execute("DELETE FROM sheets WHERE project_id = ?1", [&id])?;
    tx.execute("DELETE FROM symbols WHERE project_id = ?1", [&id])?;
    tx.execute("DELETE FROM components WHERE project_id = ?1", [&id])?;
    tx.execute("DELETE FROM nets WHERE project_id = ?1", [&id])?;
    tx.execute("DELETE FROM footprints WHERE project_id = ?1", [&id])?;
    Ok(())
}

fn insert_projection_rows(tx: &Transaction<'_>, project: &core::Project) -> Result<()> {
    let project_id = project.id.to_string();

    let mut sheet_statement =
        tx.prepare("INSERT INTO sheets (id, project_id, name, data) VALUES (?1, ?2, ?3, ?4)")?;
    for sheet in &project.schematic.sheets {
        sheet_statement.execute(rusqlite::params![
            sheet.id.to_string(),
            &project_id,
            &sheet.name,
            serde_json::to_string(sheet)?,
        ])?;
    }
    drop(sheet_statement);

    let mut symbol_statement = tx.prepare(
        "INSERT INTO symbols (id, project_id, name, library, data) VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    for symbol in &project.schematic.symbols {
        symbol_statement.execute(rusqlite::params![
            symbol.id.to_string(),
            &project_id,
            &symbol.name,
            &symbol.library,
            serde_json::to_string(symbol)?,
        ])?;
    }
    drop(symbol_statement);

    let mut component_statement = tx.prepare(
        "INSERT INTO components (id, project_id, symbol_id, footprint_id, data)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    for component in &project.schematic.components {
        component_statement.execute(rusqlite::params![
            component.id.to_string(),
            &project_id,
            component.symbol_id.to_string(),
            component.footprint_id.to_string(),
            serde_json::to_string(component)?,
        ])?;
    }
    drop(component_statement);

    let mut net_statement =
        tx.prepare("INSERT INTO nets (id, project_id, name, data) VALUES (?1, ?2, ?3, ?4)")?;
    for net in &project.schematic.nets {
        net_statement.execute(rusqlite::params![
            net.id.to_string(),
            &project_id,
            &net.name,
            serde_json::to_string(net)?,
        ])?;
    }
    drop(net_statement);

    let mut footprint_statement =
        tx.prepare("INSERT INTO footprints (id, project_id, data) VALUES (?1, ?2, ?3)")?;
    for footprint_id in &project.pcb.footprint_ids {
        footprint_statement.execute(rusqlite::params![
            footprint_id.to_string(),
            &project_id,
            serde_json::json!({ "id": footprint_id.to_string() }).to_string(),
        ])?;
    }
    drop(footprint_statement);

    Ok(())
}
