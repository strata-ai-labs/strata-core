//! IDL-driven command discovery binary for Strata V1.

fn main() {
    let output = strata_cli_next::run_args(std::env::args_os());
    if !output.stdout.is_empty() {
        print!("{}", output.stdout);
    }
    if !output.stderr.is_empty() {
        eprint!("{}", output.stderr);
    }
    std::process::exit(output.exit_code);
}
