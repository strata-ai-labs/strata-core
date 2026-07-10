# CLI-Next User Experience Parity Implementation Plan

Status: Draft implementation plan

Related documents:

- [cli-next-implementation-plan.md](cli-next-implementation-plan.md)
- [cli-next-test-plan.md](cli-next-test-plan.md)
- [old-executor-to-v1-gap-analysis.md](../old-executor-to-v1-gap-analysis.md)

## Objective

Bring the handwritten `cli-next` crate to user-experience parity with the old
Strata CLI where the V1 executor already has the required capability.

Parity here means shell behavior, ergonomics, rendering, and discoverability.
It does not mean restoring every old executor command. Commands that no longer
exist in `executor-next` should be recorded, documented, and either omitted from
active help or surfaced with a precise "not available in V1 yet" error.

The current `strata` binary already opens cache and durable databases and maps a
large typed command surface into `executor-next`. The missing user-experience
pieces are the old CLI shell mechanics: optional one-shot command mode, REPL,
pipe mode, human/raw rendering, ergonomic aliases, file inputs, contextual
branch/space selection, and clear treatment of deferred commands.

## Inventory Source

Use git history as the UX reference:

- old app loop: `cb01f0dd:crates/cli/src/app.rs`
- old open semantics: `cb01f0dd:crates/cli/src/open.rs`
- old context and prompt: `cb01f0dd:crates/cli/src/context.rs`
- old REPL and pipe mode: `cb01f0dd:crates/cli/src/repl.rs`
- old rendering modes: `cb01f0dd:crates/cli/src/render.rs`
- old parser surface: `cb01f0dd:crates/cli/src/parse.rs`

The active implementation target is:

- `crates/cli-next/src/main.rs`
- `crates/cli-next/src/lib.rs`
- `crates/cli-next/src/options.rs`
- `crates/executor-next/src/command.rs`

## Non-Goals

Do not use this plan to rebuild missing engine or executor features.

Deferred or intentionally excluded:

- git-style branch diff, merge, tags, notes, and branch-version UX;
- graph ontology and graph analytics;
- search, recipes, and auto-embedding pipelines;
- public transaction/session commands;
- public storage maintenance controls such as flush and compact;
- old daemon lifecycle commands unless a V1 daemon/server contract exists;
- generated CLI work from IDL overlays.

The CLI can preserve discoverability for these areas, but it should not fake
successful execution.

## Target User Experience

The V1 CLI should support these flows:

```sh
strata
strata ./my-db
strata --db ./my-db kv put user Claude
strata ./my-db kv get user --raw
strata --cache
printf 'kv put a b\nkv get a\n' | strata --cache
```

Expected behavior:

- With a one-shot command, open the selected database, execute one command,
  render one response, close the executor, and return an appropriate exit code.
- Without a one-shot command and with an interactive terminal, open the selected
  database and enter the REPL.
- Without a one-shot command and with piped stdin, execute lines in pipe mode.
- `strata ./my-db` should be the natural durable open path.
- `strata` from inside a database directory should open that directory.
- `strata --cache` should open an in-memory executor for the process.
- Branch and space should be carried in a session context and reflected in the
  prompt.
- JSON mode should expose exact executor responses.
- Human mode should be concise and stable.
- Raw mode should be script-friendly.

## Database Path Semantics

The current `cli-next` defaults the durable database path to the current
directory. The old CLI defaulted to a `.strata` child path. V1 should resolve
this explicitly:

1. If `--db <path>` is provided, open `<path>`.
2. If a positional database path is provided, open that path.
3. If neither is provided and the current directory is a Strata database root,
   open the current directory.
4. If neither is provided and the current directory contains a legacy `.strata`
   database, open `.strata` only as compatibility behavior.
5. If neither is provided and no database exists, print creation/open guidance
   instead of silently creating durable state.
6. `--cache` must remain incompatible with `--db` and positional paths.

This keeps the product flow of `strata ./my-db` and `cd ./my-db && strata`
without accidentally hiding persistent database creation.

## Slice 1: App Shell And Optional Command Mode

Split the current single-file execution loop into small modules:

- `app.rs`: top-level run loop and exit-code policy;
- `open.rs`: cache/durable path resolution;
- `context.rs`: branch, space, database label, and prompt state;
- `parse.rs`: CLI parse helpers and REPL line parsing;
- `render.rs`: output and error renderers;
- `repl.rs`: interactive and pipe loops.

