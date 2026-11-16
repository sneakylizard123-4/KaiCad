//! Versioning management for collaboration sessions.

use crate::CollabSession;
use crate::VersionEntry;
use uuid::Uuid;

/// Collaboration session versioning.
#[derive(Debug, Clone)]
pub struct VersioningManager {
    pub sessions: Vec<CollabSession>,
    pub versions: Vec<VersionEntry>,
}

impl Default for VersioningManager {
    fn default() -> Self {
        Self::new()
    }
}

impl VersioningManager {
    pub fn new() -> Self {
        Self { sessions: Vec::new(), versions: Vec::new() }
    }

    /// Record a new version for `author`, bumping the version number.
    pub fn record(&mut self, author: Uuid, summary: impl Into<String>) -> u64 {
        let version = self.versions.len() as u64 + 1;
        self.versions.push(VersionEntry {
            version,
            timestamp: chrono::Utc::now(),
            author,
            change_summary: summary.into(),
        });
        version
    }

    pub fn latest(&self) -> Option<&VersionEntry> {
        self.versions.last()
    }
}
