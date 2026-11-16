//! KaiCad PCB engine — board outline, layers, stackup, routing.

pub mod footprint;
pub mod layer;
pub mod routing;

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

/// A PCB track (copper trace).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Track {
    pub id: uuid::Uuid,
    pub net_id: uuid::Uuid,
    pub layer: u16,
    pub path: Vec<core::Point>,
}

/// A PCB via (plated hole connecting layers).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Via {
    pub id: uuid::Uuid,
    pub position: core::Point,
    pub layer_start: u16,
    pub layer_end: u16,
}

#[cfg(test)]
mod tests {
    use super::{PcbDesign, Track, Via};
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
    fn track_and_via_defaults_are_empty() {
        let track = Track::default();
        assert!(track.id.is_nil());
        assert!(track.net_id.is_nil());
        assert_eq!(track.layer, 0);
        assert!(track.path.is_empty());

        let via = Via::default();
        assert!(via.id.is_nil());
        assert_eq!(via.layer_start, 0);
        assert_eq!(via.layer_end, 0);
        assert_eq!(via.position, core::Point::default());
    }
}
