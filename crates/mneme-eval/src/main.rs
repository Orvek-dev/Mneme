//! Command-line entry point for the Mneme eval harness.

fn main() {
    if let Err(error) = mneme_eval::run_cli(std::env::args()) {
        eprintln!("{error}");
        std::process::exit(error.exit_code());
    }
}
