//! Command-line entry point for the internal IDL resolver.

#![deny(unsafe_code)]

use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    let Some(command) = env::args().nth(1) else {
        eprintln!(
            "usage: cargo run -p strata-executor-next --features idl-tooling --bin strata-idl -- <generate|check>"
        );
        return ExitCode::from(2);
    };

    let root = strata_executor_next::idl_tooling::default_repo_root();
    let result = match command.as_str() {
        "generate" => strata_executor_next::idl_tooling::generate(&root),
        "check" => strata_executor_next::idl_tooling::check(&root),
        _ => {
            eprintln!("unknown command `{command}`; expected generate or check");
            return ExitCode::from(2);
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}
