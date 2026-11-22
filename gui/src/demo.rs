//! A small deterministic board used when KaiCad Studio starts.

use core::{LayerKind, PcbLayer, PcbOutline, Point, Stackup};
use pcb::footprint::{Footprint, Outline, OutlineShape, PadPosition, PadType};
use pcb::{PcbDesign, Track, Via};
use uuid::Uuid;

const VBUS: u128 = 0x00CA_1CAD_1001;
const N1: u128 = 0x00CA_1CAD_1002;
const GND: u128 = 0x00CA_1CAD_1003;
const N2: u128 = 0x00CA_1CAD_1004;
const GND2: u128 = 0x00CA_1CAD_1005;

fn net(id: u128) -> Uuid {
    Uuid::from_u128(id)
}

fn pad(x: f64, y: f64, pad_type: PadType, width: f64, height: f64, net_id: u128) -> PadPosition {
    PadPosition { x, y, pad_type, width, height, rotation: 0.0, net_id: Some(net(net_id)) }
}

fn footprint(
    name: &str,
    reference: &str,
    position: Point,
    pads: Vec<PadPosition>,
    outline: Outline,
) -> Footprint {
    Footprint {
        name: name.to_string(),
        reference_designator: reference.to_string(),
        position,
        pad_positions: pads,
        outline,
    }
}

fn track(id: u128, net_id: u128, layer: u16, path: &[(f64, f64)]) -> Track {
    Track {
        id: Uuid::from_u128(id),
        net_id: net(net_id),
        layer,
        width: 0.35,
        path: path.iter().map(|(x, y)| Point::new(*x, *y)).collect(),
    }
}

fn via(id: u128, net_id: u128, x: f64, y: f64) -> Via {
    Via {
        id: Uuid::from_u128(id),
        net_id: Some(net(net_id)),
        position: Point::new(x, y),
        layer_start: 0,
        layer_end: 1,
        ..Via::default()
    }
}

