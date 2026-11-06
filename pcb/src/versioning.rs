//! Versioning management for collaboration sessions.

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Version entry for a project.
#[derive(Debug, Clone)]
pub struct VersionEntry {
    pub version: u64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub author: uuid::Uuid,
    pub change_summary: String,
}

/// Collaboration session versioning.
#[derive(Debug, Clone)]
pub struct VersioningManager {
    pub sessions: Vec<CollabSession>,
    pub versions: Vec<VersionEntry>,
}
