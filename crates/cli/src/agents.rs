//! The self-describing agent surface (first-run D3).
//!
//! `strata agents` exposes the machine-readable knowledge the binary already
//! carries — the command catalog, the error registry, and a complete offline
//! usage guide — so an agent that lands anywhere on the surface is one hop
//! from the whole map, without web searches and version-locked to the
//! installed binary. `agents init` plants that pointer inside a repo so every
//! future agent session there starts oriented.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use serde_json::{json, Value};
use strata_executor::cli_metadata::CliCommandCatalog;
use strata_executor::error_registry::public_error_code_entries;

use crate::options::{AgentsCommand, Format};
use crate::render::render_value;
use crate::{guidance, CliError};

const POINTER_MARKER: &str = "## Strata";

/// Runs one agents command and returns the process exit code.
pub(crate) fn run(command: &AgentsCommand, format: Format) -> Result<i32, CliError> {
    match command {
        AgentsCommand::Guide => {
            // The guide is markdown; it is the format.
            println!("{}", guide_markdown()?);
        }
        AgentsCommand::Commands => render_value(&commands_value()?, format)?,
        AgentsCommand::Errors => render_value(&errors_value(), format)?,
        AgentsCommand::Init { apply } => render_value(&run_repo_init(*apply)?, format)?,
    }
    Ok(0)
}

// ---- guide -----------------------------------------------------------------

