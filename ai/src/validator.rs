//! AI suggestion validation.

use crate::{Constraint, OptimizationSuggestion, ValidationError, ValidationResult};

/// Validator for AI suggestions.
#[derive(Debug, Clone, Default)]
pub struct SuggestionValidator {
    pub deterministic_mode: bool,
}

impl SuggestionValidator {
    /// Deterministic checks only: no network, no model. Returns every problem found.
    ///
    /// - A `confidence_score` outside `0.0..=1.0` is an error on `confidence_score`.
    /// - An empty `changes` list is an error on `changes`.
    /// - A change whose `old_value` equals its `new_value` is an error on
    ///   `changes[<index>]` (a no-op change).
    /// - A [`Constraint`] with a negative `value` produces a warning naming it.
    ///
    /// [`ValidationResult::valid`] is true iff no errors were found.
    pub fn validate(
        &self,
        suggestion: &OptimizationSuggestion,
        constraints: &[Constraint],
    ) -> ValidationResult {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        if !(0.0..=1.0).contains(&suggestion.confidence_score) {
            errors.push(ValidationError {
                field: "confidence_score".to_string(),
                message: format!(
                    "confidence_score {} is outside the range 0.0..=1.0",
                    suggestion.confidence_score
                ),
            });
        }

        if suggestion.changes.is_empty() {
            errors.push(ValidationError {
                field: "changes".to_string(),
                message: "suggestion contains no changes".to_string(),
            });
        }

        for (index, change) in suggestion.changes.iter().enumerate() {
            if change.old_value == change.new_value {
                errors.push(ValidationError {
                    field: format!("changes[{index}]"),
                    message: format!(
                        "change to '{}' on '{}' is a no-op (old_value == new_value)",
                        change.field, change.target
                    ),
                });
            }
        }

        for constraint in constraints {
            if constraint.value < 0.0 {
                warnings.push(format!(
                    "constraint '{}' has a negative value ({})",
                    constraint.name, constraint.value
                ));
            }
        }

        ValidationResult { valid: errors.is_empty(), errors, warnings }
    }
}
