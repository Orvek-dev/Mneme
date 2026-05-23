//! Command-line entry point for the Mneme eval harness.

use mneme_core::{BuildStage, PRODUCT_NAME};

fn main() {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        None | Some("doctor") => {
            println!(
                "{PRODUCT_NAME} eval harness: {}",
                BuildStage::Bootstrap.as_str()
            );
        }
        Some("--version" | "version") => {
            println!("{}", env!("CARGO_PKG_VERSION"));
        }
        Some(command) => {
            eprintln!("unknown mneme-eval command: {command}");
            eprintln!("available commands: doctor, version");
            std::process::exit(2);
        }
    }
}
