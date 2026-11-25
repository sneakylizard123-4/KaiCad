//! KaiCad Studio desktop binary.

use std::path::PathBuf;
use std::process::ExitCode;

use kaicad_gui::StudioApp;

const USAGE: &str = "kai-cad-gui [board.kicad_pcb|board.json]";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            eprintln!("usage: {USAGE}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut initial_path = None;
    for argument in std::env::args_os().skip(1) {
        let text = argument.to_string_lossy().to_string();
        match text.as_str() {
            "-h" | "--help" => {
                print_help();
                return Ok(());
            }
            "-V" | "--version" => {
                println!("kai-cad-gui {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            flag if flag.starts_with('-') => {
                return Err(format!("unrecognized option: {flag}"));
            }
            _ => {
                if initial_path.is_some() {
                    return Err("only one board path may be supplied".to_string());
                }
                initial_path = Some(PathBuf::from(argument));
            }
        }
    }

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("KaiCad Studio")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([960.0, 600.0]),
        renderer: eframe::Renderer::Glow,
        centered: true,
        ..eframe::NativeOptions::default()
    };

    eframe::run_native(
        "KaiCad Studio",
        options,
        Box::new(move |cc| Ok(Box::new(StudioApp::new(cc, initial_path)))),
    )
    .map_err(|error| format!("failed to start KaiCad Studio: {error}"))
}

fn print_help() {
    println!("KaiCad Studio — native PCB editor and inspection shell (pre-alpha).");
    println!();
    println!("usage:");
    println!("  {USAGE}");
    println!();
    println!("  -h, --help     print this help and exit");
    println!("  -V, --version  print the version and exit");
    println!();
    println!("With no board path, KaiCad Studio opens a built-in demo board.");
    println!("Boards open and save as .kicad_pcb natively; .json remains supported.");
    println!("Use the Route tool in the 2D view to add and undo tracks.");
    println!("The viewport uses the deterministic software renderer; no Vulkan runtime is needed.");
}
