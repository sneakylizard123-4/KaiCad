//! Synchronization logic for collaboration sessions.

use crate::CollabSession;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// Synchronization manager for collaboration sessions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncManager {
    pub sessions: Vec<CollabSession>,
    pub pending_changes: Vec<ChangeProposal>,
}

/// A change proposal from an AI suggestion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeProposal {
    pub id: Uuid,
    pub target: String,
    pub field: String,
    pub old_value: Value,
    pub new_value: Value,
}