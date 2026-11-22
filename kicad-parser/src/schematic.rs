//! Import and export for the supported subset of KiCad `.kicad_sch` files.
//!
//! KaiCad's schematic model covers placed components, wires, labels, junctions,
//! no-connects and hierarchical sheet symbols. Everything else that KiCad
//! stores in a schematic — library symbol graphics, bus objects, text, images,
//! instance paths — is retained as opaque source data and re-emitted on export.
//! That makes a compatibility round trip possible without pretending those
//! constructs have engineering semantics inside KaiCad.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::Path;

use core::component::{Component, ComponentPlacement, ComponentProperty};
use core::sheet::{Junction, Label, LabelPosition, LabelType, NoConnect, Sheet, Wire, WirePoint};
use core::{Point, PreservedNode, SourceDocument};
use uuid::Uuid;

use super::{ParseError, SExpr, parse_document, write_quoted};

/// A KaiCad schematic imported from KiCad, plus conversion warnings.
#[derive(Debug, Clone)]
pub struct SchematicImport {
    /// Imported schematic data. The first sheet is the root sheet.
    pub data: core::project::SchematicData,
    /// Deterministic messages describing constructs retained opaquely.
    pub warnings: Vec<String>,
}

/// Parse `.kicad_sch` source into schematic data and conversion warnings.
pub fn parse_schematic(input: &str) -> Result<SchematicImport, ParseError> {
    let root = parse_document(input)?;
    let items =
        root.items().ok_or_else(|| ParseError::Syntax("expected a top-level list".to_string()))?;
    match items.first().and_then(SExpr::atom) {
        Some("kicad_sch") => {}
        Some(other) => {
            return Err(ParseError::Unsupported(format!(
                "expected a kicad_sch document, found '{other}'"
            )));
        }
        None => return Err(ParseError::Syntax("empty top-level list".to_string())),
    }

    let mut warnings = Warnings::default();
    let mut source = SourceDocument {
        format: "kicad_sch".to_string(),
        version: child_atom(&root, "version"),
        generator: child_atom(&root, "generator"),
        uuid: parse_uuid(&root),
        preserved: Vec::new(),
    };
    let root_id =
        source.uuid.unwrap_or_else(|| Uuid::new_v5(&Uuid::NAMESPACE_OID, input.as_bytes()));
    let mut root_sheet = Sheet { id: root_id, name: "Root".to_string(), ..Sheet::default() };
    let mut components = Vec::new();
    let mut sheets = Vec::new();
    let mut component_index = 0usize;

    for item in items.iter().skip(1) {
        match item.head() {
            Some("symbol") => {
                let component = parse_component(item, component_index, &mut warnings)?;
                component_index += 1;
                root_sheet.components.push(component.id);
                components.push(component);
            }
            Some("wire") => root_sheet.wires.push(parse_wire(item, &mut warnings)?),
            Some("junction") => root_sheet.junctions.push(parse_junction(item)?),
            Some("no_connect") => root_sheet.no_connects.push(parse_no_connect(item)?),
            Some(head @ ("label" | "global_label" | "hierarchical_label")) => {
                root_sheet.labels.push(parse_label(item, head)?);
            }
            Some("sheet") => {
                let sheet = parse_sheet(item, root_id)?;
                root_sheet.children.push(sheet.id);
                sheets.push(sheet);
            }
            Some("version" | "generator" | "uuid") => {}
            Some(
                "lib_symbols" | "paper" | "title_block" | "sheet_instances" | "symbol_instances"
                | "embedded_fonts",
            ) => {
                source.preserved.push(preserved_node(item).expect("item has a head"));
            }
            Some(head) => {
                warnings.preserved(head);
                if let Some(node) = preserved_node(item) {
                    source.preserved.push(node);
                }
            }
            None => {}
        }
    }

    sheets.insert(0, root_sheet);
    let data = core::project::SchematicData {
        sheets,
        symbols: Vec::<core::Symbol>::new(),
        nets: Vec::new(),
        components,
        source: Some(source),
    };
    Ok(SchematicImport { data, warnings: warnings.finish() })
}

