//! Strata command-line entrypoint.

fn main() {
    std::process::exit(strata_cli_next::run(std::env::args_os()));
}
