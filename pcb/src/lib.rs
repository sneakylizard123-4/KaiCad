//! KaiCad PCB engine — board outline, layers, stackup, routing and design rules.

pub mod drc;
pub mod footprint;
pub mod layer;
pub mod routing;

pub use drc::{DrcReport, DrcRules, DrcRulesError, DrcSeverity, DrcViolation, run_drc};
pub use footprint::{Footprint, FootprintLibrary, FootprintLibraryError, PadPosition, PadType};
pub use layer::{LayerIssue, LayerIssueKind, LayerStack};

/// A complete PCB design.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PcbDesign {
    pub id: uuid::Uuid,
    pub outline: core::PcbOutline,
    pub layers: Vec<core::PcbLayer>,
    pub stackup: core::Stackup,
    pub footprints: Vec<footprint::Footprint>,
    pub tracks: Vec<Track>,
    pub vias: Vec<Via>,
    /// Design-rule limits used when checking this design.
    #[serde(default)]
    pub rules: drc::DrcRules,
}

impl Default for PcbDesign {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            outline: core::PcbOutline::default(),
            layers: Vec::new(),
            stackup: core::Stackup::default(),
            footprints: Vec::new(),
            tracks: Vec::new(),
            vias: Vec::new(),
            rules: drc::DrcRules::default(),
        }
    }
}

impl PcbDesign {
    /// Total number of copper layers in the stackup.
    ///
    /// Counts the copper layers defined in [`PcbDesign::layers`]. When the
    /// stackup lists layer ids, only layers present in it are counted.
    pub fn copper_layer_count(&self) -> usize {
        if self.stackup.layers.is_empty() {
            return self.layers.iter().filter(|layer| layer.is_copper()).count();
        }
        self.layers
            .iter()
            .filter(|layer| layer.is_copper() && self.stackup.layers.contains(&layer.id))
            .count()
    }
}

/// Default copper width for a track, in millimetres.
fn default_track_width() -> f64 {
    0.25
}

/// Default outer copper diameter for a via, in millimetres.
fn default_via_diameter() -> f64 {
    0.6
}

/// Default finished drill diameter for a via, in millimetres.
fn default_via_drill() -> f64 {
    0.3
}

/// A PCB track (copper trace).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Track {
    pub id: uuid::Uuid,
    pub net_id: uuid::Uuid,
    pub layer: u16,
    /// Copper width in millimetres. Documents from before this field existed
    /// load with the default of `0.25`.
    #[serde(default = "default_track_width")]
    pub width: f64,
    pub path: Vec<core::Point>,
}

impl Default for Track {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::nil(),
            net_id: uuid::Uuid::nil(),
            layer: 0,
            width: default_track_width(),
            path: Vec::new(),
        }
    }
}

/// A PCB via (plated hole connecting layers).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Via {
    pub id: uuid::Uuid,
    pub position: core::Point,
    pub layer_start: u16,
    pub layer_end: u16,
    /// Outer copper diameter in millimetres. Documents from before this field
    /// existed load with the default of `0.6`.
    #[serde(default = "default_via_diameter")]
    pub diameter: f64,
    /// Finished drill diameter in millimetres. Documents from before this
    /// field existed load with the default of `0.3`.
    #[serde(default = "default_via_drill")]
    pub drill: f64,
}

impl Default for Via {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::nil(),
            position: core::Point::default(),
            layer_start: 0,
            layer_end: 0,
            diameter: default_via_diameter(),
            drill: default_via_drill(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DrcRules, PcbDesign, Track, Via};
    use core::{LayerKind, PcbLayer};

    #[test]
    fn default_design_has_fresh_id_and_empty_collections() {
        let design = PcbDesign::default();

        assert!(!design.id.is_nil());
        assert!(design.outline.points.is_empty());
        assert!(design.layers.is_empty());
        assert!(design.stackup.layers.is_empty());
        assert!(design.footprints.is_empty());
        assert!(design.tracks.is_empty());
        assert!(design.vias.is_empty());
    }