/// Read and parse a `.kicad_sch` file.
pub fn load_schematic(path: impl AsRef<Path>) -> Result<SchematicImport, ParseError> {
    let path = path.as_ref();
    let document = std::fs::read_to_string(path)?;
    parse_schematic(&document)
}

/// Serialize schematic data as a KiCad `.kicad_sch` document.
pub fn write_schematic(data: &core::project::SchematicData) -> Result<String, ParseError> {
    let root = data
        .sheets
        .iter()
        .find(|sheet| sheet.parent.is_none())
        .or_else(|| data.sheets.first())
        .ok_or_else(|| ParseError::Unsupported("schematic has no root sheet".to_string()))?;
    let source = data.source.as_ref();
    let version = source.and_then(|source| source.version.as_deref()).unwrap_or("20231120");
    let generator = source.and_then(|source| source.generator.as_deref()).unwrap_or("kaicad");
    let preserved = source.map(|source| source.preserved.as_slice()).unwrap_or_default();

    let mut output = String::new();
    let _ = write!(output, "(kicad_sch (version {version}) (generator ");
    write_quoted(&mut output, generator);
    output.push_str(")\n");

    let root_uuid = source.and_then(|source| source.uuid).unwrap_or(root.id);
    if !has_head(preserved, "uuid") {
        let _ = writeln!(output, "  (uuid {})", quote(&root_uuid.to_string()));
    }
    for head in ["paper", "title_block", "lib_symbols"] {
        write_preserved(&mut output, preserved, head, 2);
    }
    if !has_head(preserved, "lib_symbols") {
        output.push_str("  (lib_symbols)\n");
    }

    for wire in &root.wires {
        write_wire(&mut output, wire);
    }
    for junction in &root.junctions {
        write_junction(&mut output, junction);
    }
    for no_connect in &root.no_connects {
        write_no_connect(&mut output, no_connect);
    }
    for label in &root.labels {
        write_label(&mut output, label);
    }
    for component in &data.components {
        write_component(&mut output, component)?;
    }
    for sheet in data.sheets.iter().filter(|sheet| sheet.parent.is_some()) {
        write_sheet(&mut output, sheet);
    }

    for head in ["sheet_instances", "symbol_instances", "embedded_fonts"] {
        write_preserved(&mut output, preserved, head, 2);
    }
    if !has_head(preserved, "sheet_instances") {
        write_sheet_instances(&mut output, data);
    }
    for node in preserved {
        if !matches!(
            node.head.as_str(),
            "paper"
                | "title_block"
                | "lib_symbols"
                | "uuid"
                | "sheet_instances"
                | "symbol_instances"
                | "embedded_fonts"
                | "version"
                | "generator"
        ) {
            write_node(&mut output, &node.source, 2);
        }
    }
    output.push_str(")\n");
    Ok(output)
}

/// Write a schematic to `path`, creating parent directories as needed.
pub fn save_schematic(
    data: &core::project::SchematicData,
    path: impl AsRef<Path>,
) -> Result<(), ParseError> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, write_schematic(data)?)?;
    Ok(())
}

/// A `.kicad_pro` project file retained as JSON.
///
/// KiCad's project file is JSON with many editor settings KaiCad does not
/// model. Keeping the parsed value lets the CLI read a project and write it
/// back without silently discarding those settings.
#[derive(Debug, Clone)]
pub struct ProjectFile {
    /// Project file content.
    pub value: serde_json::Value,
    /// Path recorded by the caller, when loaded from disk.
    pub path: Option<String>,
}

impl ProjectFile {
    /// Parse project JSON, rejecting non-object documents.
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        let value: serde_json::Value = serde_json::from_str(input)
            .map_err(|error| ParseError::Syntax(format!("invalid .kicad_pro JSON: {error}")))?;
        if !value.is_object() {
            return Err(ParseError::Unsupported(
                ".kicad_pro root must be a JSON object".to_string(),
            ));
        }
        Ok(Self { value, path: None })
    }

    /// Load a `.kicad_pro` file.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ParseError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)?;
        let mut project = Self::parse(&text)?;
        project.path = Some(path.display().to_string());
        Ok(project)
    }

    /// Write the project as pretty JSON with one trailing newline.
    pub fn to_json(&self) -> Result<String, ParseError> {
        let mut json = serde_json::to_string_pretty(&self.value)
            .map_err(|error| ParseError::Syntax(format!("cannot serialize .kicad_pro: {error}")))?;
        json.push('\n');
        Ok(json)
    }

    /// Save the project, creating parent directories as needed.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), ParseError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        std::fs::write(path, self.to_json()?)?;
        Ok(())
    }
}

