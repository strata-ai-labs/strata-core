# Engine KV Parity Implementation Plan

## Problem

The executor command boundary should be a stateless delegator over engine APIs.
That only works if the engine owns the full KV operational surface needed by the
serialized command contract. Today the rebuilt engine exposes branch creation,
latest point reads, single put/delete, and put/delete batches. The old engine
also exposed list, range scan, batch reads, existence checks, version history,
point-in-time reads, count, and sample.

If the executor is implemented before this parity work, it will either be unable
to support the old KV command set or it will start recreating engine behavior in
the wrong layer. This slice restores the engine KV API surface first.

## Old Evidence

- `crates/engine/src/primitives/kv.rs`
- `crates/engine/src/database/transaction.rs`
- `crates/engine/src/transaction/context.rs`
- `crates/executor/src/handlers/kv.rs`
- `crates/executor/src/command.rs`

## Current Targets

- `crates/engine-next/src/api/`
- `crates/engine-next/src/data/kv/`
- `crates/engine-next/src/persistence/`
- `crates/engine-next/tests/`

## Current Status

Already present:

- cache and durable-local database open
- branch create/list/lookup
- KV latest point read
- KV single put/delete
- KV put/delete batches
- durable reopen tests for control-plane and simple KV writes

Missing from engine:

- batch get
- exists and batch exists
- list by prefix
- paginated list
- range scan
- count by prefix
- sample by prefix
- point-in-time get
- point-in-time list
- full version history
- public versioned KV read outcomes
- public scan/list/page/sample outcomes

Storage already exposes the lower-level pieces needed for most of this:

- `PointReadRequest` with latest, version, and timestamp bounds
- `HistoryReadRequest`
- `PrefixScanReadRequest`
- `ScanReadRequest`
- `ReadLimit`
- `ScanRange`

## Design Decisions

1. **Parity means operational parity, not copying the old product value model.**
   The old engine KV primitive stored `strata_core::Value`. The rebuilt engine
   KV spine is byte-oriented. This slice preserves byte KV and restores the old
   operation set. Typed product values can be layered above byte KV later.

2. **Engine owns all KV read semantics.** Prefix filtering, range bounds,
   historical visibility, tombstone suppression, branch isolation, space
   isolation, and version metadata must be handled in engine APIs, not executor.

3. **Executor must not need storage types.** Every method added here returns
   engine-owned structs and accepts engine-owned `BranchName`, `ProductSpace`,
   `KvKey`, and `KvValue`.

4. **The encoded KV row key remains an internal persistence detail.** Engine
   needs a checked decode path for scan/history outputs, but decoded user keys
   must be exposed as `KvKey`, not raw storage row keys.

5. **Batch read outputs are positional.** Batch get and batch exists return one
   result per input key in input order. Missing keys are item-level misses, not
   command failures.

6. **Batch write duplicate behavior stays strict.** Current `put_batch` and
   `delete_batch` reject duplicate encoded keys inside one batch. Keep that
   behavior unless deliberately changed in a separate compatibility decision.

7. **Historical reads use timestamp/version metadata from storage rows.** Engine
   should expose commit version and timestamp for latest, historical, scan, and
   history outputs.

8. **Count and sample are correctness APIs first.** Use storage scans through
   engine persistence initially; optimize later only if perf diagnostics show it
   matters. Do not add lower-layer shortcuts to benchmarks or executor.

9. **KV establishes the shared primitive shape.** The engine should implement
   KV, JSON, event, vector, and graph with a consistent internal structure unless
   a primitive has a real semantic reason to differ. KV should set the pattern:
   public API types in `api`, product service in `data`, persistence translation
   isolated in `persistence`, engine-owned outcome structs, branch/space
   resolution in one place, and no storage types crossing the public API.

10. **Differences must be semantic, not accidental.** Event ordering, vector
    collection metadata, graph topology, and JSON path behavior can justify
    primitive-specific code. Different error mapping, branch/space handling,
    batch behavior, or storage bypasses are not acceptable unless documented as
    deliberate design decisions.

## Shared Primitive Structure Target

KV should establish reusable structure for the other product primitives:

- `api/<primitive>.rs` re-exports only public service and outcome types.
- `data/<primitive>/types.rs` owns validated input types.
- `data/<primitive>/service.rs` owns product operations and branch/space
  semantics.
- `data/<primitive>/outcome.rs` owns engine-level read/list/history results.
- `persistence` owns row-key encoding, row-key decoding, storage request
  construction, and storage outcome mapping.
- Tests are grouped by public behavior, persistence translation, and source
  guards.
- Public methods return engine result types, never storage request or outcome
  types.

## Public Engine API Target

Add engine-owned output types:

- `KvVersionedValue`
  - value bytes
  - commit version
  - commit timestamp
- `KvScanRow`
  - key bytes
  - value bytes
  - commit version
  - commit timestamp
- `KvListPage`
  - keys
  - has_more
  - cursor
- `KvHistory`
  - rows newest-first
- `KvHistoryRow`
  - value bytes when present
  - tombstone flag
  - commit version
  - commit timestamp
- `KvSample`
  - total_count
  - sampled rows

Add `KvService` methods:

