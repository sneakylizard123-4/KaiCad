//! Import and export for the supported subset of KiCad `.kicad_pcb` files.
//!
//! The importer maps board layers, named nets, Edge.Cuts outlines, footprints,
//! pads, tracks and vias into [`pcb::PcbDesign`]. It reports constructs it had
//! to skip instead of pretending the conversion was lossless.
//!
//! The writer emits the same subset in a form KiCad can open. It deliberately
//! does not claim full `.kicad_pcb` fidelity: copper zones, rule areas, arcs,
//! pad shapes other than rectangular copper, footprint graphics, text, 3D
//! models, dimensions and DRC constraints are not represented by KaiCad's
//! current board model.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use core::Point;
use pcb::footprint::{Outline, OutlineShape, PadPosition, PadType};
use pcb::{Footprint, PcbDesign, PcbNet, Track, Via};
use uuid::Uuid;

use super::{ParseError, SExpr, parse_document};

/// A KaiCad board imported from a KiCad PCB, plus conversion warnings.
#[derive(Debug, Clone)]
pub struct PcbImport {
    /// Imported KaiCad board.
    pub design: PcbDesign,
    /// Deterministic messages describing constructs that were skipped or
    /// approximated. An empty list means the supported subset was complete.
    pub warnings: Vec<String>,
}

/// Parse `.kicad_pcb` source into a board and a list of conversion warnings.
///
/// This is the in-memory entry point; [`load_pcb`] reads a file. Unsupported
/// graphical or manufacturing constructs are skipped and reported, never
/// silently treated as supported. Malformed S-expressions and missing
/// structural data return [`ParseError`].
pub fn parse_pcb(input: &str) -> Result<PcbImport, ParseError> {
    let root = parse_document(input)?;
    let items =
        root.items().ok_or_else(|| ParseError::Syntax("expected a top-level list".to_string()))?;
    match items.first().and_then(SExpr::atom) {
        Some("kicad_pcb") => {}
        Some(other) => {
            return Err(ParseError::Unsupported(format!(
                "expected a kicad_pcb document, found '{other}'"
            )));
        }
        None => return Err(ParseError::Syntax("empty top-level list".to_string())),
    }

    let mut warnings = Warnings::default();
    let (layers, stackup) = parse_layers(&root, &mut warnings)?;
    let layer_ids: BTreeMap<String, u16> =
        layers.iter().map(|layer| (layer.name.clone(), layer.id)).collect();
    let mut nets = NetTable::parse(&root, &mut warnings)?;
    let edge_layer = layer_ids.get("Edge.Cuts").copied();
    let outline = parse_outline(&root, edge_layer, &mut warnings)?;

    let mut design = PcbDesign {
        id: Uuid::new_v5(&Uuid::NAMESPACE_OID, input.as_bytes()),
        outline,
        layers,
        stackup: core::Stackup { layers: stackup },
        nets: Vec::new(),
        ..PcbDesign::default()
    };

    let mut footprint_index = 0usize;
    for item in items.iter().skip(1) {
        match item.head() {
            Some("segment") => {
                design.tracks.push(parse_segment(item, &layer_ids, &mut nets, &mut warnings)?);
            }
            Some("via") => {
                design.vias.push(parse_via(item, &layer_ids, &mut nets, &mut warnings)?);
            }
            Some("footprint") | Some("module") => {
                design.footprints.push(parse_footprint(
                    item,
                    &mut nets,
                    footprint_index,
                    &mut warnings,
                )?);
                footprint_index += 1;
            }
            Some(head @ ("gr_line" | "gr_rect" | "gr_poly" | "gr_arc")) if !is_edge_cuts(item) => {
                warnings.skipped(head);
            }
            Some(
                head @ ("zone" | "arc" | "gr_curve" | "gr_circle" | "gr_text" | "group"
                | "dimension" | "image" | "target" | "rule_area" | "generated"),
            ) => warnings.skipped(head),
            _ => {}
        }
    }

    design.nets = nets.into_nets();
    if design.outline.points.len() >= 3 && !design.outline.is_closed() {
        warnings.note("Edge.Cuts geometry did not form a closed outline");
    }

    Ok(PcbImport { design, warnings: warnings.finish() })
}

/// Read and parse a `.kicad_pcb` file.
pub fn load_pcb(path: impl AsRef<std::path::Path>) -> Result<PcbImport, ParseError> {
    let path = path.as_ref();
    let document = std::fs::read_to_string(path)?;
    parse_pcb(&document)
}

/// Serialize a board as a KiCad `.kicad_pcb` document.
///
/// The output contains the supported subset only. It is intended to open in
/// KiCad and to preserve KaiCad's board geometry and net assignment, not to
/// reproduce unavailable zones, graphics, models or DRC setup.
pub fn write_pcb(design: &PcbDesign) -> Result<String, ParseError> {
    let exporter = Exporter::new(design)?;
    Ok(exporter.render(design))
}

/// Write a board to `path`, creating parent directories as needed.
pub fn save_pcb(design: &PcbDesign, path: impl AsRef<std::path::Path>) -> Result<(), ParseError> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, write_pcb(design)?)?;
    Ok(())
}

#[derive(Default)]
struct Warnings {
    skipped: BTreeMap<String, usize>,
    notes: BTreeSet<String>,
}

impl Warnings {
    fn skipped(&mut self, construct: &str) {
        *self.skipped.entry(construct.to_string()).or_default() += 1;
    }

    fn note(&mut self, message: impl Into<String>) {
        self.notes.insert(message.into());
    }

    fn finish(self) -> Vec<String> {
        let mut messages: Vec<String> = self
            .skipped
            .into_iter()
            .map(|(construct, count)| {
                format!("skipped {count} unsupported '{construct}' construct(s)")
            })
            .collect();
        messages.extend(self.notes);
        messages
    }
}

