//! Opaque source data retained for lossless file interchange.
//!
//! KaiCad normally discards constructs it cannot model. A compatibility import
//! can retain those constructs here so that a later export can re-emit them.
//! The source strings are canonical S-expressions and are never interpreted by
//! the engineering engines.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Source-file metadata and preserved constructs for one imported document.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceDocument {
    /// Source format identifier, for example `kicad_pcb` or `kicad_sch`.
    pub format: String,
    /// Format version atom/string when the source records one.
    #[serde(default)]
    pub version: Option<String>,
    /// Generator name when the source records one.
    #[serde(default)]
    pub generator: Option<String>,
    /// Root document UUID when the source records one.
    #[serde(default)]
    pub uuid: Option<Uuid>,
    /// Constructs that have no corresponding KaiCad model field.
    #[serde(default)]
    pub preserved: Vec<PreservedNode>,
}

impl SourceDocument {
    /// Create an empty source document for `format`.
    pub fn new(format: impl Into<String>) -> Self {
        Self { format: format.into(), ..Self::default() }
    }

    /// Whether a preserved node with `head` exists.
    ///
    /// This is used by exporters to avoid emitting a generated construct when
    /// the original source already contains it.
    pub fn contains(&self, head: &str) -> bool {
        self.preserved.iter().any(|node| node.head == head)
    }
}

/// One unmodelled source construct retained verbatim in canonical form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreservedNode {
    /// S-expression head, for example `zone` or `fp_line`.
    pub head: String,
    /// Canonical S-expression text.
    pub source: String,
}
