//! Circuit optimization algorithms.

/// Circuit optimizer.
#[derive(Debug, Clone, Default)]
pub struct CircuitOptimizer {
    pub strategies: Vec<OptimizationStrategy>,
}

/// Optimization strategy.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub enum OptimizationStrategy {
    #[default]
    ReduceComponentCount,
    MinimizeTraceLength,
    ImproveSignalIntegrity,
}
