//! KaiCad collaboration layer — project sharing, versioning, real-time sync.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub mod sharing;
pub mod sync;
pub mod versioning;
pub mod window;

pub use sharing::SharingManager;
pub use sync::SyncManager;
pub use versioning::VersioningManager;
pub use window::Window;

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

/// Why a collaboration operation was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollabError {
    UnknownSession(Uuid),
    UnknownParticipant(Uuid),
    DuplicateParticipant(Uuid),
    ReadOnly,
    Edit(core::edit::EditError),
    History(String),
}

impl std::fmt::Display for CollabError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownSession(id) => write!(formatter, "unknown collaboration session {id}"),
            Self::UnknownParticipant(id) => {
                write!(formatter, "unknown collaboration participant {id}")
            }
            Self::DuplicateParticipant(id) => {
                write!(formatter, "participant {id} is already in the session")
            }
            Self::ReadOnly => write!(formatter, "participant does not have edit permission"),
            Self::Edit(error) => write!(formatter, "collaborative edit failed: {error}"),
            Self::History(message) => write!(formatter, "version history failed: {message}"),
        }
    }
}

impl std::error::Error for CollabError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Edit(error) => Some(error),
            _ => None,
        }
    }
}

/// Version history entry for a project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VersionEntry {
    pub version: u64,
    pub timestamp: DateTime<Utc>,
    pub author: Uuid,
    pub change_summary: String,
    /// Transaction that produced this version, when it was recorded from one.
    #[serde(default)]
    pub transaction_id: Option<Uuid>,
    /// Deterministic UUIDv5 digest of the project JSON at this version.
    #[serde(default)]
    pub project_digest: Option<Uuid>,
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

    /// Add a participant to a session, rejecting duplicate user ids.
    pub fn add_participant(
        &mut self,
        session_id: Uuid,
        participant: Participant,
    ) -> Result<(), CollabError> {
        let index = self
            .sessions
            .iter()
            .position(|session| session.session_id == session_id)
            .ok_or(CollabError::UnknownSession(session_id))?;
        if self.sessions[index]
            .participants
            .iter()
            .any(|existing| existing.user_id == participant.user_id)
        {
            return Err(CollabError::DuplicateParticipant(participant.user_id));
        }
        self.sessions[index].participants.push(participant);
        Ok(())
    }

    /// Replace a participant's role.
    pub fn set_role(
        &mut self,
        session_id: Uuid,
        user_id: Uuid,
        role: ParticipantRole,
    ) -> Result<(), CollabError> {
        let index = self
            .sessions
            .iter()
            .position(|session| session.session_id == session_id)
            .ok_or(CollabError::UnknownSession(session_id))?;
        let participant = self.sessions[index]
            .participants
            .iter_mut()
            .find(|participant| participant.user_id == user_id)
            .ok_or(CollabError::UnknownParticipant(user_id))?;
        participant.role = role;
        Ok(())
    }

    /// Whether `user_id` may edit in `session_id`.
    pub fn can_edit(&self, session_id: Uuid, user_id: Uuid) -> bool {
        let Some(session) = self.sessions.iter().find(|session| session.session_id == session_id)
        else {
            return false;
        };
        session.participants.iter().any(|participant| {
            participant.user_id == user_id
                && matches!(participant.role, ParticipantRole::Owner | ParticipantRole::Editor)
        })
    }

    /// Apply an atomic edit through a session and bump its version on success.
    ///
    /// Unknown sessions, unknown participants and viewers are rejected before
    /// the project is touched. A failed [`core::edit::apply`] leaves both the
    /// project and the session version unchanged.
    pub fn apply_edit(
        &mut self,
        session_id: Uuid,
        user_id: Uuid,
        project: &mut core::Project,
        label: &str,
        changes: &[core::ChangeProposal],
    ) -> Result<core::edit::EditTransaction, CollabError> {
        let index = self
            .sessions
            .iter()
            .position(|session| session.session_id == session_id)
            .ok_or(CollabError::UnknownSession(session_id))?;
        let participant = self.sessions[index]
            .participants
            .iter()
            .find(|participant| participant.user_id == user_id)
            .ok_or(CollabError::UnknownParticipant(user_id))?;
        if participant.role == ParticipantRole::Viewer {
            return Err(CollabError::ReadOnly);
        }

        let transaction = core::edit::apply(project, label, changes).map_err(CollabError::Edit)?;
        self.sessions[index].version += 1;
        Ok(transaction)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sharing::SharingManager;
    use crate::sync::SyncManager;
    use crate::versioning::VersioningManager;
    use crate::window::Window;
    use serde_json::json;

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

    fn project() -> core::Project {
        let mut project = core::Project::new("collab-test");
        project.schematic.components.push(core::Component {
            id: Uuid::from_u128(100),
            reference_designator: "R1".to_string(),
            properties: vec![core::ComponentProperty {
                name: "Value".to_string(),
                value: "10k".to_string(),
            }],
            ..core::Component::default()
        });
        project
    }

    fn participant(user_id: Uuid, username: &str, role: ParticipantRole) -> Participant {
        Participant { user_id, username: username.to_string(), role }
    }

    fn change() -> core::ChangeProposal {
        core::ChangeProposal::new("component:R1", "value", json!("10k"), json!("22k"))
    }

    fn project_value(project: &core::Project) -> &str {
        &project.schematic.components[0].properties[0].value
    }

    fn setup() -> (CollabManager, Uuid, Uuid, Uuid, Uuid) {
        let mut manager = CollabManager::new();
        let owner = Uuid::from_u128(10);
        let session = manager.create_session(Uuid::from_u128(1), owner);
        let editor = Uuid::from_u128(11);
        let viewer = Uuid::from_u128(12);
        manager
            .add_participant(
                session.session_id,
                participant(editor, "editor", ParticipantRole::Editor),
            )
            .expect("editor is added");
        manager
            .add_participant(
                session.session_id,
                participant(viewer, "viewer", ParticipantRole::Viewer),
            )
            .expect("viewer is added");
        (manager, session.session_id, owner, editor, viewer)
    }

    fn session_version(manager: &CollabManager) -> u64 {
        manager.sessions[0].version
    }

    #[test]
    fn add_participants_and_reject_duplicates() {
        let (mut manager, session_id, _, editor, _) = setup();

        assert_eq!(manager.sessions[0].participants.len(), 3);
        let error = manager
            .add_participant(
                session_id,
                participant(editor, "editor-again", ParticipantRole::Editor),
            )
            .expect_err("duplicate participant");
        assert!(matches!(error, CollabError::DuplicateParticipant(id) if id == editor));

        let extra = Uuid::from_u128(13);
        manager
            .add_participant(session_id, participant(extra, "viewer-2", ParticipantRole::Viewer))
            .expect("new participant is added");
        assert_eq!(manager.sessions[0].participants.len(), 4);
    }

    #[test]
    fn can_edit_allows_owner_and_editor_and_rejects_viewer_and_unknown_users() {
        let (manager, session_id, owner, editor, viewer) = setup();

        assert!(manager.can_edit(session_id, owner));
        assert!(manager.can_edit(session_id, editor));
        assert!(!manager.can_edit(session_id, viewer));
        assert!(!manager.can_edit(session_id, Uuid::from_u128(99)));
        assert!(!manager.can_edit(Uuid::from_u128(99), owner));
    }

    #[test]
    fn unknown_session_is_rejected_by_membership_and_editing_methods() {
        let mut manager = CollabManager::new();
        let session_id = Uuid::from_u128(99);
        let user = Uuid::from_u128(10);
        let mut project = project();
        let before = serde_json::to_string(&project).expect("serialize");

        assert!(matches!(
            manager.add_participant(session_id, participant(user, "user", ParticipantRole::Owner)),
            Err(CollabError::UnknownSession(id)) if id == session_id
        ));
        assert!(!manager.can_edit(session_id, user));
        assert!(matches!(
            manager.set_role(session_id, user, ParticipantRole::Editor),
            Err(CollabError::UnknownSession(id)) if id == session_id
        ));
        assert!(matches!(
            manager.apply_edit(session_id, user, &mut project, "edit", &[change()]),
            Err(CollabError::UnknownSession(id)) if id == session_id
        ));
        assert_eq!(before, serde_json::to_string(&project).expect("serialize"));
    }

    #[test]
    fn set_role_changes_permissions() {
        let (mut manager, session_id, _, _, viewer) = setup();

        assert!(!manager.can_edit(session_id, viewer));
        manager.set_role(session_id, viewer, ParticipantRole::Editor).expect("promote viewer");
        assert!(manager.can_edit(session_id, viewer));
        manager.set_role(session_id, viewer, ParticipantRole::Viewer).expect("demote viewer");
        assert!(!manager.can_edit(session_id, viewer));
        assert!(matches!(
            manager.set_role(session_id, Uuid::from_u128(99), ParticipantRole::Editor),
            Err(CollabError::UnknownParticipant(_))
        ));
    }

    #[test]
    fn viewer_edit_is_rejected_atomically() {
        let (mut manager, session_id, _, _, viewer) = setup();
        let mut project = project();
        let before = serde_json::to_string(&project).expect("serialize");

        let error = manager
            .apply_edit(session_id, viewer, &mut project, "viewer edit", &[change()])
            .expect_err("viewer cannot edit");

        assert!(matches!(error, CollabError::ReadOnly));
        assert_eq!(session_version(&manager), 0);
        assert_eq!(before, serde_json::to_string(&project).expect("serialize"));
    }

    #[test]
    fn owner_and_editor_edits_bump_the_session_version() {
        let (mut manager, session_id, owner, editor, _) = setup();
        let mut project = project();

        manager
            .apply_edit(session_id, owner, &mut project, "owner edit", &[change()])
            .expect("owner edit applies");
        assert_eq!(session_version(&manager), 1);
        assert_eq!(project_value(&project), "22k");

        manager
            .apply_edit(
                session_id,
                editor,
                &mut project,
                "editor edit",
                &[core::ChangeProposal::new("component:R1", "value", json!("22k"), json!("33k"))],
            )
            .expect("editor edit applies");
        assert_eq!(session_version(&manager), 2);
        assert_eq!(project_value(&project), "33k");
    }

    #[test]
    fn failed_stale_edit_leaves_project_and_version_unchanged() {
        let (mut manager, session_id, owner, _, _) = setup();
        let mut project = project();
        manager
            .apply_edit(session_id, owner, &mut project, "first", &[change()])
            .expect("first edit applies");

        let error = manager
            .apply_edit(
                session_id,
                owner,
                &mut project,
                "stale",
                &[core::ChangeProposal::new("component:R1", "value", json!("10k"), json!("33k"))],
            )
            .expect_err("stale edit");

        assert!(matches!(error, CollabError::Edit(core::edit::EditError::StaleChange { .. })));
        assert_eq!(project_value(&project), "22k");
        assert_eq!(session_version(&manager), 1);
    }

    #[test]
    fn apply_edit_matches_a_direct_core_edit() {
        let (mut manager, session_id, owner, _, _) = setup();
        let mut through_session = project();
        let mut direct = through_session.clone();
        let changes = [change()];

        let collaborative = manager
            .apply_edit(session_id, owner, &mut through_session, "same edit", &changes)
            .expect("collaborative edit");
        let standalone =
            core::edit::apply(&mut direct, "same edit", &changes).expect("direct edit");

        assert_eq!(collaborative, standalone);
        assert_eq!(
            serde_json::to_string(&through_session).expect("serialize"),
            serde_json::to_string(&direct).expect("serialize")
        );
    }

    #[test]
    fn record_transaction_stores_id_and_digest() {
        let (mut manager, session_id, owner, _, _) = setup();
        let mut project = project();
        let transaction = manager
            .apply_edit(session_id, owner, &mut project, "edit", &[change()])
            .expect("edit applies");
        let mut history = VersioningManager::new();

        let version = history
            .record_transaction(owner, "recorded edit", &transaction, &project)
            .expect("version is recorded");

        let entry = history.latest().expect("entry exists");
        assert_eq!(version, 1);
        assert_eq!(entry.transaction_id, Some(transaction.id));
        assert!(entry.project_digest.is_some_and(|digest| !digest.is_nil()));
    }

    #[test]
    fn digest_is_stable_for_identical_json_and_changes_after_an_edit() {
        let mut project = project();
        let first = core::edit::apply(&mut project, "one", &[change()]).expect("edit one");
        let mut history = VersioningManager::new();
        history
            .record_transaction(Uuid::from_u128(10), "one", &first, &project)
            .expect("first version");
        let first_digest = history.latest().and_then(|entry| entry.project_digest).expect("digest");

        history
            .record_transaction(Uuid::from_u128(10), "same json", &first, &project)
            .expect("second version");
        let second_digest =
            history.latest().and_then(|entry| entry.project_digest).expect("digest");
        assert_eq!(first_digest, second_digest);

        let second = core::edit::apply(
            &mut project,
            "two",
            &[core::ChangeProposal::new("component:R1", "value", json!("22k"), json!("33k"))],
        )
        .expect("edit two");
        history
            .record_transaction(Uuid::from_u128(10), "two", &second, &project)
            .expect("third version");
        let third_digest = history.latest().and_then(|entry| entry.project_digest).expect("digest");
        assert_ne!(first_digest, third_digest);
    }

    #[test]
    fn legacy_record_stores_no_transaction_or_digest() {
        let mut history = VersioningManager::new();
        assert_eq!(history.record(Uuid::from_u128(10), "legacy"), 1);

        let entry = history.latest().expect("entry exists");
        assert_eq!(entry.transaction_id, None);
        assert_eq!(entry.project_digest, None);
    }

    #[test]
    fn find_by_transaction_returns_the_recorded_version() {
        let mut project = project();
        let transaction = core::edit::apply(&mut project, "edit", &[change()]).expect("edit");
        let mut history = VersioningManager::new();
        let version = history
            .record_transaction(Uuid::from_u128(10), "edit", &transaction, &project)
            .expect("record");

        let found = history.find_by_transaction(transaction.id).expect("found");
        assert_eq!(found.version, version);
        assert!(history.find_by_transaction(Uuid::from_u128(999)).is_none());
    }

    #[test]
    fn version_numbers_continue_sequentially() {
        let mut project = project();
        let mut history = VersioningManager::new();
        assert_eq!(history.record(Uuid::from_u128(10), "one"), 1);
        let transaction = core::edit::apply(&mut project, "two", &[change()]).expect("edit");
        assert_eq!(
            history
                .record_transaction(Uuid::from_u128(10), "two", &transaction, &project)
                .expect("record"),
            2
        );
        assert_eq!(history.record(Uuid::from_u128(10), "three"), 3);
        assert_eq!(history.latest().map(|entry| entry.version), Some(3));
    }

    #[test]
    fn stage_deduplicates_by_transaction_id() {
        let mut project = project();
        let transaction = core::edit::apply(&mut project, "edit", &[change()]).expect("edit");
        let mut sync = SyncManager::default();

        assert!(sync.stage(transaction.clone()));
        assert!(!sync.stage(transaction));
        assert_eq!(sync.staged_count(), 1);
    }

    #[test]
    fn drain_preserves_order_and_empties_the_queue() {
        let mut project = project();
        let first = core::edit::apply(&mut project, "one", &[change()]).expect("one");
        let second = core::edit::apply(
            &mut project,
            "two",
            &[core::ChangeProposal::new("component:R1", "value", json!("22k"), json!("33k"))],
        )
        .expect("two");
        let mut sync = SyncManager::default();
        assert!(sync.stage(first.clone()));
        assert!(sync.stage(second.clone()));

        let drained = sync.drain();
        assert_eq!(
            drained.iter().map(|transaction| transaction.id).collect::<Vec<_>>(),
            vec![first.id, second.id]
        );
        assert_eq!(sync.staged_count(), 0);
        assert!(sync.drain().is_empty());
    }

    #[test]
    fn staging_does_not_mutate_the_project() {
        let mut project = project();
        let transaction = core::edit::apply(&mut project, "edit", &[change()]).expect("edit");
        let before = serde_json::to_string(&project).expect("serialize");
        let mut sync = SyncManager::default();

        assert!(sync.stage(transaction));
        assert_eq!(before, serde_json::to_string(&project).expect("serialize"));
    }

    #[test]
    fn sync_manager_round_trips_staged_transactions() {
        let mut project = project();
        let transaction = core::edit::apply(&mut project, "edit", &[change()]).expect("edit");
        let mut sync = SyncManager::default();
        assert!(sync.stage(transaction));

        let encoded = serde_json::to_string(&sync).expect("serialize");
        let decoded: SyncManager = serde_json::from_str(&encoded).expect("deserialize");
        assert_eq!(decoded.staged, sync.staged);

        let legacy: SyncManager = serde_json::from_str(r#"{"sessions":[],"pending_changes":[]}"#)
            .expect("legacy sync manager loads");
        assert!(legacy.staged.is_empty());
    }

    #[test]
    fn unknown_participant_edit_is_rejected_without_mutation() {
        let (mut manager, session_id, _, _, _) = setup();
        let mut project = project();
        let before = serde_json::to_string(&project).expect("serialize");

        let error = manager
            .apply_edit(session_id, Uuid::from_u128(99), &mut project, "unknown user", &[change()])
            .expect_err("unknown participant");

        assert!(matches!(error, CollabError::UnknownParticipant(_)));
        assert_eq!(session_version(&manager), 0);
        assert_eq!(before, serde_json::to_string(&project).expect("serialize"));
    }
}
