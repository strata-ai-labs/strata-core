//! Strata command-line entrypoint.

fn main() {
    std::process::exit(strata_cli::run(std::env::args_os()));
}
