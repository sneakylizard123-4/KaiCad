//! Recursive KiCad project-bundle discovery and round-trip support.
//!
//! A KiCad project is not one file: a `.kicad_pro` points at a root
//! `.kicad_sch`, that schematic embeds child sheet symbols, and each child is a
//! separate `.kicad_sch`. A board is a separate `.kicad_pcb`. This module loads
//! that graph, merges child content into [`core::project::SchematicData`] while
//! retaining each child's source document, and can write the hierarchy back.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use core::project::SchematicData;
use core::sheet::Sheet;
use pcb::PcbDesign;

use super::schematic::{SchematicImport, load_schematic, write_schematic};
use super::{ParseError, ProjectFile, load_pcb, write_pcb};

/// A loaded KiCad project hierarchy.
#[derive(Debug, Clone)]
pub struct ProjectBundle {
    /// The `.kicad_pro` document when one was found.
    pub project: Option<ProjectFile>,
    /// Path of the project file, when present.
    pub project_path: Option<PathBuf>,
    /// Root schematic path.
    pub root_path: PathBuf,
    /// Loaded board, when a sibling `.kicad_pcb` exists.
    pub board: Option<PcbDesign>,
    /// Board path, when present.
    pub board_path: Option<PathBuf>,
    /// Merged schematic data. Child sheet contents live on their `Sheet`.
    pub schematic: SchematicData,
    /// Import warnings from every loaded file.
    pub warnings: Vec<String>,
}

impl ProjectBundle {
    /// Load a `.kicad_pro` or root `.kicad_sch` and recursively load siblings.
    pub fn load(input: impl AsRef<Path>) -> Result<Self, ParseError> {
        let input = input.as_ref();
        let (project, project_path, root_path) = resolve_inputs(input)?;
        let board_path = sibling_with_extension(&root_path, "kicad_pcb");
        let mut warnings = Vec::new();

        let mut root = load_schematic(&root_path)?;
        warnings.append(&mut root.warnings);
        let base = root_path.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
        let mut visited = BTreeSet::new();
        visited.insert(canonical(&root_path));
        merge_children(&mut root.data, &base, &mut visited, &mut warnings)?;

        let (board, board_path) = match board_path {
            Some(path) if path.exists() => {
                let imported = load_pcb(&path)?;
                warnings.extend(imported.warnings);
                (Some(imported.design), Some(path))
            }
            _ => (None, None),
        };

        Ok(Self {
            project,
            project_path,
            root_path,
            board,
            board_path,
            schematic: root.data,
            warnings,
        })
    }

    /// Write the project hierarchy into `output_dir`.
    ///
    /// Files keep their source names. Child sheet paths are relative to the
    /// root schematic, as KiCad expects.
    pub fn save(&self, output_dir: impl AsRef<Path>) -> Result<(), ParseError> {
        let output_dir = output_dir.as_ref();
        std::fs::create_dir_all(output_dir)?;

        if let Some(project) = &self.project {
            let name = self
                .project_path
                .as_ref()
                .and_then(|path| path.file_name())
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("project.kicad_pro"));
            project.save(output_dir.join(name))?;
        }

        let root = self.root_sheet()?;
        let root_document = self.document_for(root, true);
        let root_name = self
            .root_path
            .file_name()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("project.kicad_sch"));
        write_document(&root_document, output_dir.join(root_name))?;

        for sheet in self.schematic.sheets.iter().filter(|sheet| sheet.parent.is_some()) {
            let Some(source) = &sheet.source else {
                continue;
            };
            if sheet.file_name.is_empty() {
                continue;
            }
            let document = self.document_for(sheet, false);
            let path = output_dir.join(&sheet.file_name);
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)?;
                }
            }
            let _ = source;
            write_document(&document, path)?;
        }

        if let (Some(board), Some(path)) = (&self.board, &self.board_path) {
            let name = path.file_name().map(PathBuf::from).unwrap_or_else(|| {
                self.root_path
                    .with_extension("kicad_pcb")
                    .file_name()
                    .map(PathBuf::from)
                    .unwrap_or_default()
            });
            std::fs::write(output_dir.join(name), write_pcb(board)?)?;
        }
        Ok(())
    }

    fn root_sheet(&self) -> Result<&Sheet, ParseError> {
        self.schematic
            .sheets
            .iter()
            .find(|sheet| sheet.parent.is_none())
            .or_else(|| self.schematic.sheets.first())
            .ok_or_else(|| {
                ParseError::Unsupported("project has no root schematic sheet".to_string())
            })
    }

    fn document_for(&self, sheet: &Sheet, root: bool) -> SchematicData {
        let mut sheets = vec![sheet.clone()];
        sheets[0].parent = None;
        let direct: Vec<Sheet> = self
            .schematic
            .sheets
            .iter()
            .filter(|candidate| candidate.parent == Some(sheet.id))
            .cloned()
            .collect();
        sheets.extend(direct);

        let components = self
            .schematic
            .components
            .iter()
            .filter(|component| sheet.components.contains(&component.id))
            .cloned()
            .collect();
        SchematicData {
            sheets,
            symbols: Vec::new(),
            nets: Vec::new(),
            components,
            source: if root { self.schematic.source.clone() } else { sheet.source.clone() },
        }
    }

    /// Number of schematic files represented by this bundle.
    pub fn sheet_file_count(&self) -> usize {
        1 + self.schematic.sheets.iter().filter(|sheet| sheet.parent.is_some()).count()
    }
}

