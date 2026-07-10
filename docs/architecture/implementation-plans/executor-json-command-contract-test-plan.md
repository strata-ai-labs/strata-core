# Executor JSON Command Contract Test Plan

## Purpose

Prove that the executor crate exposes a stable serialized JSON command boundary
and remains a thin delegator over engine JSON APIs.

## Test Matrix

| Area | Cache | Durable Local |
| --- | --- | --- |
| Command serde round-trip | Required | Required |
| Output serde round-trip | Required | Required |
| Single set/get/delete | Required | Required |
| Path mutation | Required | Required |
| Batch set/get/delete | Required | Required |
| List/count/sample | Required | Required |
| Version history and as-of reads | Required | Required |
| Index commands | Required | Required |
| Error mapping | Required | Required |
| Reopen persistence | Not applicable | Required |

## Contract Tests

### Command JSON Round Trip

- Serialize and deserialize every JSON command variant.
- Include omitted branch/space.
- Include explicit branch/space.
- Include root path and nested paths.
- Include scalar, array, and object values.
- Include empty batches.
- Include invalid-looking batch items as payload shape.
- Include cursor, limit, and `as_of`.
- Include every index type.
- Assert deserialized command equality.

### Output JSON Round Trip

- Serialize and deserialize every JSON output variant.
- Include missing values.
- Include latest versioned values.
- Include timestamp values.
- Include history rows with tombstones.
- Include paginated list output.
- Include batch write/delete item results.
- Include batch get item results.
- Include sample output.
- Include index definition and index list output.

### Command Name Coverage

- Assert `Command::name()` returns the stable name for every JSON command.
- The match must be exhaustive so adding a command without naming it fails
  compilation.

### Command-To-Output Mapping

- Execute each JSON command on a small cache database.
- Assert the output variant exactly matches the documented mapping.
- Latest `JsonGet` returns `JsonVersionedValue`.
- Timestamp `JsonGet` returns `JsonValue`.
- `JsonGetv` returns `JsonVersionHistory`.
- Index commands return structured index outputs.

## Delegation Tests

### Executor Uses Engine APIs

- Source guard rejects storage crate imports in executor sources.
- Source guard rejects storage row, storage commit, table, WAL, lifecycle, and
  compaction type names in executor JSON code.
- Source guard rejects engine persistence adapter imports in executor JSON code.

### Convenience Facade Uses Commands

- Any JSON convenience method must call `execute(Command::Json...)`.
- Convenience methods must not directly call engine JSON service methods.

### No Benchmark Bypass

- JSON smoke loaders and benchmarks use executor `JsonBatchSet` or engine JSON
  public APIs.
- Source guard rejects direct storage writes from those binaries.

## Behavior Tests

Run behavior tests in both cache and durable-local executor fixtures unless the
test specifically targets reopen.

### Single Set And Get

- Execute `JsonSet` at root on a missing document.
- Execute latest `JsonGet` at root.
- Assert value, version, timestamp, and document version.
- Execute `JsonGet` at a nested path.
- Assert nested value.

### Path Mutation

- Set a nested object path.
- Set an array element if path arrays are supported.
- Read the mutated path.
- Read root and assert the full document changed.
- Invalid path returns executor invalid-input.

### Delete

- Delete a nested path.
- Assert `deleted=true`.
- Read the nested path and assert missing.
- Read root and assert the document still exists.
- Delete root.
- Assert the document is absent.
- Delete root again and assert `deleted=false`.

### Exists

- Missing document returns false.
- Created document returns true.
- Path deletion that leaves the document present returns true.
- Root deletion returns false.

### Branch And Space Defaults

- Omit branch and space and assert the default branch and `"default"` space.
- Repeat with explicit branch and explicit space.
- Set the executor default branch and assert omitted branch uses it.

### Branch Isolation

- Create a second branch through executor branch commands.
- Write the same document id to both branches.
- Assert reads stay branch-local.
- List/count/sample stay branch-local.

### Space Isolation

- Write the same document id to two spaces.
- Assert reads stay space-local.
- List/count/sample stay space-local.
- Index metadata stays space-local.

### Batch Set

