//! Bounded multi-step undo/redo history over [`crate::edit`] transactions.
//!
//! Each successful [`EditHistory::apply`] pushes one transaction onto the undo
//! stack and clears the redo branch. Undo and redo move transactions between
//! the two stacks only after the underlying project edit succeeds, so a failed
//! operation leaves both the project and the history untouched.
//!
//! The undo stack is bounded by [`EditHistory::limit`]; the oldest transaction
//! is evicted when a new edit exceeds the limit. History is intentionally not
//! serialized: it describes an editing session, not project data.

use std::fmt;

use crate::{Project, proposal::ChangeProposal};

/// Why a history operation could not be completed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryError {
    /// The underlying project edit failed.
    Edit(crate::edit::EditError),
    /// There is no transaction available to undo.
    EmptyUndo,
    /// There is no transaction available to redo.
    EmptyRedo,
    /// A history limit of zero is not usable.
    InvalidLimit,
}

impl fmt::Display for HistoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Edit(error) => write!(formatter, "history edit failed: {error}"),
            Self::EmptyUndo => write!(formatter, "no edit is available to undo"),
            Self::EmptyRedo => write!(formatter, "no edit is available to redo"),
            Self::InvalidLimit => write!(formatter, "history limit must be greater than zero"),
        }
    }
}

impl std::error::Error for HistoryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Edit(error) => Some(error),
            _ => None,
        }
    }
}

/// Bounded undo/redo stacks for project edits.
#[derive(Debug, Clone, PartialEq)]
pub struct EditHistory {
    undo_stack: Vec<crate::edit::EditTransaction>,
    redo_stack: Vec<crate::edit::EditTransaction>,
    limit: usize,
}

impl Default for EditHistory {
    fn default() -> Self {
        Self { undo_stack: Vec::new(), redo_stack: Vec::new(), limit: 100 }
    }
}

impl EditHistory {
    /// Create an empty history with a positive maximum undo depth.
    pub fn new(limit: usize) -> Result<Self, HistoryError> {
        if limit == 0 {
            return Err(HistoryError::InvalidLimit);
        }
        Ok(Self { undo_stack: Vec::new(), redo_stack: Vec::new(), limit })
    }

    /// Maximum number of retained undo transactions.
    pub fn limit(&self) -> usize {
        self.limit
    }

    /// Apply an edit, push it onto the undo stack and clear the redo branch.
    ///
    /// The project and both stacks are unchanged when the edit fails.
    pub fn apply(
        &mut self,
        project: &mut Project,
        label: &str,
        changes: &[ChangeProposal],
    ) -> Result<crate::edit::EditTransaction, HistoryError> {
        let transaction =
            crate::edit::apply(project, label, changes).map_err(HistoryError::Edit)?;

        self.redo_stack.clear();
        self.undo_stack.push(transaction.clone());
        self.enforce_limit();
        Ok(transaction)
    }

    /// Undo the newest transaction and move it to the redo stack.
    pub fn undo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let transaction = self.undo_stack.last().cloned().ok_or(HistoryError::EmptyUndo)?;
        crate::edit::undo(project, &transaction).map_err(HistoryError::Edit)?;

