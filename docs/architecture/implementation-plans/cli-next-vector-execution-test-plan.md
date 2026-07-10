# CLI Next Vector Execution Test Plan

## Status

Implemented for the first executor-backed vector command slice.

## Goal

Verify that `cli-next` executes vector commands through `executor-next` while
preserving the IDL-driven discovery behavior.

## Required Tests

### Durable Execution

1. `vector collection create` creates a durable collection.
2. `vector upsert` writes vectors with metadata.
3. `vector get` reads a later-process value.
4. `vector exists` reflects visible state.
5. `vector count` reflects visible rows.
6. `vector keys --prefix --limit` returns a bounded page.
7. `vector metadata update` patches metadata.
8. `vector delete` removes one vector.
9. `vector delete-by-filter` removes matching vectors.

### Search Output

1. `vector query` returns executor `Output::VectorMatches` JSON.
2. `vector index query` returns executor `Output::VectorIndexQuery` JSON with
   diagnostics.
3. Metadata filter shorthand narrows results.
4. Query vectors parse from comma-separated and JSON-array forms.

### Parser Guards

1. Missing `--db` fails for executor-backed vector commands.
2. Duplicate shared scope flags fail.
3. Invalid vector literals fail before executor execution.
4. Unknown vector operations fail with a usage error.
5. Vector keys that look like flags work after `--`.
6. Batch vector commands remain explicitly deferred.

### Regression Coverage

1. `strata commands --family vector` still reflects generated metadata.
2. `strata vector --help` shows executable vector usage.
3. `strata vector query --help` shows generated command metadata.
4. Existing KV execution tests remain green.
5. Runtime CLI code still does not read authored YAML or prose.

## Acceptance Criteria

1. Tests cover at least one durable cross-process collection/vector path.
2. Tests cover human and JSON vector execution output.
3. Tests cover executor-backed failures and parser-level failures.
4. Existing CLI discovery and KV execution checks continue to pass.
