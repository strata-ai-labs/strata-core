//! Command-line entry point for the internal IDL resolver.

#![deny(unsafe_code)]

use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    let Some(command) = env::args().nth(1) else {
        eprintln!(
            "usage: cargo run -p strata-executor --features idl-tooling --bin strata-idl -- <generate|check|generate-cli|check-cli|verify-fixtures [--update]>"
        );
        return ExitCode::from(2);
    };

    let root = strata_executor::idl_tooling::default_repo_root();
    let result = match command.as_str() {
        "generate" => strata_executor::idl_tooling::generate(&root),
        "check" => strata_executor::idl_tooling::check(&root),
        "generate-cli" => strata_executor::idl_tooling::generate_cli(&root),
        "check-cli" => strata_executor::idl_tooling::check_cli(&root),
        "verify-fixtures" => {
            let update = env::args().nth(2).is_some_and(|flag| flag == "--update");
            strata_executor::idl_tooling::verify_fixtures(&root, update).map(|blessed| {
                for path in blessed {
                    eprintln!("blessed {}", path.display());
                }
            })
        }
        _ => {
            eprintln!(
                "unknown command `{command}`; expected generate, check, generate-cli, check-cli, or verify-fixtures"
            );
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
