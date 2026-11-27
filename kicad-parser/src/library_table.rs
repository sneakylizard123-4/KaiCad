//! Read and write KiCad `sym-lib-table` and `fp-lib-table` files.
//!
//! Library tables are small S-expression indexes. KaiCad keeps every table
//! entry, its order and any table-level nodes it does not model, so a project
//! bundle can be rewritten without dropping library search paths.

use std::fmt::Write as _;
use std::path::Path;

use super::{ParseError, SExpr, parse_document, write_quoted};

/// Which KiCad library table a document is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryTableKind {
    /// A `sym-lib-table` document indexing symbol libraries.
    Symbol,
    /// An `fp-lib-table` document indexing footprint libraries.
    Footprint,
}

impl LibraryTableKind {
    /// The KiCad root keyword for this table kind.
    pub fn root(self) -> &'static str {
        match self {
            Self::Symbol => "sym_lib_table",
            Self::Footprint => "fp_lib_table",
        }
    }

    /// The conventional file name for this table kind.
    pub fn file_name(self) -> &'static str {
        match self {
            Self::Symbol => "sym-lib-table",
            Self::Footprint => "fp-lib-table",
        }
    }
}

/// One library entry in a KiCad library table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryEntry {
    /// Library nickname used by schematic or board files.
    pub name: String,
    /// KiCad library type, normally `KiCad`.
    pub library_type: String,
    /// Library path or URI, often containing `${KIPRJMOD}`.
    pub uri: String,
    /// Library options string, normally empty.
    pub options: String,
    /// Human-readable description.
    pub description: String,
    /// Whether KiCad has disabled this entry.
    pub disabled: bool,
    /// Unknown child nodes retained in source form.
    pub preserved: Vec<String>,
}

impl LibraryEntry {
    /// Build an enabled `KiCad` entry with empty options and description.
    pub fn new(name: impl Into<String>, uri: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            library_type: "KiCad".to_string(),
            uri: uri.into(),
            options: String::new(),
            description: String::new(),
            disabled: false,
            preserved: Vec::new(),
        }
    }
}

/// A parsed `sym-lib-table` or `fp-lib-table` document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryTable {
    /// Table kind, fixed by the document root.
    pub kind: LibraryTableKind,
    /// Optional table format version.
    pub version: Option<String>,
    /// Libraries in source order.
    pub libraries: Vec<LibraryEntry>,
    /// Unknown root nodes retained in source form.
    pub preserved: Vec<String>,
}

impl LibraryTable {
    /// Create an empty table of the requested kind.
    pub fn new(kind: LibraryTableKind) -> Self {
        Self { kind, version: None, libraries: Vec::new(), preserved: Vec::new() }
    }

    /// Find a library by nickname.
    pub fn find(&self, name: &str) -> Option<&LibraryEntry> {
        self.libraries.iter().find(|entry| entry.name == name)
    }

    /// Parse a table from source text.
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        parse_library_table(input)
    }

    /// Load a table from disk.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ParseError> {
        load_library_table(path)
    }

    /// Render the table as KiCad S-expression source.
    pub fn to_source(&self) -> String {
        write_library_table(self)
    }

    /// Save the table, creating parent directories as needed.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), ParseError> {
        save_library_table(self, path)
    }
}

/// Parse a `sym-lib-table` or `fp-lib-table` document.
pub fn parse_library_table(input: &str) -> Result<LibraryTable, ParseError> {
    let root = parse_document(input)?;
    let items =
        root.items().ok_or_else(|| ParseError::Syntax("expected a top-level list".to_string()))?;
    let kind = match items.first().and_then(SExpr::atom) {
        Some("sym_lib_table") => LibraryTableKind::Symbol,
        Some("fp_lib_table") => LibraryTableKind::Footprint,
        Some(other) => {
            return Err(ParseError::Unsupported(format!(
                "expected sym_lib_table or fp_lib_table, found '{other}'"
            )));
        }
        None => return Err(ParseError::Syntax("empty top-level list".to_string())),
    };

    let mut table = LibraryTable::new(kind);
    for item in items.iter().skip(1) {
        match item.head() {
            Some("version") => {
                table.version = item
                    .items()
                    .and_then(|values| values.get(1))
                    .and_then(SExpr::atom)
                    .map(str::to_string);
            }
            Some("lib") => table.libraries.push(parse_library_entry(item)?),
            Some(_) => table.preserved.push(item.to_source()),
            None => {}
        }
    }
    Ok(table)
}

fn parse_library_entry(node: &SExpr) -> Result<LibraryEntry, ParseError> {
    let name = child_atom(node, "name")
        .ok_or_else(|| ParseError::Syntax("library entry is missing its name".to_string()))?;
    let library_type = child_atom(node, "type").unwrap_or("KiCad").to_string();
    let uri = child_atom(node, "uri").unwrap_or_default().to_string();
    let options = child_atom(node, "options").unwrap_or_default().to_string();
    let description = child_atom(node, "descr").unwrap_or_default().to_string();
    let disabled = node.child("disabled").is_some();
    let mut preserved = Vec::new();
    for child in node.items().unwrap_or_default().iter().skip(1) {
        if !matches!(child.head(), Some("name" | "type" | "uri" | "options" | "descr" | "disabled"))
        {
            preserved.push(child.to_source());
        }
    }
    Ok(LibraryEntry {
        name: name.to_string(),
        library_type,
        uri,
        options,
        description,
        disabled,
        preserved,
    })
}