- `get_versioned(&KvKey) -> Option<KvVersionedValue>`
- `get_at(&KvKey, Timestamp) -> Option<KvValue>`
- `get_at_version(&KvKey, CommitVersion) -> Option<KvValue>`
- `get_versions(&KvKey) -> Option<KvHistory>`
- `batch_get<I>(&[KvKey]) -> Vec<Option<KvVersionedValue>>`
- `exists(&KvKey) -> bool`
- `batch_exists<I>(&[KvKey]) -> Vec<bool>`
- `list(prefix: Option<&KvKey>) -> Vec<KvKey>`
- `list_page(prefix: Option<&KvKey>, cursor: Option<&KvKey>, limit: usize) -> KvListPage`
- `list_at(prefix: Option<&KvKey>, timestamp: Timestamp) -> Vec<KvKey>`
- `scan(start: Option<&KvKey>, limit: Option<usize>) -> Vec<KvScanRow>`
- `scan_range(start: Option<&KvKey>, end: Option<&KvKey>, limit: Option<usize>) -> Vec<KvScanRow>`
- `count(prefix: Option<&KvKey>) -> u64`
- `sample(prefix: Option<&KvKey>, count: usize) -> KvSample`

Keep existing:

- `put`
- `put_batch`
- `delete`
- `delete_batch`

## Implementation Order

### 1. KV Key Decode Helpers

- Add checked decoding for engine KV row keys:
  - verify key version byte
  - verify KV discriminator byte
  - verify encoded space length
  - verify encoded space matches the selected `ProductSpace`
  - return decoded user-key bytes as `KvKey`
- Keep decode helpers in the persistence/key module or a KV-private module.
- Add tests for malformed version, discriminator, truncated space length,
  mismatched space, empty user key, and binary user key.

### 2. Public KV Outcome Types

- Add engine-owned read outcome structs under `data/kv` and re-export through
  `api`.
- Expose accessors only; keep fields private.
- Use `strata_core_next::CommitVersion` and `Timestamp`.
- Ensure outcomes carry byte values without serialization assumptions.

### 3. Persistence Read Row Adapter

Extend `StoragePersistence` with engine-owned read helpers:

- latest point read returning value + version/timestamp
- point read at version
- point read at timestamp
- history read
- prefix scan
- range scan

These helpers may use storage API request types internally, but those types must
not leave the persistence module.

### 4. Batch Read And Exists

- Implement `KvService::batch_get` using one branch lookup and repeated
  persistence point reads.
- Implement `KvService::exists` via latest point read.
- Implement `KvService::batch_exists` using one branch lookup and repeated
  latest point reads.
- Preserve positional results.
- Do not fail the whole command for missing keys.

### 5. Prefix List And Paginated List

- Implement prefix encoding by reusing the selected space prefix:
  `encode_kv_key_bytes(space, prefix_bytes)`.
- Use storage prefix scan with latest bound.
- Decode each returned row key back into a user key.
- Suppress tombstones.
- For `list_page`, request `limit + 1` rows from storage when possible.
- Cursor semantics: return keys strictly greater than cursor.
- `has_more` is true when more than `limit` visible keys were found.
- `cursor` is the last returned key when `has_more` is true.

### 6. Range Scan

- Implement `scan` as an inclusive start-key scan over the selected space.
- Implement `scan_range` with inclusive lower bound and exclusive upper bound,
  matching the storage `ScanRange` shape.
- Return ordered key/value rows with version/timestamp.
- Respect `limit == Some(0)` as an empty result at the engine layer, without
  sending an invalid storage `ReadLimit`.
- Ensure scan does not cross product-space boundaries.

### 7. Count And Sample

- Implement `count` through prefix scan row counting.
- Implement `sample` through one prefix scan and evenly-spaced selection, matching
  the old engine behavior.
- `sample(count=0)` returns the total count and zero sampled rows.
- Do not materialize values twice.
- Keep this correctness-first; add optimization only after executor command
  parity is green.

### 8. Historical Reads

- Implement `get_at` with `ReadBound::AtTimestamp`.
- Implement `get_at_version` with `ReadBound::AtVersion`.
- Implement `list_at` with prefix scan and timestamp bound.
- Implement `get_versions` with `HistoryReadRequest`.
- Decide and document whether history includes tombstones. Preferred: include
  tombstones in engine history rows so the executor can represent deletes
  accurately.

### 9. Error Mapping

- Map malformed decoded storage rows to corruption/data-loss engine errors.
- Map invalid limit, range, and storage request errors through the existing
  persistence mapper.
- Ensure public engine errors do not contain storage type names.

### 10. Source Guards

- Engine public API modules must not re-export storage API request/outcome types.
- Executor crate must not implement KV list/scan/history by importing storage.
- Benchmarks must use engine or executor public KV APIs, not persistence adapter
  internals.

## Non-Goals

- Executor command implementation.
- SDKs, CLI, MCP server, or IPC server.
- Typed `Value` semantics from the old product core.
- Search indexing side effects from the old engine.
- JSON, event, vector, graph, or full-text primitives.
- Stateful transaction command handles.
- Storage scan optimization beyond what storage already provides.

## Exit Gates

- Engine exposes every method needed by the KV executor command set.
- Cache and durable-local tests pass for the full KV operation set.
- Durable reopen preserves values, deletes, scans, counts, and histories.
- Branch and space isolation hold for point reads, list, scan, count, and sample.
- Historical reads match versions/timestamps captured from write outcomes.
- Public engine API does not expose storage API types.
- Executor implementation can be written as a pure delegator after this slice.