fn parse_component(
    node: &SExpr,
    index: usize,
    warnings: &mut Warnings,
) -> Result<Component, ParseError> {
    let items = node.items().unwrap_or_default();
    let lib_id = child_atom(node, "lib_id").unwrap_or_default();
    let (position, rotation) = parse_at(node)?;
    let unit = node
        .child("unit")
        .and_then(|unit| unit.get(1))
        .and_then(SExpr::atom)
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(1);
    let uuid = parse_uuid(node);
    let properties = parse_properties(node);
    let reference = properties
        .iter()
        .find(|property| property.name == "Reference")
        .map(|property| property.value.clone())
        .unwrap_or_else(|| format!("U{index}"));
    let footprint = properties
        .iter()
        .find(|property| property.name == "Footprint")
        .map(|property| property.value.clone())
        .unwrap_or_default();
    if lib_id.is_empty() {
        warnings.note(format!("component '{reference}' has no lib_id"));
    }
    let symbol_id =
        Uuid::new_v5(&Uuid::NAMESPACE_OID, format!("kicad-schematic-symbol|{lib_id}").as_bytes());
    let footprint_id = if footprint.is_empty() {
        Uuid::nil()
    } else {
        Uuid::new_v5(&Uuid::NAMESPACE_OID, format!("kicad-footprint|{footprint}").as_bytes())
    };
    let id = uuid.unwrap_or_else(|| {
        Uuid::new_v5(
            &Uuid::NAMESPACE_OID,
            format!("kicad-schematic-component|{index}|{lib_id}|{reference}").as_bytes(),
        )
    });

    let mut preserved = Vec::new();
    for child in items.iter().skip(1) {
        if matches!(child.head(), Some("lib_id" | "at" | "unit")) {
            continue;
        }
        if let Some(node) = preserved_node(child) {
            preserved.push(node);
        }
    }

    Ok(Component {
        id,
        reference_designator: reference,
        symbol_id,
        footprint_id,
        properties,
        placement: ComponentPlacement { position, rotation, unit },
        library_id: lib_id,
        preserved,
    })
}

fn parse_properties(node: &SExpr) -> Vec<ComponentProperty> {
    node.items()
        .unwrap_or_default()
        .iter()
        .filter(|item| item.head() == Some("property"))
        .filter_map(|item| {
            let items = item.items()?;
            Some(ComponentProperty {
                name: items.get(1)?.atom()?.to_string(),
                value: items.get(2)?.atom()?.to_string(),
            })
        })
        .collect()
}

fn parse_wire(node: &SExpr, warnings: &mut Warnings) -> Result<Wire, ParseError> {
    let points = node
        .child("pts")
        .ok_or_else(|| ParseError::Syntax("wire is missing its points".to_string()))?;
    let mut wire_points = Vec::new();
    for point in points.iter().skip(1) {
        if point.head() != Some("xy") {
            continue;
        }
        let values = point.items().unwrap_or_default();
        wire_points.push(WirePoint {
            x: number_at(values, 1, "wire x")?,
            y: number_at(values, 2, "wire y")?,
        });
    }
    if wire_points.len() < 2 {
        return Err(ParseError::Unsupported("wire has fewer than two points".to_string()));
    }
    let id = parse_uuid(node).unwrap_or_else(|| {
        let mut seed = String::from("kicad-schematic-wire");
        for point in &wire_points {
            let _ = write!(seed, "|{}|{}", point.x, point.y);
        }
        Uuid::new_v5(&Uuid::NAMESPACE_OID, seed.as_bytes())
    });
    if node.child("stroke").is_none() {
        warnings.note("wire has no stroke; export will use KiCad's default");
    }
    Ok(Wire { id, points: wire_points, preserved: preserved_children(node, &["pts"]) })
}