fn parse_layers(
    root: &SExpr,
    warnings: &mut Warnings,
) -> Result<(Vec<core::PcbLayer>, Vec<u16>), ParseError> {
    let table = root
        .child("layers")
        .ok_or_else(|| ParseError::Unsupported("board has no layers table".to_string()))?;
    let mut layers = Vec::new();
    let mut used_names = BTreeSet::new();
    let mut used_ids = BTreeSet::new();

    for (index, entry) in table.iter().skip(1).enumerate() {
        let items = entry
            .items()
            .ok_or_else(|| ParseError::Syntax("layer entry must be a list".to_string()))?;
        let Some(name) = items.get(1).and_then(SExpr::atom) else {
            warnings.note("skipped a layer table entry without a name");
            continue;
        };
        let parsed_id = items.first().and_then(SExpr::atom).and_then(|id| id.parse::<u16>().ok());
        let id = canonical_layer_id(name)
            .or(parsed_id)
            .unwrap_or_else(|| 1000u16.saturating_add(index as u16));
        if !used_names.insert(name.to_string()) || !used_ids.insert(id) {
            warnings.note(format!("skipped duplicate layer entry '{name}'"));
            continue;
        }
        layers.push(core::PcbLayer::new(id, name, layer_kind(name)));
    }

    if layers.is_empty() {
        return Err(ParseError::Unsupported("board has no usable layers".to_string()));
    }

    let stackup = layers.iter().filter(|layer| layer.is_copper()).map(|layer| layer.id).collect();
    Ok((layers, stackup))
}

fn parse_outline(
    root: &SExpr,
    edge_layer: Option<u16>,
    warnings: &mut Warnings,
) -> Result<core::PcbOutline, ParseError> {
    let Some(_edge_layer) = edge_layer else {
        return Ok(core::PcbOutline::default());
    };
    let items = root.items().unwrap_or_default();
    let mut segments = Vec::new();

    for item in items {
        if !is_edge_cuts(item) {
            continue;
        }
        match item.head() {
            Some("gr_line") => {
                segments.push((parse_point(item, "start")?, parse_point(item, "end")?));
            }
            Some("gr_rect") => {
                let start = parse_point(item, "start")?;
                let end = parse_point(item, "end")?;
                segments.push((start, Point::new(end.x, start.y)));
                segments.push((Point::new(end.x, start.y), end));
                segments.push((end, Point::new(start.x, end.y)));
                segments.push((Point::new(start.x, end.y), start));
            }
            Some("gr_poly") => {
                let points = parse_polygon(item)?;
                for pair in points.windows(2) {
                    segments.push((pair[0], pair[1]));
                }
                if let (Some(first), Some(last)) = (points.first(), points.last()) {
                    segments.push((*last, *first));
                }
            }
            Some("arc" | "gr_arc") => {
                warnings.skipped("Edge.Cuts arc");
            }
            _ => {}
        }
    }

    if segments.is_empty() {
        return Ok(core::PcbOutline::default());
    }

    let mut points = vec![segments[0].0, segments[0].1];
    let mut used = vec![false; segments.len()];
    used[0] = true;
    loop {
        let first = points.first().copied().unwrap_or_default();
        let last = points.last().copied().unwrap_or_default();
        if same_point(first, last) {
            points.pop();
            break;
        }
        let current = *points.last().unwrap_or(&Point::default());
        let next = segments.iter().enumerate().find_map(|(index, (start, end))| {
            if used[index] {
                return None;
            }
            if same_point(*start, current) {
                Some((index, *end))
            } else if same_point(*end, current) {
                Some((index, *start))
            } else {
                None
            }
        });
        match next {
            Some((index, point)) => {
                used[index] = true;
                points.push(point);
            }
            None => {
                warnings.note("Edge.Cuts outline is not closed; imported its open chain");
                break;
            }
        }
    }

    if used.iter().any(|used| !used) {
        warnings.note("skipped additional Edge.Cuts loops; imported the first chain");
    }

    Ok(core::PcbOutline::new(points))
}

fn is_edge_cuts(node: &SExpr) -> bool {
    node.child("layer")
        .and_then(|layer| layer.get(1))
        .and_then(SExpr::atom)
        .is_some_and(|layer| layer == "Edge.Cuts")
}

fn parse_polygon(node: &SExpr) -> Result<Vec<Point>, ParseError> {
    let points = node
        .child("pts")
        .ok_or_else(|| ParseError::Syntax("gr_poly is missing its points".to_string()))?;
    let mut result = Vec::new();
    for point in points.iter().skip(1) {
        if point.head() != Some("xy") {
            continue;
        }
        let items = point.items().unwrap_or_default();
        result
            .push(Point::new(number_at(items, 1, "polygon x")?, number_at(items, 2, "polygon y")?));
    }
    Ok(result)
}

struct NetTable {
    by_code: BTreeMap<u32, Uuid>,
    by_id: BTreeMap<Uuid, String>,
}

impl NetTable {
    fn parse(root: &SExpr, warnings: &mut Warnings) -> Result<Self, ParseError> {
        let mut table = Self { by_code: BTreeMap::new(), by_id: BTreeMap::new() };
        for item in root.items().unwrap_or_default() {
            if item.head() != Some("net") {
                continue;
            }
            let items = item.items().unwrap_or_default();
            let code = items
                .get(1)
                .and_then(SExpr::atom)
                .ok_or_else(|| ParseError::Syntax("net is missing its code".to_string()))?
                .parse::<u32>()
                .map_err(|_| ParseError::Syntax("net code must be an integer".to_string()))?;
            let name = items.get(2).and_then(SExpr::atom).unwrap_or_default();
            if code == 0 || name.is_empty() {
                continue;
            }
            let id = net_id(name);
            if let Some(existing) = table.by_code.insert(code, id) {
                if existing != id {
                    warnings.note(format!(
                        "net code {code} was declared more than once; kept the last name"
                    ));
                }
            }
            table.by_id.entry(id).or_insert_with(|| name.to_string());
        }
        Ok(table)
    }

