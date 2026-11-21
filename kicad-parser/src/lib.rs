//! Minimal, deterministic KiCad symbol-library parsing.
//!
//! KiCad's file format is an import/export target only; it never becomes
//! KaiCad's internal representation. This crate reads a `.kicad_sym` file and
//! maps it into [`core::SymbolLibrary`].
//!
//! The parser deliberately implements only the S-expression subset KiCad
//! symbol libraries use. It never panics on malformed input and performs no I/O
//! beyond reading the requested file.

use std::fmt;
use std::path::Path;

use core::symbol::{Pin, PinDirection, PinPosition, PinType, Symbol};
use core::symbol_library::{SymbolLibrary, SymbolLibrarySource};
use uuid::Uuid;

/// Errors raised while reading a KiCad symbol library.
#[derive(Debug)]
pub enum ParseError {
    /// The file could not be read.
    Io(std::io::Error),
    /// The document is not well-formed S-expression syntax.
    Syntax(String),
    /// The document is valid syntax but not a supported KiCad construct.
    Unsupported(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "I/O error: {error}"),
            Self::Syntax(message) => write!(f, "syntax error: {message}"),
            Self::Unsupported(message) => write!(f, "unsupported KiCad construct: {message}"),
        }
    }
}

impl std::error::Error for ParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Syntax(_) | Self::Unsupported(_) => None,
        }
    }
}

impl From<std::io::Error> for ParseError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// Parse a KiCad `.kicad_sym` file and return a [`SymbolLibrary`].
///
/// Symbol ids are derived deterministically from the library and symbol names,
/// so parsing the same content always produces the same ids.
pub fn parse_symbol_library(path: impl AsRef<Path>) -> Result<SymbolLibrary, ParseError> {
    let path = path.as_ref();
    let document = std::fs::read_to_string(path)?;
    let root = parse_document(&document)?;
    let items =
        root.items().ok_or_else(|| ParseError::Syntax("expected a top-level list".to_string()))?;

    match items.first().and_then(SExpr::atom) {
        Some("kicad_symbol_lib") => {}
        Some(other) => {
            return Err(ParseError::Unsupported(format!(
                "expected a kicad_symbol_lib document, found '{other}'"
            )));
        }
        None => return Err(ParseError::Syntax("empty top-level list".to_string())),
    }

    let name = library_name(path);
    let mut symbols = Vec::new();
    for item in items.iter().skip(1) {
        if item.head() == Some("symbol") {
            symbols.push(parse_symbol(item, &name)?);
        }
    }

    let id = library_id(&name, &symbols);
    let source = SymbolLibrarySource::KiCad { path: path.display().to_string() };

    Ok(SymbolLibrary { id, name, source, symbols })
}

/// Use the file stem as the library name, as KiCad files do not record one.
fn library_name(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or("library")
        .to_string()
}

/// Derive the library id from its name and the symbol ids it contains.
fn library_id(name: &str, symbols: &[Symbol]) -> Uuid {
    let mut seed = format!("kicad-library|{name}");
    for symbol in symbols {
        seed.push('|');
        seed.push_str(&symbol.id.to_string());
    }
    Uuid::new_v5(&Uuid::NAMESPACE_OID, seed.as_bytes())
}

fn parse_symbol(node: &SExpr, library: &str) -> Result<Symbol, ParseError> {
    let items =
        node.items().ok_or_else(|| ParseError::Syntax("symbol must be a list".to_string()))?;
    let name = items
        .get(1)
        .and_then(SExpr::atom)
        .ok_or_else(|| ParseError::Syntax("symbol is missing its name".to_string()))?
        .to_string();

    let id =
        Uuid::new_v5(&Uuid::NAMESPACE_OID, format!("kicad-symbol|{library}|{name}").as_bytes());

    let mut pins = Vec::new();
    collect_pins(node, id, &mut pins)?;

    Ok(Symbol { id, name, pins, library: library.to_string() })
}

/// Collect pin definitions, including those inside nested unit symbols.
fn collect_pins(node: &SExpr, symbol_id: Uuid, pins: &mut Vec<Pin>) -> Result<(), ParseError> {
    let Some(items) = node.items() else {
        return Ok(());
    };

    for item in items {
        match item.head() {
            Some("pin") => pins.push(parse_pin(item, symbol_id)?),
            Some("symbol") => collect_pins(item, symbol_id, pins)?,
            _ => {}
        }
    }

    Ok(())
}

