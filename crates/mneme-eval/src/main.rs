//! Command-line entry point for the Mneme eval harness.

use std::io::Write;

fn main() {
    if let Err(error) = mneme_eval::run_cli(std::env::args()) {
        let _ = std::io::stdout().flush();
        eprintln!("{error}");
        std::process::exit(error.exit_code());
    }
}
