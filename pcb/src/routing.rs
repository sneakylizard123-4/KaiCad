//! PCB routing algorithms.

/// Router for generating copper traces.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Router {
    pub algorithm: RoutingAlgorithm,
    pub optimization: RoutingOptimization,
}

impl Router {
    /// Create a router with the default algorithm and optimization strategy.
    pub fn new() -> Self {
        Self::default()
    }
}

/// Routing algorithm type.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RoutingAlgorithm {
    #[default]
    PushAndShove,
    LengthMatching,
    Hybrid,
}

/// Routing optimization strategy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RoutingOptimization {
    #[default]
    MinimalLength,
    MinimalArea,
    Balanced,
}

#[cfg(test)]
mod tests {
    use super::{Router, RoutingAlgorithm, RoutingOptimization};

    #[test]
    fn router_defaults_to_push_and_shove() {
        let router = Router::new();

        assert_eq!(router.algorithm, RoutingAlgorithm::PushAndShove);
        assert_eq!(router.optimization, RoutingOptimization::MinimalLength);
    }

    #[test]
    fn router_new_matches_default() {
        let router = Router::new();
        let default = Router::default();

        assert_eq!(router.algorithm, default.algorithm);
        assert_eq!(router.optimization, default.optimization);
    }
}
