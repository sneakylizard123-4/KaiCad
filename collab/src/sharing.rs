//! Sharing functionality for collaboration sessions.

use crate::CollabSession;
use serde::{Deserialize, Serialize};

/// Sharing manager for project collaboration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SharingManager {
    pub sessions: Vec<CollabSession>,
    pub shared_projects: Vec<String>,
}