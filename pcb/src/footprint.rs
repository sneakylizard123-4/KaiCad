//! Footprint definitions and footprint libraries for PCB design.

use std::collections::HashSet;
use std::path::Path;

/// A PCB footprint representing a component's physical dimensions and properties.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Footprint {
    pub name: String,
    pub reference_designator: String,
    /// Placement origin on the board in millimetres. Pad coordinates are
    /// relative to this origin.
    #[serde(default)]
    pub position: core::Point,
    /// Footprint rotation in degrees, counter-clockwise about [`Self::position`].
    #[serde(default)]
    pub rotation: f64,
    /// Board side the footprint is placed on, for example `F.Cu` or `B.Cu`.
    #[serde(default = "default_footprint_layer")]
    pub layer: String,
    pub pad_positions: Vec<PadPosition>,
    pub outline: Outline,
    /// Source constructs with no KaiCad model field, retained for export.
    #[serde(default)]
    pub preserved: Vec<core::PreservedNode>,
}

fn default_footprint_layer() -> String {
    "F.Cu".to_string()
}

impl Default for Footprint {
    fn default() -> Self {
        Self {
            name: String::new(),
            reference_designator: String::new(),
            position: core::Point::default(),
            rotation: 0.0,
            layer: default_footprint_layer(),
            pad_positions: Vec::new(),
            outline: Outline::default(),
            preserved: Vec::new(),
        }
    }
}

fn default_pad_width() -> f64 {
    1.0
}

fn default_pad_height() -> f64 {
    1.0
}

/// A single pad position on a footprint.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PadPosition {
    /// Pad number or name from the footprint, for example `1` or `A1`.
    ///
    /// Older KaiCad documents and programmatically created pads load with an
    /// empty number; exporters generate one from the pad index in that case.
    #[serde(default)]
    pub number: String,
    pub x: f64,
    pub y: f64,
    pub pad_type: PadType,
    /// Copper width of the pad in millimetres.
    #[serde(default = "default_pad_width")]
    pub width: f64,
    /// Copper height of the pad in millimetres.
    #[serde(default = "default_pad_height")]
    pub height: f64,
    /// Pad rotation in degrees, counter-clockwise about its centre.
    /// Older documents load with zero rotation.
    #[serde(default)]
    pub rotation: f64,
    /// Net this pad belongs to, when one is assigned.
    #[serde(default)]
    pub net_id: Option<uuid::Uuid>,
    /// Source pad shape, for example `rect`, `roundrect` or `custom`.
    #[serde(default = "default_pad_shape")]
    pub shape: String,
    /// Source pad kind, for example `smd`, `thru_hole` or `np_thru_hole`.
    /// An empty value is derived from [`PadPosition::pad_type`].
    #[serde(default)]
    pub kind: String,
    /// Source constructs with no KaiCad model field, retained for export.
    #[serde(default)]
    pub preserved: Vec<core::PreservedNode>,
}

fn default_pad_shape() -> String {
    "rect".to_string()
}

impl Default for PadPosition {
    fn default() -> Self {
        Self {
            number: String::new(),
            x: 0.0,
            y: 0.0,
            pad_type: PadType::default(),
            width: default_pad_width(),
            height: default_pad_height(),
            rotation: 0.0,
            net_id: None,
            shape: default_pad_shape(),
            kind: String::new(),
            preserved: Vec::new(),
        }
    }
}

/// Type of pad (e.g., through-hole, surface-mount, etc.).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PadType {
    #[default]
    ThroughHole,
    SurfaceMount,
    Tented,
}

/// Outline definition for a footprint.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Outline {
    pub shape: OutlineShape,
    pub copper: bool,
}

/// Shape of the footprint outline.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum OutlineShape {
    Rectangle { x: f64, y: f64, width: f64, height: f64 },
    RoundedRectangle { x: f64, y: f64, width: f64, height: f64, radius: f64 },
}

impl Default for OutlineShape {
    fn default() -> Self {
        Self::Rectangle { x: 0.0, y: 0.0, width: 1.0, height: 1.0 }
    }
}

impl OutlineShape {
    /// Construct an axis-aligned rectangular outline.
    pub fn rectangle(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self::Rectangle { x, y, width, height }
    }

    /// Construct a rectangular outline with rounded corners of the given radius.
    pub fn rounded_rectangle(x: f64, y: f64, width: f64, height: f64, radius: f64) -> Self {
        Self::RoundedRectangle { x, y, width, height, radius }
    }
}

/// A named collection of footprints stored as KaiCad's own JSON.
///
/// This is KaiCad's library container; KiCad `.kicad_mod` interchange lives in
/// the `kicad-parser` crate, and the in-memory types stay authoritative.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct FootprintLibrary {
    pub name: String,
    pub footprints: Vec<Footprint>,
}

