//! Window management for the collaboration layer.

use ash::{Instance, Device};
use ash::vk::{SurfaceKHR, Format, Extent2D, ShaderModule, PrimitiveTopology, PhysicalDevice};
use ash::khr::swapchain;
use uuid::Uuid;
use serde::{Serialize, Deserialize};

/// A window in the collaboration interface.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Window {
    pub id: uuid::Uuid,
    pub title: String,
    pub participants: Vec<Participant>,
    pub version: u64,
}

/// A participant in a collaboration session.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Participant {
    pub user_id: uuid::Uuid,
    pub username: String,
    pub role: ParticipantRole,
}

/// Role of a participant.
#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
pub enum ParticipantRole {
    #[default] Owner,
    Editor,
    Viewer,
}