fn parse_junction(node: &SExpr) -> Result<Junction, ParseError> {
    let (position, _) = parse_at(node)?;
    let id = parse_uuid(node).unwrap_or_else(|| {
        Uuid::new_v5(
            &Uuid::NAMESPACE_OID,
            format!("kicad-schematic-junction|{}|{}", position.x, position.y).as_bytes(),
        )
    });
    Ok(Junction { id, position, preserved: preserved_children(node, &["at", "uuid"]) })
}

fn parse_no_connect(node: &SExpr) -> Result<NoConnect, ParseError> {
    let (position, _) = parse_at(node)?;
    let id = parse_uuid(node).unwrap_or_else(|| {
        Uuid::new_v5(
            &Uuid::NAMESPACE_OID,
            format!("kicad-schematic-no-connect|{}|{}", position.x, position.y).as_bytes(),
        )
    });
    Ok(NoConnect { id, position, preserved: preserved_children(node, &["at", "uuid"]) })
}

fn parse_label(node: &SExpr, head: &str) -> Result<Label, ParseError> {
    let items = node.items().unwrap_or_default();
    let name = items.get(1).and_then(SExpr::atom).unwrap_or_default().to_string();
    let (position, rotation) = parse_at(node)?;
    let id = parse_uuid(node).unwrap_or_else(|| {
        Uuid::new_v5(
            &Uuid::NAMESPACE_OID,
            format!("kicad-schematic-label|{head}|{name}|{}|{}|{rotation}", position.x, position.y)
                .as_bytes(),
        )
    });
    Ok(Label {
        id,
        name,
        label_type: match head {
            "global_label" => LabelType::Global,
            "hierarchical_label" => LabelType::Hierarchical,
            _ => LabelType::Local,
        },
        position: LabelPosition { x: position.x, y: position.y, rotation },
        preserved: preserved_children(node, &["at", "uuid"]),
    })
}

fn parse_sheet(node: &SExpr, parent: Uuid) -> Result<Sheet, ParseError> {
    let (position, _) = parse_at(node)?;
    let size = node
        .child("size")
        .map(|size| {
            Ok::<Point, ParseError>(Point::new(
                number_at(size, 1, "sheet width")?,
                number_at(size, 2, "sheet height")?,
            ))
        })
        .transpose()?
        .unwrap_or_else(|| Point::new(0.0, 0.0));
    let mut name = String::new();
    let mut file_name = String::new();
    for property in
        node.items().unwrap_or_default().iter().filter(|item| item.head() == Some("property"))
    {
        let values = property.items().unwrap_or_default();
        match values.get(1).and_then(SExpr::atom) {
            Some("Sheetname") => {
                name = values.get(2).and_then(SExpr::atom).unwrap_or_default().to_string();
            }
            Some("Sheetfile") => {
                file_name = values.get(2).and_then(SExpr::atom).unwrap_or_default().to_string();
            }
            _ => {}
        }
    }
    let id = parse_uuid(node).unwrap_or_else(|| {
        Uuid::new_v5(
            &Uuid::NAMESPACE_OID,
            format!("kicad-schematic-sheet|{}|{}|{}", position.x, position.y, file_name).as_bytes(),
        )
    });
    Ok(Sheet {
        id,
        name: if name.is_empty() { "Sheet".to_string() } else { name },
        parent: Some(parent),
        file_name,
        position,
        size,
        preserved: preserved_children(node, &["at", "size", "uuid"]),
        ..Sheet::default()
    })
}

fn write_component(output: &mut String, component: &Component) -> Result<(), ParseError> {
    if component.library_id.is_empty() {
        return Err(ParseError::Unsupported(format!(
            "component '{}' has no library_id",
            component.reference_designator
        )));
    }
    let _ = writeln!(
        output,
        "  (symbol (lib_id {}) (at {} {} {}) (unit {})",
        quote(&component.library_id),
        number(component.placement.position.x),
        number(component.placement.position.y),
        number(component.placement.rotation),
        component.placement.unit.max(1)
    );
    for node in &component.preserved {
        if !matches!(node.head.as_str(), "lib_id" | "at" | "unit") {
            write_node(output, &node.source, 4);
        }
    }
    if !has_head(&component.preserved, "property") {
        for property in &component.properties {
            let _ = writeln!(
                output,
                "    (property {} {} (at 0 0 0) (effects (font (size 1.27 1.27))))",
                quote(&property.name),
                quote(&property.value)
            );
        }
        if !component.properties.iter().any(|property| property.name == "Reference") {
            let _ = writeln!(
                output,
                "    (property \"Reference\" {} (at 0 0 0) (effects (font (size 1.27 1.27))))",
                quote(&component.reference_designator)
            );
        }
    }
    if !has_head(&component.preserved, "uuid") {
        let _ = writeln!(output, "    (uuid {})", quote(&component.id.to_string()));
    }
    output.push_str("  )\n");
    Ok(())
}