impl FootprintLibrary {
    /// Footprints sorted by `(name, reference_designator)`.
    ///
    /// The library itself keeps insertion order; this is a stable view for
    /// display and export.
    pub fn sorted(&self) -> Vec<&Footprint> {
        let mut footprints: Vec<&Footprint> = self.footprints.iter().collect();
        footprints.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then(left.reference_designator.cmp(&right.reference_designator))
        });
        footprints
    }

    /// Look up a footprint by exact name.
    pub fn find(&self, name: &str) -> Option<&Footprint> {
        self.footprints.iter().find(|footprint| footprint.name == name)
    }

    /// Serialize as pretty JSON with exactly one trailing newline.
    pub fn to_json(&self) -> Result<String, FootprintLibraryError> {
        let mut json = serde_json::to_string_pretty(self)?;
        json.push('\n');
        Ok(json)
    }

    /// Parse a library document, rejecting duplicate footprint names.
    pub fn from_json(json: &str) -> Result<Self, FootprintLibraryError> {
        let library: Self = serde_json::from_str(json)?;

        let mut names = HashSet::new();
        for footprint in &library.footprints {
            if !names.insert(footprint.name.as_str()) {
                return Err(FootprintLibraryError::Duplicate(footprint.name.clone()));
            }
        }
        Ok(library)
    }

    /// Write the library to `path`, creating parent directories as needed.
    ///
    /// The file is written from [`FootprintLibrary::to_json`], so the bytes are
    /// identical to the serialized form and stable across runs.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), FootprintLibraryError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        std::fs::write(path, self.to_json()?)?;
        Ok(())
    }

    /// Load a library from `path`.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, FootprintLibraryError> {
        let json = std::fs::read_to_string(path)?;
        Self::from_json(&json)
    }
}

/// Failure while reading or writing a [`FootprintLibrary`].
#[derive(Debug)]
pub enum FootprintLibraryError {
    /// The document is not valid footprint-library JSON.
    Json(serde_json::Error),
    /// The file could not be read or written.
    Io(std::io::Error),
    /// Two footprints in the document share a name.
    Duplicate(String),
}

impl std::fmt::Display for FootprintLibraryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(error) => write!(formatter, "invalid footprint library JSON: {error}"),
            Self::Io(error) => write!(formatter, "footprint library I/O failed: {error}"),
            Self::Duplicate(name) => write!(formatter, "duplicate footprint name: {name:?}"),
        }
    }
}

impl std::error::Error for FootprintLibraryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            Self::Io(error) => Some(error),
            Self::Duplicate(_) => None,
        }
    }
}

