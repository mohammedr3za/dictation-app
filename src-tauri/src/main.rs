// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Headless mode for validating the ASR pipeline without the UI.
    if let Some(index) = args.iter().position(|a| a == "--transcribe-file") {
        if let Some(path) = args.get(index + 1) {
            let model_dir = args
                .iter()
                .position(|a| a == "--model-dir")
                .and_then(|i| args.get(i + 1))
                .map(std::path::PathBuf::from)
                .unwrap_or_else(dictation_app_lib::asr::default_model_dir);

            match dictation_app_lib::asr::transcribe_path(std::path::Path::new(path), &model_dir) {
                Ok(text) => {
                    println!("{text}");
                    std::process::exit(0);
                }
                Err(error) => {
                    eprintln!("error: {error:#}");
                    std::process::exit(1);
                }
            }
        }
    }

    dictation_app_lib::run()
}