        self.undo_stack.pop();
        self.redo_stack.push(transaction);
        Ok(())
    }

    /// Redo the newest undone transaction and move it back to the undo stack.
    pub fn redo(&mut self, project: &mut Project) -> Result<(), HistoryError> {
        let transaction = self.redo_stack.last().cloned().ok_or(HistoryError::EmptyRedo)?;
        crate::edit::redo(project, &transaction).map_err(HistoryError::Edit)?;

        self.redo_stack.pop();
        self.undo_stack.push(transaction);
        self.enforce_limit();
        Ok(())
    }

    /// Whether an undo transaction is available.
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Whether a redo transaction is available.
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Label of the transaction that undo would apply.
    pub fn undo_label(&self) -> Option<&str> {
        self.undo_stack.last().map(|transaction| transaction.label.as_str())
    }

    /// Label of the transaction that redo would apply.
    pub fn redo_label(&self) -> Option<&str> {
        self.redo_stack.last().map(|transaction| transaction.label.as_str())
    }

    /// Number of retained undo transactions.
    pub fn undo_len(&self) -> usize {
        self.undo_stack.len()
    }

    /// Number of available redo transactions.
    pub fn redo_len(&self) -> usize {
        self.redo_stack.len()
    }

    /// Remove every undo and redo transaction.
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    fn enforce_limit(&mut self) {
        if self.undo_stack.len() > self.limit {
            let excess = self.undo_stack.len() - self.limit;
            self.undo_stack.drain(0..excess);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{EditHistory, HistoryError};
    use crate::{Component, ComponentProperty, Project, proposal::ChangeProposal};
    use serde_json::json;
    use uuid::Uuid;

    fn project() -> Project {
        let mut project = Project::new("history-test");
        project.schematic.components.push(Component {
            id: Uuid::from_u128(500),
            reference_designator: "R1".to_string(),
            properties: vec![ComponentProperty {
                name: "Value".to_string(),
                value: "10k".to_string(),
            }],
            ..Component::default()
        });
        project
    }

    fn change(old: &str, new: &str) -> ChangeProposal {
        ChangeProposal::new("component:R1", "value", json!(old), json!(new))
    }

    fn value(project: &Project) -> &str {
        &project.schematic.components[0].properties[0].value
    }

    fn project_json(project: &Project) -> String {
        serde_json::to_string(project).expect("serialize")
    }

    #[test]
    fn zero_limit_is_rejected_and_default_is_100() {
        assert!(matches!(EditHistory::new(0), Err(HistoryError::InvalidLimit)));

        let history = EditHistory::default();
        assert_eq!(history.limit(), 100);
        assert_eq!(history.undo_len(), 0);
        assert_eq!(history.redo_len(), 0);
    }

    #[test]
    fn apply_pushes_undo_and_leaves_redo_empty() {
        let mut project = project();
        let mut history = EditHistory::default();

        history.apply(&mut project, "one", &[change("10k", "22k")]).expect("apply");

        assert_eq!(history.undo_len(), 1);
        assert_eq!(history.redo_len(), 0);
        assert!(history.can_undo());
        assert!(!history.can_redo());
        assert_eq!(history.undo_label(), Some("one"));
    }

    #[test]
    fn undo_restores_previous_json_and_moves_the_transaction_to_redo() {
        let mut project = project();
        let before = project_json(&project);
        let mut history = EditHistory::default();
        history.apply(&mut project, "one", &[change("10k", "22k")]).expect("apply");
        let edited = project_json(&project);

        history.undo(&mut project).expect("undo");

        assert_eq!(project_json(&project), before);
        assert_ne!(project_json(&project), edited);
        assert_eq!(history.undo_len(), 0);
        assert_eq!(history.redo_len(), 1);
        assert_eq!(history.redo_label(), Some("one"));
    }

    #[test]
    fn redo_restores_edited_json_and_moves_the_transaction_back() {
        let mut project = project();
        let mut history = EditHistory::default();
        history.apply(&mut project, "one", &[change("10k", "22k")]).expect("apply");
        let edited = project_json(&project);
        history.undo(&mut project).expect("undo");

        history.redo(&mut project).expect("redo");

        assert_eq!(project_json(&project), edited);
        assert_eq!(history.undo_len(), 1);
        assert_eq!(history.redo_len(), 0);
    }

    #[test]
    fn undo_on_empty_history_is_rejected() {
        let mut project = project();
        let before = project_json(&project);
        let mut history = EditHistory::default();

        let error = history.undo(&mut project).expect_err("nothing to undo");

        assert!(matches!(error, HistoryError::EmptyUndo));
        assert_eq!(project_json(&project), before);
        assert_eq!(history.undo_len(), 0);
    }

    #[test]
    fn redo_on_empty_history_is_rejected() {
        let mut project = project();
        let before = project_json(&project);
        let mut history = EditHistory::default();

        let error = history.redo(&mut project).expect_err("nothing to redo");

        assert!(matches!(error, HistoryError::EmptyRedo));
        assert_eq!(project_json(&project), before);
        assert_eq!(history.redo_len(), 0);
    }

    #[test]
    fn new_apply_after_undo_clears_the_redo_branch() {
        let mut project = project();
        let mut history = EditHistory::default();
        history.apply(&mut project, "one", &[change("10k", "22k")]).expect("one");
        history.undo(&mut project).expect("undo");
        assert_eq!(history.redo_len(), 1);

        history.apply(&mut project, "branch", &[change("10k", "33k")]).expect("branch");

        assert_eq!(history.redo_len(), 0);
        assert_eq!(history.undo_len(), 1);
        assert_eq!(history.undo_label(), Some("branch"));
        assert_eq!(value(&project), "33k");
    }

    #[test]
    fn multiple_undos_run_in_reverse_order() {
        let mut project = project();
        let mut history = EditHistory::default();
        history.apply(&mut project, "one", &[change("10k", "22k")]).expect("one");
        history.apply(&mut project, "two", &[change("22k", "33k")]).expect("two");
        history.apply(&mut project, "three", &[change("33k", "47k")]).expect("three");

        history.undo(&mut project).expect("undo three");
        assert_eq!(value(&project), "33k");
        history.undo(&mut project).expect("undo two");

        assert_eq!(value(&project), "22k");
        assert_eq!(history.undo_len(), 1);
        assert_eq!(history.redo_len(), 2);
    }

    #[test]
    fn multiple_redos_run_in_forward_order() {
        let mut project = project();
        let mut history = EditHistory::default();
        history.apply(&mut project, "one", &[change("10k", "22k")]).expect("one");
        history.apply(&mut project, "two", &[change("22k", "33k")]).expect("two");
        history.apply(&mut project, "three", &[change("33k", "47k")]).expect("three");
        history.undo(&mut project).expect("undo three");
        history.undo(&mut project).expect("undo two");

        history.redo(&mut project).expect("redo two");
        assert_eq!(value(&project), "33k");
        history.redo(&mut project).expect("redo three");

        assert_eq!(value(&project), "47k");
        assert_eq!(history.undo_len(), 3);
        assert_eq!(history.redo_len(), 0);
    }

    #[test]
    fn labels_identify_the_next_transactions() {
        let mut project = project();
        let mut history = EditHistory::default();
        assert_eq!(history.undo_label(), None);
        assert_eq!(history.redo_label(), None);

        history.apply(&mut project, "one", &[change("10k", "22k")]).expect("one");
        history.apply(&mut project, "two", &[change("22k", "33k")]).expect("two");
        assert_eq!(history.undo_label(), Some("two"));

        history.undo(&mut project).expect("undo");
        assert_eq!(history.redo_label(), Some("two"));
        assert_eq!(history.undo_label(), Some("one"));
    }

    #[test]
    fn predicates_match_stack_state() {
        let mut project = project();
        let mut history = EditHistory::default();
        assert!(!history.can_undo());
        assert!(!history.can_redo());

        history.apply(&mut project, "one", &[change("10k", "22k")]).expect("one");
        assert!(history.can_undo());
        assert!(!history.can_redo());

        history.undo(&mut project).expect("undo");
        assert!(!history.can_undo());
        assert!(history.can_redo());

        history.redo(&mut project).expect("redo");
        assert!(history.can_undo());
        assert!(!history.can_redo());
    }

    #[test]
    fn clear_empties_both_stacks() {
        let mut project = project();
        let mut history = EditHistory::default();
        history.apply(&mut project, "one", &[change("10k", "22k")]).expect("one");
        history.apply(&mut project, "two", &[change("22k", "33k")]).expect("two");
        history.undo(&mut project).expect("undo");

        history.clear();

        assert_eq!(history.undo_len(), 0);
        assert_eq!(history.redo_len(), 0);
        assert!(!history.can_undo());
        assert!(!history.can_redo());
        assert_eq!(history.undo_label(), None);
        assert_eq!(history.redo_label(), None);
    }

    #[test]
    fn limit_evicts_the_oldest_undo_entry() {
        let mut project = project();
        let mut history = EditHistory::new(2).expect("valid limit");
        history.apply(&mut project, "one", &[change("10k", "22k")]).expect("one");
        history.apply(&mut project, "two", &[change("22k", "33k")]).expect("two");
        history.apply(&mut project, "three", &[change("33k", "47k")]).expect("three");

        assert_eq!(history.undo_len(), 2);
        assert_eq!(history.undo_label(), Some("three"));
        history.undo(&mut project).expect("undo three");
        history.undo(&mut project).expect("undo two");

        assert_eq!(value(&project), "22k");
        assert!(!history.can_undo());
        assert_eq!(history.redo_len(), 2);
    }

    #[test]
    fn failed_apply_leaves_history_unchanged() {
        let mut project = project();
        let mut history = EditHistory::default();
        history.apply(&mut project, "one", &[change("10k", "22k")]).expect("one");

        let error =
            history.apply(&mut project, "stale", &[change("10k", "33k")]).expect_err("stale edit");

        assert!(matches!(error, HistoryError::Edit(_)));
        assert_eq!(value(&project), "22k");
        assert_eq!(history.undo_len(), 1);
        assert_eq!(history.redo_len(), 0);
        assert_eq!(history.undo_label(), Some("one"));
    }

    #[test]
    fn failed_undo_leaves_stacks_unchanged() {
        let mut project = project();
        let mut history = EditHistory::default();
        history.apply(&mut project, "one", &[change("10k", "22k")]).expect("one");
        project.schematic.components[0].properties[0].value = "33k".to_string();

        let error = history.undo(&mut project).expect_err("stale undo");

        assert!(matches!(error, HistoryError::Edit(_)));
        assert_eq!(value(&project), "33k");
        assert_eq!(history.undo_len(), 1);
        assert_eq!(history.redo_len(), 0);
    }

    #[test]
    fn failed_redo_leaves_stacks_unchanged() {
        let mut project = project();
        let mut history = EditHistory::default();
        history.apply(&mut project, "one", &[change("10k", "22k")]).expect("one");
        history.undo(&mut project).expect("undo");
        project.schematic.components[0].properties[0].value = "33k".to_string();

        let error = history.redo(&mut project).expect_err("stale redo");

        assert!(matches!(error, HistoryError::Edit(_)));
        assert_eq!(value(&project), "33k");
        assert_eq!(history.undo_len(), 0);
        assert_eq!(history.redo_len(), 1);
    }

    #[test]
    fn undo_len_never_exceeds_the_limit() {
        let mut project = project();
        let mut history = EditHistory::new(1).expect("valid limit");
        history.apply(&mut project, "one", &[change("10k", "22k")]).expect("one");
        history.apply(&mut project, "two", &[change("22k", "33k")]).expect("two");
        history.apply(&mut project, "three", &[change("33k", "47k")]).expect("three");

        assert_eq!(history.undo_len(), 1);
        assert_eq!(history.undo_label(), Some("three"));
        history.undo(&mut project).expect("undo");
        assert_eq!(history.undo_len(), 0);
        assert_eq!(history.redo_len(), 1);
        assert!(matches!(history.undo(&mut project), Err(HistoryError::EmptyUndo)));
    }

    #[test]
    fn history_error_exposes_the_edit_source() {
        let mut project = project();
        let mut history = EditHistory::default();
        let error =
            history.apply(&mut project, "stale", &[change("1k", "22k")]).expect_err("stale edit");

        assert!(matches!(error, HistoryError::Edit(_)));
        assert!(std::error::Error::source(&error).is_some());
        assert!(error.to_string().contains("history edit failed"));
        assert_eq!(
            HistoryError::InvalidLimit.to_string(),
            "history limit must be greater than zero"
        );
    }
}