fn write_wire(output: &mut String, wire: &Wire) {
    let _ = write!(output, "  (wire (pts");
    for point in &wire.points {
        let _ = write!(output, " (xy {} {})", number(point.x), number(point.y));
    }
    output.push(')');
    for node in &wire.preserved {
        if !matches!(node.head.as_str(), "pts" | "uuid") {
            output.push(' ');
            output.push_str(&node.source);
        }
    }
    if !has_head(&wire.preserved, "uuid") {
        let _ = write!(output, " (uuid {})", quote(&wire.id.to_string()));
    }
    output.push_str(")\n");
}

fn write_junction(output: &mut String, junction: &Junction) {
    let _ = write!(
        output,
        "  (junction (at {} {})",
        number(junction.position.x),
        number(junction.position.y)
    );
    for node in &junction.preserved {
        if node.head != "at" && node.head != "uuid" {
            output.push(' ');
            output.push_str(&node.source);
        }
    }
    if !has_head(&junction.preserved, "uuid") {
        let _ = write!(output, " (uuid {})", quote(&junction.id.to_string()));
    }
    output.push_str(")\n");
}

fn write_no_connect(output: &mut String, no_connect: &NoConnect) {
    let _ = write!(
        output,
        "  (no_connect (at {} {})",
        number(no_connect.position.x),
        number(no_connect.position.y)
    );
    for node in &no_connect.preserved {
        if node.head != "at" && node.head != "uuid" {
            output.push(' ');
            output.push_str(&node.source);
        }
    }
    if !has_head(&no_connect.preserved, "uuid") {
        let _ = write!(output, " (uuid {})", quote(&no_connect.id.to_string()));
    }
    output.push_str(")\n");
}

fn write_label(output: &mut String, label: &Label) {
    let head = match label.label_type {
        LabelType::Global | LabelType::Power => "global_label",
        LabelType::Hierarchical => "hierarchical_label",
        LabelType::Local => "label",
    };
    let _ = write!(
        output,
        "  ({head} {} (at {} {} {})",
        quote(&label.name),
        number(label.position.x),
        number(label.position.y),
        number(label.position.rotation)
    );
    for node in &label.preserved {
        if node.head != "at" && node.head != "uuid" {
            output.push(' ');
            output.push_str(&node.source);
        }
    }
    if !has_head(&label.preserved, "uuid") {
        let _ = write!(output, " (uuid {})", quote(&label.id.to_string()));
    }
    output.push_str(")\n");
}

fn write_sheet(output: &mut String, sheet: &Sheet) {
    let _ = writeln!(
        output,
        "  (sheet (at {} {}) (size {} {})",
        number(sheet.position.x),
        number(sheet.position.y),
        number(sheet.size.x),
        number(sheet.size.y)
    );
    for node in &sheet.preserved {
        if !matches!(node.head.as_str(), "at" | "size" | "uuid") {
            write_node(output, &node.source, 4);
        }
    }
    if !has_head(&sheet.preserved, "property") {
        let _ = writeln!(
            output,
            "    (property \"Sheetname\" {} (at 0 0 0) (effects (font (size 1.27 1.27))))",
            quote(&sheet.name)
        );
        let _ = writeln!(
            output,
            "    (property \"Sheetfile\" {} (at 0 0 0) (effects (font (size 1.27 1.27))))",
            quote(&sheet.file_name)
        );
    }
    if !has_head(&sheet.preserved, "uuid") {
        let _ = writeln!(output, "    (uuid {})", quote(&sheet.id.to_string()));
    }
    output.push_str("  )\n");
}

