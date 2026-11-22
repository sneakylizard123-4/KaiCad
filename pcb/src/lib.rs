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

/// Axis-aligned copper rectangle in board coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CopperRect {
    pub min: core::Point,
    pub max: core::Point,
}

impl CopperRect {
    fn from_center(center: core::Point, width: f64, height: f64) -> Self {
        let half_width = width / 2.0;
        let half_height = height / 2.0;
        Self {
            min: core::Point::new(center.x - half_width, center.y - half_height),
            max: core::Point::new(center.x + half_width, center.y + half_height),
        }
    }

    pub(crate) fn corners(&self) -> [core::Point; 4] {
        [
            self.min,
            core::Point::new(self.max.x, self.min.y),
            self.max,
            core::Point::new(self.min.x, self.max.y),
        ]
    }

    fn contains(&self, point: &core::Point) -> bool {
        if !point.x.is_finite() || !point.y.is_finite() {
            return false;
        }
        point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
    }
}

/// Distance from a finite point to an axis-aligned rectangle.
pub(crate) fn rect_distance_to_point(rect: &CopperRect, point: &core::Point) -> f64 {
    if !point.x.is_finite() || !point.y.is_finite() {
        return f64::NAN;
    }
    let dx = (rect.min.x - point.x).max(point.x - rect.max.x).max(0.0);
    let dy = (rect.min.y - point.y).max(point.y - rect.max.y).max(0.0);
    (dx * dx + dy * dy).sqrt()
}

/// Distance from a segment to an axis-aligned rectangle.
pub(crate) fn rect_distance_to_segment(
    rect: &CopperRect,
    start: &core::Point,
    end: &core::Point,
) -> f64 {
    if rect.contains(start) || rect.contains(end) {
        return 0.0;
    }

    let corners = rect.corners();
    let mut distance = f64::INFINITY;
    for index in 0..4 {
        distance = distance.min(core::geometry::segment_distance(
            &corners[index],
            &corners[(index + 1) % 4],
            start,
            end,
        ));
    }
    distance
}

/// Distance between two axis-aligned rectangles.
pub(crate) fn rect_distance_to_rect(a: &CopperRect, b: &CopperRect) -> f64 {
    let overlap_x = a.min.x <= b.max.x && b.min.x <= a.max.x;
    let overlap_y = a.min.y <= b.max.y && b.min.y <= a.max.y;
    if overlap_x && overlap_y {
        return 0.0;
    }

    let a_corners = a.corners();
    let b_corners = b.corners();
    let mut distance = f64::INFINITY;
    for a_index in 0..4 {
        for b_index in 0..4 {
            distance = distance.min(core::geometry::segment_distance(
                &a_corners[a_index],
                &a_corners[(a_index + 1) % 4],
                &b_corners[b_index],
                &b_corners[(b_index + 1) % 4],
            ));
        }
    }
    distance
}

/// Pad copper placed in board coordinates, with the layers it occupies.
#[derive(Debug, Clone)]
pub(crate) struct PadCopper {
    pub id: uuid::Uuid,
    pub footprint_index: usize,
    pub center: core::Point,
    pub rect: CopperRect,
    pub width: f64,
    pub height: f64,
    pub layers: Vec<u16>,
    pub net_id: Option<uuid::Uuid>,
}

impl PadCopper {
    pub(crate) fn has_valid_size(&self) -> bool {
        self.width.is_finite() && self.width > 0.0 && self.height.is_finite() && self.height > 0.0
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

    /// Copper layer ids in stackup order, or layer order when the stackup is empty.
    pub(crate) fn copper_layer_ids(&self) -> Vec<u16> {
        let mut ids = Vec::new();
        for id in &self.stackup.layers {
            if !ids.contains(id)
                && self.layers.iter().any(|layer| layer.id == *id && layer.is_copper())
            {
                ids.push(*id);
            }
        }

        if ids.is_empty() {
            for layer in &self.layers {
                if layer.is_copper() && !ids.contains(&layer.id) {
                    ids.push(layer.id);
                }
            }
        }
        ids
    }

    /// Top copper layer, which is where surface-mount pads live.
    pub(crate) fn top_copper_layer(&self) -> Option<u16> {
        self.copper_layer_ids().first().copied()
    }

    /// Whether `layer` lies within the via's span.
    pub(crate) fn via_covers_layer(&self, via: &Via, layer: u16) -> bool {
        if via.layer_start == via.layer_end {
            return layer == via.layer_start;
        }

        let copper = self.copper_layer_ids();
        let start = copper.iter().position(|id| *id == via.layer_start);
        let end = copper.iter().position(|id| *id == via.layer_end);

        match (start, end) {
            (Some(start), Some(end)) => {
                let low = start.min(end);
                let high = start.max(end);
                copper[low..=high].contains(&layer)
            }
            _ => layer == via.layer_start || layer == via.layer_end,
        }
    }

    /// Placed pad copper with absolute coordinates and layer coverage.
    ///
    /// Through-hole pads occupy every copper layer; surface-mount and tented
    /// pads occupy the top copper layer. Pads keep their relative coordinates
    /// and are offset by [`footprint::Footprint::position`].
    pub(crate) fn pad_copper(&self) -> Vec<PadCopper> {
        let copper = self.copper_layer_ids();
        let top = self.top_copper_layer();
        let mut pads = Vec::new();

        for (footprint_index, footprint) in self.footprints.iter().enumerate() {
            for (pad_index, pad) in footprint.pad_positions.iter().enumerate() {
                let center =
                    core::Point::new(footprint.position.x + pad.x, footprint.position.y + pad.y);
                let layers = match pad.pad_type {
                    footprint::PadType::ThroughHole => copper.clone(),
                    footprint::PadType::SurfaceMount | footprint::PadType::Tented => {
                        top.into_iter().collect()
                    }
                };
                let name = format!(
                    "pad:{}:{}:{}:{}:{}:{}:{}",
                    footprint.name,
                    footprint.reference_designator,
                    footprint_index,
                    pad_index,
                    pad.x.to_bits(),
                    pad.y.to_bits(),
                    footprint.position.x.to_bits(),
                );
                pads.push(PadCopper {
                    id: uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, name.as_bytes()),
                    footprint_index,
                    center,
                    rect: CopperRect::from_center(center, pad.width, pad.height),
                    width: pad.width,
                    height: pad.height,
                    layers,
                    net_id: pad.net_id,
                });
            }
        }
        pads
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
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Via {
    pub id: uuid::Uuid,
    /// Net connected by the via, when known. Documents from before this field
    /// existed load with `None`.
    #[serde(default)]
    pub net_id: Option<uuid::Uuid>,
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
            net_id: None,
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
