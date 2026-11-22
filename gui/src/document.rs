//! Board-document persistence for KaiCad Studio.

use std::path::Path;

use pcb::PcbDesign;

/// Failure while reading or writing a board document.
#[derive(Debug)]
pub enum DocumentError {
    /// No path was supplied.
    EmptyPath,
    /// Reading or writing the file failed.
    Io(std::io::Error),
    /// The document is not valid [`PcbDesign`] JSON.
    Json(serde_json::Error),
}

impl std::fmt::Display for DocumentError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyPath => write!(formatter, "no board path was provided"),
            Self::Io(error) => write!(formatter, "board file I/O failed: {error}"),
            Self::Json(error) => write!(formatter, "board JSON is invalid: {error}"),
        }
    }
}

impl std::error::Error for DocumentError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::EmptyPath => None,
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
        }
    }
}

impl From<std::io::Error> for DocumentError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for DocumentError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

/// Serialize a board as pretty JSON with exactly one trailing newline.
pub fn design_to_json(design: &PcbDesign) -> Result<String, DocumentError> {
    let mut json = serde_json::to_string_pretty(design).map_err(DocumentError::Json)?;
    json.push('\n');
    Ok(json)
}

/// Parse a board document.
pub fn design_from_json(json: &str) -> Result<PcbDesign, DocumentError> {
    serde_json::from_str(json).map_err(DocumentError::Json)
}

/// Load a board from `path`.
pub fn load_design(path: impl AsRef<Path>) -> Result<PcbDesign, DocumentError> {
    let path = path.as_ref();
    if path.as_os_str().is_empty() {
        return Err(DocumentError::EmptyPath);
    }
    let json = std::fs::read_to_string(path)?;
    design_from_json(&json)
}

/// Save a board to `path`, creating parent directories as needed.
pub fn save_design(design: &PcbDesign, path: impl AsRef<Path>) -> Result<(), DocumentError> {
    let path = path.as_ref();
    if path.as_os_str().is_empty() {
        return Err(DocumentError::EmptyPath);
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, design_to_json(design)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo::demo_design;

    fn temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir()
            .join(format!("kaicad-gui-document-{}-{}", std::process::id(), uuid::Uuid::new_v4()))
            .join(name)
    }

    #[test]
    fn json_round_trip_is_pretty_and_newline_terminated() {
        let design = demo_design();
        let json = design_to_json(&design).expect("design serializes");

        assert!(json.ends_with('\n'));
        assert!(!json.ends_with("\n\n"));
        assert!(json.contains("\"outline\""));

        let decoded = design_from_json(&json).expect("design parses");
        assert_eq!(decoded.id, design.id);
        assert_eq!(decoded.tracks.len(), design.tracks.len());
        assert_eq!(decoded.vias.len(), design.vias.len());
    }

    #[test]
    fn empty_paths_are_rejected() {
        let design = demo_design();

        assert!(matches!(load_design(""), Err(DocumentError::EmptyPath)));
        assert!(matches!(save_design(&design, ""), Err(DocumentError::EmptyPath)));
    }

    #[test]
    fn save_creates_parent_directories_and_load_reads_the_board() {
        let path = temp_path("nested/board.kaicad.json");
        let design = demo_design();

        save_design(&design, &path).expect("board saves");
        let loaded = load_design(&path).expect("board loads");

        assert_eq!(loaded.id, design.id);
        assert_eq!(loaded.outline, design.outline);
        let _ = std::fs::remove_dir_all(path.parent().and_then(Path::parent).expect("temp root"));
    }

    #[test]
    fn malformed_json_is_reported_without_panicking() {
        let error = design_from_json("{\"outline\":").expect_err("truncated JSON is invalid");
        assert!(matches!(error, DocumentError::Json(_)));
    }
}