    #[test]
    fn copper_layer_count_counts_copper_layers() {
        let design = PcbDesign {
            layers: vec![
                PcbLayer::new(0, "F.Cu", LayerKind::Copper),
                PcbLayer::new(1, "F.SilkS", LayerKind::SilkScreen),
                PcbLayer::new(2, "B.Cu", LayerKind::Copper),
            ],
            ..PcbDesign::default()
        };

        assert_eq!(design.copper_layer_count(), 2);
    }

    #[test]
    fn copper_layer_count_respects_the_stackup() {
        let design = PcbDesign {
            layers: vec![
                PcbLayer::new(0, "F.Cu", LayerKind::Copper),
                PcbLayer::new(2, "B.Cu", LayerKind::Copper),
            ],
            stackup: core::Stackup { layers: vec![0] },
            ..PcbDesign::default()
        };

        assert_eq!(design.copper_layer_count(), 1);
    }

    #[test]
    fn track_and_via_defaults_are_empty_with_standard_dimensions() {
        let track = Track::default();
        assert!(track.id.is_nil());
        assert!(track.net_id.is_nil());
        assert_eq!(track.layer, 0);
        assert_eq!(track.width, 0.25);
        assert!(track.path.is_empty());

        let via = Via::default();
        assert!(via.id.is_nil());
        assert_eq!(via.layer_start, 0);
        assert_eq!(via.layer_end, 0);
        assert_eq!(via.diameter, 0.6);
        assert_eq!(via.drill, 0.3);
        assert_eq!(via.position, core::Point::default());
    }

    #[test]
    fn serde_defaults_fill_missing_track_and_via_dimensions() {
        let id = uuid::Uuid::new_v4();

        let track: Track = serde_json::from_str(&format!(
            r#"{{"id":"{id}","net_id":"{id}","layer":0,"path":[]}}"#
        ))
        .expect("track without a width should deserialize");
        assert_eq!(track.width, 0.25);

        let via: Via = serde_json::from_str(&format!(
            r#"{{"id":"{id}","position":{{"x":1.0,"y":2.0}},"layer_start":0,"layer_end":1}}"#
        ))
        .expect("via without dimensions should deserialize");
        assert_eq!(via.diameter, 0.6);
        assert_eq!(via.drill, 0.3);
    }

    #[test]
    fn design_persists_drc_rules_and_loads_defaults_for_old_documents() {
        let design = PcbDesign {
            rules: DrcRules { clearance: 0.3, ..DrcRules::default() },
            ..PcbDesign::default()
        };

        let json = serde_json::to_string(&design).expect("design should serialize");
        let restored: PcbDesign = serde_json::from_str(&json).expect("design should parse");
        assert_eq!(restored.rules, design.rules);

        let mut value = serde_json::to_value(&design).expect("design should serialize");
        value.as_object_mut().expect("design is an object").remove("rules");
        let old: PcbDesign = serde_json::from_value(value).expect("old design should parse");
        assert_eq!(old.rules, DrcRules::default());
    }

    #[test]
    fn dimensioned_track_and_via_round_trip_through_json() {
        let track = Track {
            width: 0.4,
            path: vec![core::Point::new(1.0, 2.0), core::Point::new(3.0, 4.0)],
            ..Track::default()
        };
        let json = serde_json::to_string(&track).expect("track should serialize");
        let restored: Track = serde_json::from_str(&json).expect("track should parse");
        assert_eq!(restored.width, 0.4);
        assert_eq!(restored.path.len(), 2);

        let via = Via { diameter: 0.8, drill: 0.4, ..Via::default() };
        let json = serde_json::to_string(&via).expect("via should serialize");
        let restored: Via = serde_json::from_str(&json).expect("via should parse");
        assert_eq!(restored.diameter, 0.8);
        assert_eq!(restored.drill, 0.4);
    }
}
