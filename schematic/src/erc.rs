//! Electrical rule checking for schematics.

use serde::{Serialize, Deserialize};

/// Electrical rule check result.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ERCResult {
    pub valid: bool,
    pub violations: Vec<Violation>,
}

/// A violation of electrical rules.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Violation {
    pub rule_id: String,
    pub description: String,
    pub location: String,
    pub severity: ViolationSeverity,
}

/// Severity of an electrical rule violation.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub enum ViolationSeverity {
    #[default]
    Warning,
    Error,
    Critical,
}
