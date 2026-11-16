//! Construct a board outline and layer stack, then print a short summary.
//!
//! Run with:
//!
//! ```text
//! cargo run -p pcb --example board_outline
//! ```

use pcb::PcbDesign;

fn main() {
    let mut design =
        PcbDesign { outline: core::PcbOutline::rectangle(100.0, 80.0), ..PcbDesign::default() };
    design.layers.push(core::PcbLayer::new(0, "F.Cu", core::LayerKind::Copper));
    design.stackup.layers.push(0);

    println!("KaiCad board outline example (pre-alpha)");
    println!("  outline points:      {}", design.outline.points.len());
    println!("  outline closed:      {}", design.outline.is_closed());
    println!("  layers:              {}", design.layers.len());
    println!("  copper layers:       {}", design.copper_layer_count());
    for layer in &design.layers {
        println!("    [{}] {} ({:?})", layer.id, layer.name, layer.kind);
    }
}
