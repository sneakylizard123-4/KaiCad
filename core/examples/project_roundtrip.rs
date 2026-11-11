//! Build a small [`Project`], round-trip it through JSON, and print the result.
//!
//! Run with:
//!
//! ```text
//! cargo run -p core --example project_roundtrip
//! ```

use core::Project;
use core::net::{Net, NetClass};
use core::symbol::{Pin, PinDirection, PinPosition, PinType, Symbol};
use uuid::Uuid;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Start from the default project and give it a name/version.
    let mut project = Project {
        name: "Roundtrip Demo".to_string(),
        version: "0.1.0".to_string(),
        ..Project::default()
    };

    // Add a single passive symbol (one pin) to the schematic.
    let pin = Pin {
        id: Uuid::new_v4(),
        number: "1".to_string(),
        name: "~".to_string(),
        electrical_type: PinType::Passive,
        position: PinPosition::default(),
        direction: PinDirection::In,
    };
    let symbol = Symbol {
        id: Uuid::new_v4(),
        name: "R".to_string(),
        pins: vec![pin],
        library: "Device".to_string(),
    };

    // Connect the symbol pin to a power net.
    let net = Net {
        id: Uuid::new_v4(),
        name: "VCC".to_string(),
        pins: vec![symbol.pins[0].id],
        netclass: NetClass::Power,
    };

    project.schematic.symbols.push(symbol);
    project.schematic.nets.push(net);

    // Serialize the whole project to pretty JSON.
    let json = serde_json::to_string_pretty(&project)?;
    println!("{json}");

    // Deserialize it again and confirm the identity survived the round trip.
    let decoded: Project = serde_json::from_str(&json)?;
    assert_eq!(decoded.id, project.id, "project id changed during round trip");
    assert_eq!(decoded.name, project.name, "project name changed");
    assert_eq!(decoded.version, project.version, "project version changed");
    assert_eq!(decoded.schematic.symbols.len(), 1);
    assert_eq!(decoded.schematic.nets.len(), 1);
    assert_eq!(decoded.schematic.nets[0].name, "VCC");
    assert_eq!(decoded.schematic.nets[0].netclass, NetClass::Power);

    println!(
        "round trip ok: {} ({}), {} symbol(s), {} net(s)",
        decoded.name,
        decoded.version,
        decoded.schematic.symbols.len(),
        decoded.schematic.nets.len()
    );

    Ok(())
}
