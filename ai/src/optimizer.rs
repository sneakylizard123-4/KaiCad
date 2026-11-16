//! Circuit optimization algorithms.

/// Circuit optimizer.
#[derive(Debug, Clone, Default)]
pub struct CircuitOptimizer {
    pub strategies: Vec<OptimizationStrategy>,
}

impl CircuitOptimizer {
    /// Create an optimizer with no strategies selected.
    pub fn new() -> Self {
        Self::default()
    }
}

/// Optimization strategy.
#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum OptimizationStrategy {
    #[default]
    ReduceComponentCount,
    MinimizeTraceLength,
    ImproveSignalIntegrity,
}
