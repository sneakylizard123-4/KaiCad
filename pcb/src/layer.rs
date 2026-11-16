//! PCB layer definitions.

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
}

#[cfg(test)]
mod tests {
    use super::LayerStack;
    use core::{LayerKind, PcbLayer};

    fn sample_stack() -> LayerStack {
        LayerStack::new(vec![
            PcbLayer::new(0, "F.Cu", LayerKind::Copper),
            PcbLayer::new(1, "F.SilkS", LayerKind::SilkScreen),
            PcbLayer::new(2, "B.Cu", LayerKind::Copper),
        ])
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
}
