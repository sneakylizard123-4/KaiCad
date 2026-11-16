//! Field-level change proposals shared by the `ai` and `collab` crates.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// A proposed change to a single field of a design object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangeProposal {
    pub id: Uuid,
    pub target: String,
    pub field: String,
    pub old_value: Value,
    pub new_value: Value,
}

impl Default for ChangeProposal {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            target: String::new(),
            field: String::new(),
            old_value: Value::Null,
            new_value: Value::Null,
        }
    }
}

impl ChangeProposal {
    /// Create a proposal to replace `field` on `target` with `new_value`.
    pub fn new(
        target: impl Into<String>,
        field: impl Into<String>,
        old_value: Value,
        new_value: Value,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            target: target.into(),
            field: field.into(),
            old_value,
            new_value,
        }
    }
}