    fn resolve(
        &mut self,
        node: &SExpr,
        warnings: &mut Warnings,
    ) -> Result<Option<Uuid>, ParseError> {
        let Some(net) = node.child("net") else {
            return Ok(None);
        };
        let code = net
            .get(1)
            .and_then(SExpr::atom)
            .ok_or_else(|| ParseError::Syntax("net reference is missing its code".to_string()))?
            .parse::<u32>()
            .map_err(|_| ParseError::Syntax("net reference code must be an integer".to_string()))?;
        if code == 0 {
            return Ok(None);
        }
        if let Some(id) = self.by_code.get(&code) {
            return Ok(Some(*id));
        }
        let name = net.get(2).and_then(SExpr::atom).unwrap_or_default();
        if name.is_empty() {
            warnings.note(format!("skipped undefined net code {code}"));
            return Ok(None);
        }
        let id = net_id(name);
        self.by_code.insert(code, id);
        self.by_id.entry(id).or_insert_with(|| name.to_string());
        warnings.note(format!("recovered undeclared net {code} '{name}' from a reference"));
        Ok(Some(id))
    }

    fn into_nets(self) -> Vec<PcbNet> {
        self.by_id.into_iter().map(|(id, name)| PcbNet { id, name }).collect()
    }
}

fn parse_segment(
    node: &SExpr,
    layers: &BTreeMap<String, u16>,
    nets: &mut NetTable,
    warnings: &mut Warnings,
) -> Result<Track, ParseError> {
    let layer = resolve_layer(node, layers)?;
    let start = parse_point(node, "start")?;
    let end = parse_point(node, "end")?;
    let width = number_in_child(node, "width")?;
    if !width.is_finite() || width <= 0.0 {
        return Err(ParseError::Unsupported(
            "segment width must be finite and positive".to_string(),
        ));
    }
    let net_id = nets.resolve(node, warnings)?.unwrap_or_default();
    let id = parse_uuid(node).unwrap_or_else(|| {
        Uuid::new_v5(
            &Uuid::NAMESPACE_OID,
            format!(
                "kicad-segment|{}|{}|{}|{}|{}|{}",
                start.x, start.y, end.x, end.y, width, net_id
            )
            .as_bytes(),
        )
    });
    Ok(Track { id, net_id, layer, width, path: vec![start, end] })
}

fn parse_via(
    node: &SExpr,
    layers: &BTreeMap<String, u16>,
    nets: &mut NetTable,
    warnings: &mut Warnings,
) -> Result<Via, ParseError> {
    let position = parse_point(node, "at")?;
    let diameter = number_in_child(node, "size")?;
    let drill = number_in_child(node, "drill")?;
    if !diameter.is_finite() || diameter <= 0.0 || !drill.is_finite() || drill <= 0.0 {
        return Err(ParseError::Unsupported(
            "via size and drill must be finite and positive".to_string(),
        ));
    }
    let layer_names = node
        .child("layers")
        .ok_or_else(|| ParseError::Syntax("via is missing its layer span".to_string()))?;
    let start_name = layer_names
        .get(1)
        .and_then(SExpr::atom)
        .ok_or_else(|| ParseError::Syntax("via is missing its start layer".to_string()))?;
    let end_name = layer_names
        .get(2)
        .and_then(SExpr::atom)
        .ok_or_else(|| ParseError::Syntax("via is missing its end layer".to_string()))?;
    let layer_start = *layers.get(start_name).ok_or_else(|| {
        ParseError::Unsupported(format!("via references undeclared layer '{start_name}'"))
    })?;
    let layer_end = *layers.get(end_name).ok_or_else(|| {
        ParseError::Unsupported(format!("via references undeclared layer '{end_name}'"))
    })?;
    let net_id = nets.resolve(node, warnings)?;
    let id = parse_uuid(node).unwrap_or_else(|| {
        Uuid::new_v5(
            &Uuid::NAMESPACE_OID,
            format!(
                "kicad-via|{}|{}|{}|{}|{}|{}|{}",
                position.x,
                position.y,
                diameter,
                drill,
                layer_start,
                layer_end,
                net_id.map(|id| id.to_string()).unwrap_or_default()
            )
            .as_bytes(),
        )
    });
    Ok(Via { id, net_id, position, layer_start, layer_end, diameter, drill })
}

fn parse_footprint(
    node: &SExpr,
    nets: &mut NetTable,
    index: usize,
    warnings: &mut Warnings,
) -> Result<Footprint, ParseError> {
    let name = node
        .items()
        .and_then(|items| items.get(1))
        .and_then(SExpr::atom)
        .unwrap_or("Footprint")
        .to_string();
    let (position, angle) = parse_at(node)?;
    let layer_name =
        node.child("layer").and_then(|layer| layer.get(1)).and_then(SExpr::atom).unwrap_or("F.Cu");
    if layer_name.ends_with(".Cu") && layer_name != "F.Cu" {
        warnings.note(format!(
            "flattened footprint '{name}' pads from unsupported back-side layer '{layer_name}'"
        ));
    }
    let reference = footprint_reference(node).unwrap_or_else(|| format!("FP{index}"));
    let dropped_metadata = node.items().unwrap_or_default().iter().any(|item| match item.head() {
        Some(
            "fp_line" | "fp_rect" | "fp_circle" | "fp_arc" | "fp_poly" | "fp_curve" | "fp_text"
            | "model" | "attr" | "zone",
        ) => true,
        Some("property") => {
            item.items().unwrap_or_default().get(1).and_then(SExpr::atom) != Some("Reference")
        }
        _ => false,
    });
    if dropped_metadata {
        warnings.note(format!(
            "omitted graphical, 3D or non-reference metadata from footprint '{name}'"
        ));
    }

    let mut pads = Vec::new();
    for pad in node.items().unwrap_or_default().iter().filter(|item| item.head() == Some("pad")) {
        pads.push(parse_pad(pad, nets, position, angle, &name, warnings)?);
    }

    let outline = footprint_outline(&pads);
    Ok(Footprint { name, reference_designator: reference, position, pad_positions: pads, outline })
}

