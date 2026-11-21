//! Parse a KiCad symbol library and print a summary.
//!
//! ```text
//! cargo run -p kicad-parser --example parse_library -- path/to/Device.kicad_sym
//! ```
//!
//! With no argument a small fixture is written to the system temp directory,
//! parsed, and removed again.

use std::path::PathBuf;

use core::symbol_library::SymbolLibrarySource;

const FIXTURE: &str = r#"(kicad_symbol_lib
  (version 20231120)
  (generator kicad_symbol_editor)
  (symbol "Device:R"
    (symbol "R_1_1"
      (pin passive line (at 0 3.81 270) (length 1.27)
        (name "~") (number "1"))
      (pin passive line (at 0 -3.81 90) (length 1.27)
        (name "~") (number "2"))
    )
  )
)"#;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut fixture_path = None;
    let path = match std::env::args_os().nth(1) {
        Some(path) => PathBuf::from(path),
        None => {
            let path = std::env::temp_dir().join("kaicad-parse-example.kicad_sym");
            std::fs::write(&path, FIXTURE)?;
            fixture_path = Some(path.clone());
            path
        }
    };

    let library = kicad_parser::parse_symbol_library(&path)?;

    println!("library: {}", library.name);
    match &library.source {
        SymbolLibrarySource::Internal => println!("source: internal"),
        SymbolLibrarySource::KiCad { path } => println!("source: KiCad {path}"),
    }
    println!("symbols: {}", library.symbols.len());
    for symbol in &library.symbols {
        println!("  {} ({} pins)", symbol.name, symbol.pins.len());
    }

    if let Some(path) = fixture_path {
        let _ = std::fs::remove_file(path);
    }

    println!("parse example: ok");
    Ok(())
}
