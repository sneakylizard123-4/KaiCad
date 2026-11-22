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
    /// Named nets used by tracks, vias and pads.
    ///
    /// Documents from before this field existed load with an empty list. Ids
    /// are stable UUIDs; names are metadata and do not affect connectivity.
    #[serde(default)]
    pub nets: Vec<PcbNet>,
    /// Design-rule limits used when checking this design.
    #[serde(default)]
    pub rules: drc::DrcRules,
    /// Opaque source constructs retained for lossless KiCad interchange.
    #[serde(default)]
    pub source: Option<core::SourceDocument>,
}

/// A named PCB net.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PcbNet {
    /// Stable net id referenced by tracks, vias and pads.
    pub id: uuid::Uuid,
    /// Human-readable net name, for example `GND`.
    pub name: String,
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
            nets: Vec::new(),
            rules: drc::DrcRules::default(),
            source: None,
        }
    }
}

/// Copper rectangle in board coordinates, optionally rotated about its centre.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CopperRect {
    center: core::Point,
    half_width: f64,
    half_height: f64,
    /// Rotation in radians, counter-clockwise.
    rotation: f64,
}

impl CopperRect {
    fn from_center(center: core::Point, width: f64, height: f64, rotation_degrees: f64) -> Self {
        Self {
            center,
            half_width: width / 2.0,
            half_height: height / 2.0,
            rotation: rotation_degrees.to_radians(),
        }
    }

    fn local_point(&self, point: &core::Point) -> core::Point {
        let dx = point.x - self.center.x;
        let dy = point.y - self.center.y;
        let (sin, cos) = self.rotation.sin_cos();
        core::Point::new(dx * cos + dy * sin, -dx * sin + dy * cos)
    }

    pub(crate) fn corners(&self) -> [core::Point; 4] {
        let local = [
            core::Point::new(-self.half_width, -self.half_height),
            core::Point::new(self.half_width, -self.half_height),
            core::Point::new(self.half_width, self.half_height),
            core::Point::new(-self.half_width, self.half_height),
        ];
        let (sin, cos) = self.rotation.sin_cos();
        local.map(|point| {
            core::Point::new(
                self.center.x + point.x * cos - point.y * sin,
                self.center.y + point.x * sin + point.y * cos,
            )
        })
    }

    fn contains(&self, point: &core::Point) -> bool {
        if !point.x.is_finite() || !point.y.is_finite() {
            return false;
        }
        let local = self.local_point(point);
        local.x >= -self.half_width
            && local.x <= self.half_width
            && local.y >= -self.half_height
            && local.y <= self.half_height
    }
}

/// Distance from a finite point to a rotated rectangle.
pub(crate) fn rect_distance_to_point(rect: &CopperRect, point: &core::Point) -> f64 {
    if !point.x.is_finite() || !point.y.is_finite() || !rect.rotation.is_finite() {
        return f64::NAN;
    }
    let local = rect.local_point(point);
    let dx = (local.x.abs() - rect.half_width).max(0.0);
    let dy = (local.y.abs() - rect.half_height).max(0.0);
    (dx * dx + dy * dy).sqrt()
}

/// Distance from a segment to a rotated rectangle.
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