fn parse_pad(
    node: &SExpr,
    nets: &mut NetTable,
    footprint_position: Point,
    footprint_angle: f64,
    footprint_name: &str,
    warnings: &mut Warnings,
) -> Result<PadPosition, ParseError> {
    let items = node.items().unwrap_or_default();
    let number = items.get(1).and_then(SExpr::atom).unwrap_or_default();
    let kind = items.get(2).and_then(SExpr::atom).unwrap_or("smd");
    let shape = items.get(3).and_then(SExpr::atom).unwrap_or("rect");
    if shape != "rect" {
        warnings.note(format!(
            "approximated {shape} pad '{number}' on '{footprint_name}' as rectangular copper"
        ));
    }
    let pad_type = match kind {
        "smd" => PadType::SurfaceMount,
        "thru_hole" | "np_thru_hole" | "connect" => PadType::ThroughHole,
        other => {
            warnings.note(format!(
                "treated unknown pad type '{other}' on '{footprint_name}' as through-hole"
            ));
            PadType::ThroughHole
        }
    };

    let (local, pad_angle) = parse_at(node)?;
    let size = node
        .child("size")
        .ok_or_else(|| ParseError::Syntax(format!("pad '{number}' is missing its size")))?;
    let width = number_at(size, 1, "pad width")?;
    let height = number_at(size, 2, "pad height")?;
    let (sin, cos) = footprint_angle.to_radians().sin_cos();
    let center = Point::new(
        footprint_position.x + local.x * cos - local.y * sin,
        footprint_position.y + local.x * sin + local.y * cos,
    );
    let net_id = nets.resolve(node, warnings)?;
    Ok(PadPosition {
        number: number.to_string(),
        x: center.x - footprint_position.x,
        y: center.y - footprint_position.y,
        pad_type,
        width,
        height,
        rotation: pad_angle + footprint_angle,
        net_id,
    })
}

