use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();

    if let Some(cfg) = browser_renderer::parse_renderer_args(&args) {
        if let Err(err) = browser_renderer::run_renderer_main(cfg) {
            eprintln!("[renderer] fatal error: {err}");
            return ExitCode::FAILURE;
        }
        return ExitCode::SUCCESS;
    }

    if let Err(err) = browser_shell::win32_app::run_browser_window() {
        eprintln!("[browser] fatal error: {err}");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}