fn parse_pin(node: &SExpr, symbol_id: Uuid) -> Result<Pin, ParseError> {
    let items = node.items().ok_or_else(|| ParseError::Syntax("pin must be a list".to_string()))?;
    let electrical = items
        .get(1)
        .and_then(SExpr::atom)
        .ok_or_else(|| ParseError::Syntax("pin is missing its electrical type".to_string()))?;
    let number = node
        .child("number")
        .and_then(|items| items.get(1))
        .and_then(SExpr::atom)
        .ok_or_else(|| ParseError::Syntax("pin is missing its number".to_string()))?
        .to_string();
    let name = node
        .child("name")
        .and_then(|items| items.get(1))
        .and_then(SExpr::atom)
        .unwrap_or_default()
        .to_string();

    let id =
        Uuid::new_v5(&Uuid::NAMESPACE_OID, format!("kicad-pin|{symbol_id}|{number}").as_bytes());

    Ok(Pin {
        id,
        number,
        name,
        electrical_type: pin_type(electrical),
        position: PinPosition::default(),
        direction: PinDirection::default(),
    })
}

/// Map KiCad electrical types; unknown types preserve the pin as unspecified.
fn pin_type(kicad: &str) -> PinType {
    match kicad {
        "input" => PinType::Input,
        "output" => PinType::Output,
        "bidirectional" => PinType::Bidirectional,
        "passive" => PinType::Passive,
        "power_in" | "power_out" => PinType::Power,
        "open_collector" => PinType::OpenCollector,
        "open_emitter" => PinType::OpenEmitter,
        "tri_state" => PinType::TriState,
        _ => PinType::Unspecified,
    }
}

/// A parsed S-expression: either an atom or a list of expressions.
#[derive(Debug, PartialEq)]
enum SExpr {
    Atom(String),
    List(Vec<SExpr>),
}

impl SExpr {
    fn atom(&self) -> Option<&str> {
        match self {
            Self::Atom(value) => Some(value),
            Self::List(_) => None,
        }
    }

    fn items(&self) -> Option<&[SExpr]> {
        match self {
            Self::List(items) => Some(items),
            Self::Atom(_) => None,
        }
    }

    /// First atom of a list, i.e. the expression's keyword.
    fn head(&self) -> Option<&str> {
        self.items()?.first()?.atom()
    }

    /// First direct child list whose keyword is `name`.
    fn child(&self, name: &str) -> Option<&[SExpr]> {
        self.items()?.iter().find_map(|item| {
            let items = item.items()?;
            (items.first()?.atom()? == name).then_some(items)
        })
    }
}

#[derive(Debug, PartialEq)]
enum Token {
    Open,
    Close,
    Atom(String),
}

fn parse_document(input: &str) -> Result<SExpr, ParseError> {
    let tokens = tokenize(input)?;
    if tokens.is_empty() {
        return Err(ParseError::Syntax("empty document".to_string()));
    }

    let mut index = 0;
    let expression = parse_expression(&tokens, &mut index)?;
    if index != tokens.len() {
        return Err(ParseError::Syntax(
            "trailing tokens after the top-level expression".to_string(),
        ));
    }
    Ok(expression)
}

fn parse_expression(tokens: &[Token], index: &mut usize) -> Result<SExpr, ParseError> {
    match tokens.get(*index) {
        Some(Token::Open) => {
            *index += 1;
            let mut items = Vec::new();
            loop {
                match tokens.get(*index) {
                    Some(Token::Close) => {
                        *index += 1;
                        return Ok(SExpr::List(items));
                    }
                    Some(_) => items.push(parse_expression(tokens, index)?),
                    None => return Err(ParseError::Syntax("unclosed list".to_string())),
                }
            }
        }
        Some(Token::Atom(value)) => {
            *index += 1;
            Ok(SExpr::Atom(value.clone()))
        }
        Some(Token::Close) => Err(ParseError::Syntax("unexpected ')'".to_string())),
        None => Err(ParseError::Syntax("unexpected end of document".to_string())),
    }
}

fn tokenize(input: &str) -> Result<Vec<Token>, ParseError> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(character) = chars.next() {
        match character {
            '(' => tokens.push(Token::Open),
            ')' => tokens.push(Token::Close),
            character if character.is_whitespace() => {}
            '"' => tokens.push(Token::Atom(read_string(&mut chars)?)),
            character => {
                let mut atom = String::from(character);
                while let Some(&next) = chars.peek() {
                    if next.is_whitespace() || matches!(next, '(' | ')' | '"') {
                        break;
                    }
                    atom.push(next);
                    chars.next();
                }
                tokens.push(Token::Atom(atom));
            }
        }
    }

    Ok(tokens)
}

