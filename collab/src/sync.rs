//! Synchronization logic for collaboration sessions.

use crate::CollabSession;
use serde::{Deserialize, Serialize};

pub use core::proposal::ChangeProposal;

/// Synchronization manager for collaboration sessions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncManager {
    pub sessions: Vec<CollabSession>,
    pub pending_changes: Vec<ChangeProposal>,
    /// Whole transactions waiting to be sent to collaborators.
    #[serde(default)]
    pub staged: Vec<core::edit::EditTransaction>,
}

impl SyncManager {
    /// Queue a transaction, returning `false` when its id is already staged.
    pub fn stage(&mut self, transaction: core::edit::EditTransaction) -> bool {
        if self.staged.iter().any(|existing| existing.id == transaction.id) {
            return false;
        }
        self.staged.push(transaction);
        true
    }

    /// Remove every staged transaction, preserving insertion order.
    pub fn drain(&mut self) -> Vec<core::edit::EditTransaction> {
        std::mem::take(&mut self.staged)
    }

    /// Number of transactions waiting to be sent.
    pub fn staged_count(&self) -> usize {
        self.staged.len()
    }
}