impl From<serde_json::Error> for FootprintLibraryError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<std::io::Error> for FootprintLibraryError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Footprint, FootprintLibrary, FootprintLibraryError, OutlineShape, PadPosition, PadType,
    };

    fn sample_footprint(name: &str, reference_designator: &str) -> Footprint {
        Footprint {
            name: name.to_string(),
            reference_designator: reference_designator.to_string(),
            pad_positions: vec![
                PadPosition {
                    x: -0.8,
                    y: 0.0,
                    pad_type: PadType::SurfaceMount,
                    ..PadPosition::default()
                },
                PadPosition {
                    x: 0.8,
                    y: 0.0,
                    pad_type: PadType::SurfaceMount,
                    ..PadPosition::default()
                },
            ],
            outline: super::Outline {
                shape: OutlineShape::rounded_rectangle(-1.6, -0.8, 3.2, 1.6, 0.2),
                copper: false,
            },
            ..Footprint::default()
        }
    }

    fn sample_library() -> FootprintLibrary {
        FootprintLibrary {
            name: "Smoke".to_string(),
            footprints: vec![
                sample_footprint("R_0603", "R"),
                sample_footprint("C_0603", "C"),
                sample_footprint("R_0603", "R_ALT"),
            ],
        }
    }

    #[test]
    fn rectangle_stores_dimensions() {
        match OutlineShape::rectangle(1.0, 2.0, 3.0, 4.0) {
            OutlineShape::Rectangle { x, y, width, height } => {
                assert_eq!(x, 1.0);
                assert_eq!(y, 2.0);
                assert_eq!(width, 3.0);
                assert_eq!(height, 4.0);
            }
            other => panic!("expected Rectangle, got {other:?}"),
        }
    }

    #[test]
    fn rounded_rectangle_stores_dimensions_and_radius() {
        match OutlineShape::rounded_rectangle(1.0, 2.0, 3.0, 4.0, 0.5) {
            OutlineShape::RoundedRectangle { x, y, width, height, radius } => {
                assert_eq!(x, 1.0);
                assert_eq!(y, 2.0);
                assert_eq!(width, 3.0);
                assert_eq!(height, 4.0);
                assert_eq!(radius, 0.5);
            }
            other => panic!("expected RoundedRectangle, got {other:?}"),
        }
    }

    #[test]
    fn sorted_and_find_are_deterministic() {
        let library = sample_library();

        let names: Vec<&str> =
            library.sorted().iter().map(|footprint| footprint.name.as_str()).collect();
        assert_eq!(names, ["C_0603", "R_0603", "R_0603"]);

        let found = library.find("R_0603").expect("R_0603 exists");
        assert_eq!(found.reference_designator, "R");
        assert!(library.find("missing").is_none());
    }

    #[test]
    fn json_round_trip_preserves_footprints_pads_and_outlines() {
        let library = FootprintLibrary {
            name: "Smoke".to_string(),
            footprints: vec![sample_footprint("R_0603", "R")],
        };

        let json = library.to_json().expect("serialize library");
        assert!(json.ends_with('\n'));
        assert!(!json.ends_with("\n\n"));

        let restored = FootprintLibrary::from_json(&json).expect("parse library");
        assert_eq!(restored.name, "Smoke");
        assert_eq!(restored.footprints.len(), 1);

        let footprint = &restored.footprints[0];
        assert_eq!(footprint.name, "R_0603");
        assert_eq!(footprint.pad_positions.len(), 2);
        assert_eq!(footprint.pad_positions[0].x, -0.8);
        assert_eq!(footprint.pad_positions[1].pad_type, PadType::SurfaceMount);
        assert!(matches!(
            footprint.outline.shape,
            OutlineShape::RoundedRectangle { radius, .. } if radius == 0.2
        ));
    }

    #[test]
    fn empty_library_round_trips() {
        let library = FootprintLibrary::default();

        let json = library.to_json().expect("serialize empty library");
        let restored = FootprintLibrary::from_json(&json).expect("parse empty library");

        assert!(restored.name.is_empty());
        assert!(restored.footprints.is_empty());
    }

    #[test]
    fn from_json_rejects_duplicate_names_and_malformed_input() {
        let library = sample_library();
        let json = library.to_json().expect("serialize library");

        match FootprintLibrary::from_json(&json) {
            Err(FootprintLibraryError::Duplicate(name)) => assert_eq!(name, "R_0603"),
            other => panic!("expected a duplicate-name error, got {other:?}"),
        }

        assert!(matches!(
            FootprintLibrary::from_json("not json"),
            Err(FootprintLibraryError::Json(_))
        ));
        assert!(matches!(
            FootprintLibrary::from_json(r#"{"name":"x","footprints":[{"pads":[]}]}"#),
            Err(FootprintLibraryError::Json(_))
        ));
    }

    #[test]
    fn save_and_load_round_trip_through_a_directory() {
        let library = FootprintLibrary {
            name: "Saved".to_string(),
            footprints: vec![sample_footprint("R_0402", "R")],
        };
        let directory =
            std::env::temp_dir().join(format!("kaicad-footprint-test-{}", uuid::Uuid::new_v4()));
        let path = directory.join("nested/library.json");

        library.save(&path).expect("save library");
        assert!(path.exists());

        let loaded = FootprintLibrary::load(&path).expect("load library");
        assert_eq!(loaded.name, "Saved");
        assert_eq!(loaded.footprints.len(), 1);
        assert_eq!(loaded.footprints[0].pad_positions.len(), 2);

        std::fs::remove_dir_all(&directory).expect("clean up temp directory");
    }

    #[test]
    fn pad_dimensions_default_and_load_from_old_json() {
        let pad = PadPosition::default();
        assert_eq!(pad.width, 1.0);
        assert_eq!(pad.height, 1.0);
        assert!(pad.net_id.is_none());

        let footprint: Footprint = serde_json::from_str(
            r#"{"name":"R","reference_designator":"R1","pad_positions":[{"x":0.0,"y":0.0,"pad_type":"SurfaceMount"}],"outline":{"shape":{"Rectangle":{"x":0.0,"y":0.0,"width":1.0,"height":1.0}},"copper":false}}"#,
        )
        .expect("old footprint JSON should load");

        assert_eq!(footprint.position, core::Point::default());
        assert_eq!(footprint.pad_positions[0].width, 1.0);
        assert_eq!(footprint.pad_positions[0].height, 1.0);
        assert!(footprint.pad_positions[0].net_id.is_none());
        assert_eq!(footprint.pad_positions[0].rotation, 0.0);
    }

    #[test]
    fn pad_rotation_round_trips_and_defaults_to_zero() {
        let pad = PadPosition { rotation: 45.0, ..PadPosition::default() };
        let json = serde_json::to_string(&pad).expect("pad serializes");
        let restored: PadPosition = serde_json::from_str(&json).expect("pad parses");
        assert_eq!(restored.rotation, 45.0);

        let legacy = PadPosition::default();
        assert_eq!(legacy.rotation, 0.0);
        assert!(legacy.number.is_empty());
    }

    #[test]
    fn pad_numbers_round_trip_and_default_for_old_documents() {
        let pad = PadPosition { number: "A1".to_string(), ..PadPosition::default() };
        let json = serde_json::to_string(&pad).expect("pad serializes");
        let restored: PadPosition = serde_json::from_str(&json).expect("pad parses");
        assert_eq!(restored.number, "A1");

        let mut value = serde_json::to_value(&pad).expect("pad serializes");
        value.as_object_mut().expect("pad is an object").remove("number");
        let old: PadPosition = serde_json::from_value(value).expect("old pad parses");
        assert!(old.number.is_empty());
    }
}