fn read_string(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Result<String, ParseError> {
    let mut value = String::new();

    loop {
        match chars.next() {
            Some('"') => return Ok(value),
            Some('\\') => match chars.next() {
                Some('n') => value.push('\n'),
                Some('t') => value.push('\t'),
                Some('r') => value.push('\r'),
                Some(other) => value.push(other),
                None => {
                    return Err(ParseError::Syntax(
                        "unterminated escape sequence in string".to_string(),
                    ));
                }
            },
            Some(other) => value.push(other),
            None => return Err(ParseError::Syntax("unterminated string".to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ParseError, Token, parse_symbol_library, tokenize};
    use core::symbol::PinType;
    use core::symbol_library::SymbolLibrarySource;

    fn fixture(body: &str) -> String {
        format!("(kicad_symbol_lib (version 20231120) (generator kicad_symbol_editor) {body})")
    }

    fn parse_fixture(body: &str) -> core::SymbolLibrary {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("Device.kicad_sym");
        std::fs::write(&path, fixture(body)).expect("write fixture");
        parse_symbol_library(&path).expect("parse fixture")
    }

    #[test]
    fn tokenizer_handles_atoms_strings_and_nesting() {
        let tokens = tokenize(r#"(a "b c" (d 1))"#).expect("tokenize");

        assert_eq!(
            tokens,
            vec![
                Token::Open,
                Token::Atom("a".to_string()),
                Token::Atom("b c".to_string()),
                Token::Open,
                Token::Atom("d".to_string()),
                Token::Atom("1".to_string()),
                Token::Close,
                Token::Close,
            ]
        );
    }

    #[test]
    fn tokenizer_rejects_unterminated_string() {
        let error = tokenize("(a \"b)").expect_err("unterminated string must fail");

        assert!(matches!(error, ParseError::Syntax(_)));
    }

    #[test]
    fn missing_file_is_io_error() {
        let dir = tempfile::tempdir().expect("temp dir");
        let error = parse_symbol_library(dir.path().join("missing.kicad_sym"))
            .expect_err("missing file must fail");

        assert!(matches!(error, ParseError::Io(_)));
    }

    #[test]
    fn malformed_document_is_syntax_error() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("Broken.kicad_sym");
        std::fs::write(&path, "(kicad_symbol_lib (symbol \"R\"").expect("write fixture");

        let error = parse_symbol_library(&path).expect_err("unclosed list must fail");

        assert!(matches!(error, ParseError::Syntax(_)));
    }

    #[test]
    fn minimal_library_maps_name_source_and_symbol() {
        let library = parse_fixture(
            r#"(symbol "Device:R" (symbol "R_1_1" (pin passive line (name "~") (number "1"))))"#,
        );

        assert_eq!(library.name, "Device");
        match &library.source {
            SymbolLibrarySource::KiCad { path } => {
                assert!(path.ends_with("Device.kicad_sym"));
            }
            other => panic!("expected KiCad source, got {other:?}"),
        }
        assert_eq!(library.symbols.len(), 1);
        assert_eq!(library.symbols[0].name, "Device:R");
        assert_eq!(library.symbols[0].library, "Device");
    }

    #[test]
    fn multiple_symbols_get_distinct_ids() {
        let library = parse_fixture(
            r#"(symbol "Device:R" (pin passive line (name "~") (number "1")))
               (symbol "Device:C" (pin passive line (name "~") (number "1")))"#,
        );

        assert_eq!(library.symbols.len(), 2);
        assert_ne!(library.symbols[0].id, library.symbols[1].id);
    }

    #[test]
    fn pins_keep_name_number_and_electrical_type() {
        let library = parse_fixture(
            r#"(symbol "Device:U" (symbol "U_1_1"
                 (pin input line (name "IN") (number "1"))
                 (pin power_in line (name "VCC") (number "2"))))"#,
        );

        let pins = &library.symbols[0].pins;
        assert_eq!(pins.len(), 2);
        assert_eq!(pins[0].number, "1");
        assert_eq!(pins[0].name, "IN");
        assert!(matches!(pins[0].electrical_type, PinType::Input));
        assert_eq!(pins[1].number, "2");
        assert_eq!(pins[1].name, "VCC");
        assert!(matches!(pins[1].electrical_type, PinType::Power));
        assert_ne!(pins[0].id, pins[1].id);
    }

    #[test]
    fn parsing_is_deterministic() {
        let body = r#"(symbol "Device:R" (symbol "R_1_1"
            (pin passive line (name "~") (number "1"))))"#;

        let first = parse_fixture(body);
        let second = parse_fixture(body);

        assert_eq!(first.id, second.id);
        assert_eq!(first.symbols[0].id, second.symbols[0].id);
        assert_eq!(first.symbols[0].pins[0].id, second.symbols[0].pins[0].id);
    }

    #[test]
    fn unknown_electrical_type_is_unspecified() {
        let library =
            parse_fixture(r#"(symbol "Device:R" (pin free line (name "N") (number "1")))"#);

        assert!(matches!(library.symbols[0].pins[0].electrical_type, PinType::Unspecified));
    }
}
