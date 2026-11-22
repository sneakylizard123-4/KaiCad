//! Deterministic 2D and 3D rasterization of PCB designs.

use pcb::PcbDesign;

use crate::framebuffer::pixel_count;
use crate::{Camera, Color, Framebuffer, RendererError, ViewMode};

const BOARD_THICKNESS: f64 = 1.6;
const BACKGROUND: Color = Color::rgb(10, 13, 19);
const BOARD_FILL: Color = Color::rgb(19, 79, 46);
const BOARD_EDGE: Color = Color::rgb(151, 214, 172);
const COPPER_TOP: Color = Color::rgb(232, 145, 51);
const COPPER_BOTTOM: Color = Color::rgb(74, 139, 231);
const COPPER_INNER_A: Color = Color::rgb(183, 82, 190);
const COPPER_INNER_B: Color = Color::rgb(73, 190, 151);
const PAD_COPPER: Color = Color::rgb(225, 187, 80);
const VIA_COPPER: Color = Color::rgb(221, 196, 117);
const DRILL: Color = Color::rgb(28, 24, 17);

#[derive(Debug, Clone, Copy)]
struct Bounds {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

#[derive(Debug, Clone)]
struct PadShape {
    corners: [(f64, f64); 4],
}

pub(crate) fn render_pcb(
    design: &PcbDesign,
    framebuffer: &mut Framebuffer,
    depth: &mut [f64],
    view: ViewMode,
    camera: &Camera,
) -> Result<(), RendererError> {
    match view {
        ViewMode::TwoD => render_2d(design, framebuffer),
        ViewMode::ThreeD => render_3d(design, framebuffer, depth, camera),
    }
}

fn render_2d(design: &PcbDesign, framebuffer: &mut Framebuffer) -> Result<(), RendererError> {
    framebuffer.clear(BACKGROUND);
    let transform =
        crate::BoardTransform::for_board(design, framebuffer.width(), framebuffer.height())?;
    let scale = transform.scale();
    let map = |point: (f64, f64)| {
        let [x, y] = transform.board_to_pixel(point.0, point.1);
        (x, y)
    };

    let outline: Vec<(f64, f64)> =
        design.outline.points.iter().map(|point| map((point.x, point.y))).collect();
    fill_polygon(framebuffer, &outline, BOARD_FILL);

    let copper = copper_layers(design);
    for layer in copper.iter().rev() {
        let colour = copper_colour(design, *layer);
        for track in design.tracks.iter().filter(|track| track.layer == *layer) {
            if !track.width.is_finite() || track.width <= 0.0 {
                continue;
            }
            let line_width = (track.width * scale).max(1.0);
            for segment in track.path.windows(2) {
                draw_thick_line(
                    framebuffer,
                    map((segment[0].x, segment[0].y)),
                    map((segment[1].x, segment[1].y)),
                    line_width,
                    colour,
                );
            }
        }
    }

    for pad in pad_shapes(design) {
        let corners: Vec<(f64, f64)> = pad.corners.iter().copied().map(map).collect();
        fill_polygon(framebuffer, &corners, PAD_COPPER);
    }

    for via in &design.vias {
        if !via.position.x.is_finite()
            || !via.position.y.is_finite()
            || !via.diameter.is_finite()
            || via.diameter <= 0.0
        {
            continue;
        }
        let (x, y) = map((via.position.x, via.position.y));
        fill_circle(framebuffer, x, y, via.diameter * scale / 2.0, VIA_COPPER);
        if via.drill.is_finite() && via.drill > 0.0 {
            fill_circle(framebuffer, x, y, via.drill * scale / 2.0, DRILL);
        }
    }

    let outline_width = (scale * 0.15).clamp(1.0, 4.0);
    for index in 0..outline.len() {
        draw_thick_line(
            framebuffer,
            outline[index],
            outline[(index + 1) % outline.len()],
            outline_width,
            BOARD_EDGE,
        );
    }

    Ok(())
}

fn render_3d(
    design: &PcbDesign,
    framebuffer: &mut Framebuffer,
    depth: &mut [f64],
    camera: &Camera,
) -> Result<(), RendererError> {
    let pixel_count = pixel_count(framebuffer.width(), framebuffer.height())
        .ok_or_else(|| RendererError::InvalidConfig("framebuffer has no pixels".to_string()))?;
    if depth.len() != pixel_count {
        return Err(RendererError::InvalidConfig(
            "depth buffer does not match the framebuffer".to_string(),
        ));
    }
    framebuffer.clear(BACKGROUND);
    depth.fill(f64::INFINITY);

    board_bounds(design)?;
    let projector = Projector::new(camera, framebuffer.width(), framebuffer.height())?;
    let mut faces = Vec::new();
    add_board(&mut faces, design);
    for track in &design.tracks {
        if !track.width.is_finite() || track.width <= 0.0 {
            continue;
        }
        let colour = copper_colour(design, track.layer);
        for segment in track.path.windows(2) {
            if let Some(quad) = segment_quad(
                (segment[0].x, segment[0].y),
                (segment[1].x, segment[1].y),
                track.width,
                BOARD_THICKNESS + 0.02,
            ) {
                add_quad(&mut faces, quad, colour);
            }
        }
    }
    for pad in pad_shapes(design) {
        let quad = pad.corners.map(|(x, y)| [x, y, BOARD_THICKNESS + 0.04]);
        add_quad(&mut faces, quad, PAD_COPPER);
    }
    for via in &design.vias {
        if !via.position.x.is_finite()
            || !via.position.y.is_finite()
            || !via.diameter.is_finite()
            || via.diameter <= 0.0
        {
            continue;
        }
        add_prism(
            &mut faces,
            (via.position.x, via.position.y),
            via.diameter / 2.0,
            0.0,
            BOARD_THICKNESS + 0.06,
            VIA_COPPER,
        );
        if via.drill.is_finite() && via.drill > 0.0 {
            add_disc(
                &mut faces,
                (via.position.x, via.position.y),
                via.drill / 2.0,
                BOARD_THICKNESS + 0.08,
                DRILL,
            );
        }
    }

    for face in &faces {
        let Some(vertices) = projector.project(face.vertices) else {
            continue;
        };
        rasterize(framebuffer, depth, vertices, face.colour);
    }
    Ok(())
}

fn board_bounds(design: &PcbDesign) -> Result<Bounds, RendererError> {
    let bbox = design
        .outline
        .bounding_box()
        .ok_or_else(|| RendererError::InvalidConfig("board outline is empty".to_string()))?;
    let bounds =
        Bounds { min_x: bbox.min.x, min_y: bbox.min.y, max_x: bbox.max.x, max_y: bbox.max.y };
    if !bounds.min_x.is_finite()
        || !bounds.min_y.is_finite()
        || !bounds.max_x.is_finite()
        || !bounds.max_y.is_finite()
        || bounds.max_x < bounds.min_x
        || bounds.max_y < bounds.min_y
    {
        return Err(RendererError::InvalidConfig("board outline has invalid bounds".to_string()));
    }
    Ok(bounds)
}

fn copper_layers(design: &PcbDesign) -> Vec<u16> {
    let mut layers = Vec::new();
    for id in &design.stackup.layers {
        if !layers.contains(id) && design.layers.iter().any(|l| l.id == *id && l.is_copper()) {
            layers.push(*id);
        }
    }
    if layers.is_empty() {
        for layer in &design.layers {
            if layer.is_copper() && !layers.contains(&layer.id) {
                layers.push(layer.id);
            }
        }
    }
    layers
}

fn copper_colour(design: &PcbDesign, layer: u16) -> Color {
    let layers = copper_layers(design);
    match layers.iter().position(|id| *id == layer) {
        Some(0) if layers.len() == 1 => COPPER_TOP,
        Some(0) => COPPER_TOP,
        Some(index) if index + 1 == layers.len() => COPPER_BOTTOM,
        Some(index) if index % 2 == 0 => COPPER_INNER_A,
        _ => COPPER_INNER_B,
    }
}

fn pad_shapes(design: &PcbDesign) -> Vec<PadShape> {
    let mut pads = Vec::new();
    for footprint in &design.footprints {
        for pad in &footprint.pad_positions {
            if !pad.width.is_finite()
                || pad.width <= 0.0
                || !pad.height.is_finite()
                || pad.height <= 0.0
                || !pad.rotation.is_finite()
            {
                continue;
            }
            let center = (footprint.position.x + pad.x, footprint.position.y + pad.y);
            let (sin, cos) = pad.rotation.to_radians().sin_cos();
            let (half_width, half_height) = (pad.width / 2.0, pad.height / 2.0);
            let local = [
                (-half_width, -half_height),
                (half_width, -half_height),
                (half_width, half_height),
                (-half_width, half_height),
            ];
            let corners =
                local.map(|(x, y)| (center.0 + x * cos - y * sin, center.1 + x * sin + y * cos));
            pads.push(PadShape { corners });
        }
    }
    pads
}

fn fill_polygon(framebuffer: &mut Framebuffer, polygon: &[(f64, f64)], colour: Color) {
    if polygon.len() < 3 {
        return;
    }
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for point in polygon {
        min_y = min_y.min(point.1);
        max_y = max_y.max(point.1);
    }
    let y_start = min_y.floor().max(0.0) as i64;
    let y_end = max_y.ceil().min(framebuffer.height() as f64) as i64;

    for y in y_start..y_end {
        let scan_y = y as f64 + 0.5;
        let mut crossings = Vec::new();
        for index in 0..polygon.len() {
            let (x1, y1) = polygon[index];
            let (x2, y2) = polygon[(index + 1) % polygon.len()];
            if (y1 > scan_y) != (y2 > scan_y) {
                let x = x1 + (scan_y - y1) * (x2 - x1) / (y2 - y1);
                if x.is_finite() {
                    crossings.push(x);
                }
            }
        }
        crossings.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        for pair in crossings.chunks_exact(2) {
            let x_start = pair[0].ceil().max(0.0);
            let x_end = pair[1].floor().min(framebuffer.width() as f64 - 1.0);
            if x_start > x_end {
                continue;
            }
            for x in x_start as u32..=x_end as u32 {
                framebuffer.set_pixel(x, y as u32, colour);
            }
        }
    }
}

fn draw_thick_line(
    framebuffer: &mut Framebuffer,
    start: (f64, f64),
    end: (f64, f64),
    width: f64,
    colour: Color,
) {
    let half = width.max(1.0) / 2.0;
    let x_start = ((start.0.min(end.0) - half).floor() as i64).max(-1);
    let x_end = ((start.0.max(end.0) + half).ceil() as i64).min(framebuffer.width() as i64);
    let y_start = ((start.1.min(end.1) - half).floor() as i64).max(-1);
    let y_end = ((start.1.max(end.1) + half).ceil() as i64).min(framebuffer.height() as i64);
    let dx = end.0 - start.0;
    let dy = end.1 - start.1;
    let length_squared = dx * dx + dy * dy;

    for y in y_start..y_end {
        let py = y as f64 + 0.5;
        for x in x_start..x_end {
            let px = x as f64 + 0.5;
            let t = if length_squared <= f64::EPSILON {
                0.0
            } else {
                (((px - start.0) * dx + (py - start.1) * dy) / length_squared).clamp(0.0, 1.0)
            };
            let nearest = (start.0 + t * dx, start.1 + t * dy);
            let distance_squared = (px - nearest.0).powi(2) + (py - nearest.1).powi(2);
            if distance_squared <= half * half {
                framebuffer.set_pixel(x as u32, y as u32, colour);
            }
        }
    }
}

fn fill_circle(
    framebuffer: &mut Framebuffer,
    center_x: f64,
    center_y: f64,
    radius: f64,
    colour: Color,
) {
    if !radius.is_finite() || radius <= 0.0 {
        return;
    }
    let x_start = ((center_x - radius).floor() as i64).max(-1);
    let x_end = ((center_x + radius).ceil() as i64).min(framebuffer.width() as i64);
    let y_start = ((center_y - radius).floor() as i64).max(-1);
    let y_end = ((center_y + radius).ceil() as i64).min(framebuffer.height() as i64);
    for y in y_start..y_end {
        for x in x_start..x_end {
            let dx = x as f64 + 0.5 - center_x;
            let dy = y as f64 + 0.5 - center_y;
            if dx * dx + dy * dy <= radius * radius {
                framebuffer.set_pixel(x as u32, y as u32, colour);
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Face {
    vertices: [[f64; 3]; 3],
    colour: Color,
}

const LIGHT: [f64; 3] = [0.35, -0.55, 0.76];

fn add_face(faces: &mut Vec<Face>, vertices: [[f64; 3]; 3], colour: Color) {
    let u = subtract(vertices[1], vertices[0]);
    let v = subtract(vertices[2], vertices[0]);
    let Some(normal) = normalize(cross(u, v)) else {
        return;
    };
    let brightness = 0.45 + 0.55 * dot(normal, LIGHT).max(0.0);
    faces.push(Face { vertices, colour: colour.shade(brightness) });
}

fn add_quad(faces: &mut Vec<Face>, quad: [[f64; 3]; 4], colour: Color) {
    add_face(faces, [quad[0], quad[1], quad[2]], colour);
    add_face(faces, [quad[0], quad[2], quad[3]], colour);
}

fn add_board(faces: &mut Vec<Face>, design: &PcbDesign) {
    let points: Vec<(f64, f64)> =
        design.outline.points.iter().map(|point| (point.x, point.y)).collect();
    if points.len() < 3 {
        return;
    }
    let triangles = triangulate(&points);
    let top = BOARD_THICKNESS;
    for triangle in &triangles {
        add_face(
            faces,
            [
                [points[triangle[0]].0, points[triangle[0]].1, top],
                [points[triangle[1]].0, points[triangle[1]].1, top],
                [points[triangle[2]].0, points[triangle[2]].1, top],
            ],
            BOARD_FILL,
        );
    }
    for index in 0..points.len() {
        let start = points[index];
        let end = points[(index + 1) % points.len()];
        add_quad(
            faces,
            [
                [start.0, start.1, 0.0],
                [end.0, end.1, 0.0],
                [end.0, end.1, top],
                [start.0, start.1, top],
            ],
            BOARD_FILL.shade(0.7),
        );
    }
}

fn segment_quad(start: (f64, f64), end: (f64, f64), width: f64, z: f64) -> Option<[[f64; 3]; 4]> {
    let dx = end.0 - start.0;
    let dy = end.1 - start.1;
    let length = (dx * dx + dy * dy).sqrt();
    if !length.is_finite() || length <= f64::EPSILON {
        return None;
    }
    let normal = (-dy / length * width / 2.0, dx / length * width / 2.0);
    Some([
        [start.0 + normal.0, start.1 + normal.1, z],
        [end.0 + normal.0, end.1 + normal.1, z],
        [end.0 - normal.0, end.1 - normal.1, z],
        [start.0 - normal.0, start.1 - normal.1, z],
    ])
}

fn add_prism(
    faces: &mut Vec<Face>,
    center: (f64, f64),
    radius: f64,
    bottom: f64,
    top: f64,
    colour: Color,
) {
    if !radius.is_finite() || radius <= 0.0 {
        return;
    }
    let corners: Vec<[f64; 3]> = (0..8)
        .map(|index| {
            let angle = std::f64::consts::TAU * index as f64 / 8.0;
            [center.0 + angle.cos() * radius, center.1 + angle.sin() * radius, top]
        })
        .collect();
    for index in 0..8 {
        let next = (index + 1) % 8;
        add_quad(
            faces,
            [
                [corners[index][0], corners[index][1], bottom],
                [corners[next][0], corners[next][1], bottom],
                corners[next],
                corners[index],
            ],
            colour,
        );
        if (1..7).contains(&index) {
            add_face(faces, [corners[0], corners[index], corners[index + 1]], colour);
        }
    }
}

fn add_disc(faces: &mut Vec<Face>, center: (f64, f64), radius: f64, z: f64, colour: Color) {
    if !radius.is_finite() || radius <= 0.0 {
        return;
    }
    let corners: Vec<[f64; 3]> = (0..8)
        .map(|index| {
            let angle = std::f64::consts::TAU * index as f64 / 8.0;
            [center.0 + angle.cos() * radius, center.1 + angle.sin() * radius, z]
        })
        .collect();
    for index in 1..7 {
        add_face(faces, [corners[0], corners[index], corners[index + 1]], colour);
    }
}

fn triangulate(points: &[(f64, f64)]) -> Vec<[usize; 3]> {
    if points.len() < 3 {
        return Vec::new();
    }
    let mut indices: Vec<usize> = (0..points.len()).collect();
    if signed_area(points) < 0.0 {
        indices.reverse();
    }
    let mut triangles = Vec::new();
    let mut guard = 0usize;
    while indices.len() > 3 && guard < points.len() * points.len() {
        guard += 1;
        let mut clipped = false;
        for position in 0..indices.len() {
            let previous = (position + indices.len() - 1) % indices.len();
            let next = (position + 1) % indices.len();
            let a = points[indices[previous]];
            let b = points[indices[position]];
            let c = points[indices[next]];
            if cross_2d(a, b, c) <= 1e-12 {
                continue;
            }
            let contains_other = indices.iter().enumerate().any(|(other, index)| {
                other != previous
                    && other != position
                    && other != next
                    && point_in_triangle(points[*index], a, b, c)
            });
            if contains_other {
                continue;
            }
            triangles.push([indices[previous], indices[position], indices[next]]);
            indices.remove(position);
            clipped = true;
            break;
        }
        if !clipped {
            break;
        }
    }
    if indices.len() == 3 {
        triangles.push([indices[0], indices[1], indices[2]]);
    } else if indices.len() > 3 {
        for index in 1..indices.len() - 1 {
            triangles.push([indices[0], indices[index], indices[index + 1]]);
        }
    }
    triangles
}

fn signed_area(points: &[(f64, f64)]) -> f64 {
    let mut area = 0.0;
    for index in 0..points.len() {
        let (x1, y1) = points[index];
        let (x2, y2) = points[(index + 1) % points.len()];
        area += x1 * y2 - x2 * y1;
    }
    area / 2.0
}

fn cross_2d(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> f64 {
    (b.0 - a.0) * (c.1 - b.1) - (b.1 - a.1) * (c.0 - b.0)
}

fn point_in_triangle(point: (f64, f64), a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> bool {
    let ab = (b.0 - a.0) * (point.1 - a.1) - (b.1 - a.1) * (point.0 - a.0);
    let bc = (c.0 - b.0) * (point.1 - b.1) - (c.1 - b.1) * (point.0 - b.0);
    let ca = (a.0 - c.0) * (point.1 - c.1) - (a.1 - c.1) * (point.0 - c.0);
    ab >= -1e-12 && bc >= -1e-12 && ca >= -1e-12
}

#[derive(Debug, Clone, Copy)]
struct ScreenVertex {
    x: f64,
    y: f64,
    depth: f64,
}

#[derive(Debug)]
struct Projector {
    eye: [f64; 3],
    right: [f64; 3],
    up: [f64; 3],
    forward: [f64; 3],
    factor: f64,
    aspect: f64,
    width: f64,
    height: f64,
    near: f64,
}

impl Projector {
    fn new(camera: &Camera, width: u32, height: u32) -> Result<Self, RendererError> {
        let invalid = |message: &str| RendererError::InvalidConfig(message.to_string());
        if camera.eye.iter().chain(&camera.target).chain(&camera.up).any(|v| !v.is_finite())
            || !camera.fov_y_degrees.is_finite()
            || !(1.0..179.0).contains(&camera.fov_y_degrees)
            || !camera.near.is_finite()
            || camera.near <= 0.0
            || width == 0
            || height == 0
        {
            return Err(invalid("camera parameters are invalid"));
        }
        let forward = normalize(subtract(camera.target, camera.eye))
            .ok_or_else(|| invalid("camera eye and target coincide"))?;
        let right = normalize(cross(forward, camera.up))
            .ok_or_else(|| invalid("camera up is parallel to its view direction"))?;
        let up = cross(right, forward);
        Ok(Self {
            eye: camera.eye,
            right,
            up,
            forward,
            factor: 1.0 / (camera.fov_y_degrees.to_radians() / 2.0).tan(),
            aspect: width as f64 / height as f64,
            width: width as f64,
            height: height as f64,
            near: camera.near,
        })
    }

    fn project(&self, vertices: [[f64; 3]; 3]) -> Option<[ScreenVertex; 3]> {
        let mut projected = [ScreenVertex { x: 0.0, y: 0.0, depth: 0.0 }; 3];
        for (index, vertex) in vertices.iter().enumerate() {
            let relative = subtract(*vertex, self.eye);
            let x = dot(relative, self.right);
            let y = dot(relative, self.up);
            let depth = dot(relative, self.forward);
            if depth <= self.near {
                return None;
            }
            projected[index] = ScreenVertex {
                x: (0.5 + x * self.factor / (self.aspect * depth) * 0.5) * self.width,
                y: (0.5 - y * self.factor / depth * 0.5) * self.height,
                depth,
            };
        }
        Some(projected)
    }
}

fn rasterize(
    framebuffer: &mut Framebuffer,
    depth_buffer: &mut [f64],
    vertices: [ScreenVertex; 3],
    colour: Color,
) {
    let area = edge(vertices[0], vertices[1], vertices[2]);
    if area.abs() < 1e-9 {
        return;
    }
    let min_x = vertices.iter().map(|v| v.x).fold(f64::INFINITY, f64::min);
    let max_x = vertices.iter().map(|v| v.x).fold(f64::NEG_INFINITY, f64::max);
    let min_y = vertices.iter().map(|v| v.y).fold(f64::INFINITY, f64::min);
    let max_y = vertices.iter().map(|v| v.y).fold(f64::NEG_INFINITY, f64::max);
    let x_start = min_x.floor().max(0.0) as u32;
    let x_end = max_x.ceil().min(framebuffer.width() as f64 - 1.0) as u32;
    let y_start = min_y.floor().max(0.0) as u32;
    let y_end = max_y.ceil().min(framebuffer.height() as f64 - 1.0) as u32;

    for y in y_start..=y_end {
        for x in x_start..=x_end {
            let sample = ScreenVertex { x: x as f64 + 0.5, y: y as f64 + 0.5, depth: 0.0 };
            let w0 = edge(vertices[1], vertices[2], sample) / area;
            let w1 = edge(vertices[2], vertices[0], sample) / area;
            let w2 = 1.0 - w0 - w1;
            if w0 < -1e-9 || w1 < -1e-9 || w2 < -1e-9 {
                continue;
            }
            let depth = w0 * vertices[0].depth + w1 * vertices[1].depth + w2 * vertices[2].depth;
            let index = (y as usize) * (framebuffer.width() as usize) + x as usize;
            if depth < depth_buffer[index] {
                depth_buffer[index] = depth;
                framebuffer.set_pixel(x, y, colour);
            }
        }
    }
}

fn edge(a: ScreenVertex, b: ScreenVertex, point: ScreenVertex) -> f64 {
    (point.x - a.x) * (b.y - a.y) - (point.y - a.y) * (b.x - a.x)
}

fn subtract(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn normalize(vector: [f64; 3]) -> Option<[f64; 3]> {
    let length = dot(vector, vector).sqrt();
    if !length.is_finite() || length <= f64::EPSILON {
        return None;
    }
    Some([vector[0] / length, vector[1] / length, vector[2] / length])
}
