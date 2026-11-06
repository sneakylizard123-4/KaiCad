//! Window management for the collaboration layer.

use crate::Participant;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A window in the collaboration interface.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Window {
    pub id: Uuid,
    pub title: String,
    pub participants: Vec<Participant>,
    pub version: u64,
}