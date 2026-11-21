//! PCB layer definitions and stack validation.

use std::collections::{BTreeSet, HashSet};

/// A problem found while validating a layer stack.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LayerIssue {
    /// The layer id the issue refers to. Stackup ids with no layer use the id
    /// listed in the stackup.
    pub layer_id: u16,
    pub kind: LayerIssueKind,
    pub description: String,
}

/// The kind of problem a [`LayerIssue`] describes.
///
/// The declaration order is the tie-break when issues share a layer id.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum LayerIssueKind {
    /// More than one layer uses the same id.
    DuplicateId,
    /// More than one layer uses the same non-empty name.
    DuplicateName,
    /// The layer has no name.
    EmptyName,
    /// The stackup lists an id that has no layer.
    MissingFromStackup,
    /// The stackup lists a layer that is not made of copper.
    NonCopperInStackup,
}

/// An ordered collection of PCB layers.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct LayerStack {
    pub layers: Vec<core::PcbLayer>,
}

impl LayerStack {
    /// Build a layer stack from its layers.
    pub fn new(layers: Vec<core::PcbLayer>) -> Self {
        Self { layers }
    }

    /// Append a layer to the top of the stack.
    pub fn push(&mut self, layer: core::PcbLayer) {
        self.layers.push(layer);
    }

    /// Number of layers in the stack.
    pub fn len(&self) -> usize {
        self.layers.len()
    }

    /// Whether the stack contains no layers.
    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    /// All copper layers in the stack, in order.
    pub fn copper_layers(&self) -> Vec<&core::PcbLayer> {
        self.layers.iter().filter(|layer| layer.is_copper()).collect()
    }

    /// Find a layer by its id.
    pub fn find(&self, id: u16) -> Option<&core::PcbLayer> {
        self.layers.iter().find(|layer| layer.id == id)
    }

    /// Validate the layers and, when it is non-empty, their stackup ordering.
    ///
    /// Duplicate ids and duplicate names are reported once against every layer
    /// that carries them, so a name shared by three layers produces three
    /// issues. An empty stackup imposes no ordering and is not an error.
    /// Issues are sorted by `(layer_id, kind)`, which makes the report
    /// deterministic.
    pub fn validate(&self, stackup: &core::Stackup) -> Vec<LayerIssue> {
        let mut issues = Vec::new();

        for layer in &self.layers {
            let same_id = self.layers.iter().filter(|other| other.id == layer.id).count();
            if same_id > 1 {
                issues.push(LayerIssue {
                    layer_id: layer.id,
                    kind: LayerIssueKind::DuplicateId,
                    description: format!("layer id {} is used by {same_id} layers", layer.id),
                });
            }

            if layer.name.is_empty() {
                issues.push(LayerIssue {
                    layer_id: layer.id,
                    kind: LayerIssueKind::EmptyName,
                    description: format!("layer {} has no name", layer.id),
                });
            } else {
                let same_name = self.layers.iter().filter(|other| other.name == layer.name).count();
                if same_name > 1 {
                    issues.push(LayerIssue {
                        layer_id: layer.id,
                        kind: LayerIssueKind::DuplicateName,
                        description: format!(
                            "layer name {:?} is used by {same_name} layers",
                            layer.name
                        ),
                    });
                }
            }
        }

        let mut missing = BTreeSet::new();
        let mut non_copper = BTreeSet::new();
        for id in &stackup.layers {
            match self.find(*id) {
                None => {
                    missing.insert(*id);
                }
                Some(layer) if !layer.is_copper() => {
                    non_copper.insert(*id);
                }
                Some(_) => {}
            }
        }

        for id in missing {
            issues.push(LayerIssue {
                layer_id: id,
                kind: LayerIssueKind::MissingFromStackup,
                description: format!("stackup id {id} has no layer"),
            });
        }
        for id in non_copper {
            let name = self.find(id).map(|layer| layer.name.as_str()).unwrap_or_default();
            issues.push(LayerIssue {
                layer_id: id,
                kind: LayerIssueKind::NonCopperInStackup,
                description: format!("stackup id {id} refers to non-copper layer {name:?}"),
            });
        }

        issues.sort_by_key(|issue| (issue.layer_id, issue.kind));
        issues
    }

    /// Copper layers in stackup order.
    ///
    /// An empty stackup falls back to stack order. Stackup ids with no layer or
    /// with a non-copper layer are skipped, and repeated ids are reported once.
    pub fn copper_layers_in_stackup<'a>(
        &'a self,
        stackup: &core::Stackup,
    ) -> Vec<&'a core::PcbLayer> {
        if stackup.layers.is_empty() {
            return self.copper_layers();
        }

        let mut seen = HashSet::new();
        let mut copper = Vec::new();
        for id in &stackup.layers {
            if !seen.insert(*id) {
                continue;
            }
            if let Some(layer) = self.find(*id) {
                if layer.is_copper() {
                    copper.push(layer);
                }
            }
        }
        copper
    }
}

