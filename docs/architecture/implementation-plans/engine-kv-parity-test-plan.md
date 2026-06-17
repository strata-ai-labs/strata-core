# Engine KV Parity Test Plan

## Purpose

Prove that the rebuilt engine owns the full KV operation surface required by the
serialized executor command contract, without pushing storage semantics into the
executor layer.

## Test Matrix

| Area | Cache | Durable Local |
| --- | --- | --- |
| Key encoding and decoding | Required | Required |
| Single put/get/delete | Required | Required |
| Batch put/get/delete/exists | Required | Required |
| Branch isolation | Required | Required |
| Space isolation | Required | Required |
| List and paginated list | Required | Required |
| Range scan | Required | Required |
| Count and sample | Required | Required |
| Version history | Required | Required |
| Timestamp and version reads | Required | Required |
| Shared primitive structure guards | Required | Required |
| Reopen persistence | Not applicable | Required |

## Unit Tests

### Key Decode

- Decodes ASCII user keys.
- Decodes binary user keys.
- Rejects unknown key version.
- Rejects unknown discriminator.
- Rejects truncated space length.
- Rejects truncated space bytes.
- Rejects mismatched product space.
- Rejects empty decoded user key.
- Preserves ordering for keys inside one product space.
- Does not decode control-plane row keys as KV keys.

### Outcome Types

- Accessors return the stored key, value, version, timestamp, tombstone flag,
  page cursor, and counts.
- Outcome structs do not expose mutable internal vectors.
- Outcome structs can be compared in tests without leaking storage types.

### Primitive Structure Guards

- KV public API modules expose engine-owned service and outcome types only.
- KV service code owns product behavior and does not construct storage requests
  directly.
- Persistence modules own storage request construction and storage outcome
  mapping.
- Branch and space resolution happen in the same layer for all KV operations.
- Batch, historical, list, scan, count, and sample methods share the same error
  mapping path.
- Source scans document any primitive-specific divergence as an explicit design
  decision before later JSON, event, vector, or graph code copies a different
  pattern.

## Engine Behavior Tests

Run each behavior test against both cache and durable-local fixtures unless the
test is specifically about reopen.

### Single Write And Read

- Put one key.
- Read latest value.
- Read versioned latest value.
- Assert version/timestamp match the commit outcome.

### Delete

- Put then delete a key.
- Latest point read returns missing.
- `exists` returns false.
- Second delete reports zero deleted rows or the documented no-op outcome.
- Version history records the delete tombstone when history includes tombstones.

### Batch Put

- Put several keys in one batch.
- Assert one commit outcome with put count equal to input length.
- Read every key back.
- Duplicate keys in the same batch fail with the stable duplicate-key error.
- Empty batch fails with the stable empty-batch error.

### Batch Get

- Batch get existing and missing keys.
- Results are positional.
- Missing keys return `None`.
- Duplicate read keys are allowed and return duplicate positional values.
- All returned version/timestamp metadata match individual reads.

### Batch Delete

- Delete existing and missing keys in one call.
- Results preserve input order if the API returns per-key outcomes; otherwise
  commit delete count matches the number of existing keys.
- Deleted keys are absent after commit.
- Missing keys do not fail the command.
- Duplicate delete keys follow the documented strict duplicate-key behavior.

### Exists And Batch Exists

- `exists` agrees with `batch_exists` for a single key.
- Batch exists returns positional booleans.
- Missing keys return false.
- Duplicate read keys are allowed.

### Branch Isolation

- Create a second branch.
- Write the same key to both branches.
- Point reads return branch-local values.
- List, scan, count, sample, and history stay branch-local.

### Space Isolation

- Write the same key to two product spaces.
- Point reads return space-local values.
- Prefix list and range scan do not cross spaces.
- Count and sample do not cross spaces.

### List

- Insert ordered keys with multiple prefixes.
- List all keys in sorted order.
- List prefix keys in sorted order.
- Missing prefix returns an empty list.
- Prefix with binary bytes returns only matching binary keys.
- Tombstoned keys are suppressed.

### Paginated List

- Insert more keys than the page limit.
- First page returns `limit` keys and `has_more=true`.
- Cursor is the last returned key.
- Second page starts strictly after cursor.
- Last page returns `has_more=false` and no cursor.
- `limit == 0` returns invalid input or a documented empty page, whichever the
  implementation decision selects.

### Range Scan

- Scan from the beginning with no limit.
- Scan from an inclusive start key.
- Scan with a limit.
- Scan where start falls between existing keys.
- Scan empty range.
- Scan bounded range with exclusive upper bound.
- Scan suppresses tombstones.
- Scan returns key/value/version/timestamp rows.

### Count

- Count all keys.
- Count by prefix.
- Count after delete.
- Count in empty space.
- Count in one branch does not include another branch.

### Sample

- `sample(count=0)` returns total count and zero items.
- `sample(count >= total)` returns all matching items.
- `sample(count < total)` returns at most count items.
- Every sampled key matches the prefix.
- Sampling does not cross branch or space.
- Repeated sample over unchanged data is deterministic if the implementation uses
  evenly-spaced selection.

## Historical Tests

### Version History

- Write a key multiple times.
- Delete it.
- `get_versions` returns newest-first rows.
- Rows include versions and timestamps.
- Tombstone row is present when history includes tombstones.
- Missing key returns `None` or an empty history according to the documented API.

### Point Read At Version

- Capture versions from two commits.
- Read at the first version and assert the first value.
- Read at the second version and assert the second value.
- Read before creation and assert missing.
- Read after delete and assert missing.

### Point Read At Timestamp

- Capture timestamps from two commits.
- Read at each timestamp and assert the visible value.
- Read before creation and assert missing.
- Read after delete and assert missing.

### List At Timestamp

- Create key A.
- Create key B.
- Delete key A.
- List at each timestamp and assert historical key set.
- Prefix list at timestamp stays prefix-local.

## Durable Reopen Tests

- Open durable-local database.
- Batch put keys in two spaces.
- Delete a subset.
- Close.
- Reopen.
- Assert point reads, list, scan, count, sample, and history match pre-close
  state.
- Create a branch, write branch-local keys, close, reopen, and assert branch
  isolation is preserved.

## Error Tests

- Invalid product space remains invalid.
- Empty key remains invalid.
- Missing branch returns not-found.
- Closed database rejects every KV operation.
- Invalid range returns invalid input.
- Invalid history limit returns invalid input.
- Corrupt decoded row key maps to engine corruption/data-loss class.
- Public error messages do not leak storage API type names.

## Source Guards

- Engine `api` modules do not publicly expose storage request or outcome types.
- `KvService` does not import executor command or output types.
- Executor crate does not import storage crates for KV behavior.
- Benchmark and smoke-loader code uses engine or executor public APIs, not
  persistence adapter internals.
- No test names or comments use planning-slice labels.

## Performance/Shape Smoke Tests

These are not benchmark gates; they prevent obviously pathological API shape.

- Batch get of 1,000 keys performs one branch lookup and no writes.
- Prefix list with limit does not return more than `limit + 1` internal rows to
  the pagination layer when storage limit pushdown is available.
- Count and sample do not issue one point read per matching key.
- Range scan with `limit=1` returns one visible row.

## Verification Commands

```text
cargo fmt --all
cargo check -p strata-engine-next --all-features
cargo test -p strata-engine-next --all-features
cargo clippy -p strata-engine-next --all-features --all-targets -- -D warnings
```
