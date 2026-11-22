//! Versioning management for collaboration sessions.

use crate::{CollabError, CollabSession, VersionEntry};
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
            transaction_id: None,
            project_digest: None,
        });
        version
    }

    /// Record a version linked to an edit transaction and project digest.
    ///
    /// The digest is a UUIDv5 over the compact project JSON, so identical
    /// project content always produces the same value.
    pub fn record_transaction(
        &mut self,
        author: Uuid,
        summary: impl Into<String>,
        transaction: &core::edit::EditTransaction,
        project: &core::Project,
    ) -> Result<u64, CollabError> {
        let json = core::serialization::to_json(project)
            .map_err(|error| CollabError::History(error.to_string()))?;
        let digest = Uuid::new_v5(&Uuid::NAMESPACE_OID, json.as_bytes());
        let version = self.versions.len() as u64 + 1;
        self.versions.push(VersionEntry {
            version,
            timestamp: chrono::Utc::now(),
            author,
            change_summary: summary.into(),
            transaction_id: Some(transaction.id),
            project_digest: Some(digest),
        });
        Ok(version)
    }

    pub fn latest(&self) -> Option<&VersionEntry> {
        self.versions.last()
    }

    /// Find the first version recorded for `transaction_id`.
    pub fn find_by_transaction(&self, transaction_id: Uuid) -> Option<&VersionEntry> {
        self.versions.iter().find(|entry| entry.transaction_id == Some(transaction_id))
    }
}