#[cfg(test)]
mod tests {
    use super::{LayerIssueKind, LayerStack};
    use core::{LayerKind, PcbLayer};

    fn sample_stack() -> LayerStack {
        LayerStack::new(vec![
            PcbLayer::new(0, "F.Cu", LayerKind::Copper),
            PcbLayer::new(1, "F.SilkS", LayerKind::SilkScreen),
            PcbLayer::new(2, "B.Cu", LayerKind::Copper),
        ])
    }

    fn stackup(ids: &[u16]) -> core::Stackup {
        core::Stackup { layers: ids.to_vec() }
    }

    #[test]
    fn new_and_len_report_the_layers() {
        let stack = sample_stack();

        assert_eq!(stack.len(), 3);
        assert!(!stack.is_empty());
    }

    #[test]
    fn default_stack_is_empty() {
        let stack = LayerStack::default();

        assert!(stack.is_empty());
        assert_eq!(stack.len(), 0);
    }

    #[test]
    fn push_appends_a_layer() {
        let mut stack = LayerStack::default();
        stack.push(PcbLayer::new(0, "F.Cu", LayerKind::Copper));

        assert_eq!(stack.len(), 1);
        assert_eq!(stack.layers[0].name, "F.Cu");
    }

    #[test]
    fn copper_layers_filters_non_copper_layers() {
        let stack = sample_stack();

        let copper = stack.copper_layers();

        assert_eq!(copper.len(), 2);
        assert_eq!(copper[0].id, 0);
        assert_eq!(copper[1].id, 2);
    }

    #[test]
    fn find_returns_the_layer_with_the_matching_id() {
        let stack = sample_stack();

        let found = stack.find(1).expect("layer 1 exists");
        assert_eq!(found.name, "F.SilkS");
        assert_eq!(found.kind, LayerKind::SilkScreen);

        assert!(stack.find(99).is_none());
    }

    #[test]
    fn validate_accepts_a_consistent_stack() {
        let stack = sample_stack();

        assert!(stack.validate(&stackup(&[0, 2])).is_empty());
        assert!(stack.validate(&core::Stackup::default()).is_empty());
    }

    #[test]
    fn validate_reports_every_layer_with_a_duplicate_id() {
        let stack = LayerStack::new(vec![
            PcbLayer::new(0, "F.Cu", LayerKind::Copper),
            PcbLayer::new(1, "F.SilkS", LayerKind::SilkScreen),
            PcbLayer::new(1, "B.SilkS", LayerKind::SilkScreen),
            PcbLayer::new(3, "B.Cu", LayerKind::Copper),
        ]);

        let issues = stack.validate(&core::Stackup::default());

        assert_eq!(issues.len(), 2);
        assert!(issues.iter().all(|issue| issue.layer_id == 1));
        assert!(issues.iter().all(|issue| issue.kind == LayerIssueKind::DuplicateId));
    }

    #[test]
    fn validate_reports_duplicate_and_empty_names() {
        let stack = LayerStack::new(vec![
            PcbLayer::new(0, "F.Cu", LayerKind::Copper),
            PcbLayer::new(1, "F.Cu", LayerKind::SilkScreen),
            PcbLayer::new(2, "", LayerKind::Mechanical),
        ]);

        let issues = stack.validate(&core::Stackup::default());

        assert_eq!(issues.len(), 3);
        assert_eq!((issues[0].layer_id, issues[0].kind), (0, LayerIssueKind::DuplicateName));
        assert_eq!((issues[1].layer_id, issues[1].kind), (1, LayerIssueKind::DuplicateName));
        assert_eq!((issues[2].layer_id, issues[2].kind), (2, LayerIssueKind::EmptyName));
    }

    #[test]
    fn validate_reports_stackup_problems_in_layer_order() {
        let stack = LayerStack::new(vec![
            PcbLayer::new(0, "F.Cu", LayerKind::Copper),
            PcbLayer::new(1, "F.SilkS", LayerKind::SilkScreen),
        ]);

        let issues = stack.validate(&stackup(&[7, 1, 0, 7, 1]));

        assert_eq!(issues.len(), 2);
        assert_eq!((issues[0].layer_id, issues[0].kind), (1, LayerIssueKind::NonCopperInStackup));
        assert_eq!((issues[1].layer_id, issues[1].kind), (7, LayerIssueKind::MissingFromStackup));
    }

    #[test]
    fn copper_layers_in_stackup_follows_order_and_falls_back() {
        let stack = sample_stack();

        let ordered = stack.copper_layers_in_stackup(&stackup(&[2, 0, 1, 2]));
        assert_eq!(ordered.len(), 2);
        assert_eq!(ordered[0].id, 2);
        assert_eq!(ordered[1].id, 0);

        let fallback = stack.copper_layers_in_stackup(&core::Stackup::default());
        assert_eq!(fallback.len(), 2);
        assert_eq!(fallback[0].id, 0);
        assert_eq!(fallback[1].id, 2);
    }
}
