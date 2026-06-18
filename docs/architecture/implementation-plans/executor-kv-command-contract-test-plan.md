# Executor KV Command Contract Test Plan

## Purpose

Prove that the executor crate is a stable serialized command boundary for all
KV operations and that it remains a thin delegator over engine APIs.

## Test Matrix

| Area | Cache | Durable Local |
| --- | --- | --- |
| Command serde round-trip | Required | Required |
| Single put/get/delete | Required | Required |
| List/scan pagination | Required | Required |
| Batch put/get/delete/exists | Required | Required |
| Version history and as-of reads | Required | Required |
| Count and sample | Required | Required |
| Error mapping | Required | Required |
| Reopen persistence | Not applicable | Required |

## Contract Tests

1. **Command JSON Round Trip**
   - Serialize and deserialize every KV command variant.
   - Include omitted branch/space, explicit branch/space, byte keys, byte
     values, empty batches, cursor fields, limits, and `as_of`.
   - Assert deserialized command equality.

2. **Output JSON Round Trip**
   - Serialize and deserialize every KV output variant.
   - Include missing values, versioned values, history, paginated keys, scan
     rows, batch results, boolean lists, counts, and samples.

3. **Command Name Coverage**
   - Assert `Command::name()` returns the expected string for every KV command.
   - The match must be exhaustive so adding a command without naming it fails
     compilation.

4. **Command-To-Output Mapping**
   - Execute each KV command on a small cache database.
   - Assert the output variant exactly matches the documented mapping.

## Delegation Tests

1. **Executor Uses Engine APIs**
   - Use source guards to reject storage crate imports in executor sources.
   - Reject storage commit, row, table, WAL, lifecycle, and compaction type names
     in executor command modules.

2. **Convenience Facade Uses Commands**
   - Source guard verifies convenience methods call `execute(Command::...)`.
   - Convenience methods must not directly call engine KV methods.

3. **No Benchmark Bypass**
   - Benchmark binaries and smoke loaders must use `KvBatchPut` through executor
     command execution.
   - Source guard rejects direct storage or lower-layer batch writes from those
     binaries.

## Behavior Tests

1. **Single Write And Read**
   - Open cache database.
   - Execute `KvPut`.
   - Execute current `KvGet`.
   - Assert value, version, and timestamp are present.

2. **Delete**
   - Put a key.
   - Delete the key.
   - Assert `deleted=true`.
   - Read the key and assert missing.
   - Delete it again and assert `deleted=false`.

3. **Branch And Space Defaults**
   - Omit branch and space.
   - Assert operations hit the default branch and `"default"` space.
   - Repeat with explicit branch and explicit space.

4. **Branch Isolation**
   - Create a second branch through the engine branch API or executor branch
     helper when available.
   - Write the same key to two branches.
   - Assert reads stay branch-local.

5. **Space Isolation**
   - Write the same key to two spaces on one branch.
   - Assert reads and list/scan results stay space-local.

6. **List**
   - Insert ordered keys with mixed prefixes.
   - Assert prefix filtering.
   - Assert cursor/limit pagination and `has_more`.
   - Assert empty prefix and missing prefix cases.

7. **Scan**
   - Insert ordered keys.
   - Assert scan starts at the inclusive start key.
   - Assert limit handling.
   - Assert missing start key starts at the next greater key.

8. **Batch Put**
   - Execute one `KvBatchPut` with multiple entries.
   - Assert every result has a version and no error.
   - Read each key back.
   - Assert empty batch put returns `BatchResults([])`.
   - Assert invalid batch put items return positional item errors without
     preventing valid items from applying.
   - Assert duplicate-key behavior matches the engine contract.

9. **Batch Get**
   - Batch get existing and missing keys.
   - Assert positional results preserve input order.
   - Assert missing keys produce empty value fields, not command failure.
   - Assert invalid batch get keys return positional item errors.

10. **Batch Delete**
    - Delete existing and missing keys.
    - Assert positional results preserve input order.
    - Assert empty batch delete returns `BatchResults([])`.
    - Assert invalid batch delete keys return positional item errors without
      preventing valid deletes from applying.
    - Assert executor delete outputs use engine delete outcome facts rather than
      executor-side read-before-delete logic.
    - Assert embedding or side-effect hooks, when present, run only for deleted
      keys.

11. **Batch Exists And Exists**
    - Compare `KvExists` to single-key `KvBatchExists`.
    - Assert booleans are positional and order-preserving.

12. **Version History**
    - Write a key multiple times and delete it once.
    - Assert `KvGetv` returns newest-first history with versions/timestamps.
    - Assert missing key returns `None`.

13. **As-Of Reads**
    - Capture versions/timestamps from two writes.
    - Assert `KvGet { as_of }` returns the value visible at that timestamp.
    - Assert `KvList { as_of }` reflects the historical key set.

14. **Count**
    - Insert keys with two prefixes.
    - Assert whole-space count and prefix count.

15. **Sample**
    - Insert more rows than requested sample size.
    - Assert `total_count` is the matching count.
    - Assert item count is bounded by requested sample size.
    - Assert sampled keys all match the prefix when provided.

## Durable Tests

1. **Durable Open/Reopen**
   - Open durable-local executor database.
   - Execute batch put.
   - Close.
   - Reopen.
   - Assert values, list, count, and history survived.

2. **Durable Delete Reopen**
   - Put keys.
   - Delete a subset.
   - Close and reopen.
   - Assert deleted keys remain absent and retained keys remain present.

3. **Large Batch Smoke**
   - Execute repeated `KvBatchPut` commands for a scaled row count.
   - Assert no per-row command loop in benchmark or smoke loader code.

## Error Tests

1. **Invalid Key**
   - Empty key fails with executor invalid-input class.
   - Batch put/get/delete commands report invalid item errors positionally when
     the old contract expects per-item errors.

2. **Invalid Space**
   - Reserved or empty space fails with executor invalid-input class.

3. **Missing Branch**
   - Write/delete commands fail with not-found.
   - Read commands return the selected not-found behavior defined by the engine
     API contract.

4. **Closed Handle**
   - Close executor database.
   - Any command returns a closed-handle executor error.

5. **Error Boundary**
   - Serialized errors must not contain storage crate names, storage type names,
     table names, WAL internals, or lifecycle terminology.

## Source Guards

- `Cargo.toml` for executor crate must not depend on storage crates.
- Executor source files must not import storage crates.
- Executor command modules must not contain storage row/commit/table/WAL type
  names.
- Benchmark loaders must use executor `Command::KvBatchPut`.
- Command and output modules must stay serde-serializable.
- Output vocabulary uses KV-specific `KvValue` and `KvVersionedValue` rather
  than generic optional-value names.
- No command variant may be added without command-name and output-mapping tests.

## Verification Commands

```text
cargo fmt --all
cargo check -p strata-executor-next --all-features
cargo test -p strata-executor-next --all-features
cargo clippy -p strata-executor-next --all-features --all-targets -- -D warnings
```