- Execute one `JsonBatchSet` with multiple documents.
- Assert positional result count equals input count.
- Assert valid items have version/timestamp/document version.
- Read each valid document back.
- Empty batch returns an empty result list.
- Invalid item keys, paths, and values produce positional errors.
- Valid items still apply when other items are invalid.
- Duplicate behavior matches the engine contract.

### Batch Get

- Batch get existing and missing documents.
- Batch get existing and missing paths.
- Assert positional results preserve input order.
- Missing items have empty value fields, not command failure.
- Invalid item keys and paths produce positional errors.
- Duplicate reads are allowed.

### Batch Delete

- Batch root-delete existing and missing documents.
- Batch path-delete existing and missing paths.
- Assert positional results preserve input order.
- Empty batch returns an empty result list.
- Invalid item keys and paths produce positional errors.
- Valid deletes still apply when other items are invalid.

### List

- Insert ordered document ids with mixed prefixes.
- Execute `JsonList` with no prefix.
- Execute `JsonList` with a prefix.
- Assert cursor/limit pagination and `has_more`.
- Assert tombstoned documents are not listed.
- Assert internal index metadata is not listed.

### Count

- Insert documents with two prefixes.
- Count whole space.
- Count one prefix.
- Count after root delete.
- Count excludes internal index metadata.

### Sample

- Insert more documents than requested sample size.
- Assert `total_count`.
- Assert sampled item count is bounded by requested count.
- Assert sampled keys match the prefix.
- Assert sampled values are JSON values, not byte payloads.

### Version History

- Set a document multiple times.
- Delete a path.
- Root-delete the document.
- Execute `JsonGetv`.
- Assert newest-first history.
- Assert document versions, commit versions, timestamps, values, and tombstone
  facts.

### As-Of Reads

- Capture timestamps from two writes and a delete.
- Execute `JsonGet` with `as_of`.
- Assert historical path values.
- Execute `JsonList` with `as_of`.
- Assert historical document set.

### Index Commands

- Create numeric, tag, and text indexes when supported.
- Duplicate create returns invalid input.
- List indexes returns structured definitions.
- Drop existing index returns true.
- Drop missing index returns false.
- Normal list/count/sample do not expose index metadata.

## Durable Tests

### Durable Open/Reopen

- Open durable-local executor handle.
- Execute JSON batch set.
- Execute path mutations.
- Create index metadata.
- Delete a subset of documents.
- Close.
- Reopen.
- Assert reads, list, count, sample, history, and index list survived.

### Large Batch Smoke

- Execute repeated `JsonBatchSet` commands for a scaled row count.
- Assert no per-row command loop in benchmark or smoke-loader code.
- Assert final count equals expected visible documents.

## Error Tests

### Invalid Document Id

- Empty document id fails with executor invalid-input.
- Batch commands return positional item errors where the contract allows.

### Invalid Path

- Malformed path fails with executor invalid-input.
- Batch commands return positional item errors where the contract allows.

### Invalid Value

- Oversized document fails with executor invalid-input.
- Excessive nesting fails with executor invalid-input.
- Batch commands return positional item errors where the contract allows.

### Invalid Space

- Empty or reserved space fails with executor invalid-input.

### Missing Branch

- Write/delete commands fail with not-found.
- Read/list/count/sample commands use the documented engine behavior and map it
  to executor errors consistently.

### Closed Handle

- Close executor database.
- Any JSON command returns a closed-handle executor error.

### Error Boundary

- Serialized errors must not contain storage crate names.
- Serialized errors must not contain storage row/table/WAL/lifecycle terms.
- Item errors must not expose engine persistence internals.

## Source Guards

- `Cargo.toml` for executor crate does not depend on storage crates.
- Executor JSON source files do not import storage crates.
- Executor JSON source files do not import engine persistence modules.
- Command and output modules stay serde-serializable.
- JSON output vocabulary does not use generic optional-value variants.
- Every JSON command has command-name and output-mapping coverage.
- No test names or comments use planning-slice labels.

## Verification Commands

```text
cargo fmt --all
cargo check -p strata-executor-next --all-features
cargo test -p strata-executor-next --all-features json
cargo test -p strata-executor-next --all-features
cargo clippy -p strata-executor-next --all-features --all-targets -- -D warnings
```