fn child_atom<'a>(node: &'a SExpr, head: &str) -> Option<&'a str> {
    node.child(head)?.get(1)?.atom()
}

/// Render a library table as deterministic KiCad source.
pub fn write_library_table(table: &LibraryTable) -> String {
    let mut output = String::new();
    let _ = writeln!(output, "({}", table.kind.root());
    if let Some(version) = &table.version {
        let _ = writeln!(output, "  (version {version})");
    }
    for node in &table.preserved {
        let _ = writeln!(output, "  {node}");
    }
    for entry in &table.libraries {
        output.push_str("  (lib");
        write_string_field(&mut output, "name", &entry.name);
        write_string_field(&mut output, "type", &entry.library_type);
        write_string_field(&mut output, "uri", &entry.uri);
        write_string_field(&mut output, "options", &entry.options);
        write_string_field(&mut output, "descr", &entry.description);
        if entry.disabled {
            output.push_str(" (disabled)");
        }
        for node in &entry.preserved {
            let _ = write!(output, " {node}");
        }
        output.push_str(")\n");
    }
    output.push_str(")\n");
    output
}

fn write_string_field(output: &mut String, name: &str, value: &str) {
    let _ = write!(output, " ({name} ");
    write_quoted(output, value);
    output.push(')');
}

/// Load and parse a `sym-lib-table` or `fp-lib-table` file.
pub fn load_library_table(path: impl AsRef<Path>) -> Result<LibraryTable, ParseError> {
    let path = path.as_ref();
    parse_library_table(&std::fs::read_to_string(path)?)
}

/// Write a library table, creating parent directories as needed.
pub fn save_library_table(table: &LibraryTable, path: impl AsRef<Path>) -> Result<(), ParseError> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, write_library_table(table))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        LibraryTable, LibraryTableKind, load_library_table, parse_library_table,
        save_library_table, write_library_table,
    };

    const SYMBOL_TABLE: &str = r#"(sym_lib_table
      (version 7)
      (lib (name "Device")(type "KiCad")(uri "${KICAD8_SYMBOL_DIR}/Device.kicad_sym")(options "")(descr "Standard symbols"))
      (lib (name "Local")(type "KiCad")(uri "${KIPRJMOD}/Local.kicad_sym")(options "")(descr "")(disabled))
    )"#;

    #[test]
    fn parses_symbol_and_footprint_tables() {
        let table = parse_library_table(SYMBOL_TABLE).expect("symbol table parses");
        assert_eq!(table.kind, LibraryTableKind::Symbol);
        assert_eq!(table.version.as_deref(), Some("7"));
        assert_eq!(table.libraries.len(), 2);
        assert_eq!(table.libraries[0].name, "Device");
        assert_eq!(table.libraries[0].library_type, "KiCad");
        assert_eq!(table.libraries[0].description, "Standard symbols");
        assert!(table.libraries[1].disabled);

        let footprint =
            parse_library_table("(fp_lib_table (version 7))").expect("footprint table parses");
        assert_eq!(footprint.kind, LibraryTableKind::Footprint);
        assert!(footprint.libraries.is_empty());
    }

    #[test]
    fn round_trip_preserves_entries_and_unknown_nodes() {
        let table = parse_library_table(SYMBOL_TABLE).expect("table parses");
        let document = write_library_table(&table);
        let restored = parse_library_table(&document).expect("written table parses");
        assert_eq!(restored, table);
        assert!(document.contains("${KIPRJMOD}/Local.kicad_sym"));

        let with_unknown = "(sym_lib_table (version 7) (future_setting \"keep\") (lib (name \"A\")(type \"KiCad\")(uri \"a\")))";
        let table = parse_library_table(with_unknown).expect("unknown root node parses");
        assert_eq!(table.preserved, vec!["(future_setting \"keep\")".to_string()]);
        let restored = parse_library_table(&write_library_table(&table)).expect("reparses");
        assert_eq!(restored.preserved, table.preserved);
    }

    #[test]
    fn file_helpers_round_trip() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("nested").join("sym-lib-table");
        let table = parse_library_table(SYMBOL_TABLE).expect("table parses");
        save_library_table(&table, &path).expect("table saves");
        let restored = load_library_table(&path).expect("table loads");
        assert_eq!(restored, table);
    }

    #[test]
    fn wrong_root_is_rejected() {
        let error = parse_library_table("(kicad_pcb (version 20240108))")
            .expect_err("wrong root must fail");
        assert!(matches!(error, super::ParseError::Unsupported(_)));
    }

    #[test]
    fn missing_library_name_is_an_error() {
        let error = parse_library_table("(sym_lib_table (lib (type \"KiCad\")(uri \"a\")))")
            .expect_err("missing name must fail");
        assert!(matches!(error, super::ParseError::Syntax(_)));
    }

    #[test]
    fn missing_file_is_io_error() {
        let directory = tempfile::tempdir().expect("temp dir");
        let error = load_library_table(directory.path().join("missing"))
            .expect_err("missing file must fail");
        assert!(matches!(error, super::ParseError::Io(_)));
    }

    #[test]
    fn new_table_uses_expected_file_name() {
        assert_eq!(LibraryTable::new(LibraryTableKind::Symbol).kind.file_name(), "sym-lib-table");
        assert_eq!(LibraryTable::new(LibraryTableKind::Footprint).kind.file_name(), "fp-lib-table");
    }
}
