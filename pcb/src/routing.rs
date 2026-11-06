//! PCB routing algorithms.

use serde::{Serialize, Deserialize};

/// Router for generating copper traces.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Router {
    pub algorithm: RoutingAlgorithm,
    pub optimization: OptimizationStrategy,
}

/// Routing algorithm type.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub enum RoutingAlgorithm {
    #[default]
    PushAndShove,
    LengthMatching,
    Hybrid,
}

/// Routing optimization strategy.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub enum OptimizationStrategy {
    #[default]
    MinimalLength,
    MinimalArea,
    Balanced,
}