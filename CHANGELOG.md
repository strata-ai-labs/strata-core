# Changelog

All notable changes to StrataDB are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [1.2.0] - 2026-09-04

A hardening release on the V1 line, led by a self-update command and a batch of
error-contract and `event_range` corrections surfaced by the V1 test-coverage
audit. Several changes are **wire-visible**: a small set of error codes were
renamed or reclassified, and the `event_range` family's window semantics were
corrected. Callers that switch on the affected error codes/classes, or that
relied on the previous `event_range` behavior, should review **Changed** below.
The on-disk format is unchanged.

### Added

- **`strata update`.** Channel-aware in-place self-update: fetches the latest
  release for your channel, verifies the download, and swaps the binary. (#3038)

### Changed

- **`event_range` reverse now returns the descending window (the tail).** A
  reverse range anchored at the log start previously returned only the first
  event; it now walks the same window as the forward range, newest-first, so a
  reverse read returns the newest N events. (#2694)
- **`event_range_by_time` end is now exclusive.** The time window is half-open
  `[start_ts, end_ts)`, matching the sequence-addressed `event_range`; an event
  exactly at `end_ts` is no longer included. (#2695)
- **`data_loss.*` errors now report `class = "data_loss"`.** A new `data_loss`
  error class distinguishes unrecoverable durable-state loss from a detected
  integrity violation (`corruption`); the `data_loss.engine.*` codes previously
  surfaced `class = "corruption"`. (#2749)
- **Feature-disabled errors are now `unsupported.executor.*`.** The hub/arrow
  "feature not enabled in this build" codes moved from
  `invalid_argument.executor.{hub,arrow}_feature_disabled` to
  `unsupported.executor.{hub,arrow}_feature_disabled`, keeping their
  retry-after-rebuild advice. (#2750)
- **Missing durable artifacts refuse open as permanent corruption.** Opening a
  store whose current manifest or snapshot objects are gone now fails with a
  non-retryable `corruption.engine.persistence_recovery` instead of a retryable
  `unavailable`; the loss is permanent, so retry-forever advice was wrong.
  (#2754)
- **The command catalog publishes executable wire names.** Each
  `command-index.json` entry now carries a `wire` field with the exact `type`
  literal to invoke the command, so a tool reading the offline catalog can
  construct a call. (#2704)

### Fixed

- **Vendored llama.cpp updated** to upstream b10766 (from b5440), tracking the
  current stable local-inference FFI surface. (#3043)
- **Branch scan read failures preserve their underlying cause** in the error
  chain instead of masking it, sharpening recovery diagnostics. (#3047)

### Documentation

- **Graph batch atomicity is documented.** `graph.batch_write` applies all
  operations in one commit or none; unlike the itemwise kv/json/event batch
  writes, a graph batch rejects the whole batch on an invalid item to preserve
  referential integrity (an edge references its endpoint nodes). (#2701)

## [1.1.1] - 2026-09-02

A maintenance and hardening release on the V1 line. Modest additive polish —
StrataHub browsing, an in-browser CLI playground, and CLI ergonomics — on top of
correctness fixes and a large test-coverage expansion. No breaking changes; the
on-disk format, error codes, and CLI surface remain stable contracts.

### Added

- **StrataHub browse commands and clone progress.** Browse and list published
  StrataHub artifacts from the CLI, and see live progress while cloning.
- **Real CLI in the browser playground.** The hosted playground now runs the
  actual `strata` CLI compiled to WebAssembly (cache mode) rather than a
  simulation, and the wasm bundle ships as a release asset.
- **`strata uninstall`.** Restored on the V1 line — cleanly removes the binary
  and user-level Strata files.
- **Friendlier REPL.** A startup banner, errors you can recover from instead of
  a crash, Ctrl+C handling, human-readable `describe`, and actionable paging.
- **Dataset-aware bare `strata`.** Running `strata` with no path now recognizes
  when it is standing inside (or above) a dataset directory and responds
  accordingly, and every dataset directory self-describes with an advisory
  `README.md`.

### Fixed

- **Manifest loss no longer fabricates a fresh store.** Opening a non-empty
  database whose table manifest is missing now refuses the open with a
  structured error instead of silently starting from an empty manifest (which
  could mask data). (#3015)
- **Graph CDLP matches the LDBC definition.** Community-detection label
  propagation now propagates synchronously, so results conform to the LDBC
  Graphalytics reference instead of an iteration-order-dependent variant. (#3024)
- **Spelling-independent IPC broker.** Multi-process socket resolution is
  canonicalized so a broker and client agree on the socket regardless of path
  spelling. (#3006)

### Changed

- **Substantial test-coverage expansion (internal).** Vendored conformance
  suites (JSONTestSuite parsing, IEEE-754/ryu number formatting, SIFT exact-kNN
  ground truth, LDBC Graphalytics kernels), a format golden-vector matrix with an
  adversarial decode contract, a config × capability cross-product differential,
  fuzzing (value-fidelity, codec round-trip, dual op+byte mutation, corpus
  harvest), and elle-style concurrent-history checking. No user-facing behavior
  change; reliability only.

## [1.1.0] - 2026-08-29

The first feature release on the V1 line (relative to the `1.0.0` V1 baseline).
Headlined by branch operations and transparent multi-process access.

### Added

- **Branch operations — compare, preview, and promote (merge).** `branch.diff` /
  compare reports per-capability, per-space differences (KV, JSON, vector,
  vector-collection configs, event, graph); `branch.preview` reports the conflicts
  a promotion would hit; `branch.merge` promotes a source branch's changes into a
  target as a single atomic commit under `strict` (refuse on conflict with zero
  target mutation) or `source-wins` strategies, recording promotion lineage on the
  target. **Scope:** KV, JSON, and vector data (with their collection configs) are
  promoted; events and graphs are compare-only in V1. Copy (cherry-pick) and undo
  (revert) remain deferred to post-V1.
- **Transparent multi-process access.** A database directory can be opened by
  several processes at once through an owner-socket + client-broker transport, with
  a `strata start` / `strata stop` broker lifecycle, an `--ipc` opt-in, and
  `ipc_status` / `ipc_stop` admin commands. Includes read-only client sessions,
  protocol version ticks, and per-request deadlines.
- **Vector-collection comparison.** `branch.compare` surfaces collection create,
  delete, and reshape — including empty collections that hold no vectors — as a
  dedicated comparison capability.

### Changed

- Vector-collection promotion reconciles configs as a full base→source→target
  three-way; an incompatible dimension/metric refuses under every strategy.
- Promotion now carries source-side space and vector-collection **deletions**, not
  just additions, while keeping spaces the target still holds live rows in
  registered.

### Fixed

- A pre-release audit hardened branch operations end to end: the repeated-promotion
  merge base is the source frontier (no longer deletes target-only rows), source
  metadata deletions propagate, the collection-config three-way avoids both false
  conflicts and silent vector-shape mismatches, and the space-deletion guard
  preserves target-only event and graph state.
- Durability, recovery, and concurrency hardening from an extensive
  deterministic-simulation, loom model-checking, and differential-testing campaign
  (against Redis Streams, Neo4j, and exact k-NN oracles).

## [0.11.1] - 2026-02-07

### Added

- **Time-travel queries**: Read any primitive as-of a past timestamp. All read commands (`KvGet`, `KvList`, `StateGet`, `StateList`, `EventGet`, `EventGetByType`, `JsonGet`, `JsonList`, `VectorGet`, `VectorSearch`) accept an optional `as_of` field (microseconds since epoch) to query historical state.
- **WAL timestamp index**: Storage-level `get_at_timestamp()` and `scan_prefix_at_timestamp()` methods for MVCC lookups by timestamp, using the existing version chain (newest-first scan).
- **Version-aware HNSW**: Temporal tracking on HNSW nodes (`created_at`/`deleted_at`). New `is_alive_at()` check and `search_at()` method that filters by node liveness at the target timestamp — zero reconstruction cost for historical vector search.
- **Historical state reconstruction**: Per-primitive `get_at()` / `list_at()` methods (KV, State, Event, JSON, Vector) that read directly from storage version chains without requiring snapshot reconstruction.
- **`TimeRange` command**: Returns the oldest and newest timestamps for a branch, enabling clients to discover the available time-travel window.
- **`HistoryUnavailable` error**: Returned when a requested timestamp predates the oldest available data (e.g., after compaction or WAL truncation).
- **Dual time-travel strategy**: KV, State, Event, and JSON use in-memory version chain lookup; Vector uses live HNSW index with temporal filtering. Both achieve O(1)-per-key or O(log n) search cost with no data copying.
- **WAL replay timestamp preservation**: WAL replay now uses `insert_with_id_and_timestamp` / `delete_with_timestamp` to preserve vector `created_at`/`deleted_at` timestamps, ensuring `search_at()` works correctly after recovery.

## [0.5.1] - 2026-02-04

### Added

- **Spaces**: organizational namespaces within branches. Each branch contains one or more spaces, each with independent instances of all primitives (KV, Event, State, JSON, Vector). API: `set_space`, `current_space`, `list_spaces`, `delete_space`, `delete_space_force`.
- **Space auto-registration**: spaces are created on first write — no explicit `create_space` needed. The `default` space always exists and cannot be deleted.
- **Space parameter on all data commands**: `KvPut`, `KvGet`, `KvDelete`, `KvList`, `KvGetv`, `JsonSet`, `JsonGet`, `JsonDelete`, `JsonGetv`, `JsonList`, `EventAppend`, `EventRead`, `EventReadByType`, `EventLen`, `StateSet`, `StateRead`, `StateCas`, `StateInit`, `StateReadv`, `VectorUpsert`, `VectorBatchUpsert`, `VectorGet`, `VectorDelete`, `VectorSearch`, `VectorCreateCollection`, `VectorDeleteCollection`, `VectorListCollections`, `VectorCollectionStats` all accept an optional `space` field. When `None`, defaults to the current space on the handle (initially `"default"`).
- **Space commands**: `SpaceList`, `SpaceCreate`, `SpaceDelete` (with `force` flag), `SpaceExists` command variants for SDK builders.
- **Structured logging**: `tracing` instrumentation across 10 subsystem targets — `strata::branch`, `strata::vector`, `strata::space`, `strata::db`, `strata::txn`, `strata::command`, `strata::wal`, `strata::snapshot`, `strata::recovery`, `strata::compaction`. Zero overhead unless a `tracing` subscriber is wired up by the caller. Configurable per-subsystem log levels via standard `RUST_LOG` filtering (e.g. `RUST_LOG=strata::txn=debug`).
- **`tracing` dependency**: added to executor and engine crates for structured span and event instrumentation.

## [0.4.0] - 2026-02-03

### Added

- **HNSW index backend**: O(log n) approximate nearest neighbor search built from scratch, verified against the Malkov & Yashunin paper (arXiv:1603.09320). Configurable M, ef_construction, ef_search parameters. Selectable per collection via `IndexBackendFactory`.
- **Advanced metadata filters**: 8 filter operators (Eq, Ne, Gt, Gte, Lt, Lte, In, Contains) with `FilterCondition` and `FilterOp` types in core. Full executor bridge support.
- **Batch vector upsert**: `VectorBatchUpsert` command and `vector_batch_upsert()` API for atomic bulk vector insertion in a single transaction.
- **Collection statistics**: `VectorCollectionStats` command and `vector_collection_stats()` API. CollectionInfo now includes `index_type` and `memory_bytes` fields. Backed by `index_type_name()` and `memory_usage()` on the `VectorIndexBackend` trait.
- **Reserved internal vector namespace**: `_system_*` collections for the intelligence layer with `validate_system_collection_name()` and internal `system_insert`/`system_search` methods. Hidden from `vector_list_collections`.
- **Shared distance functions**: Extracted distance computation into `distance.rs` module shared by both BruteForce and HNSW backends (cosine, euclidean, dot product).
- **strata-security crate**: Read-only access mode for database connections (from PR #1012).

## [0.1.0] - 2026-01-30

### Added

- **Six data primitives**: KV Store, Event Log, State Cell, JSON Store, Vector Store, Run
- **Value type system**: 8-variant `Value` enum (Null, Bool, Int, Float, String, Bytes, Array, Object) with strict typing rules
- **Run-based data isolation**: git-like branches for isolating agent sessions and experiments
- **OCC transactions**: optimistic concurrency control with snapshot isolation and read-your-writes semantics via the `Session` API
- **Three durability modes**: None, Buffered (default), and Strict
- **Write-ahead log (WAL)**: CRC32-checked entries for crash recovery
- **Snapshots**: periodic full-state captures for bounded recovery time
- **Run bundles**: export/import runs as portable `.runbundle.tar.zst` archives
- **Hybrid search**: BM25 keyword scoring with Reciprocal Rank Fusion across primitives
- **Vector store**: collection management, similarity search (Cosine, Euclidean, DotProduct), metadata support
- **JSON store**: path-level reads and writes with cursor-based pagination
- **Versioned reads**: `getv()`/`readv()` API for version history access
- **Typed Strata API**: high-level Rust API with `Into<Value>` ergonomics
- **Command/Output enums**: serializable instruction set for SDK builders
- **7-crate workspace**: core, storage, concurrency, durability, engine, intelligence, executor
