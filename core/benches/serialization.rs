//! Criterion benchmarks for the core data model's JSON serialization.
//!
//! Run with:
//!
//! ```text
//! cargo bench -p core
//! ```
//!
//! Compile without running with:
//!
//! ```text
//! cargo bench -p core --no-run
//! ```

use core::Project;
use core::net::{Net, NetClass};
use core::sheet::{Label, LabelPosition, LabelType, Sheet, Wire, WirePoint};
use core::symbol::{Pin, PinDirection, PinPosition, PinType, Symbol};
use criterion::{Criterion, black_box, criterion_group, criterion_main};
use uuid::Uuid;

/// Build a project with enough data to make serialization costs visible.
fn populated_project(symbol_count: usize, net_count: usize) -> Project {
    let mut project = Project {
        name: "Benchmark Project".to_string(),
        version: "0.1.0".to_string(),
        ..Project::default()
    };
    project.metadata.author = "KaiCad benchmarks".to_string();
    project.metadata.description = "Synthetic project used by the serialization bench".to_string();

    for index in 0..symbol_count {
        let pins = (0..4)
            .map(|pin_number| Pin {
                id: Uuid::new_v4(),
                number: pin_number.to_string(),
                name: format!("P{pin_number}"),
                electrical_type: PinType::Passive,
                position: PinPosition { x: f64::from(pin_number) * 2.54, y: 0.0, angle: 0.0 },
                direction: PinDirection::In,
            })
            .collect();

        project.schematic.symbols.push(Symbol {
            id: Uuid::new_v4(),
            name: format!("SYMBOL_{index}"),
            pins,
            library: "BenchLib".to_string(),
        });
    }

    for index in 0..net_count {
        let mut net = Net {
            id: Uuid::new_v4(),
            name: format!("NET_{index}"),
            pins: Vec::new(),
            netclass: match index % 3 {
                0 => NetClass::Power,
                1 => NetClass::Signal,
                _ => NetClass::HighSpeed,
            },
        };

        // Attach the first pin of two different symbols to the net.
        if symbol_count > 0 {
            let first = &project.schematic.symbols[index % symbol_count];
            let second = &project.schematic.symbols[(index + 1) % symbol_count];
            if let Some(pin) = first.pins.first() {
                net.pins.push(pin.id);
            }
            if let Some(pin) = second.pins.first() {
                net.pins.push(pin.id);
            }
        }

        project.schematic.nets.push(net);
    }

    project.schematic.sheets.push(Sheet {
        id: Uuid::new_v4(),
        name: "Root".to_string(),
        parent: None,
        children: Vec::new(),
        components: project.schematic.symbols.iter().map(|s| s.id).collect(),
        wires: vec![Wire {
            id: Uuid::new_v4(),
            points: vec![
                WirePoint { x: 0.0, y: 0.0 },
                WirePoint { x: 25.4, y: 0.0 },
                WirePoint { x: 25.4, y: 12.7 },
            ],
            ..Default::default()
        }],
        labels: vec![Label {
            id: Uuid::new_v4(),
            name: "VCC".to_string(),
            label_type: LabelType::Global,
            position: LabelPosition::default(),
            ..Default::default()
        }],
        ..Default::default()
    });

    project
}

fn project_json(c: &mut Criterion) {
    let project = populated_project(100, 100);
    let json = serde_json::to_string(&project).expect("project should serialize");

    let mut group = c.benchmark_group("project_json");
    group.bench_function("serialize", |b| {
        b.iter(|| serde_json::to_string(black_box(&project)).expect("project should serialize"));
    });
    group.bench_function("deserialize", |b| {
        b.iter(|| {
            serde_json::from_str::<Project>(black_box(&json)).expect("project should deserialize")
        });
    });
    group.finish();
}

criterion_group!(benches, project_json);
criterion_main!(benches);