Implementation steps:

1. Make the top-level command optional.
2. Preserve `strata --db <path> <command>` and `strata <path> <command>`.
3. Route no-command terminal execution into REPL mode.
4. Route no-command non-terminal stdin into pipe mode.
5. Keep local no-database actions, such as raw command printing, outside the
   executor open path.
6. Centralize exit-code policy:
   - `0` for success;
   - `1` for executor/runtime failures;
   - `2` for CLI usage/parse failures;
   - nonzero pipe exit when any line fails.

Exit criteria:

1. `strata --db ./db ping` still works.
2. `strata ./db` enters a REPL when stdin is a terminal.
3. `printf 'ping\n' | strata ./db` runs pipe mode.
4. Missing database paths produce guidance, not silent durable creation.

## Slice 2: REPL And Pipe Mode

Port the old shell mechanics without the old executor session model.

Implementation steps:

1. Add `rustyline` and `shlex` dependencies.
2. Store command history in `~/.strata_history`.
3. Render prompts as `strata:{branch}/{space}>`.
4. Add local REPL commands:
   - `help [command]`;
   - `use <branch>`;
   - `use <branch>/<space>`;
   - `use <branch> <space>`;
   - `clear`;
   - `quit`;
   - `exit`.
5. Validate `use <branch>` through `BranchGet` or equivalent executor command
   before updating context.
6. Validate `use <branch>/<space>` through branch and space existence checks
   before updating context.
7. In pipe mode, skip blank lines and comment lines beginning with `#`.
8. In pipe mode, continue after per-line failures and return nonzero after the
   pipe finishes.

Exit criteria:

1. REPL and one-shot execution share the same command parser.
2. Command failures do not terminate the REPL.
3. Pipe mode can execute a migration-style command file.
4. Context changes affect subsequent REPL and pipe commands.

## Slice 3: Rendering Parity

The current CLI only supports compact and pretty JSON. Restore the old
three-mode rendering model:

- default human;
- `--json`;
- `--raw`.

Implementation steps:

1. Replace or alias `--format pretty|json` with user-facing `--json` and
   `--raw`.
2. Keep a hidden or transitional `--format` only if needed for compatibility
   with current tests.
3. Human rendering should cover representative V1 outputs:
   - mutation acknowledgements;
   - optional reads;
   - pages;
   - batches;
   - diagnostics;
   - health/info/config/admin facts;
   - vector matches;
   - graph nodes and edges.
4. Raw rendering should be script-friendly:
   - present KV values as bytes/text when possible;
   - present missing values as empty output;
   - present booleans as `true`/`false` or `1`/`0`, then document the choice;
   - present lists and pages one item per line;
   - present vector matches as tab-separated rows;
   - fall back to compact JSON for structured values that do not have a safe
     raw form.
5. Error rendering should preserve public executor error fields in JSON mode and
   show code, message, retryability, and reference ID in human mode.

Exit criteria:

1. JSON rendering is lossless for executor outputs.
2. Human rendering is useful without requiring `jq`.
3. Raw rendering works for shell pipelines.
4. Every unsupported/deferred command has a clear human and JSON error.

## Slice 4: Ergonomic Aliases And Inputs

Restore old command-line conveniences that do not change executor semantics.

Implementation steps:

1. Add aliases:
   - `kv del` -> `kv delete`;
   - `json del` -> `json delete`;
   - `vector del` -> `vector delete`;
   - `branch del` -> `branch delete`;
   - `space del` -> `space delete`.
2. For commands that support batches, allow repeated positional values or JSON
   files where the executor already supports batch commands.
3. Add value input helpers:
   - literal positional values;
   - `@path` file shorthand;
   - explicit `--file <path>`;
   - `-` for stdin where unambiguous.
4. Add JSON input helpers:
   - inline JSON;
   - `@path`;
   - `--file <path>`.
5. Add vector input helpers:
   - inline JSON array;
   - comma-separated numeric list if unambiguous;
   - `@path`;
   - `--metadata <json-or-file>`.
6. Preserve bytes exactly for KV values unless the user explicitly selects JSON
   or text encoding.

Exit criteria:

1. Common old examples keep working when the executor capability exists.
2. File inputs and inline inputs produce the same executor command.
3. Ambiguous input forms fail with actionable usage errors.

## Slice 5: Command Coverage And Deferred Registry

Build a CLI coverage inventory from the old parser and the current executor
command enum.