fn resolve_inputs(
    input: &Path,
) -> Result<(Option<ProjectFile>, Option<PathBuf>, PathBuf), ParseError> {
    match input.extension().and_then(|extension| extension.to_str()) {
        Some("kicad_pro") => {
            let project = ProjectFile::load(input)?;
            let root = input.with_extension("kicad_sch");
            if !root.exists() {
                return Err(ParseError::Unsupported(format!(
                    "project has no sibling root schematic: {}",
                    root.display()
                )));
            }
            Ok((Some(project), Some(input.to_path_buf()), root))
        }
        Some("kicad_sch") => {
            let project_path = input.with_extension("kicad_pro");
            let project =
                if project_path.exists() { Some(ProjectFile::load(&project_path)?) } else { None };
            Ok((project, project_path.exists().then_some(project_path), input.to_path_buf()))
        }
        _ => Err(ParseError::Unsupported(
            "project bundle input must be .kicad_pro or .kicad_sch".to_string(),
        )),
    }
}

fn merge_children(
    data: &mut SchematicData,
    base: &Path,
    visited: &mut BTreeSet<PathBuf>,
    warnings: &mut Vec<String>,
) -> Result<(), ParseError> {
    let mut index = 0;
    while index < data.sheets.len() {
        let (parent_id, file_name) = {
            let sheet = &data.sheets[index];
            (sheet.id, sheet.file_name.clone())
        };
        if data.sheets[index].parent.is_none() || file_name.is_empty() {
            index += 1;
            continue;
        }

        let path = if Path::new(&file_name).is_absolute() {
            PathBuf::from(&file_name)
        } else {
            base.join(&file_name)
        };
        if !path.exists() {
            warnings.push(format!("child sheet file is missing: {}", path.display()));
            index += 1;
            continue;
        }
        let canonical_path = canonical(&path);
        if !visited.insert(canonical_path.clone()) {
            warnings.push(format!("skipped recursive sheet cycle at {}", path.display()));
            index += 1;
            continue;
        }

        let SchematicImport { data: mut child_data, warnings: mut child_warnings } =
            load_schematic(&path)?;
        warnings.append(&mut child_warnings);
        if child_data.sheets.is_empty() {
            warnings.push(format!("child schematic has no root sheet: {}", path.display()));
            index += 1;
            continue;
        }
        let mut child_root = child_data.sheets.remove(0);

        let components = child_root.components.clone();
        {
            let sheet = &mut data.sheets[index];
            sheet.wires = std::mem::take(&mut child_root.wires);
            sheet.labels = std::mem::take(&mut child_root.labels);
            sheet.junctions = std::mem::take(&mut child_root.junctions);
            sheet.no_connects = std::mem::take(&mut child_root.no_connects);
            sheet.components = components;
            sheet.source = child_data.source.take();
            sheet.children.clear();
        }
        data.components.extend(child_data.components);
        data.symbols.extend(child_data.symbols);
        data.nets.extend(child_data.nets);

        for mut child_sheet in child_data.sheets {
            child_sheet.parent = Some(parent_id);
            data.sheets[index].children.push(child_sheet.id);
            data.sheets.push(child_sheet);
        }
        index += 1;
    }
    Ok(())
}

fn write_document(data: &SchematicData, path: PathBuf) -> Result<(), ParseError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, write_schematic(data)?)?;
    Ok(())
}

fn sibling_with_extension(path: &Path, extension: &str) -> Option<PathBuf> {
    let candidate = path.with_extension(extension);
    candidate.exists().then_some(candidate)
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}
