# CLI Next KV Execution Test Plan

## Status

Implemented for the first executor-backed KV command slice.

## Goal

Verify that `cli-next` can execute KV commands through `executor-next` while
preserving the generated discovery behavior from the IDL overlay.

## Required Tests

### Durable Execution

1. `kv put` writes to a durable database selected by `--db`.
2. `kv get` reads the value in a later process.
3. `kv delete` removes a value.
4. `kv exists` reflects put/delete state.
5. `kv count` reflects visible rows.
6. `kv list --prefix --limit` returns a bounded page.
7. `kv scan --start --limit` returns rows with values.

### Output Rendering

1. Human put output includes key, effect, and commit version.
2. Human get output prints a found value.
3. Human get output prints a missing result.
4. JSON get output is valid executor `Output` JSON.
5. JSON errors carry `schema_version`, `kind`, and `error`.
6. CLI usage errors remain structured and deterministic.
7. KV keys and values that look like flags work after `--`.

### Parser Guards

1. Missing `--db` fails for executor-backed KV commands.
2. Duplicate `--db` fails.
3. Duplicate `--branch`, `--space`, `--prefix`, `--cursor`, `--start`,
   `--limit`, and `--as-of` fail when in scope.
4. Invalid numeric values fail before executor execution.
5. Unknown KV operations fail with a usage error.
6. Metadata commands ignore no database and do not create `.strata`.
7. `strata kv --help` shows executable KV usage.
8. `strata kv put --help` shows generated command metadata for the operation.

### Regression Coverage

1. `strata commands` still works without `--db`.
2. `strata explain kv.put` still works without `--db`.
3. Runtime CLI code still does not read authored YAML or prose.
4. Runtime CLI code still does not depend on the old executor/engine/storage
   stack.
5. IDL-generated metadata paths remain hyphenated and command-discovery tests
   stay green.

## Acceptance Criteria

1. Tests cover at least one durable cross-process write/read path.
2. Tests cover both human and JSON execution output.
3. Tests cover executor-backed failures and parser-level failures.
4. Existing CLI discovery and executor IDL checks continue to pass.