Implementation steps:

1. Create a hand-maintained `docs/architecture/cli-command-coverage.md`
   table with:
   - old CLI command;
   - current CLI command;
   - executor command;
   - status: supported, renamed, intentionally removed, deferred;
   - tracking document or issue.
2. Ensure every current `executor-next::Command` has either:
   - a first-class CLI command;
   - a raw `command run` path;
   - an explicit internal-only/deferred reason.
3. Add an unavailable-command registry for known old commands that users may
   still try:
   - branch diff/merge/tag/note commands;
   - graph ontology and analytics;
   - search and recipe commands;
   - transaction commands;
   - daemon lifecycle commands;
   - old maintenance controls.
4. Decide per deferred command whether it should:
   - appear in help as "coming later";
   - be hidden but recognized with a helpful error;
   - be fully unknown.
5. Keep intentionally removed storage maintenance controls out of normal help.

Exit criteria:

1. There is no accidental silent gap between `executor-next` and CLI.
2. Users get a precise answer when they try an old unsupported command.
3. Help output reflects what can actually run.

## Slice 6: Init And First-Run Guidance

Restore the useful first-run shape without old daemon assumptions.

Implementation steps:

1. Make `strata init` a machine-level setup command, not a database creation
   command.
2. Create or update `~/.strata`.
3. Detect basic environment facts:
   - OS and architecture;
   - CPU count;
   - memory;
   - likely local inference capability when available;
   - default model cache directory.
4. Write a machine profile file only after explicit confirmation or
   non-interactive defaults.
5. Print next-step guidance:
   - open a database: `strata ./my-db`;
   - run a command: `strata --db ./my-db kv put key value`;
   - use cache mode: `strata --cache`;
   - configure local AI later when implemented.
6. Treat old `up`, `down`, and `uninstall` as deferred unless a V1 daemon/server
   contract is introduced.

Exit criteria:

1. `strata init` is idempotent.
2. `strata init` does not create a durable database.
3. `strata init` does not download models unless explicitly requested in a later
   local-AI slice.
4. First-run errors are actionable.

## Slice 7: Help And Documentation

Make CLI help match the user journey.

Implementation steps:

1. Add examples to top-level help:
   - open REPL;
   - one-shot durable command;
   - cache mode;
   - pipe mode;
   - raw output.
2. Add command-group examples for KV, JSON, vector, event, graph, branch, and
   space.
3. Add a deferred-command help section or a docs page linked from unsupported
   command errors.
4. Keep generated clap help and written docs consistent through snapshot tests.

Exit criteria:

1. A new user can discover how to create/open/use a database from `strata help`.
2. A returning old-CLI user can understand which commands moved or are deferred.
3. Help output does not advertise commands that cannot run.

## Slice 8: Test Coverage

Add tests in layers rather than only binary smoke tests.

Required tests:

1. Parser tests for global flags, path forms, aliases, and file inputs.
2. Path resolver tests for cache, durable path, current directory, legacy
   `.strata`, and missing database guidance.
3. REPL parser tests for `help`, `use`, `clear`, `quit`, and `exit`.
4. Pipe mode tests for blank lines, comments, multiple commands, and partial
   failure exit behavior.
5. Rendering tests for JSON, human, raw, and errors.
6. Cache integration tests across multiple commands in the same process.
7. Durable integration tests across process-style open/close boundaries.
8. Unsupported old-command tests for the deferred registry.
9. Help snapshot tests for top-level and primary command groups.
10. Dependency guard tests that prevent importing old CLI/executor/storage
    crates.

Exit criteria:

1. `cargo test -p strata-cli-next` covers parser, renderer, REPL, pipe, cache,
   and durable flows.
2. `cargo clippy -p strata-cli-next --all-targets --all-features` is green.
3. Workspace `cargo test --all-targets --all-features` remains green.

## Completion Criteria

The parity milestone is complete when:

1. `strata`, `strata ./db`, `strata --db ./db <command>`, and `strata --cache`
   match the intended user flows.
2. Interactive REPL and pipe mode are implemented.
3. Human, JSON, and raw rendering are implemented and tested.
4. Branch/space context works across REPL and pipe commands.
5. Ergonomic aliases and file input cover the common old CLI workflows.
6. Every current executor command has a CLI disposition.
7. Every old unavailable command has a documented disposition.
8. The CLI does not depend on old crates or missing runtime surfaces.