fn write_sheet_instances(output: &mut String, data: &core::project::SchematicData) {
    output.push_str("  (sheet_instances\n    (path \"/\" (page \"1\"))\n");
    for (page, sheet) in (2..).zip(data.sheets.iter().filter(|sheet| sheet.parent.is_some())) {
        let _ = writeln!(
            output,
            "    (path {} (page {}))",
            quote(&format!("/{}", sheet.id)),
            quote(&page.to_string())
        );
    }
    output.push_str("  )\n");
}

fn preserved_children(node: &SExpr, excluded: &[&str]) -> Vec<PreservedNode> {
    node.items()
        .unwrap_or_default()
        .iter()
        .skip(1)
        .filter(|child| !child.head().is_some_and(|head| excluded.contains(&head)))
        .filter_map(preserved_node)
        .collect()
}

fn preserved_node(node: &SExpr) -> Option<PreservedNode> {
    Some(PreservedNode { head: node.head()?.to_string(), source: node.to_source() })
}

fn has_head(nodes: &[PreservedNode], head: &str) -> bool {
    nodes.iter().any(|node| node.head == head)
}

fn write_preserved(output: &mut String, nodes: &[PreservedNode], head: &str, indent: usize) {
    for node in nodes.iter().filter(|node| node.head == head) {
        write_node(output, &node.source, indent);
    }
}

fn write_node(output: &mut String, source: &str, indent: usize) {
    for _ in 0..indent {
        output.push(' ');
    }
    output.push_str(source);
    output.push('\n');
}

#[derive(Default)]
struct Warnings {
    preserved: BTreeSet<String>,
    notes: BTreeSet<String>,
}

impl Warnings {
    fn preserved(&mut self, head: &str) {
        self.preserved.insert(head.to_string());
    }

    fn note(&mut self, message: impl Into<String>) {
        self.notes.insert(message.into());
    }

    fn finish(self) -> Vec<String> {
        let mut messages: Vec<String> = self
            .preserved
            .into_iter()
            .map(|head| format!("preserved unsupported '{head}' construct(s) for export"))
            .collect();
        messages.extend(self.notes);
        messages
    }
}

fn child_atom(node: &SExpr, child: &str) -> Option<String> {
    node.child(child)?.get(1)?.atom().map(str::to_string)
}

