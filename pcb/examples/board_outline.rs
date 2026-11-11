//! Construct a board-outline shape and print a short summary.
//!
//! Run with:
//!
//! ```text
//! cargo run -p pcb --example board_outline
//! ```

use pcb::footprint::{Outline, OutlineShape};

fn describe(shape: &OutlineShape) -> String {
    match shape {
        OutlineShape::Rectangle { x, y, width, height } => {
            format!("rectangle at ({x}, {y}) sized {width} x {height} mm")
        }
        OutlineShape::RoundedRectangle { x, y, width, height, radius } => format!(
            "rounded rectangle at ({x}, {y}) sized {width} x {height} mm, radius {radius} mm"
        ),
    }
}

fn main() {
    // `OutlineShape` exposes `rectangle()` and `rounded_rectangle()`
    // constructors; building the variants directly works just as well.
    let outline = OutlineShape::rounded_rectangle(0.0, 0.0, 100.0, 80.0, 3.0);
    let footprint_outline = Outline { shape: outline.clone(), copper: false };

    println!("KaiCad board outline example (pre-alpha)");
    println!("  shape:  {}", describe(&outline));
    println!("  copper: {}", footprint_outline.copper);
    println!("  corners: 4 (2D footprint outline; no layer or stackup data yet)");
}
