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
    design.tracks.push(pcb::Track {
        layer: 0,
        width: 0.25,
        path: vec![core::Point::new(10.0, 10.0), core::Point::new(90.0, 70.0)],
        ..pcb::Track::default()
    });

    let stack = pcb::LayerStack::new(design.layers.clone());
    let issues = stack.validate(&design.stackup);
    let drc = pcb::run_drc(&design, &design.rules);

    println!("KaiCad board outline example (pre-alpha)");
    println!("  outline points:      {}", design.outline.points.len());
    println!("  outline closed:      {}", design.outline.is_closed());
    println!("  outline area:        {:.1} mm^2", design.outline.area());
    println!("  outline perimeter:   {:.1} mm", design.outline.perimeter());
    println!("  layers:              {}", design.layers.len());
    println!("  layer issues:        {}", issues.len());
    println!("  copper layers:       {}", design.copper_layer_count());
    println!("  tracks:              {}", design.tracks.len());
    println!("  drc violations:      {}", drc.violations.len());
    println!("  drc valid:           {}", drc.is_valid());
    for layer in &design.layers {
        println!("    [{}] {} ({:?})", layer.id, layer.name, layer.kind);
    }
}
