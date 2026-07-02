# CLI Next KV Execution Implementation Plan

## Status

Implemented for the first executor-backed KV command slice.

## Goal

Extend `cli-next` beyond generated command discovery so it can execute a
narrow, real KV workflow through `executor-next`.

The target flow is:

```text
strata --db ./my-db kv put key value
  -> cli-next parser
  -> executor-next Command::KvPut
  -> Executor::open_durable_local
  -> executor-next Output / ErrorStatus
  -> CLI human or JSON rendering
```

This proves that the generated discovery surface and the serialized executor
boundary can coexist in the same CLI without introducing a second command
model.

## Scope

In scope:

1. Durable local database open with `--db <path>`.
2. KV command execution for:
   - `kv put <key> <value>`
   - `kv get <key>`
   - `kv delete <key>`
   - `kv list`
   - `kv scan`
   - `kv exists <key>`
   - `kv count`
3. Shared CLI flags:
   - `--format human|json`
   - `--json`
   - `--db <path>`
   - `--branch <name>`
   - `--space <name>`
4. KV read/list flags:
   - `--prefix <prefix>`
   - `--cursor <cursor>`
   - `--limit <n>`
   - `--start <key>`
   - `--as-of <micros>`
5. `--` argument delimiter support for KV keys and values that look like CLI
   flags.
6. JSON output using the executor response/error contract.
7. Compact human output for common KV workflows.
8. Preserve local metadata actions:
   - `commands`
   - `explain`

Out of scope:

1. REPL mode.
2. Path shorthand such as `strata ./my-db`.
3. Cache-mode CLI execution.
4. KV batch commands.
5. JSON, vector, event, graph, space, admin, Arrow, or inference execution.
6. Shell completion generation.
7. Rich binary/encoding flags beyond UTF-8 command arguments.

## Design

`cli-next` remains a thin product shell. It does not create a second durable
runtime or bypass the executor.

The implementation should:

1. parse a CLI command into a serialized `executor-next::Command`;
2. open `Executor::open_durable_local` only for executor-backed actions;
3. call `Executor::execute`;
4. serialize successful output directly for JSON mode;
5. serialize `ExecutorError::status()` for JSON error mode;
6. render compact human summaries from the returned `Output`;
7. avoid opening a database for `commands`, `explain`, and help.

## CLI Shape

Examples:

```sh
strata --db ./my-db kv put user Claude
strata --db ./my-db kv put flag -- --json
strata --db ./my-db kv put -- --db literal
strata --db ./my-db kv get user
strata --db ./my-db kv exists user
strata --db ./my-db kv list --prefix us --limit 50
strata --db ./my-db kv scan --start user --limit 10
strata --db ./my-db --format json kv get user
```

The development binary remains `strata-next` until the old CLI is retired.
User-facing help should continue to describe the production command as
`strata`.

## Acceptance Criteria

1. KV commands execute through `Executor::execute`.
2. Missing `--db` fails before opening a database.
3. Metadata commands still do not require or open a database.
4. JSON success output is the executor `Output` wire shape.
5. JSON failure output carries the executor `ErrorStatus` shape when execution
   reaches the executor.
6. CLI usage errors are structured consistently with existing CLI discovery
   errors.
7. Durable reopen proves data was written to the selected path.
8. Existing IDL discovery tests still pass.
9. Executor-backed errors close the opened database handle best-effort before
   returning the original error.
