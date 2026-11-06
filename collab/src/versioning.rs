//! Versioning management for collaboration sessions.

use crate::CollabSession;
use crate::VersionEntry;

/// Collaboration session versioning.
#[derive(Debug, Clone)]
pub struct VersioningManager {
    pub sessions: Vec<CollabSession>,
    pub versions: Vec<VersionEntry>,
}