/// Distance between two rotated rectangles.
pub(crate) fn rect_distance_to_rect(a: &CopperRect, b: &CopperRect) -> f64 {
    if a.corners().iter().any(|corner| b.contains(corner))
        || b.corners().iter().any(|corner| a.contains(corner))
    {
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
    pub rotation: f64,
    pub layers: Vec<u16>,
    pub net_id: Option<uuid::Uuid>,
}

impl PadCopper {
    pub(crate) fn has_valid_size(&self) -> bool {
        self.width.is_finite() && self.width > 0.0 && self.height.is_finite() && self.height > 0.0
    }

    pub(crate) fn has_valid_rotation(&self) -> bool {
        self.rotation.is_finite()
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
    pub fn copper_layer_ids(&self) -> Vec<u16> {
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

    /// Look up the name of an assigned net.
    ///
    /// Returns `None` for the nil UUID or a net id absent from
    /// [`PcbDesign::nets`].
    pub fn net_name(&self, id: uuid::Uuid) -> Option<&str> {
        if id.is_nil() {
            return None;
        }
        self.nets.iter().find(|net| net.id == id).map(|net| net.name.as_str())
    }

    /// Assigned net ids, sorted and deduplicated.
    ///
    /// Nets are collected from tracks, vias and footprint pads. The nil UUID
    /// used by unassigned tracks is omitted.
    pub fn net_ids(&self) -> Vec<uuid::Uuid> {
        let mut ids = Vec::new();
        for track in &self.tracks {
            if !track.net_id.is_nil() {
                ids.push(track.net_id);
            }
        }
        for via in &self.vias {
            if let Some(net_id) = via.net_id {
                if !net_id.is_nil() {
                    ids.push(net_id);
                }
            }
        }
        for footprint in &self.footprints {
            for pad in &footprint.pad_positions {
                if let Some(net_id) = pad.net_id {
                    if !net_id.is_nil() {
                        ids.push(net_id);
                    }
                }
            }
        }
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    /// Top copper layer, which is where surface-mount pads live.
    pub(crate) fn top_copper_layer(&self) -> Option<u16> {
        self.copper_layer_ids().first().copied()
    }

    /// Classify a via as through, blind or buried on this board.
    ///
    /// Returns `None` when the start/end are equal or either layer is not in
    /// the copper stackup. Start/end order does not matter.
    pub fn via_kind(&self, via: &Via) -> Option<ViaKind> {
        let copper = self.copper_layer_ids();
        let start = copper.iter().position(|id| *id == via.layer_start)?;
        let end = copper.iter().position(|id| *id == via.layer_end)?;
        if start == end {
            return None;
        }

        let low = start.min(end);
        let high = start.max(end);
        if low == 0 && high == copper.len() - 1 {
            Some(ViaKind::Through)
        } else if low == 0 || high == copper.len() - 1 {
            Some(ViaKind::Blind)
        } else {
            Some(ViaKind::Buried)
        }
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
    /// pads occupy the copper layer named by [`footprint::Footprint::layer`].
    /// Pads keep their relative coordinates, rotate with the footprint and are
    /// offset by [`footprint::Footprint::position`].
    pub(crate) fn pad_copper(&self) -> Vec<PadCopper> {
        let copper = self.copper_layer_ids();
        let top = self.top_copper_layer();
        let bottom = copper.last().copied();
        let mut pads = Vec::new();

        for (footprint_index, footprint) in self.footprints.iter().enumerate() {
            let side_layer = self
                .layers
                .iter()
                .find(|layer| layer.name == footprint.layer)
                .map(|layer| layer.id);
            let half_turn = if footprint.layer.starts_with('B') { 180.0 } else { 0.0 };
            let (footprint_sin, footprint_cos) = footprint.rotation.to_radians().sin_cos();
            let placed = |x: f64, y: f64| {
                core::Point::new(
                    footprint.position.x + x * footprint_cos - y * footprint_sin,
                    footprint.position.y + x * footprint_sin + y * footprint_cos,
                )
            };
            for (pad_index, pad) in footprint.pad_positions.iter().enumerate() {
                let center = placed(pad.x, pad.y);
                let rotation = pad.rotation + footprint.rotation + half_turn;
                let layers = match pad.pad_type {
                    footprint::PadType::ThroughHole => copper.clone(),
                    footprint::PadType::SurfaceMount | footprint::PadType::Tented => side_layer
                        .or_else(|| if footprint.layer.starts_with('B') { bottom } else { top })
                        .into_iter()
                        .collect(),
                };
                let name = format!(
                    "pad:{}:{}:{}:{}:{}:{}:{}:{}",
                    footprint.name,
                    footprint.reference_designator,
                    footprint_index,
                    pad_index,
                    pad.x.to_bits(),
                    pad.y.to_bits(),
                    footprint.position.x.to_bits(),
                    footprint.rotation.to_bits(),
                );
                pads.push(PadCopper {
                    id: uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, name.as_bytes()),
                    footprint_index,
                    center,
                    rect: CopperRect::from_center(center, pad.width, pad.height, rotation),
                    width: pad.width,
                    height: pad.height,
                    rotation,
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

/// Kind of via span on a particular board.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ViaKind {
    /// Connects the top and bottom copper layers.
    Through,
    /// Connects one outer copper layer to an inner layer.
    Blind,
    /// Connects two inner copper layers.
    Buried,
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
    fn net_ids_are_sorted_unique_and_skip_unassigned_nets() {
        let first = uuid::Uuid::from_u128(1);
        let second = uuid::Uuid::from_u128(2);
        let design = PcbDesign {
            tracks: vec![
                Track { net_id: second, ..Track::default() },
                Track { net_id: uuid::Uuid::nil(), ..Track::default() },
                Track { net_id: first, ..Track::default() },
            ],
            vias: vec![Via { net_id: Some(first), ..Via::default() }],
            footprints: vec![crate::Footprint {
                pad_positions: vec![crate::PadPosition {
                    net_id: Some(second),
                    ..crate::PadPosition::default()
                }],
                ..crate::Footprint::default()
            }],
            ..PcbDesign::default()
        };

        assert_eq!(design.net_ids(), vec![first, second]);
    }

    #[test]
    fn named_nets_round_trip_and_old_designs_load_without_names() {
        let id = uuid::Uuid::from_u128(41);
        let design = PcbDesign {
            nets: vec![crate::PcbNet { id, name: "GND".to_string() }],
            ..PcbDesign::default()
        };

        assert_eq!(design.net_name(id), Some("GND"));
        assert_eq!(design.net_name(uuid::Uuid::nil()), None);
        assert_eq!(design.net_name(uuid::Uuid::from_u128(42)), None);

        let json = serde_json::to_string(&design).expect("design serializes");
        let restored: PcbDesign = serde_json::from_str(&json).expect("design parses");
        assert_eq!(restored.nets, design.nets);

        let mut value = serde_json::to_value(&design).expect("design serializes");
        value.as_object_mut().expect("design is an object").remove("nets");
        let old: PcbDesign = serde_json::from_value(value).expect("old design parses");
        assert!(old.nets.is_empty());
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
    fn via_kind_classifies_through_blind_and_buried_vias() {
        let design = PcbDesign {
            layers: vec![
                PcbLayer::new(0, "F.Cu", LayerKind::Copper),
                PcbLayer::new(1, "In1.Cu", LayerKind::Copper),
                PcbLayer::new(2, "In2.Cu", LayerKind::Copper),
                PcbLayer::new(3, "B.Cu", LayerKind::Copper),
            ],
            stackup: core::Stackup { layers: vec![0, 1, 2, 3] },
            ..PcbDesign::default()
        };

        let via = |start, end| Via { layer_start: start, layer_end: end, ..Via::default() };
        assert_eq!(design.via_kind(&via(0, 3)), Some(super::ViaKind::Through));
        assert_eq!(design.via_kind(&via(3, 0)), Some(super::ViaKind::Through));
        assert_eq!(design.via_kind(&via(0, 2)), Some(super::ViaKind::Blind));
        assert_eq!(design.via_kind(&via(1, 3)), Some(super::ViaKind::Blind));
        assert_eq!(design.via_kind(&via(1, 2)), Some(super::ViaKind::Buried));
        assert_eq!(design.via_kind(&via(1, 1)), None);
        assert_eq!(design.via_kind(&via(0, 9)), None);
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
