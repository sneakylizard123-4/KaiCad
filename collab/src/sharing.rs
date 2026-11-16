//! Sharing functionality for collaboration sessions.

use crate::CollabSession;
use serde::{Deserialize, Serialize};

/// Sharing manager for project collaboration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SharingManager {
    pub sessions: Vec<CollabSession>,
    pub shared_projects: Vec<String>,
}

impl SharingManager {
    /// Share `project`, returning `false` if it is already shared.
    pub fn share(&mut self, project: impl Into<String>) -> bool {
        let project = project.into();
        if self.shared_projects.contains(&project) {
            return false;
        }
        self.shared_projects.push(project);
        true
    }

    /// Stop sharing `project`, returning `false` if it was not shared.
    pub fn unshare(&mut self, project: &str) -> bool {
        let Some(index) = self.shared_projects.iter().position(|shared| shared == project) else {
            return false;
        };
        self.shared_projects.remove(index);
        true
    }
}
