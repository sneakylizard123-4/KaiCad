//! KaiCad PCB engine — board outline, layers, stackup, routing.

pub mod footprint;
pub mod layer;
pub mod routing;

use uuid::Uuid;
use serde::{Serialize, Deserialize};

/// Represents a complete PCB design.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PcbDesign {
    pub id: uuid::Uuid,
    pub outline: PcbOutline,
    pub layers: Vec<PcbLayer>,
    pub stackup: Stackup,
    pub footprints: Vec<footprint::Footprint>,
    pub tracks: Vec<Track>,
    pub vias: Vec<Via>,
}

impl Default for PcbDesign {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            outline: PcbOutline::default(),
            layers: Vec::new(),
            stackup: Stackup::default(),
            footprints: Vec::new(),
            tracks: Vec::new(),
            vias: Vec::new(),
        }
    }
}

/// A PCB track (copper trace).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Track {
    pub id: uuid::Uuid,
    pub net_id: uuid::Uuid,
    pub layer: usize,
    pub path: Vec<geometry::Point>,
}

/// A PCB via (hole connecting layers).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Via {
    pub id: uuid::Uuid,
    pub position: geometry::Point,
    pub layer_start: usize,
    pub layer_end: usize,
}

/// PCB board outline definition.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PcbOutline {}

/// Placeholder for PCB layer definition.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PcbLayer {}

/// PCB layer stackup definition.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Stackup {}

mod geometry {
    //! Simple 2D geometry primitives.

    #[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
    pub struct Point {
        pub x: f64,
        pub y: f64,
    }

    impl Default for Point {
        fn default() -> Self {
            Self { x: 0.0, y: 0.0 }
        }
    }
}