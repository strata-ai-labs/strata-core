# CLI Next Vector Execution Implementation Plan

## Status

Implemented for the first executor-backed vector command slice.

## Goal

Extend `cli-next` so users can execute the core vector workflow through
`executor-next` from the generated CLI family.

The target flow is:

```text
strata --db ./my-db vector upsert docs key --vector 1,0
  -> cli-next parser
  -> executor-next Command::VectorUpsert
  -> Executor::open_durable_local
  -> executor-next Output / ErrorStatus
  -> CLI human or JSON rendering
```

## Scope

In scope:

1. Durable local database open with `--db <path>`.
2. Vector collection execution for create, delete, list, and stats.
3. Vector row execution for upsert, get, history, exists, keys, delete, count,
   metadata update, delete-all, and delete-by-filter.
4. Vector search execution for query and index query.
5. Shared CLI flags:
   - `--format human|json`
   - `--json`
   - `--db <path>`
   - `--branch <name>`
   - `--space <name>`
   - `--`
6. Vector command flags:
   - `--dimension <n>`
   - `--metric cosine|euclidean|dot-product`
   - `--vector <csv|json-array>`
   - `--query <csv|json-array>`
   - `--k <n>`
   - `--metadata <json>`
   - `--patch <json>`
   - `--filter <json>`
   - `--prefix <prefix>`
   - `--cursor <cursor>`
   - `--limit <n>`
   - `--as-of <micros>`
7. JSON output using the executor response/error contract.
8. Compact human output for common vector workflows.

Out of scope:

1. Vector batch commands.
2. File-based vector or metadata input.
3. Shell completion generation.
4. REPL mode.
5. Cache-mode CLI execution.

## Design

`cli-next` remains a thin shell. It parses CLI arguments into serialized
`executor-next::Command` values, then uses the shared execution helper to run
against a durable local executor.

The vector parser accepts vectors as comma-separated floats or JSON arrays.
Filters accept either the executor `VectorMetadataFilter` JSON shape or a
shorthand JSON object whose scalar fields are mapped to equality conditions.

## Acceptance Criteria

1. Vector commands execute through `Executor::execute`.
2. Missing `--db` fails before opening a database.
3. JSON success output is the executor `Output` wire shape.
4. JSON failure output carries executor `ErrorStatus` when execution reaches
   the executor.
5. CLI usage errors are structured consistently with KV execution.
6. Durable reopen proves collections and vectors are written to the selected
   path.
7. Existing IDL discovery and KV execution tests still pass.
