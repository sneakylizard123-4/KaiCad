//! KaiCad collaboration layer — project sharing, versioning, real-time sync.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub mod sharing;
pub mod sync;
pub mod versioning;
pub mod window;

/// Collaboration session for sharing projects between users.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollabSession {
    pub session_id: Uuid,
    pub project_id: Uuid,
    pub participants: Vec<Participant>,
    pub version: u64,
}

/// A participant in a collaboration session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Participant {
    pub user_id: Uuid,
    pub username: String,
    pub role: ParticipantRole,
}

/// Role of a participant in a collaboration session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ParticipantRole {
    Owner,
    Editor,
    Viewer,
}

/// Version history entry for a project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

impl Default for CollabManager {
    fn default() -> Self {
        Self::new()
    }
}

impl CollabManager {
    pub fn new() -> Self {
        Self { sessions: Vec::new(), versions: Vec::new() }
    }

    /// Create a session for `project_id`, owned by `owner_id`.
    pub fn create_session(&mut self, project_id: Uuid, owner_id: Uuid) -> CollabSession {
        let session = CollabSession {
            session_id: Uuid::new_v4(),
            project_id,
            participants: vec![Participant {
                user_id: owner_id,
                username: format!("user-{}", &owner_id.to_string()[..8]),
                role: ParticipantRole::Owner,
            }],
            version: 0,
        };
        self.sessions.push(session.clone());
        session
    }

    /// Number of sessions currently tracked by this manager.
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sharing::SharingManager;
    use crate::versioning::VersioningManager;
    use crate::window::Window;

    #[test]
    fn create_session_uses_passed_project_id() {
        let mut manager = CollabManager::new();
        let project_id = Uuid::new_v4();
        let owner_id = Uuid::new_v4();

        let session = manager.create_session(project_id, owner_id);

        assert_eq!(session.project_id, project_id);
        assert_ne!(session.session_id, Uuid::nil());
        assert_eq!(session.version, 0);
    }

    #[test]
    fn create_session_creates_owner_participant() {
        let mut manager = CollabManager::new();
        let owner_id = Uuid::new_v4();

        let session = manager.create_session(Uuid::new_v4(), owner_id);

        assert_eq!(session.participants.len(), 1);
        let owner = &session.participants[0];
        assert_eq!(owner.user_id, owner_id);
        assert_eq!(owner.role, ParticipantRole::Owner);
        assert_eq!(owner.username, format!("user-{}", &owner_id.to_string()[..8]));
    }

    #[test]
    fn create_session_is_stored_in_sessions() {
        let mut manager = CollabManager::new();

        let session = manager.create_session(Uuid::new_v4(), Uuid::new_v4());

        assert_eq!(manager.sessions.len(), 1);
        assert_eq!(manager.sessions[0], session);
    }

    #[test]
    fn session_count_tracks_sessions() {
        let mut manager = CollabManager::new();
        assert_eq!(manager.session_count(), 0);

        manager.create_session(Uuid::new_v4(), Uuid::new_v4());
        manager.create_session(Uuid::new_v4(), Uuid::new_v4());

        assert_eq!(manager.session_count(), 2);
        assert_eq!(manager.session_count(), manager.sessions.len());
    }

    #[test]
    fn record_increments_versions_and_latest_returns_it() {
        let mut manager = VersioningManager::new();
        assert!(manager.latest().is_none());

        let author = Uuid::new_v4();
        let first = manager.record(author, "initial import");
        let second = manager.record(author, "moved resistor");

        assert_eq!(first, 1);
        assert_eq!(second, 2);
        assert_eq!(manager.versions.len(), 2);

        let latest = manager.latest().expect("a version was recorded");
        assert_eq!(latest.version, 2);
        assert_eq!(latest.author, author);
        assert_eq!(latest.change_summary, "moved resistor");
    }

    #[test]
    fn share_is_idempotent() {
        let mut manager = SharingManager::default();

        assert!(manager.share("project-a"));
        assert!(!manager.share("project-a"));
        assert_eq!(manager.shared_projects, vec!["project-a".to_string()]);

        assert!(manager.share("project-b"));
        assert_eq!(manager.shared_projects.len(), 2);
    }

    #[test]
    fn unshare_removes_shared_project() {
        let mut manager = SharingManager::default();
        manager.share("project-a");

        assert!(manager.unshare("project-a"));
        assert!(!manager.unshare("project-a"));
        assert!(manager.shared_projects.is_empty());
    }

    #[test]
    fn pending_changes_round_trip_through_json() {
        use crate::sync::SyncManager;
        use serde_json::json;

        let mut manager = SyncManager::default();
        manager.pending_changes.push(core::ChangeProposal::new(
            "R1",
            "value",
            json!(100),
            json!(220),
        ));

        let encoded = serde_json::to_string(&manager).expect("sync manager serializes");
        let decoded: SyncManager =
            serde_json::from_str(&encoded).expect("sync manager deserializes");

        assert_eq!(decoded.pending_changes, manager.pending_changes);
        assert_eq!(decoded.pending_changes[0].new_value, json!(220));
    }

    #[test]
    fn window_new_has_defaults() {
        let window = Window::new("Schematic");

        assert_eq!(window.title, "Schematic");
        assert!(window.participants.is_empty());
        assert_eq!(window.version, 0);
        assert_ne!(window.id, Uuid::nil());
    }
}
