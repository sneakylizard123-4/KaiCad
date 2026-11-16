//! Synchronization logic for collaboration sessions.

use crate::CollabSession;
use serde::{Deserialize, Serialize};

pub use core::proposal::ChangeProposal;

/// Synchronization manager for collaboration sessions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncManager {
    pub sessions: Vec<CollabSession>,
    pub pending_changes: Vec<ChangeProposal>,
}
