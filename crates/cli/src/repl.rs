//! Interactive and piped command execution.

use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use clap::{CommandFactory, Parser};
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use serde_json::json;
use strata_executor::ipc::Connection;
use strata_executor::{Command, Output};

use crate::context::CommandContext;
use crate::options::{Cli, Format};
use crate::render::render_value;
use crate::{execute_parsed_command, CliError};

pub(crate) fn run_repl(
    connection: &Connection,
    context: &mut CommandContext,
    format: Format,
) -> Result<(), CliError> {
    let mut editor = DefaultEditor::new().map_err(|error| CliError::usage(error.to_string()))?;
    let history = history_path();
    if let Some(path) = history.as_ref() {
        let _ = editor.load_history(path);
    }

    loop {
        match editor.readline(&context.prompt(&connection.default_branch())) {
            Ok(line) => {
                let _ = editor.add_history_entry(line.as_str());
                if handle_line(connection, context, &line, format)? == LineOutcome::Exit {
                    break;
                }
            }
            Err(ReadlineError::Interrupted) => {}
            Err(ReadlineError::Eof) => break,
            Err(error) => return Err(CliError::usage(error.to_string())),
        }
    }

    if let Some(path) = history.as_ref() {
        let _ = editor.save_history(path);
    }
    Ok(())
}

pub(crate) fn run_pipe(
    connection: &Connection,
    context: &mut CommandContext,
    format: Format,
) -> Result<bool, CliError> {
    let mut saw_error = false;
    for line in io::stdin().lock().lines() {
        let line = line?;
        match handle_line(connection, context, &line, format) {
            Ok(LineOutcome::Continue | LineOutcome::Exit) => {}
            Err(error) => {
                saw_error = true;
                eprintln!("error: {error}");
            }
        }
    }
    Ok(saw_error)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LineOutcome {
    Continue,
    Exit,
}

fn handle_line(
    connection: &Connection,
    context: &mut CommandContext,
    line: &str,
    format: Format,
) -> Result<LineOutcome, CliError> {
    match parse_line(line)? {
        ParsedLine::Empty => Ok(LineOutcome::Continue),
        ParsedLine::Exit => Ok(LineOutcome::Exit),
        ParsedLine::Clear => {
            print!("\x1b[2J\x1b[H");
            let _ = io::stdout().flush();
            Ok(LineOutcome::Continue)
        }
        ParsedLine::Help => {
            Cli::command().print_long_help().map_err(CliError::from)?;
            println!();
            Ok(LineOutcome::Continue)
        }
        ParsedLine::Use { branch, space } => {
            validate_context(connection, &branch, space.as_deref())?;
            context.set_branch(branch.clone());
            context.set_space(space.clone());
            render_value(
                &json!({
                    "type": "context",
                    "data": {
                        "branch": branch,
                        "space": space.unwrap_or_else(|| context.space_or_default().to_owned())
                    }
                }),
                format,
            )?;
            Ok(LineOutcome::Continue)
        }
        ParsedLine::Command(cli) => {
            let scope = context.scope_with_overrides(cli.branch, cli.space);
            let Some(command) = cli.command else {
                return Ok(LineOutcome::Continue);
            };
            execute_parsed_command(connection, command, &scope, format)?;
            Ok(LineOutcome::Continue)
        }
    }
}

fn parse_line(line: &str) -> Result<ParsedLine, CliError> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return Ok(ParsedLine::Empty);
    }

    let words = shlex::split(trimmed)
        .ok_or_else(|| CliError::usage("could not parse command line quoting"))?;
    if words.is_empty() {
        return Ok(ParsedLine::Empty);
    }

    match words[0].as_str() {
        "quit" | "exit" => return Ok(ParsedLine::Exit),
        "clear" => return Ok(ParsedLine::Clear),
        "help" if words.len() == 1 => return Ok(ParsedLine::Help),
        "use" => return parse_use(&words),
        _ => {}
    }

    let mut argv = Vec::with_capacity(words.len() + 1);
    argv.push("strata".to_owned());
    argv.extend(words);
    Cli::try_parse_from(argv)
        .map(|cli| ParsedLine::Command(Box::new(cli)))
        .map_err(|error| CliError::usage(error.to_string()))
}

fn parse_use(words: &[String]) -> Result<ParsedLine, CliError> {
    match words {
        [_, branch_space] => {
            if let Some((branch, space)) = branch_space.split_once('/') {
                if branch.is_empty() || space.is_empty() {
                    return Err(CliError::usage("usage: use <branch>/<space>"));
                }
                Ok(ParsedLine::Use {
                    branch: branch.to_owned(),
                    space: Some(space.to_owned()),
                })
            } else {
                Ok(ParsedLine::Use {
                    branch: branch_space.clone(),
                    space: None,
                })
            }
        }
        [_, branch, space] => Ok(ParsedLine::Use {
            branch: branch.clone(),
            space: Some(space.clone()),
        }),
        _ => Err(CliError::usage("usage: use <branch> [space]")),
    }
}

fn validate_context(
    connection: &Connection,
    branch: &str,
    space: Option<&str>,
) -> Result<(), CliError> {
    let _ = connection.execute(Command::BranchGet {
        branch: branch.to_owned(),
    })?;
    if let Some(space) = space {
        let output = connection.execute(Command::SpaceExists {
            branch: Some(branch.to_owned()),
            space: space.to_owned(),
        })?;
        match output {
            Output::Bool(true) => {}
            Output::Bool(false) => {
                return Err(CliError::usage(format!(
                    "space `{space}` does not exist on branch `{branch}`"
                )));
            }
            _ => {
                return Err(CliError::usage(
                    "space existence check returned an unexpected output",
                ))
            }
        }
    }
    Ok(())
}

fn history_path() -> Option<PathBuf> {
    std::env::var_os("STRATA_HISTORY")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".strata_history"))
        })
}

enum ParsedLine {
    Empty,
    Exit,
    Clear,
    Help,
    Use {
        branch: String,
        space: Option<String>,
    },
    Command(Box<Cli>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_use_branch_and_space() {
        let ParsedLine::Use { branch, space } = parse_line("use main docs").expect("parse") else {
            panic!("expected use command");
        };
        assert_eq!(branch, "main");
        assert_eq!(space.as_deref(), Some("docs"));
    }

    #[test]
    fn parses_repl_command() {
        let ParsedLine::Command(cli) = parse_line("kv put a b").expect("parse") else {
            panic!("expected executor command");
        };
        assert!(cli.command.is_some());
    }

    #[test]
    fn validate_context_accepts_an_existing_branch_and_rejects_a_missing_space() {
        let connection =
            Connection::cache(strata_executor::Executor::open_cache().expect("cache opens"));
        // The store default branch exists, with no space override.
        validate_context(&connection, strata_executor::DEFAULT_BRANCH, None)
            .expect("the default branch is a valid `use` target");
        // A space that was never created on that branch is refused.
        validate_context(
            &connection,
            strata_executor::DEFAULT_BRANCH,
            Some("never-created"),
        )
        .expect_err("a missing space must be rejected");
    }
}
