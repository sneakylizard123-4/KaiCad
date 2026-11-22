//! kAI — KaiCad AI subsystem for circuit optimization and design assistance.
//!
//! Provides AI-generated circuit optimization suggestions, routing assistance,
//! and design analysis. All AI outputs undergo deterministic validation before
//! being applied to the design.

pub mod optimizer;
pub mod suggester;
pub mod validator;

pub use core::proposal::ChangeProposal;
pub use validator::SuggestionValidator;

/// kAI assistant configuration.
// "kAI" is the product's brand name for the AI subsystem.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone)]
pub struct kAIConfig {
    pub model_endpoint: String,
    pub temperature: f32,
    pub max_tokens: u32,
}

impl Default for kAIConfig {
    fn default() -> Self {
        Self {
            model_endpoint: "http://localhost:11434".to_string(),
            temperature: 0.7,
            max_tokens: 2048,
        }
    }
}

/// kAI assistant — main entry point for AI assistance.
// "kAI" is the product's brand name for the AI subsystem.
#[allow(non_camel_case_types)]
pub struct kAI {
    pub config: kAIConfig,
}

impl kAI {
    pub fn new(config: kAIConfig) -> Self {
        Self { config }
    }

    /// Request AI-generated optimization suggestions for a circuit.
    ///
    /// The model backend is not wired up yet, so this always returns
    /// [`kAIError::NotImplemented`].
    pub fn suggest_optimization(
        &self,
        _context: OptimizationContext,
    ) -> Result<OptimizationSuggestion, kAIError> {
        Err(kAIError::NotImplemented)
    }

    /// Validate an AI-generated suggestion before applying it to the design.
    ///
    /// Runs the deterministic checks in [`SuggestionValidator::validate`] only:
    /// no network access and no model invocation.
    pub fn validate_suggestion(
        &self,
        suggestion: &OptimizationSuggestion,
    ) -> Result<ValidationResult, kAIError> {
        Ok(SuggestionValidator.validate(suggestion, &[]))
    }

    /// Validate and atomically apply a suggestion to `project`.
    ///
    /// Validation always runs first. Invalid suggestions and failed edits leave
    /// the project untouched. The returned transaction can be passed to
    /// [`core::edit::undo`] or [`core::edit::redo`].
    pub fn apply_suggestion(
        &self,
        project: &mut core::Project,
        suggestion: &OptimizationSuggestion,
    ) -> Result<AppliedSuggestion, kAIError> {
        let validation = self.validate_suggestion(suggestion)?;
        if !validation.valid {
            return Err(kAIError::ValidationFailed(validation.errors));
        }

        let label = format!("kAI: {}", suggestion.description);
        let transaction = core::edit::apply(project, &label, &suggestion.changes)
            .map_err(kAIError::ApplyFailed)?;

        Ok(AppliedSuggestion { suggestion_id: suggestion.id, transaction })
    }
}

/// Context provided to kAI for optimization requests.
#[derive(Debug, Clone)]
pub struct OptimizationContext {
    pub netlist: schematic::Netlist,
    pub current_design_hash: String,
    pub constraints: Vec<Constraint>,
}

/// An optimization suggestion from kAI.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OptimizationSuggestion {
    pub id: uuid::Uuid,
    pub description: String,
    pub changes: Vec<ChangeProposal>,
    pub confidence_score: f32,
}

impl OptimizationSuggestion {
    /// A suggestion is applicable when it has changes and a confidence in 0.0..=1.0.
    pub fn is_applicable(&self) -> bool {
        !self.changes.is_empty() && (0.0..=1.0).contains(&self.confidence_score)
    }
}

/// Result of successfully applying a suggestion.
#[derive(Debug, Clone, PartialEq)]
pub struct AppliedSuggestion {
    pub suggestion_id: uuid::Uuid,
    pub transaction: core::edit::EditTransaction,
}

/// A constraint that must be respected in any AI suggestion.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Constraint {
    pub name: String,
    pub constraint_type: ConstraintType,
    pub value: f64,
}