fn parse_at(node: &SExpr) -> Result<(Point, f64), ParseError> {
    let at = node
        .child("at")
        .ok_or_else(|| ParseError::Syntax("object is missing its position".to_string()))?;
    let x = number_at(at, 1, "x")?;
    let y = number_at(at, 2, "y")?;
    let rotation = match at.get(3).and_then(SExpr::atom) {
        Some(value) => value
            .parse::<f64>()
            .map_err(|_| ParseError::Syntax(format!("invalid rotation '{value}'")))?,
        None => 0.0,
    };
    Ok((Point::new(x, y), rotation))
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

fn parse_uuid(node: &SExpr) -> Option<Uuid> {
    node.child("uuid")
        .and_then(|uuid| uuid.get(1))
        .and_then(SExpr::atom)
        .and_then(|value| Uuid::parse_str(value).ok())
}

fn quote(value: &str) -> String {
    let mut quoted = String::new();
    write_quoted(&mut quoted, value);
    quoted
}

fn number(value: f64) -> String {
    let mut text = format!("{value:.6}");
    while text.contains('.') && text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    if text == "-0" {
        text = "0".to_string();
    }
    text
}

#[cfg(test)]
mod tests {
    use super::{load_schematic, parse_schematic, save_schematic, write_schematic};

    const FIXTURE: &str = r#"(kicad_sch (version 20231120) (generator eeschema)
  (uuid "00000000-0000-0000-0000-000000000001")
  (paper "A4")
  (lib_symbols (symbol "Device:R" (property "Reference" "R")))
  (junction (at 10 10) (diameter 0) (color 0 0 0 0) (uuid "00000000-0000-0000-0000-000000000002"))
  (no_connect (at 20 10) (uuid "00000000-0000-0000-0000-000000000003"))
  (wire (pts (xy 10 10) (xy 20 10)) (stroke (width 0) (type default)) (uuid "00000000-0000-0000-0000-000000000004"))
  (label "SDA" (at 10 10 0) (effects (font (size 1.27 1.27))) (uuid "00000000-0000-0000-0000-000000000005"))
  (global_label "GND" (shape input) (at 20 10 180) (uuid "00000000-0000-0000-0000-000000000006"))
  (symbol (lib_id "Device:R") (at 30 20 90) (unit 1)
    (property "Reference" "R1" (at 30 20 0) (effects (font (size 1.27 1.27))))
    (property "Value" "10k" (at 30 20 0))
    (pin "1" (uuid "00000000-0000-0000-0000-000000000007"))
    (uuid "00000000-0000-0000-0000-000000000008"))
  (sheet (at 100 100) (size 20 10)
    (property "Sheetname" "Power" (at 100 100 0))
    (property "Sheetfile" "power.kicad_sch" (at 100 100 0))
    (uuid "00000000-0000-0000-0000-000000000009"))
  (sheet_instances (path "/" (page "1")))
  (text "note" (at 0 0 0) (uuid "00000000-0000-0000-0000-00000000000a"))
)"#;

    #[test]
    fn imports_components_wires_labels_junctions_and_sheets() {
        let imported = parse_schematic(FIXTURE).expect("fixture parses");
        let data = &imported.data;

        assert_eq!(data.sheets.len(), 2);
        assert_eq!(data.sheets[0].components.len(), 1);
        assert_eq!(data.sheets[0].children.len(), 1);
        assert_eq!(data.sheets[0].wires.len(), 1);
        assert_eq!(data.sheets[0].junctions.len(), 1);
        assert_eq!(data.sheets[0].no_connects.len(), 1);
        assert_eq!(data.sheets[0].labels.len(), 2);
        assert_eq!(data.components.len(), 1);

        let component = &data.components[0];
        assert_eq!(component.reference_designator, "R1");
        assert_eq!(component.library_id, "Device:R");
        assert_eq!(component.placement.position.x, 30.0);
        assert_eq!(component.placement.rotation, 90.0);
        assert_eq!(component.placement.unit, 1);
        assert_eq!(data.sheets[1].name, "Power");
        assert_eq!(data.sheets[1].file_name, "power.kicad_sch");
        assert!(imported.warnings.iter().any(|warning| warning.contains("text")));
    }

    #[test]
    fn export_preserves_library_and_unknown_nodes_and_reimports() {
        let imported = parse_schematic(FIXTURE).expect("fixture parses");
        let document = write_schematic(&imported.data).expect("schematic writes");

        assert!(document.contains("(lib_symbols (symbol \"Device:R\""));
        assert!(document.contains("(text \"note\""));
        assert!(document.contains("(global_label \"GND\""));
        assert!(document.contains("(sheet_instances (path \"/\" (page \"1\")))"));

        let restored = parse_schematic(&document).expect("written schematic parses");
        assert_eq!(restored.data.components.len(), 1);
        assert_eq!(restored.data.sheets.len(), 2);
        assert_eq!(
            restored.data.components[0]
                .properties
                .iter()
                .find(|property| property.name == "Value")
                .map(|property| property.value.as_str()),
            Some("10k")
        );
    }

    #[test]
    fn file_helpers_create_parent_directories() {
        let imported = parse_schematic(FIXTURE).expect("fixture parses");
        let directory =
            std::env::temp_dir().join(format!("kaicad-kicad-sch-test-{}", uuid::Uuid::new_v4()));
        let path = directory.join("nested/main.kicad_sch");
        save_schematic(&imported.data, &path).expect("save schematic");
        let restored = load_schematic(&path).expect("load schematic");
        assert_eq!(restored.data.components.len(), 1);
        std::fs::remove_dir_all(directory).expect("clean temp directory");
    }

    #[test]
    fn wrong_root_is_rejected() {
        assert!(parse_schematic("(kicad_pcb)").is_err());
        assert!(parse_schematic("(kicad_sch (wire (pts)))").is_err());
    }

    #[test]
    fn unwritten_parentheses_are_rejected() {
        assert!(parse_schematic("(kicad_sch (symbol").is_err());
    }
}
