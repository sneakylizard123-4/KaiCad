//! Basic JSON serialization and file persistence for core types.

use std::path::Path;

use serde::{Serialize, de::DeserializeOwned};

use crate::project::Project;

/// Errors that can occur while reading or writing a project.
#[derive(Debug)]
pub enum Error {
    /// The JSON document could not be encoded or decoded.
    Json(serde_json::Error),
    /// The file could not be read or written.
    Io(std::io::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(error) => write!(f, "JSON error: {error}"),
            Self::Io(error) => write!(f, "I/O error: {error}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            Self::Io(error) => Some(error),
        }
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// Result type used by the serialization helpers.
pub type Result<T> = std::result::Result<T, Error>;

/// Serialize `value` to compact JSON.
pub fn to_json<T: Serialize>(value: &T) -> Result<String> {
    Ok(serde_json::to_string(value)?)
}

/// Serialize `value` to indented JSON.
pub fn to_json_pretty<T: Serialize>(value: &T) -> Result<String> {
    Ok(serde_json::to_string_pretty(value)?)
}

/// Deserialize a value from JSON text.
pub fn from_json<T: DeserializeOwned>(text: &str) -> Result<T> {
    Ok(serde_json::from_str(text)?)
}

/// Write `value` to `path` as pretty JSON, creating parent directories as needed.
pub fn save<T: Serialize, P: AsRef<Path>>(value: &T, path: P) -> Result<()> {
    let path = path.as_ref();

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    std::fs::write(path, to_json_pretty(value)?)?;
    Ok(())
}

/// Read and deserialize a value from the JSON file at `path`.
pub fn load<T: DeserializeOwned, P: AsRef<Path>>(path: P) -> Result<T> {
    let text = std::fs::read_to_string(path)?;
    from_json(&text)
}

impl Project {
    /// Serialize this project to compact JSON.
    pub fn to_json(&self) -> Result<String> {
        to_json(self)
    }

    /// Deserialize a project from JSON text.
    pub fn from_json(text: &str) -> Result<Self> {
        from_json(text)
    }

    /// Save this project to `path` as pretty JSON.
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        save(self, path)
    }

    /// Load a project from the JSON file at `path`.
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        load(path)
    }
}
