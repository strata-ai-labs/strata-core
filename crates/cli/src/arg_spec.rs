//! CLI argument spec (#3073): clap is the authority on how each verb spells its
//! arguments, but the docs example renderer lives in strata-executor, which
//! cannot see the clap tree. This module derives an authoritative, committed
//! `cli-arg-spec.json` from clap — per verb, the positional wire fields in
//! order and the wire-field → long-flag map — so the renderer can spell CLI
//! examples correctly (and fall back to `command run` for any wire field the
//! verb does not expose). A guard test keeps the committed spec in lockstep
//! with the live clap tree; a second `--ignored` test regenerates it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// How one verb spells its arguments on the command line.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct VerbArgSpec {
    /// Positional wire fields, in clap order.
    pub positionals: Vec<String>,
    /// Non-positional wire field → long flag name (without `--`).
    pub flags: BTreeMap<String, String>,
}

/// The full CLI argument spec: verb path (e.g. `"graph add-node"`) → its args.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct CliArgSpec {
    pub verbs: BTreeMap<String, VerbArgSpec>,
}

/// Builds the argument spec from the live clap tree.
pub(crate) fn build_from_clap() -> CliArgSpec {
    use clap::CommandFactory;

    let mut verbs = BTreeMap::new();
    collect(&crate::options::Cli::command(), &[], &mut verbs);
    CliArgSpec { verbs }
}

/// Walks to each leaf verb and records its argument spelling.
fn collect(command: &clap::Command, prefix: &[String], out: &mut BTreeMap<String, VerbArgSpec>) {
    let mut subs = command
        .get_subcommands()
        .filter(|sub| !sub.is_hide_set() && sub.get_name() != "help")
        .peekable();
    if subs.peek().is_none() {
        if !prefix.is_empty() {
            out.insert(prefix.join(" "), verb_arg_spec(command));
        }
        return;
    }
    for sub in subs {
        let mut path = prefix.to_vec();
        path.push(sub.get_name().to_owned());
        collect(sub, &path, out);
    }
}

/// Extracts one leaf command's positional order and flag names, skipping the
/// propagated globals (`--raw`/`--json`/`--format`/…) and clap's built-in help.
fn verb_arg_spec(command: &clap::Command) -> VerbArgSpec {
    let mut positionals: Vec<(usize, String)> = Vec::new();
    let mut flags = BTreeMap::new();
    for arg in command.get_arguments() {
        if arg.is_global_set() || arg.get_id() == "help" {
            continue;
        }
        let field = arg.get_id().as_str().to_owned();
        if arg.is_positional() {
            positionals.push((arg.get_index().unwrap_or(usize::MAX), field));
        } else if let Some(long) = arg.get_long() {
            flags.insert(field, long.to_owned());
        }
    }
    positionals.sort_by_key(|(index, _)| *index);
    VerbArgSpec {
        positionals: positionals.into_iter().map(|(_, field)| field).collect(),
        flags,
    }
}

/// The committed spec path under the executor's generated IDL directory, so the
/// docs generator (and the release bundle) can consume it.
pub(crate) fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../executor/idl/v1/generated/cli-arg-spec.json")
}

/// Serializes the spec with stable, diff-friendly formatting.
pub(crate) fn to_json(spec: &CliArgSpec) -> String {
    let mut json = serde_json::to_string_pretty(spec).expect("arg spec serializes");
    json.push('\n');
    json
}

#[cfg(test)]
mod tests {
    use super::*;

    fn committed() -> CliArgSpec {
        let text = std::fs::read_to_string(spec_path()).expect("cli-arg-spec.json exists");
        serde_json::from_str(&text).expect("cli-arg-spec.json parses")
    }

    #[test]
    fn cli_arg_spec_matches_the_clap_tree() {
        let live = build_from_clap();
        let committed = committed();
        assert_eq!(
            committed, live,
            "cli-arg-spec.json is stale; regenerate with \
             `cargo test -p strata-cli --lib arg_spec -- --ignored regenerate`"
        );
    }

    #[test]
    #[ignore = "regenerates the committed cli-arg-spec.json; run explicitly"]
    fn regenerate() {
        std::fs::write(spec_path(), to_json(&build_from_clap())).expect("write cli-arg-spec.json");
    }

    /// Collects every `.md` file's text under `dir`, recursively.
    fn markdown_files(dir: &Path, out: &mut Vec<(PathBuf, String)>) {
        for entry in std::fs::read_dir(dir).expect("read docs dir").flatten() {
            let path = entry.path();
            if path.is_dir() {
                markdown_files(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "md") {
                out.push((
                    path.clone(),
                    std::fs::read_to_string(&path).expect("read md"),
                ));
            }
        }
    }

    #[test]
    fn every_rendered_cli_example_parses_against_clap() {
        use clap::Parser as _;

        let docs_dir = spec_path().parent().expect("generated dir").join("docs");
        let mut files = Vec::new();
        markdown_files(&docs_dir, &mut files);
        assert!(!files.is_empty(), "found generated docs to check");

        let mut checked = 0usize;
        for (path, content) in &files {
            // The `### CLI` block is the only place a rendered example line
            // begins with `$ strata`; `shlex` splits shell-faithfully and drops
            // the trailing `  # note` comment for us.
            for command in content
                .lines()
                .filter_map(|line| line.strip_prefix("$ "))
                .filter(|line| line.starts_with("strata "))
            {
                let tokens = shlex::split(command).unwrap_or_else(|| {
                    panic!("un-splittable example in {}: {command}", path.display())
                });
                crate::options::Cli::try_parse_from(&tokens).unwrap_or_else(|error| {
                    panic!(
                        "rendered CLI example does not parse against clap in {}:\n  {command}\n{error}",
                        path.display(),
                    )
                });
                checked += 1;
            }
        }
        assert!(checked > 0, "expected rendered CLI examples to check");
    }
}