/// Type of constraint.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ConstraintType {
    Clearance,
    LengthMatching,
    Impedance,
    Voltage,
    Current,
}

/// Result of validating an AI suggestion.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub valid: bool,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<String>,
}

/// Validation error details.
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
}

/// kAI error types.
// "kAI" is the product's brand name for the AI subsystem.
#[allow(non_camel_case_types)]
#[derive(Debug)]
pub enum kAIError {
    NotImplemented,
    ModelUnavailable,
    ValidationFailed(Vec<ValidationError>),
    /// The suggestion passed validation but could not be applied atomically.
    ApplyFailed(core::edit::EditError),
}

impl std::fmt::Display for kAIError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            kAIError::NotImplemented => write!(f, "Feature not yet implemented"),
            kAIError::ModelUnavailable => write!(f, "AI model endpoint unavailable"),
            kAIError::ValidationFailed(errors) => {
                write!(f, "Validation failed: {:?}", errors)
            }
            kAIError::ApplyFailed(error) => write!(f, "Suggestion could not be applied: {error}"),
        }
    }
}

impl std::error::Error for kAIError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ApplyFailed(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn suggestion(changes: Vec<ChangeProposal>, confidence_score: f32) -> OptimizationSuggestion {
        OptimizationSuggestion {
            id: uuid::Uuid::new_v4(),
            description: "test suggestion".to_string(),
            changes,
            confidence_score,
        }
    }

    fn change(old_value: serde_json::Value, new_value: serde_json::Value) -> ChangeProposal {
        ChangeProposal::new("R1", "value", old_value, new_value)
    }

    fn constraint(name: &str, value: f64) -> Constraint {
        Constraint { name: name.to_string(), constraint_type: ConstraintType::Clearance, value }
    }

    #[test]
    fn is_applicable_boundaries() {
        let low = suggestion(vec![change(json!(1), json!(2))], 0.0);
        let high = suggestion(vec![change(json!(1), json!(2))], 1.0);
        let below = suggestion(vec![change(json!(1), json!(2))], -0.01);
        let above = suggestion(vec![change(json!(1), json!(2))], 1.01);
        let empty = suggestion(Vec::new(), 0.5);

        assert!(low.is_applicable());
        assert!(high.is_applicable());
        assert!(!below.is_applicable());
        assert!(!above.is_applicable());
        assert!(!empty.is_applicable());
    }

    #[test]
    fn validate_rejects_out_of_range_confidence() {
        let validator = SuggestionValidator;
        let result = validator.validate(&suggestion(vec![change(json!(1), json!(2))], 1.5), &[]);

        assert!(!result.valid);
        assert!(result.errors.iter().any(|error| error.field == "confidence_score"));
    }

    #[test]
    fn validate_rejects_empty_changes() {
        let validator = SuggestionValidator;
        let result = validator.validate(&suggestion(Vec::new(), 0.5), &[]);

        assert!(!result.valid);
        assert!(result.errors.iter().any(|error| error.field == "changes"));
    }

    #[test]
    fn validate_rejects_no_op_change() {
        let validator = SuggestionValidator;
        let result =
            validator.validate(&suggestion(vec![change(json!(100), json!(100))], 0.9), &[]);

        assert!(!result.valid);
        assert!(result.errors.iter().any(|error| error.field == "changes[0]"));
    }

    #[test]
    fn validate_accepts_good_suggestion() {
        let validator = SuggestionValidator;
        let result =
            validator.validate(&suggestion(vec![change(json!(100), json!(220))], 0.9), &[]);

        assert!(result.valid);
        assert!(result.errors.is_empty());
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn validate_warns_on_negative_constraint() {
        let validator = SuggestionValidator;
        let result = validator.validate(
            &suggestion(vec![change(json!(1), json!(2))], 0.9),
            &[constraint("min_clearance", -1.0)],
        );

        assert!(result.valid);
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("min_clearance"));
    }

    #[test]
    fn kai_validate_suggestion_returns_ok_for_valid_suggestion() {
        let kai = kAI::new(kAIConfig::default());
        let result = kai
            .validate_suggestion(&suggestion(vec![change(json!(1), json!(2))], 0.5))
            .expect("deterministic validation should not fail");

        assert!(result.valid);
    }

    fn project_with_component() -> core::Project {
        let mut project = core::Project::new("ai-test");
        project.schematic.components.push(core::Component {
            id: uuid::Uuid::from_u128(1),
            reference_designator: "R1".to_string(),
            properties: vec![core::ComponentProperty {
                name: "Value".to_string(),
                value: "10k".to_string(),
            }],
            ..core::Component::default()
        });
        project
    }

    fn project_value(project: &core::Project) -> &str {
        &project.schematic.components[0].properties[0].value
    }

    fn component_suggestion(old: &str, new: &str, confidence: f32) -> OptimizationSuggestion {
        suggestion(
            vec![ChangeProposal::new(
                "component:R1",
                "value",
                serde_json::json!(old),
                serde_json::json!(new),
            )],
            confidence,
        )
    }

    #[test]
    fn apply_suggestion_changes_the_project_and_returns_a_transaction() {
        let mut project = project_with_component();
        let kai = kAI::new(kAIConfig::default());
        let request = component_suggestion("10k", "22k", 0.9);

        let applied = kai.apply_suggestion(&mut project, &request).expect("suggestion applies");

        assert_eq!(project_value(&project), "22k");
        assert_eq!(applied.suggestion_id, request.id);
        assert_eq!(applied.transaction.changes.len(), 1);
        assert_eq!(applied.transaction.id.get_version_num(), 5);
        assert!(applied.transaction.label.contains("test suggestion"));
    }

    #[test]
    fn apply_rejects_invalid_confidence_without_mutation() {
        let mut project = project_with_component();
        let before = serde_json::to_string(&project).expect("serialize");
        let kai = kAI::new(kAIConfig::default());
        let request = component_suggestion("10k", "22k", 1.5);

        let error = kai.apply_suggestion(&mut project, &request).expect_err("invalid confidence");

        assert!(matches!(error, kAIError::ValidationFailed(_)));
        assert_eq!(before, serde_json::to_string(&project).expect("serialize"));
    }

    #[test]
    fn apply_rejects_stale_changes_without_mutation() {
        let mut project = project_with_component();
        let before = serde_json::to_string(&project).expect("serialize");
        let kai = kAI::new(kAIConfig::default());
        let request = component_suggestion("1k", "22k", 0.9);

        let error = kai.apply_suggestion(&mut project, &request).expect_err("stale change");

        assert!(matches!(error, kAIError::ApplyFailed(core::edit::EditError::StaleChange { .. })));
        assert_eq!(before, serde_json::to_string(&project).expect("serialize"));
    }

    #[test]
    fn apply_rejects_empty_and_no_op_suggestions() {
        let mut project = project_with_component();
        let before = serde_json::to_string(&project).expect("serialize");
        let kai = kAI::new(kAIConfig::default());
        let requests = [suggestion(Vec::new(), 0.5), component_suggestion("10k", "10k", 0.5)];

        for request in requests {
            let error = kai.apply_suggestion(&mut project, &request).expect_err("invalid request");
            assert!(matches!(error, kAIError::ValidationFailed(_)));
        }
        assert_eq!(before, serde_json::to_string(&project).expect("serialize"));
    }

    #[test]
    fn apply_is_deterministic_for_the_same_starting_project() {
        let mut first = project_with_component();
        let mut second = first.clone();
        let kai = kAI::new(kAIConfig::default());
        let request = component_suggestion("10k", "22k", 0.9);

        let left = kai.apply_suggestion(&mut first, &request).expect("first apply");
        let right = kai.apply_suggestion(&mut second, &request).expect("second apply");

        assert_eq!(left.transaction.id, right.transaction.id);
        assert_eq!(
            serde_json::to_string(&first).expect("serialize"),
            serde_json::to_string(&second).expect("serialize")
        );
    }
}
