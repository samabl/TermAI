//! termai CLI (headless).
//!
//! Subcommands are deliberately small and honest: each one either does real work or
//! reports that it is not wired yet. No command ever pretends to have succeeded.

mod cli;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(cli::run(&args));
}