fn footprint_reference(node: &SExpr) -> Option<String> {
    for item in node.items().unwrap_or_default() {
        match item.head() {
            Some("property") => {
                let items = item.items().unwrap_or_default();
                if items.get(1).and_then(SExpr::atom) == Some("Reference") {
                    if let Some(reference) = items.get(2).and_then(SExpr::atom) {
                        return Some(reference.to_string());
                    }
                }
            }
            Some("fp_text") => {
                let items = item.items().unwrap_or_default();
                if items.get(1).and_then(SExpr::atom) == Some("reference") {
                    if let Some(reference) = items.get(2).and_then(SExpr::atom) {
                        return Some(reference.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    None
}

fn footprint_outline(pads: &[PadPosition]) -> Outline {
    let mut min = Point::new(-0.5, -0.5);
    let mut max = Point::new(0.5, 0.5);
    if let Some(first) = pads.first() {
        let extents = pad_extent(first);
        min = Point::new(first.x - extents.0, first.y - extents.1);
        max = Point::new(first.x + extents.0, first.y + extents.1);
        for pad in pads.iter().skip(1) {
            let extents = pad_extent(pad);
            min.x = min.x.min(pad.x - extents.0);
            min.y = min.y.min(pad.y - extents.1);
            max.x = max.x.max(pad.x + extents.0);
            max.y = max.y.max(pad.y + extents.1);
        }
    }
    Outline {
        shape: OutlineShape::rectangle(
            min.x - 0.25,
            min.y - 0.25,
            max.x - min.x + 0.5,
            max.y - min.y + 0.5,
        ),
        copper: false,
    }
}

fn pad_extent(pad: &PadPosition) -> (f64, f64) {
    let (sin, cos) = pad.rotation.to_radians().sin_cos();
    (
        (pad.width / 2.0 * cos).abs() + (pad.height / 2.0 * sin).abs(),
        (pad.width / 2.0 * sin).abs() + (pad.height / 2.0 * cos).abs(),
    )
}

struct Exporter {
    layers: BTreeMap<u16, String>,
    net_codes: BTreeMap<Uuid, u32>,
    nets: Vec<(u32, Uuid, String)>,
    top_copper: String,
}

impl Exporter {
    fn new(design: &PcbDesign) -> Result<Self, ParseError> {
        let mut layers = BTreeMap::new();
        for layer in &design.layers {
            let id = canonical_layer_id(&layer.name).ok_or_else(|| {
                ParseError::Unsupported(format!(
                    "cannot write KiCad layer with non-standard name '{}'",
                    layer.name
                ))
            })?;
            if let Some(existing) = layers.insert(id, layer.name.clone()) {
                if existing != layer.name {
                    return Err(ParseError::Unsupported(format!(
                        "layers '{}' and '{}' map to KiCad layer id {id}",
                        existing, layer.name
                    )));
                }
            }
        }

        if !design.outline.points.is_empty() {
            if design
                .outline
                .points
                .iter()
                .any(|point| !point.x.is_finite() || !point.y.is_finite())
            {
                return Err(ParseError::Unsupported(
                    "board outline contains a non-finite coordinate".to_string(),
                ));
            }
            layers.entry(44).or_insert_with(|| "Edge.Cuts".to_string());
        }
        if !design.footprints.is_empty() {
            layers.entry(37).or_insert_with(|| "F.SilkS".to_string());
        }
        if layers.values().all(|name| !name.ends_with(".Cu")) {
            layers.insert(0, "F.Cu".to_string());
            layers.insert(31, "B.Cu".to_string());
        }
        let top_copper = design
            .copper_layer_ids()
            .first()
            .and_then(|id| layers.get(id))
            .cloned()
            .unwrap_or_else(|| "F.Cu".to_string());
        if design.footprints.iter().any(|footprint| !footprint.pad_positions.is_empty()) {
            layers.entry(0).or_insert_with(|| top_copper.clone());
        }

        for track in &design.tracks {
            if !layers.contains_key(&track.layer) {
                return Err(ParseError::Unsupported(format!(
                    "track references layer id {} which is not in the board layer table",
                    track.layer
                )));
            }
            if track.path.len() < 2 {
                return Err(ParseError::Unsupported(
                    "track must have at least two path points".to_string(),
                ));
            }
            if !track.width.is_finite() || track.width <= 0.0 {
                return Err(ParseError::Unsupported(
                    "track width must be finite and positive".to_string(),
                ));
            }
            if track.path.iter().any(|point| !point.x.is_finite() || !point.y.is_finite()) {
                return Err(ParseError::Unsupported(
                    "track path contains a non-finite coordinate".to_string(),
                ));
            }
        }
        for via in &design.vias {
            if !layers.contains_key(&via.layer_start) || !layers.contains_key(&via.layer_end) {
                return Err(ParseError::Unsupported(format!(
                    "via references undeclared layer {}..{}",
                    via.layer_start, via.layer_end
                )));
            }
            if !via.position.x.is_finite()
                || !via.position.y.is_finite()
                || !via.diameter.is_finite()
                || via.diameter <= 0.0
                || !via.drill.is_finite()
                || via.drill <= 0.0
            {
                return Err(ParseError::Unsupported(
                    "via position, size and drill must be finite and positive".to_string(),
                ));
            }
        }
        for footprint in &design.footprints {
            if !footprint.position.x.is_finite() || !footprint.position.y.is_finite() {
                return Err(ParseError::Unsupported(
                    "footprint position must be finite".to_string(),
                ));
            }
            for pad in &footprint.pad_positions {
                if !pad.x.is_finite()
                    || !pad.y.is_finite()
                    || !pad.width.is_finite()
                    || pad.width <= 0.0
                    || !pad.height.is_finite()
                    || pad.height <= 0.0
                    || !pad.rotation.is_finite()
                {
                    return Err(ParseError::Unsupported(format!(
                        "pad on '{}' has invalid geometry",
                        footprint.reference_designator
                    )));
                }
            }
        }

        let mut names = design.nets.clone();
        names.sort_by_key(|net| net.id);
        let mut seen_ids = BTreeSet::new();
        let mut seen_names = BTreeSet::new();
        let mut nets = vec![(0, Uuid::nil(), String::new())];
        let mut net_codes = BTreeMap::new();
        for net in names {
            if net.id.is_nil() {
                continue;
            }
            if !seen_ids.insert(net.id) {
                return Err(ParseError::Unsupported(format!("duplicate PCB net id {}", net.id)));
            }
            if net.name.is_empty() {
                return Err(ParseError::Unsupported(format!(
                    "PCB net {} has an empty name",
                    net.id
                )));
            }
            if !seen_names.insert(net.name.clone()) {
                return Err(ParseError::Unsupported(format!(
                    "duplicate PCB net name '{}'",
                    net.name
                )));
            }
            let code = u32::try_from(nets.len()).map_err(|_| {
                ParseError::Unsupported("too many PCB nets for a KiCad net table".to_string())
            })?;
            net_codes.insert(net.id, code);
            nets.push((code, net.id, net.name));
        }

        let mut used: BTreeSet<Uuid> = BTreeSet::new();
        for track in &design.tracks {
            if !track.net_id.is_nil() {
                used.insert(track.net_id);
            }
        }
        for via in &design.vias {
            if let Some(id) = via.net_id {
                if !id.is_nil() {
                    used.insert(id);
                }
            }
        }
        for footprint in &design.footprints {
            for pad in &footprint.pad_positions {
                if let Some(id) = pad.net_id {
                    if !id.is_nil() {
                        used.insert(id);
                    }
                }
            }
        }
        for id in used {
            if net_codes.contains_key(&id) {
                continue;
            }
            let mut name = format!("Net-{}", &id.simple().to_string()[..8]);
            let mut suffix = 2;
            while seen_names.contains(&name) {
                name = format!("Net-{}-{suffix}", &id.simple().to_string()[..8]);
                suffix += 1;
            }
            seen_names.insert(name.clone());
            let code = u32::try_from(nets.len()).map_err(|_| {
                ParseError::Unsupported("too many PCB nets for a KiCad net table".to_string())
            })?;
            net_codes.insert(id, code);
            nets.push((code, id, name));
        }

        Ok(Self { layers, net_codes, nets, top_copper })
    }

    fn render(&self, design: &PcbDesign) -> String {
        let mut output = String::new();
        output.push_str("(kicad_pcb (version 20240108) (generator \"kaicad\")\n");
        output.push_str("  (general (thickness 1.6))\n");
        output.push_str("  (paper \"A4\")\n");
        output.push_str("  (layers\n");
        for (id, name) in &self.layers {
            let _ = writeln!(output, "    ({id} {} {})", quote(name), layer_type(name));
        }
        output.push_str("  )\n");
        for (code, _, name) in &self.nets {
            let _ = writeln!(output, "  (net {code} {})", quote(name));
        }

        self.write_outline(&mut output, design);
        self.write_footprints(&mut output, design);
        self.write_tracks(&mut output, design);
        self.write_vias(&mut output, design);
        output.push_str(")\n");
        output
    }

    fn write_outline(&self, output: &mut String, design: &PcbDesign) {
        let points = &design.outline.points;
        if points.len() < 3 {
            return;
        }
        for (index, start) in points.iter().enumerate() {
            let end = &points[(index + 1) % points.len()];
            if start == end {
                continue;
            }
            let uuid = Uuid::new_v5(
                &Uuid::NAMESPACE_OID,
                format!("kicad-export-edge|{index}|{}|{}|{}|{}", start.x, start.y, end.x, end.y)
                    .as_bytes(),
            );
            let _ = writeln!(
                output,
                "  (gr_line (start {} {}) (end {} {}) (stroke (width 0.05) (type solid)) (layer \"Edge.Cuts\") (uuid {}))",
                number(start.x),
                number(start.y),
                number(end.x),
                number(end.y),
                quote(&uuid.to_string())
            );
        }
    }

    fn write_footprints(&self, output: &mut String, design: &PcbDesign) {
        for (index, footprint) in design.footprints.iter().enumerate() {
            let uuid = Uuid::new_v5(
                &Uuid::NAMESPACE_OID,
                format!(
                    "kicad-export-footprint|{}|{}|{}|{}|{}",
                    footprint.name,
                    footprint.reference_designator,
                    footprint.position.x,
                    footprint.position.y,
                    index
                )
                .as_bytes(),
            );
            let _ = writeln!(
                output,
                "  (footprint {} (layer {}) (uuid {})",
                quote(&footprint.name),
                quote(&self.top_copper),
                quote(&uuid.to_string())
            );
            let _ = writeln!(
                output,
                "    (at {} {})\n    (property \"Reference\" {} (at 0 0 0) (layer \"F.SilkS\"))",
                number(footprint.position.x),
                number(footprint.position.y),
                quote(&footprint.reference_designator)
            );
            for (pad_index, pad) in footprint.pad_positions.iter().enumerate() {
                let pad_number = if pad.number.is_empty() {
                    (pad_index + 1).to_string()
                } else {
                    pad.number.clone()
                };
                let (kind, layers, drill) = match pad.pad_type {
                    PadType::ThroughHole => (
                        "thru_hole",
                        "\"*.Cu\" \"*.Mask\"".to_string(),
                        format!(" (drill {})", number(pad.width.min(pad.height) / 2.0)),
                    ),
                    PadType::SurfaceMount => (
                        "smd",
                        format!("{} \"F.Paste\" \"F.Mask\"", quote(&self.top_copper)),
                        String::new(),
                    ),
                    PadType::Tented => ("smd", quote(&self.top_copper), String::new()),
                };
                let net = pad
                    .net_id
                    .and_then(|id| self.net_codes.get(&id).copied())
                    .filter(|code| *code != 0)
                    .and_then(|code| {
                        self.nets
                            .iter()
                            .find(|(candidate, _, _)| *candidate == code)
                            .map(|(_, _, name)| format!(" (net {code} {})", quote(name)))
                    })
                    .unwrap_or_default();
                let _ = writeln!(
                    output,
                    "    (pad {} {kind} rect (at {} {} {}) (size {} {}){drill} (layers {layers}){net})",
                    quote(&pad_number),
                    number(pad.x),
                    number(pad.y),
                    number(pad.rotation),
                    number(pad.width),
                    number(pad.height)
                );
            }
            output.push_str("  )\n");
        }
    }

    fn write_tracks(&self, output: &mut String, design: &PcbDesign) {
        for (index, track) in design.tracks.iter().enumerate() {
            let Some(layer) = self.layers.get(&track.layer) else {
                continue;
            };
            for (segment_index, pair) in track.path.windows(2).enumerate() {
                let uuid = if track.id.is_nil() {
                    Uuid::new_v5(
                        &Uuid::NAMESPACE_OID,
                        format!(
                            "kicad-export-segment|{}|{}|{}|{}|{}|{}|{}",
                            index,
                            segment_index,
                            track.layer,
                            track.width,
                            pair[0].x,
                            pair[0].y,
                            pair[1].x
                        )
                        .as_bytes(),
                    )
                } else {
                    Uuid::new_v5(
                        &Uuid::NAMESPACE_OID,
                        format!("kicad-export-segment|{}|{segment_index}", track.id).as_bytes(),
                    )
                };
                let net = self.net_codes.get(&track.net_id).copied().unwrap_or(0);
                let _ = writeln!(
                    output,
                    "  (segment (start {} {}) (end {} {}) (width {}) (layer {}) (net {net}) (uuid {}))",
                    number(pair[0].x),
                    number(pair[0].y),
                    number(pair[1].x),
                    number(pair[1].y),
                    number(track.width),
                    quote(layer),
                    quote(&uuid.to_string())
                );
            }
        }
    }

    fn write_vias(&self, output: &mut String, design: &PcbDesign) {
        for (index, via) in design.vias.iter().enumerate() {
            let (Some(start), Some(end)) =
                (self.layers.get(&via.layer_start), self.layers.get(&via.layer_end))
            else {
                continue;
            };
            let uuid = if via.id.is_nil() {
                Uuid::new_v5(
                    &Uuid::NAMESPACE_OID,
                    format!("kicad-export-via|{index}|{}|{}", via.position.x, via.position.y)
                        .as_bytes(),
                )
            } else {
                via.id
            };
            let net = via.net_id.and_then(|id| self.net_codes.get(&id).copied()).unwrap_or(0);
            let _ = writeln!(
                output,
                "  (via (at {} {}) (size {}) (drill {}) (layers {} {}) (net {net}) (uuid {}))",
                number(via.position.x),
                number(via.position.y),
                number(via.diameter),
                number(via.drill),
                quote(start),
                quote(end),
                quote(&uuid.to_string())
            );
        }
    }
}

fn parse_at(node: &SExpr) -> Result<(Point, f64), ParseError> {
    let at = node
        .child("at")
        .ok_or_else(|| ParseError::Syntax("object is missing its position".to_string()))?;
    let x = number_at(at, 1, "x")?;
    let y = number_at(at, 2, "y")?;
    let angle = match at.get(3).and_then(SExpr::atom) {
        Some(value) => value
            .parse::<f64>()
            .map_err(|_| ParseError::Syntax(format!("invalid angle '{value}'")))?,
        None => 0.0,
    };
    Ok((Point::new(x, y), angle))
}

fn parse_point(node: &SExpr, child: &str) -> Result<Point, ParseError> {
    let at = node
        .child(child)
        .ok_or_else(|| ParseError::Syntax(format!("object is missing '{child}'")))?;
    Ok(Point::new(number_at(at, 1, "x")?, number_at(at, 2, "y")?))
}

fn parse_uuid(node: &SExpr) -> Option<Uuid> {
    node.child("uuid")
        .and_then(|uuid| uuid.get(1))
        .and_then(SExpr::atom)
        .and_then(|value| Uuid::parse_str(value).ok())
}

fn resolve_layer(node: &SExpr, layers: &BTreeMap<String, u16>) -> Result<u16, ParseError> {
    let name = node
        .child("layer")
        .and_then(|layer| layer.get(1))
        .and_then(SExpr::atom)
        .ok_or_else(|| ParseError::Syntax("object is missing its layer".to_string()))?;
    layers.get(name).copied().ok_or_else(|| {
        ParseError::Unsupported(format!("object references undeclared layer '{name}'"))
    })
}

fn number_in_child(node: &SExpr, child: &str) -> Result<f64, ParseError> {
    let values = node
        .child(child)
        .ok_or_else(|| ParseError::Syntax(format!("object is missing '{child}'")))?;
    number_at(values, 1, child)
}

fn number_at(items: &[SExpr], index: usize, field: &str) -> Result<f64, ParseError> {
    let value = items
        .get(index)
        .and_then(SExpr::atom)
        .ok_or_else(|| ParseError::Syntax(format!("{field} is missing")))?;
    let parsed = value
        .parse::<f64>()
        .map_err(|_| ParseError::Syntax(format!("invalid {field} '{value}'")))?;
    if !parsed.is_finite() {
        return Err(ParseError::Unsupported(format!("{field} must be finite")));
    }
    Ok(parsed)
}

fn same_point(left: Point, right: Point) -> bool {
    (left.x - right.x).abs() <= 1e-9 && (left.y - right.y).abs() <= 1e-9
}

fn net_id(name: &str) -> Uuid {
    Uuid::new_v5(&Uuid::NAMESPACE_OID, format!("kicad-pcb-net|{name}").as_bytes())
}

fn canonical_layer_id(name: &str) -> Option<u16> {
    if let Some(inner) = name.strip_prefix("In").and_then(|name| name.strip_suffix(".Cu")) {
        if let Ok(number) = inner.parse::<u16>() {
            if (1..=30).contains(&number) {
                return Some(number);
            }
        }
    }
    Some(match name {
        "F.Cu" => 0,
        "B.Cu" => 31,
        "B.Adhes" => 32,
        "F.Adhes" => 33,
        "B.Paste" => 34,
        "F.Paste" => 35,
        "B.SilkS" => 36,
        "F.SilkS" => 37,
        "B.Mask" => 38,
        "F.Mask" => 39,
        "Dwgs.User" => 40,
        "Cmts.User" => 41,
        "Eco1.User" => 42,
        "Eco2.User" => 43,
        "Edge.Cuts" => 44,
        "Margin" => 45,
        "B.CrtYd" => 46,
        "F.CrtYd" => 47,
        "B.Fab" => 48,
        "F.Fab" => 49,
        _ => return None,
    })
}

fn layer_type(name: &str) -> &'static str {
    if name.ends_with(".Cu") { "signal" } else { "user" }
}

fn layer_kind(name: &str) -> core::LayerKind {
    if name.ends_with(".Cu") {
        core::LayerKind::Copper
    } else if name.contains("SilkS") {
        core::LayerKind::SilkScreen
    } else if name.contains("Mask") {
        core::LayerKind::SolderMask
    } else {
        core::LayerKind::Mechanical
    }
}

fn number(value: f64) -> String {
    let value = if value == 0.0 { 0.0 } else { value };
    value.to_string()
}

fn quote(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    for character in value.chars() {
        match character {
            '\\' => quoted.push_str("\\\\"),
            '"' => quoted.push_str("\\\""),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            other => quoted.push(other),
        }
    }
    quoted.push('"');
    quoted
}

#[cfg(test)]
mod tests {
    use super::{load_pcb, parse_pcb, save_pcb, write_pcb};
    use pcb::{PcbDesign, PcbNet, Track};
    use uuid::Uuid;

    const FIXTURE: &str = r#"
(kicad_pcb (version 20240108) (generator pcbnew)
  (layers
    (0 "F.Cu" signal)
    (31 "B.Cu" signal)
    (37 "F.SilkS" user)
    (44 "Edge.Cuts" user)
  )
  (net 0 "")
  (net 1 "GND")
  (net 2 "VBUS")
  (gr_line (start 0 0) (end 20 0) (layer "Edge.Cuts") (width 0.05))
  (gr_line (start 20 0) (end 20 10) (layer "Edge.Cuts") (width 0.05))
  (gr_line (start 20 10) (end 0 10) (layer "Edge.Cuts") (width 0.05))
  (gr_line (start 0 10) (end 0 0) (layer "Edge.Cuts") (width 0.05))
  (footprint "Package_SO:SOIC-8" (layer "F.Cu") (at 6 5 90)
    (property "Reference" "U1" (at 0 0 0) (layer "F.SilkS"))
    (pad "1" smd roundrect (at 1 0 0) (size 1 0.6) (layers "F.Cu" "F.Paste" "F.Mask") (net 1 "GND"))
    (pad "2" thru_hole circle (at -1 0 0) (size 1.2 1.2) (drill 0.6) (layers "*.Cu" "*.Mask") (net 2 "VBUS"))
  )
  (segment (start 6 3) (end 10 3) (width 0.25) (layer "F.Cu") (net 1 "GND") (uuid "11111111-1111-1111-1111-111111111111"))
  (via (at 10 3) (size 0.6) (drill 0.3) (layers "F.Cu" "B.Cu") (net 1 "GND") (uuid "22222222-2222-2222-2222-222222222222"))
)
"#;

    fn sample_design() -> PcbDesign {
        let gnd = Uuid::from_u128(1);
        let p3v3 = Uuid::from_u128(2);
        PcbDesign {
            id: Uuid::from_u128(99),
            outline: core::PcbOutline::rectangle(20.0, 10.0),
            layers: vec![
                core::PcbLayer::new(0, "F.Cu", core::LayerKind::Copper),
                core::PcbLayer::new(31, "B.Cu", core::LayerKind::Copper),
            ],
            stackup: core::Stackup { layers: vec![0, 31] },
            nets: vec![
                PcbNet { id: gnd, name: "GND".to_string() },
                PcbNet { id: p3v3, name: "3V3".to_string() },
            ],
            footprints: vec![pcb::Footprint {
                name: "R_0603".to_string(),
                reference_designator: "R1".to_string(),
                position: core::Point::new(4.0, 5.0),
                pad_positions: vec![
                    pcb::PadPosition {
                        number: "1".to_string(),
                        x: -0.8,
                        y: 0.0,
                        pad_type: pcb::PadType::SurfaceMount,
                        width: 1.4,
                        height: 0.8,
                        net_id: Some(gnd),
                        ..pcb::PadPosition::default()
                    },
                    pcb::PadPosition {
                        number: "2".to_string(),
                        x: 0.8,
                        y: 0.0,
                        pad_type: pcb::PadType::SurfaceMount,
                        width: 1.4,
                        height: 0.8,
                        net_id: Some(p3v3),
                        ..pcb::PadPosition::default()
                    },
                ],
                ..pcb::Footprint::default()
            }],
            tracks: vec![Track {
                id: Uuid::from_u128(4),
                net_id: gnd,
                layer: 0,
                width: 0.25,
                path: vec![core::Point::new(3.2, 5.0), core::Point::new(1.0, 5.0)],
            }],
            vias: vec![pcb::Via {
                id: Uuid::from_u128(5),
                net_id: Some(gnd),
                position: core::Point::new(1.0, 5.0),
                layer_start: 0,
                layer_end: 31,
                diameter: 0.6,
                drill: 0.3,
            }],
            ..PcbDesign::default()
        }
    }

    #[test]
    fn imports_outline_footprint_track_via_and_names() {
        let imported = parse_pcb(FIXTURE).expect("fixture imports");

        assert_eq!(imported.design.outline.points.len(), 4);
        assert_eq!(imported.design.tracks.len(), 1);
        assert_eq!(imported.design.vias.len(), 1);
        assert_eq!(imported.design.footprints.len(), 1);
        assert_eq!(imported.design.nets.len(), 2);
        assert_eq!(imported.warnings.len(), 2, "unexpected warnings: {:?}", imported.warnings);
        assert!(imported.warnings.iter().any(|warning| warning.contains("roundrect")));
        assert!(imported.warnings.iter().any(|warning| warning.contains("circle")));

        let footprint = &imported.design.footprints[0];
        assert_eq!(footprint.reference_designator, "U1");
        assert_eq!(footprint.pad_positions.len(), 2);
        assert_eq!(footprint.pad_positions[0].number, "1");
        assert_eq!(footprint.pad_positions[0].net_id, Some(super::net_id("GND")));
        assert_eq!(footprint.pad_positions[1].pad_type, pcb::PadType::ThroughHole);
        assert_eq!(imported.design.net_name(super::net_id("GND")), Some("GND"));
        assert_eq!(
            imported.design.tracks[0].id,
            Uuid::from_u128(0x11111111111111111111111111111111)
        );
        assert_eq!(imported.design.vias[0].layer_end, 31);
    }

    #[test]
    fn importer_rotates_footprint_pads_and_reports_unsupported_data() {
        let document = FIXTURE.replace(
            "  (footprint",
            "  (zone (net 1) (net_name \"GND\") (layer \"F.Cu\") (filled_polygon (pts (xy 0 0))))\n  (footprint",
        );
        let imported = parse_pcb(&document).expect("fixture imports");

        assert!(imported.design.footprints[0].pad_positions[0].x.abs() < 1e-9);
        assert!((imported.design.footprints[0].pad_positions[0].y - 1.0).abs() < 1e-9);
        assert_eq!(imported.design.footprints[0].pad_positions[0].rotation, 90.0);
        assert!(imported.warnings.iter().any(|warning| warning.contains("zone")));
    }

    #[test]
    fn write_then_parse_round_trips_supported_board() {
        let design = sample_design();
        let document = write_pcb(&design).expect("design writes");

        assert!(document.starts_with("(kicad_pcb"));
        assert!(document.contains("(net 1 \"GND\")"));
        assert!(document.contains("(net 2 \"3V3\")"));
        assert!(document.contains("(pad \"1\" smd rect"));
        assert!(document.contains("(net 1 \"GND\")") && document.contains("(net 2 \"3V3\")"));
        assert!(document.contains("(stroke (width 0.05) (type solid))"));
        let restored = parse_pcb(&document).expect("written board parses").design;

        assert_eq!(restored.outline.points, design.outline.points);
        assert_eq!(restored.net_name(restored.tracks[0].net_id), Some("GND"));
        assert_eq!(
            restored.net_name(restored.footprints[0].pad_positions[1].net_id.expect("pad net")),
            Some("3V3")
        );
        assert_eq!(restored.footprints.len(), 1);
        assert_eq!(restored.footprints[0].reference_designator, "R1");
        assert_eq!(restored.footprints[0].pad_positions.len(), 2);
        assert_eq!(restored.footprints[0].pad_positions[0].number, "1");
        assert_eq!(restored.footprints[0].pad_positions[1].number, "2");
        assert_eq!(restored.tracks.len(), 1);
        assert_eq!(restored.vias.len(), 1);
        assert_eq!(restored.vias[0].layer_end, 31);
    }

    #[test]
    fn writer_generates_names_for_unnamed_used_nets() {
        let mut design = sample_design();
        design.nets.clear();
        let document = write_pcb(&design).expect("design writes");

        assert!(document.contains("Net-"));
        let restored = parse_pcb(&document).expect("written board parses").design;
        assert_eq!(restored.nets.len(), 2);
        assert!(restored.nets.iter().all(|net| net.name.starts_with("Net-")));
    }

    #[test]
    fn write_rejects_nonstandard_layer_names() {
        let mut design = sample_design();
        design.layers[0].name = "TopCopper".to_string();

        let error = write_pcb(&design).expect_err("unknown layer must fail");
        assert!(error.to_string().contains("non-standard name"));
    }

    #[test]
    fn malformed_and_wrong_root_documents_are_errors() {
        assert!(parse_pcb("(kicad_pcb (layers)").is_err());
        assert!(parse_pcb("(not_a_board)").is_err());
        assert!(parse_pcb("(kicad_pcb)").is_err());
    }

    #[test]
    fn missing_layers_and_bad_segment_width_are_reported() {
        let no_layers = "(kicad_pcb)";
        assert!(parse_pcb(no_layers).is_err());

        let bad_width = FIXTURE.replace("(width 0.25)", "(width 0)");
        let error = parse_pcb(&bad_width).expect_err("zero width must fail");
        assert!(error.to_string().contains("positive"));
    }

    #[test]
    fn open_outline_is_reported_and_still_imported() {
        let document = FIXTURE
            .replace("  (gr_line (start 0 10) (end 0 0) (layer \"Edge.Cuts\") (width 0.05))\n", "");
        let imported = parse_pcb(&document).expect("open outline imports");

        assert!(imported.design.outline.points.len() >= 3);
        assert!(imported.warnings.iter().any(|warning| warning.contains("not closed")));
    }

    #[test]
    fn file_helpers_round_trip_and_create_parent_directories() {
        let directory =
            std::env::temp_dir().join(format!("kaicad-kicad-pcb-test-{}", Uuid::new_v4()));
        let path = directory.join("nested/board.kicad_pcb");
        let design = sample_design();

        save_pcb(&design, &path).expect("save board");
        let loaded = load_pcb(&path).expect("load board");
        assert_eq!(loaded.design.tracks.len(), 1);
        assert_eq!(loaded.design.nets.len(), 2);

        std::fs::remove_dir_all(directory).expect("clean temp directory");
    }
}
