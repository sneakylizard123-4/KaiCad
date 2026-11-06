//! KaiCad collaboration layer — project sharing, versioning, real-time sync.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub mod versioning;
pub mod sync;
pub mod sharing;
pub mod window;

/// Collaboration session for sharing projects between users.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollabSession {
    pub session_id: Uuid,
    pub project_id: Uuid,
    pub participants: Vec<Participant>,
    pub version: u64,
}

/// A participant in a collaboration session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Participant {
    pub user_id: Uuid,
    pub username: String,
    pub role: ParticipantRole,
}

/// Role of a participant in a collaboration session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParticipantRole {
    Owner,
    Editor,
    Viewer,
}

/// Version history entry for a project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionEntry {
    pub version: u64,
    pub timestamp: DateTime<Utc>,
    pub author: Uuid,
    pub change_summary: String,
}

/// Collab manager for handling project sharing and sync.
pub struct CollabManager {
    pub sessions: Vec<CollabSession>,
    pub versions: Vec<VersionEntry>,
}

impl CollabManager {
    pub fn new() -> Self {
        Self {
            sessions: Vec::new(),
            versions: Vec::new(),
        }
    }

    pub fn create_session(&mut self, _project_id: Uuid, _owner_id: Uuid) -> CollabSession {
        CollabSession {
            session_id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            participants: Vec::new(),
            version: 0,
        }
    }
}