pub(crate) fn guide_markdown() -> Result<String, CliError> {
    let catalog = catalog()?;
    let mut guide = String::new();
    let version = env!("CARGO_PKG_VERSION");

    // write!/writeln! into a String are infallible; results ignored throughout.
    let _ = write!(
        guide,
        "\
# Strata {version} — agent usage guide

Strata is an embedded multi-model database: one binary, one file-backed
database, no server. Six primitives share one storage substrate with
branches and time travel: key-value, JSON documents, vectors, an event log,
graphs, and product spaces. This guide is generated from the installed
binary's own metadata, so it cannot drift from what `strata {version}` does.

## Targeting a database

1. Explicit path or `--db <path>` — always wins: `strata ./my-db kv get k`
2. `STRATA_DB=<path>` — set once per session, used when no path is passed
3. `--cache` — explicit in-memory database (nothing persisted)

One-shot commands with no target refuse with
`invalid_argument.cli.no_database` — Strata never opens the current
directory implicitly. `strata <path>` with no command opens a REPL.

## Quickstart

```
{next_steps}
```

Branches fork cheaply and isolate writes; every primitive is branch-aware:

```
strata ./my-db branch fork default experiment
strata ./my-db kv put city tokyo --branch experiment
strata ./my-db kv get city --branch experiment   # tokyo
strata ./my-db kv get city                       # unchanged on default
```

Time travel: every write receipt carries a commit `timestamp`; pass it back
with `--as-of` on reads (kv/json/vector/event/graph alike):

```
strata --json ./my-db kv put k v1        # note data.commit.timestamp
strata ./my-db kv put k v2
strata ./my-db kv get k --as-of <t1>     # v1
```

## Output contract

- `--json`: one compact envelope per command — `{{\"type\": ..., \"data\": ...}}`.
  KV keys/values and cursors are base64 strings on the wire.
- `--raw`: script-friendly bare values.
- default: human-readable; binary values decode to text when valid UTF-8.
- Continuation cursors are opaque base64 tokens: pass the printed cursor
  back verbatim via `--cursor`.
- Raw serialized commands (the programmatic path):
  `strata <db> command run --command-json '{{\"type\":\"kv_get\",\"key\":\"a2V5\"}}'`

## Errors teach

Failures carry a stable code (`<class>.<area>.<detail>`), a one-line hint,
and a per-code ref (`https://stratadb.org/e/<code>`). `--json` failures emit
the full envelope on stderr. Recover by code and class, never by message
text. Full registry: `strata agents errors --json`.

## Diagnostics

`strata doctor [--json] [path]` checks the installation and (optionally) a
database, reporting coded issues with hints; it exits non-zero when
anything needs attention.

## MCP

`strata <db> mcp serve` speaks Model Context Protocol over stdio — ~20
curated tools plus `strata_guide` (this guide) and `strata_command` (any
cataloged command as raw wire JSON). Same envelopes, same error codes.
Client config: {{\"command\":\"strata\",\"args\":[\"<db-path>\",\"mcp\",\"serve\"]}}.

",
        version = version,
        next_steps = guidance::NEXT_STEPS.join("\n"),
    );

    guide.push_str("## Command catalog\n\n");
    let _ = writeln!(
        guide,
        "{} commands carry full metadata today (catalog JSON: `strata agents \
commands --json`); every command documents itself via `--help`.\n",
        catalog.commands().len()
    );
    for family in catalog.families() {
        let _ = writeln!(guide, "### {}", family.id);
        guide.push('\n');
        for id in &family.commands {
            if let Some(entry) = catalog.command_by_id(id) {
                let _ = writeln!(guide, "- `{}` — {}", entry.path_display, entry.summary);
            }
        }
        guide.push('\n');
    }

    guide.push_str(
        "## Repo onboarding\n\n`strata agents init` writes `.strata/AGENTS.md` into the current \
repo; `--apply` also appends a short pointer block to the repo's `AGENTS.md`/`CLAUDE.md` so every \
future agent session here starts oriented.\n",
    );

    Ok(guide)
}

// ---- catalogs ---------------------------------------------------------------

fn catalog() -> Result<CliCommandCatalog, CliError> {
    CliCommandCatalog::embedded()
        .map_err(|error| CliError::usage(format!("embedded command catalog is invalid: {error}")))
}

fn commands_value() -> Result<Value, CliError> {
    let catalog = catalog()?;
    let index = serde_json::to_value(catalog.index())?;
    Ok(json!({
        "type": "agents_commands",
        "data": index,
    }))
}

fn errors_value() -> Value {
    let errors = public_error_code_entries()
        .map(|entry| {
            json!({
                "code": entry.code,
                "class": entry.class,
                "retry_policy": entry.retry_policy,
                "commit_outcome": entry.commit_outcome,
                "message": entry.message_template,
                "hint": entry.suggested_fix,
                "ref": format!("https://stratadb.org/e/{}", entry.docs_slug),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "type": "agents_errors",
        "data": {
            "count": errors.len(),
            "errors": errors,
        }
    })
}

// ---- repo onboarding --------------------------------------------------------

/// The ~10-line pointer block planted in repos: a pointer, not a manual.
fn pointer_block() -> String {
    format!(
        "\
{POINTER_MARKER}

This repo uses Strata (embedded database — SQLite-shaped, zero-config).

- Full usage guide: run `strata agents guide` (offline, version-matched)
- Command catalog: `strata agents commands --json`; errors: `strata agents errors --json`
- Database targeting: pass a path or set `STRATA_DB`; never rely on cwd
- Structured output: add `--json` to any command; raw commands via `strata <db> command run --command-json`
- MCP: `strata <db> mcp serve` (stdio; same envelopes and error codes)
"
    )
}

fn run_repo_init(apply: bool) -> Result<Value, CliError> {
    fs::create_dir_all(".strata")?;
    fs::write(Path::new(".strata/AGENTS.md"), pointer_block())?;

    let pointer_target = ["AGENTS.md", "CLAUDE.md"]
        .into_iter()
        .find(|candidate| Path::new(candidate).exists());

    // absent: the repo has no agent instructions file to point from.
    // present: the pointer block is already planted (idempotent re-runs).
    // appended: --apply added the block just now.
    // pending: a target exists; re-run with --apply to plant the pointer.
    let mut pointer_state = "absent";
    if let Some(target) = pointer_target {
        let existing = fs::read_to_string(target)?;
        if existing.contains(POINTER_MARKER) {
            pointer_state = "present";
        } else if apply {
            let mut updated = existing;
            if !updated.ends_with('\n') {
                updated.push('\n');
            }
            updated.push('\n');
            updated.push_str(&pointer_block());
            fs::write(target, updated)?;
            pointer_state = "appended";
        } else {
            pointer_state = "pending";
        }
    }

    Ok(json!({
        "type": "agents_init",
        "data": {
            "written": [".strata/AGENTS.md"],
            "pointer_target": pointer_target,
            "pointer": pointer_state,
            "next": if pointer_state == "pending" {
                Value::String(
                    "run `strata agents init --apply` to append the pointer block".to_owned(),
                )
            } else {
                Value::Null
            },
        }
    }))
}