/// Build the demo board shown on startup.
///
/// It is a deliberately simple two-layer circuit with through-hole power
/// pads, 0603-style parts, front and back copper, and two through vias. The
/// board is DRC-clean under the default rules.
pub fn demo_design() -> PcbDesign {
    let front = 0;
    let back = 1;

    PcbDesign {
        id: Uuid::from_u128(0x00CA_1CAD_D001),
        outline: PcbOutline::rectangle(80.0, 50.0),
        layers: vec![
            PcbLayer::new(front, "F.Cu", LayerKind::Copper),
            PcbLayer::new(back, "B.Cu", LayerKind::Copper),
            PcbLayer::new(2, "F.SilkS", LayerKind::SilkScreen),
        ],
        stackup: Stackup { layers: vec![front, back] },
        footprints: vec![
            footprint(
                "Conn_01x02",
                "J1",
                Point::new(8.0, 25.0),
                vec![
                    pad(0.0, -6.0, PadType::ThroughHole, 1.8, 1.8, VBUS),
                    pad(0.0, 6.0, PadType::ThroughHole, 1.8, 1.8, GND),
                ],
                Outline {
                    shape: OutlineShape::rounded_rectangle(-4.0, -8.0, 8.0, 16.0, 0.6),
                    copper: false,
                },
            ),
            footprint(
                "R_0603",
                "R1",
                Point::new(28.0, 15.0),
                vec![
                    pad(-0.8, 0.0, PadType::SurfaceMount, 1.4, 0.8, VBUS),
                    pad(0.8, 0.0, PadType::SurfaceMount, 1.4, 0.8, N1),
                ],
                Outline {
                    shape: OutlineShape::rounded_rectangle(-1.7, -0.9, 3.4, 1.8, 0.2),
                    copper: false,
                },
            ),
            footprint(
                "R_0603",
                "R2",
                Point::new(28.0, 35.0),
                vec![
                    pad(-0.8, 0.0, PadType::SurfaceMount, 1.4, 0.8, GND),
                    pad(0.8, 0.0, PadType::SurfaceMount, 1.4, 0.8, N2),
                ],
                Outline {
                    shape: OutlineShape::rounded_rectangle(-1.7, -0.9, 3.4, 1.8, 0.2),
                    copper: false,
                },
            ),
            footprint(
                "C_0603",
                "C1",
                Point::new(56.0, 15.0),
                vec![
                    pad(-0.8, 0.0, PadType::SurfaceMount, 1.2, 1.4, N1),
                    pad(0.8, 0.0, PadType::SurfaceMount, 1.2, 1.4, GND2),
                ],
                Outline {
                    shape: OutlineShape::rounded_rectangle(-1.7, -0.9, 3.4, 1.8, 0.2),
                    copper: false,
                },
            ),
            footprint(
                "LED_0603",
                "D1",
                Point::new(56.0, 35.0),
                vec![
                    pad(-0.8, 0.0, PadType::SurfaceMount, 1.2, 1.4, N2),
                    pad(0.8, 0.0, PadType::SurfaceMount, 1.2, 1.4, GND2),
                ],
                Outline {
                    shape: OutlineShape::rounded_rectangle(-1.7, -0.9, 3.4, 1.8, 0.2),
                    copper: false,
                },
            ),
        ],
        tracks: vec![
            track(
                0x00CA_1CAD_7001,
                VBUS,
                front,
                &[(8.0, 19.0), (20.0, 19.0), (24.0, 15.0), (27.2, 15.0)],
            ),
            track(0x00CA_1CAD_7002, N1, front, &[(28.8, 15.0), (40.0, 15.0), (55.2, 15.0)]),
            track(
                0x00CA_1CAD_7003,
                GND,
                front,
                &[(8.0, 31.0), (20.0, 31.0), (24.0, 35.0), (27.2, 35.0)],
            ),
            track(0x00CA_1CAD_7004, N2, front, &[(28.8, 35.0), (36.0, 35.0)]),
            track(0x00CA_1CAD_7005, N2, back, &[(36.0, 35.0), (44.0, 29.0), (52.0, 29.0)]),
            track(0x00CA_1CAD_7006, N2, front, &[(52.0, 29.0), (54.0, 33.0), (55.2, 35.0)]),
            track(
                0x00CA_1CAD_7007,
                GND2,
                front,
                &[(56.8, 35.0), (64.0, 35.0), (64.0, 25.0), (56.8, 15.0)],
            ),
        ],
        vias: vec![via(0x00CA_1CAD_8001, N2, 36.0, 35.0), via(0x00CA_1CAD_8002, N2, 52.0, 29.0)],
        ..PcbDesign::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use renderer::{RendererConfig, ViewMode, WindowConfig};

    #[test]
    fn demo_board_is_complete_and_drc_clean() {
        let design = demo_design();

        assert_eq!(design.copper_layer_count(), 2);
        assert_eq!(design.footprints.len(), 5);
        assert_eq!(design.tracks.len(), 7);
        assert_eq!(design.vias.len(), 2);

        let report = pcb::run_drc(&design, &design.rules);
        assert!(report.is_valid(), "demo board has DRC errors: {:?}", report.violations);
        assert!(report.violations.is_empty(), "demo board should start clean");
    }

    #[test]
    fn demo_board_renders_in_two_and_three_dimensions() {
        let design = demo_design();
        let config = RendererConfig {
            window: WindowConfig::new("demo", 320, 240),
            ..RendererConfig::default()
        };
        let mut renderer = renderer::Renderer::new(config.clone()).expect("renderer");
        let background = [config.background.r, config.background.g, config.background.b];
        let foreground =
            |pixels: &[u8]| pixels.chunks_exact(4).filter(|pixel| pixel[..3] != background).count();

        renderer.render_pcb(&design, ViewMode::TwoD).expect("2D render");
        assert!(foreground(renderer.framebuffer().pixels()) > 1_000);

        renderer.render_pcb(&design, ViewMode::ThreeD).expect("3D render");
        assert!(foreground(renderer.framebuffer().pixels()) > 1_000);
    }
}
