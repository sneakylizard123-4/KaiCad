//! Board-document persistence for KaiCad Studio.
//!
//! `.kicad_pcb` is the native Studio document format. A board is imported into
//! [`PcbDesign`] when opened and written back through the same KiCad exporter
//! when saved, retaining source constructs that KaiCad does not model. JSON
//! remains available as an explicit interchange/debug format.

use std::path::Path;

use pcb::PcbDesign;

/// A board loaded from disk and any preservation warnings produced by import.
#[derive(Debug, Clone)]
pub struct LoadedBoard {
    /// The board model.
    pub design: PcbDesign,
    /// Constructs retained in the source document but not semantically modeled.
    pub warnings: Vec<String>,
}

/// Failure while reading or writing a board document.
#[derive(Debug)]
pub enum DocumentError {
    /// No path was supplied.
    EmptyPath,
    /// Reading or writing the file failed.
    Io(std::io::Error),
    /// The document is not valid KaiCad board JSON.
    Json(serde_json::Error),
    /// The document is not a supported KiCad board.
    KiCad(kicad_parser::ParseError),
    /// The extension does not identify a supported board format.
    UnsupportedExtension(String),
}

impl std::fmt::Display for DocumentError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyPath => write!(formatter, "no board path was provided"),
            Self::Io(error) => write!(formatter, "board file I/O failed: {error}"),
            Self::Json(error) => write!(formatter, "board JSON is invalid: {error}"),
            Self::KiCad(error) => write!(formatter, "KiCad board is invalid: {error}"),
            Self::UnsupportedExtension(extension) => write!(
                formatter,
                "unsupported board format '{extension}'; expected .kicad_pcb or .json"
            ),
        }
    }
}

impl std::error::Error for DocumentError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::EmptyPath | Self::UnsupportedExtension(_) => None,
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::KiCad(error) => Some(error),
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

impl From<kicad_parser::ParseError> for DocumentError {
    fn from(error: kicad_parser::ParseError) -> Self {
        Self::KiCad(error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BoardFormat {
    KiCadPcb,
    KaiCadJson,
}

impl BoardFormat {
    fn for_path(path: &Path) -> Result<Self, DocumentError> {
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        match extension.as_str() {
            "kicad_pcb" => Ok(Self::KiCadPcb),
            "json" => Ok(Self::KaiCadJson),
            other => Err(DocumentError::UnsupportedExtension(if other.is_empty() {
                "<none>".to_string()
            } else {
                other.to_string()
            })),
        }
    }
}

/// Serialize a board as pretty JSON with exactly one trailing newline.
pub fn design_to_json(design: &PcbDesign) -> Result<String, DocumentError> {
    let mut json = serde_json::to_string_pretty(design).map_err(DocumentError::Json)?;
    json.push('\n');
    Ok(json)
}

/// Parse a board document from KaiCad JSON.
pub fn design_from_json(json: &str) -> Result<PcbDesign, DocumentError> {
    serde_json::from_str(json).map_err(DocumentError::Json)
}

/// Load a board from `path`.
///
/// `.kicad_pcb` files are parsed natively; `.json` files use KaiCad's board
/// JSON. The extension check happens before any I/O, so an unsupported path
/// cannot overwrite or misread a file.
pub fn load_design(path: impl AsRef<Path>) -> Result<LoadedBoard, DocumentError> {
    let path = path.as_ref();
    if path.as_os_str().is_empty() {
        return Err(DocumentError::EmptyPath);
    }
    match BoardFormat::for_path(path)? {
        BoardFormat::KiCadPcb => {
            let imported = kicad_parser::load_pcb(path)?;
            Ok(LoadedBoard { design: imported.design, warnings: imported.warnings })
        }
        BoardFormat::KaiCadJson => {
            let json = std::fs::read_to_string(path)?;
            Ok(LoadedBoard { design: design_from_json(&json)?, warnings: Vec::new() })
        }
    }
}

/// Save a board to `path`, creating parent directories as needed.
///
/// `.kicad_pcb` is written through the KiCad exporter and remains the native
/// format. `.json` is retained for explicit interchange.
pub fn save_design(design: &PcbDesign, path: impl AsRef<Path>) -> Result<(), DocumentError> {
    let path = path.as_ref();
    if path.as_os_str().is_empty() {
        return Err(DocumentError::EmptyPath);
    }
    match BoardFormat::for_path(path)? {
        BoardFormat::KiCadPcb => kicad_parser::save_pcb(design, path)?,
        BoardFormat::KaiCadJson => {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)?;
                }
            }
            std::fs::write(path, design_to_json(design)?)?;
        }
    }
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
    fn native_kicad_board_round_trip_creates_directories() {
        let path = temp_path("nested/board.kicad_pcb");
        let design = demo_design();

        save_design(&design, &path).expect("board saves");
        let source = std::fs::read_to_string(&path).expect("native board is readable");
        assert!(source.starts_with("(kicad_pcb"));

        let loaded = load_design(&path).expect("board loads");
        assert_eq!(loaded.design.id, design.id);
        assert_eq!(loaded.design.outline, design.outline);
        // KiCad stores one segment per track, so a polyline becomes N-1 tracks.
        let expected_segments: usize =
            design.tracks.iter().map(|track| track.path.len().saturating_sub(1)).sum();
        assert_eq!(loaded.design.tracks.len(), expected_segments);
        let _ = std::fs::remove_dir_all(path.parent().and_then(Path::parent).expect("temp root"));
    }

    #[test]
    fn unsupported_extensions_are_rejected_before_io() {
        let error = load_design(temp_path("board.kicad_sch")).expect_err("not a board");
        assert!(matches!(
            error,
            DocumentError::UnsupportedExtension(extension) if extension == "kicad_sch"
        ));
    }

    #[test]
    fn malformed_json_is_reported_without_panicking() {
        let error = design_from_json("{\"outline\":").expect_err("truncated JSON is invalid");
        assert!(matches!(error, DocumentError::Json(_)));
    }
}
