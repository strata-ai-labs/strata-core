//! Drift guard #2 (v1-idl-overlay-strategy.md): the handwritten clap tree
//! and the generated CLI command catalog are reconciled by test, not by
//! discipline. Every catalog entry with `surface: verb` must resolve to a
//! real clap verb, and every visible clap leaf must map back to a catalog
//! entry or appear on the shrink-only allowlist
//! (`crates/executor/idl/v1/uncovered-cli-verbs.yaml`).

use std::collections::BTreeSet;
use std::path::Path;

use clap::CommandFactory;

use crate::options::Cli;

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct UncoveredCliVerbs {
    uncovered: Vec<String>,
}

fn idl_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../executor/idl/v1")
}

/// Collects visible leaf verb paths from the clap tree. Hidden commands
/// (the deferred old-CLI refusals) and clap's built-in `help` are skipped;
/// aliases are not separate leaves.
fn clap_leaves(command: &clap::Command, prefix: &[String], out: &mut BTreeSet<String>) {
    let mut subcommands = command
        .get_subcommands()
        .filter(|sub| !sub.is_hide_set() && sub.get_name() != "help")
        .peekable();
    if subcommands.peek().is_none() {
        if !prefix.is_empty() {
            out.insert(prefix.join(" "));
        }
        return;
    }
    for sub in subcommands {
        let mut path = prefix.to_vec();
        path.push(sub.get_name().to_owned());
        clap_leaves(sub, &path, out);
    }
}

#[test]
fn clap_tree_round_trips_against_the_generated_catalog() {
    let catalog = strata_executor::cli_metadata::CliCommandCatalog::embedded()
        .expect("embedded catalog parses");

    let mut leaves = BTreeSet::new();
    clap_leaves(&Cli::command(), &[], &mut leaves);
    assert!(!leaves.is_empty(), "clap tree yields leaves");

    let allowlist: UncoveredCliVerbs = serde_yaml::from_str(
        &std::fs::read_to_string(idl_root().join("uncovered-cli-verbs.yaml"))
            .expect("uncovered-cli-verbs.yaml exists"),
    )
    .expect("allowlist parses");
    let allowlisted: BTreeSet<&str> = allowlist.uncovered.iter().map(String::as_str).collect();
    assert_eq!(
        allowlisted.len(),
        allowlist.uncovered.len(),
        "allowlist entries are unique"
    );

    // Direction 1: every catalog verb resolves to a real clap leaf.
    let mut catalog_paths = BTreeSet::new();
    for entry in catalog.commands() {
        if entry.surface == "verb" {
            assert!(
                leaves.contains(&entry.path_display),
                "catalog says `{}` is a CLI verb (`{}`) but the clap tree has no such leaf",
                entry.id,
                entry.path_display,
            );
            catalog_paths.insert(entry.path_display.clone());
        }
    }

    // Direction 2: every clap leaf maps back to the catalog or the
    // shrink-only allowlist — never both, never neither.
    for leaf in &leaves {
        let covered = catalog_paths.contains(leaf);
        let listed = allowlisted.contains(leaf.as_str());
        assert!(
            covered || listed,
            "clap verb `{leaf}` has no catalog entry and is not in uncovered-cli-verbs.yaml; \
             author IDL coverage or list it explicitly"
        );
        assert!(
            !(covered && listed),
            "clap verb `{leaf}` is covered by the catalog; remove it from \
             uncovered-cli-verbs.yaml (the allowlist may only shrink)"
        );
    }

    // Allowlist hygiene: every listed verb must still exist in the tree.
    for verb in &allowlisted {
        assert!(
            leaves.contains(*verb),
            "uncovered-cli-verbs.yaml lists `{verb}` which is not a clap leaf; delete the stale entry"
        );
    }
}

/// Drift guard (#3058): the shipped `command-index.json` must not advertise a
/// `cli.path` for a command the CLI cannot resolve. A `surface: "wire"` command
/// is escape-hatch only (`command run --command-json`) and must carry NO
/// runnable path, so a consumer that renders `cli.path` as the invocation never
/// prints a subcommand that does not exist.
#[test]
fn no_command_index_entry_advertises_an_unresolvable_cli_path() {
    let mut leaves = BTreeSet::new();
    clap_leaves(&Cli::command(), &[], &mut leaves);
    assert!(!leaves.is_empty(), "clap tree yields leaves");

    let index: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(idl_root().join("generated/command-index.json"))
            .expect("command-index.json exists"),
    )
    .expect("command-index.json parses");
    let commands = index["commands"]
        .as_array()
        .expect("command-index has a commands array");

    let mut verbs_seen = 0usize;
    for command in commands {
        let id = command["id"].as_str().unwrap_or("<unknown>");
        let cli = &command["cli"];
        let path = cli.get("path").and_then(serde_json::Value::as_array);
        let surface = cli.get("surface").and_then(serde_json::Value::as_str);

        if surface == Some("verb") {
            // A verb MUST advertise a runnable path that resolves to clap.
            let path = path
                .unwrap_or_else(|| panic!("command `{id}` is a `verb` but advertises no cli.path"));
            let display = path
                .iter()
                .filter_map(serde_json::Value::as_str)
                .collect::<Vec<_>>()
                .join(" ");
            assert!(
                leaves.contains(&display),
                "command `{id}` advertises cli.path `{display}` but the clap tree has no such verb",
            );
            verbs_seen += 1;
        } else {
            // A `wire` (escape-hatch) command MUST NOT advertise a path, so a
            // consumer rendering `cli.path` never prints an unrunnable command.
            assert!(
                path.is_none(),
                "command `{id}` is surface `{surface:?}` but advertises a cli.path; \
                 omit the path for non-verb commands",
            );
        }
    }
    assert!(
        verbs_seen > 0,
        "expected at least one verb command in the index",
    );
}
