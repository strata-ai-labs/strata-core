# Storage Portability Plan

## Purpose

Make Strata's durable storage path pluggable so that the same engine
can run against:

- local POSIX filesystems (today's behavior, preserved)
- AWS S3 and S3-compatible object stores (R2, MinIO, Wasabi, Tigris)
- GCS, Azure Blob, Aliyun OSS
- IndexedDB and OPFS in browsers (the wasm-cache durability follow-on)
- in-memory (test fixtures, ephemeral cache mode)

The chosen abstraction is **Apache OpenDAL** (`opendal` crate). It is the
only Rust storage abstraction that simultaneously: (a) covers the
breadth of services Strata needs as deployment targets, (b) has working
`wasm32-unknown-unknown` support including browser-side services
(`indexed_db`), and (c) has a layer system suitable for retry, metrics,
and concurrency control without bespoke wrappers.

Strata is an **embedded** database. Adopting OpenDAL must not turn it
into an async-runtime-required library, must not impose a tokio
dependency on every embedder, and must not regress local-FS performance
beyond the noise thresholds in CLAUDE.md's non-regression protocol.

The artifact at the end of this work is:

- a single `StorageProvider` trait that engine and storage talk to
  instead of `std::fs`
- a default local-FS implementation backed by `opendal::services::Fs`
- a working S3 backend with conditional-write manifest commit
- a local-disk hot-segment cache for remote backends
- IndexedDB / OPFS providers for the wasm cache mode (closes the loop
  with `wasm-cache-target-plan.md`)
- documented capability requirements that any custom provider must meet
- CI coverage for at least: local FS, in-memory, MinIO (S3-compatible),
  IndexedDB

This is a deeply load-bearing change: it touches the WAL writer,
manifest commit path, segment IO, recovery, and snapshot/checkpoint.
It is a multi-tranche workstream, not an epic.

## Relationship to Other Plans

This plan depends on `wasm-cache-target-plan.md` landing first. That
plan's phase W2 (cache-mode reachability audit) and W3 (cfg-gating
disk-only modules) produce the first draft of "what does cache mode
need from storage" — which is the seed of the `StorageProvider` trait.
Doing storage portability before wasm gates means designing the trait
without that pressure test, and the trait will be wrong.

Sequencing summary:

```
wasm-cache-target-plan W1..W5   (~1 month, engine-only)
  └── storage-portability-plan SP1..SP7   (~3-6 months)
```

SP1 may overlap with W4/W5 if reviewer bandwidth allows; SP2 onward
must wait for the wasm plan to be fully merged.

## Scope

This plan owns:

- Defining `StorageProvider` and adjacent traits (`SegmentReader`,
  `SegmentWriter`, `ManifestStore`, `WalSink`) as the engine's only
  storage interface
- Replacing direct `std::fs`, `memmap2`, and `fs2` usage in
  `crates/storage` and the durable paths of `crates/engine` with calls
  through the trait
- Implementing a local-FS provider backed by `opendal::services::Fs`,
  preserving today's mmap and fsync semantics where they exist locally
- Implementing an in-memory provider for tests and cache mode
- Implementing an S3 / S3-compatible provider via OpenDAL with
  conditional-write manifest commit and etag-based fencing
- Implementing IndexedDB and OPFS providers for wasm targets
- The async/sync bridge: a dedicated WAL writer thread that owns a
  tokio runtime and exposes a sync API to the rest of the engine
- A local-disk hot-segment cache subsystem for remote providers
- Capability negotiation at open time: a provider declares what it
  supports, the engine refuses to open if requirements are not met
- CI matrix covering local FS, in-memory, MinIO, and IndexedDB
- Documentation: `StorageProvider` capability table, supported
  services, configuration schema, cost/latency notes

This plan does not own:

- Designing a new on-disk format. WAL, manifest, checkpoint, snapshot,
  segment formats are unchanged.
- Multi-writer concurrency. Strata remains single-writer per database;
  conditional-write fencing protects against split-brain but the
  engine does not become multi-writer.
- Read replicas across regions, geo-replication, or async replication.
  Follower mode is unchanged.
- Vector index format changes (HNSW segments still serialize the same
  bytes; only the IO path moves).
- Bundle import/export changes beyond running through the new trait.
- A sync engine refactor. The async/sync bridge is at the WAL writer
  boundary only; engine and transaction code remain sync.
- Search query optimization for high-latency backends. That is a
  separate workstream once latency profiles are measured.
- Pricing model, quotas, or billing for cloud-deployed instances.

## Load-Bearing Constraints

1. **No on-disk format change.** Every format byte that exists today
   (WAL, manifest, segment, checkpoint, snapshot, bundle) is byte-for-byte
   identical after this work on local FS. A native database opened
   before SP1 is openable after SP7 with no migration. This is checked
   by golden-file tests added in SP1.

2. **No tokio in the engine public surface.** No public type takes a
   `&Runtime`, no public method is `async`, no public trait has an
   `async fn`. The async runtime exists inside the WAL writer thread
   and is not visible to callers. Embedders without tokio (or with a
   different runtime) can use Strata exactly as they do today.

3. **Local-FS performance unchanged.** Native benchmarks (`redb`,
   `ycsb_compare`, `beir`) must show no regression on local FS beyond
   noise thresholds. The OpenDAL FS service is fast enough in
   principle, but the indirection through the trait + provider is
   measurable; SP2 must validate this and tune accordingly (likely a
   specialized fast path for the FS provider that bypasses some
   OpenDAL layering).

4. **Single-writer fencing is structural.** Conditional-write manifest
   commit (`If-Match` on etag) is the only mechanism that prevents
   two concurrent writers from corrupting a remote database. It is
   not advisory and not optional. Providers that cannot support
   conditional writes are not eligible for durable mode; they may
   only back cache mode.

5. **One canonical durability path.** The WAL writer + manifest commit
   sequence is the same regardless of provider. Different providers
   are not allowed different commit semantics. (Local FS and S3 commit
   the same way; the local case is just much faster.)

6. **Capability requirements are explicit and checked at open time.**
   Fail-fast per CLAUDE.md rule 23. A misconfigured provider must
   refuse to open, not partially work and fail at commit time.

7. **Caching is engine-owned.** The hot-segment cache is not an
   OpenDAL layer. It is a Strata subsystem with eviction tied to
   compaction events, manifest version transitions, and per-segment
   access patterns. Cache correctness is part of the engine's
   correctness contract.

8. **No process-global storage state.** Per CLAUDE.md rule 3. Each
   `Database` owns its own `StorageProvider` instance.

## Current Code Map

The storage code that must move behind the trait:

- `crates/storage/src/segment.rs` — segment file reader and writer,
  uses `std::fs`, `memmap2`
- `crates/storage/src/segment_builder.rs` — segment construction,
  zstd compression
- `crates/storage/src/{wal,manifest,checkpoint,snapshot}` paths
  (verify exact paths in SP1) — durable file IO
- `crates/engine/src/database/open.rs` — file locking via `fs2`,
  recovery from disk, lock acquisition
- `crates/engine/src/database/product_open.rs` — open-time path
  validation and lockfile handling
- `crates/engine/src/database/compaction.rs` — segment rewrite IO
- `crates/engine/src/recovery/` — WAL replay reads
- `crates/engine/src/bundle/{reader,writer}.rs` — already isolated
  but uses `zstd-sys` directly
- `crates/engine/src/search/segment.rs` — search segment files,
  mmap-backed
- `crates/engine/src/vector/{mmap,mmap_graph,hnsw}.rs` — mmap-backed
  vector indexes

The 62 `std::fs` usage sites identified in `wasm-cache-target-plan.md`
are the inventory. SP1 must categorize each as:

- "behind the trait" — gets routed through `StorageProvider`
- "test-only" — stays on `std::fs` but is gated to `#[cfg(test)]`
- "tooling-only" — stays on `std::fs` but is in cli/tools, not engine
  durable path

## Trait Sketch

This is the shape, not the final API. SP1 produces the actual
definitions and runs them past a reviewer.

```rust
pub trait StorageProvider: Send + Sync + 'static {
    fn capabilities(&self) -> StorageCapabilities;

    fn read_object(&self, key: &ObjectKey) -> StrataResult<Bytes>;
    fn read_range(&self, key: &ObjectKey, range: Range<u64>)
        -> StrataResult<Bytes>;

    fn write_object(&self, key: &ObjectKey, body: Bytes,
        condition: WriteCondition) -> StrataResult<ObjectMetadata>;

    fn delete(&self, key: &ObjectKey) -> StrataResult<()>;
    fn list(&self, prefix: &ObjectKey) -> StrataResult<Vec<ObjectMetadata>>;

    fn open_wal_sink(&self, path: &ObjectKey) -> StrataResult<Box<dyn WalSink>>;
    fn open_segment_reader(&self, key: &ObjectKey)
        -> StrataResult<Box<dyn SegmentReader>>;
}

pub struct StorageCapabilities {
    pub conditional_write: bool,   // If-Match / If-None-Match
    pub atomic_rename: bool,       // POSIX rename
    pub random_read: bool,         // range GET
    pub appendable_write: bool,    // truncate-aware append (FS only)
    pub mmap_local: bool,          // mmap-eligible local file
    pub durability_class: DurabilityClass,
}

pub enum DurabilityClass {
    Ephemeral,                     // memory, OPFS in some browsers
    SingleNode,                    // local FS with fsync
    ReplicatedDurable,             // S3, GCS, Azure
}
```

The engine's required-capability set for durable mode is:
`conditional_write && random_read && durability_class != Ephemeral`.
For cache mode: `random_read` only. SP4 enforces this at open time.

## Phasing

Seven phases, SP1 through SP7. Each phase is a contiguous PR sequence
landing under a single milestone.

### SP1 — Trait extraction with local-FS implementation

Goal: define `StorageProvider` and route all engine and storage IO
through it. The only implementation is a local-FS provider that uses
direct `std::fs` and `memmap2` (not OpenDAL yet — that is SP2).

This phase changes no behavior and no on-disk format. It is the
load-bearing refactor.

- Define `StorageProvider`, `WalSink`, `SegmentReader`, `SegmentWriter`,
  `ManifestStore`, `StorageCapabilities`
- Implement `LocalFsProvider` using current `std::fs` / `memmap2` /
  `fs2` code, moved behind the trait
- Cut over every site identified in the code map to use the trait
- Add golden-file tests for WAL, manifest, segment, checkpoint, and
  snapshot byte layouts (these tests outlive SP1 and protect every
  later phase)
- Add characterization benchmarks for the IO path before and after
  the cutover; both must run on the same dataset
- Update `engine-crate-map.md` and `storage-crate-map.md` to reflect
  the trait

Exit criteria: `cargo test --workspace` passes unchanged. Native
benchmarks within noise. Golden-file tests pin every format. The
trait is the only IO path; CI grep job fails the build if `std::fs`
appears in engine durable code outside the LocalFsProvider.

Assurance class: S4. Requires characterization tests, second reviewer,
benchmark report.

### SP2 — OpenDAL behind LocalFsProvider

Goal: replace LocalFsProvider's `std::fs` calls with
`opendal::services::Fs` calls (sync via `BlockingOperator`), proving
the OpenDAL indirection is acceptable on the hottest path.

- Add `opendal` dependency with the `services-fs` and `services-memory`
  features only (no remote services yet, no transitive bloat)
- Reimplement `LocalFsProvider` against OpenDAL
- Re-run characterization benchmarks; if regression exceeds noise,
  add an OpenDAL-bypass fast path for FS-specific operations
  (mmap, range reads) and re-measure
- Implement `MemoryProvider` against `opendal::services::Memory` for
  test fixtures and cache mode

Exit criteria: native benchmarks within noise. Golden-file tests
unchanged. The mmap fast path, if needed, is documented and
reviewed.

Assurance class: S4. Same protocol as SP1.

### SP3 — Async bridge for remote storage

Goal: introduce the WAL writer thread and async bridge architecture.
No remote provider yet — this phase only changes the WAL writer's
internal structure.

- Convert `WalWriter` to a dedicated thread that owns a tokio
  current-thread runtime
- Engine submits writes via a sync channel; the WAL writer batches
  and flushes via the trait's async API; ack returns via oneshot
- Group commit becomes load-bearing: a configurable batching window
  (microseconds for FS, milliseconds for remote) coalesces commits
- `WalWriterHealth` extends to expose batch latency and queue depth
- Native benchmarks measured: FS-backed WAL must remain within noise
  even with the channel hop, because group commit absorbs the
  overhead

Exit criteria: native FS benchmarks within noise. WAL writer thread
shutdown is clean (joins on `Database::shutdown`). The async runtime
is invisible to the public API (no tokio in any public signature).

Assurance class: S4. Touches commit ordering and durability — a
critical surface. Requires crash/restart tests and a second reviewer.

### SP4 — Capability negotiation and provider selection

Goal: open-time capability checking and provider configuration.

- `OpenSpec` learns to take a `StorageProvider` instance (default:
  `LocalFsProvider`)
- Open-time capability check: durable mode requires
  `conditional_write && random_read && !Ephemeral`; refuses with
  a typed error otherwise
- Configuration schema: provider selection via `StrataConfig`
  (URL-style: `s3://bucket/path`, `file:///path`, `memory://`,
  `indexed-db://name`)
- Validation runs at open time per CLAUDE.md rule 23

Exit criteria: passing wrong-capability provider to durable mode
returns a typed error before any IO. Configuration round-trips
through TOML and JSON. Documentation lists supported URL schemes.

Assurance class: S3 (open-time validation, fail-fast paths).

### SP5 — S3 provider live

Goal: working S3 backend with conditional-write manifest commit.
This is the marquee phase.

- `S3Provider` via `opendal::services::S3` with the async bridge
- Manifest commit uses conditional `PUT` with `If-Match` on the
  current manifest's etag; etag is propagated through the
  `ManifestStore` trait
- WAL writes batched per group-commit window; tunable batching
  parameters in `StrataConfig`
- Recovery on cold open: list manifest objects, pick the highest
  version, verify etag chain back to genesis manifest, replay WAL
  segments from the last checkpoint
- End-to-end tests against MinIO in CI; smoke tests against AWS S3
  on a nightly job
- Latency, throughput, and cost characterization documented
  (commits/sec at given batching window; GET/PUT/byte costs per
  YCSB workload)

Exit criteria: full conformance test suite passes against MinIO.
A killed writer mid-commit cannot corrupt the manifest chain
(verified by fault-injection tests). Cost characterization document
is reviewed.

Assurance class: S4. Crash/restart tests, fault-injection tests,
benchmark report, second reviewer.

### SP6 — Local-disk hot-segment cache

Goal: make S3-backed Strata performant enough to use. Without this
phase, every cold read is a 50-100ms GET and the workload becomes
unviable for interactive use.

- Cache subsystem under `crates/engine/src/storage_cache/`
- Read path: check local cache → fall back to provider → populate
  cache on miss
- Eviction tied to compaction events (segments superseded by
  compaction are demoted) and a configurable byte budget
- Cache invalidation on manifest version transitions
- Configurable on-disk cache directory; cache is itself stored via
  a `LocalFsProvider` (which is fine — the cache backing store and
  the durable provider are independent instances)
- Metrics: hit rate, byte hit rate, miss latency distribution
- The cache is correctness-relevant: a cached read returning stale
  bytes is a correctness bug, not a performance bug. Invariant tests
  check this.

Exit criteria: YCSB read workloads against MinIO are within 2x of
local-FS throughput on warm cache; cold cache is bounded by network
latency only. Manifest version transitions invalidate cache without
user-visible inconsistency (verified by version-transition tests).

Assurance class: S4. Cache correctness is on the durability path.

### SP7 — Wasm: IndexedDB and OPFS providers

Goal: close the loop with the wasm cache plan. Cache mode in a
browser tab can optionally persist to IndexedDB or OPFS.

- `IndexedDbProvider` via `opendal::services::IndexedDb`
- `OpfsProvider` if/when OpenDAL ships an OPFS service; otherwise
  cfg-gated to a future phase
- Wasm-target capability negotiation: IndexedDB-backed cache becomes
  durable-class within the browser session; cross-session durability
  depends on browser eviction policies (documented, not promised)
- The wasm test harness from W5 grows a persistence test:
  open → write → close → reopen → verify

Exit criteria: a browser tab can persist a cache database across
reloads via IndexedDB. The wasm test passes under
`wasm-bindgen-test --headless --chrome`.

Assurance class: S3. Browser durability is best-effort by browser
contract; the engine's correctness obligations are bounded by what
the browser guarantees.

## Risks and Open Questions

- **OpenDAL FS performance overhead.** OpenDAL's `BlockingOperator`
  adds an async-to-sync bridge inside the call. On local FS the
  per-op overhead is small but non-zero. SP2's benchmarks settle
  whether a bypass fast path is needed; if it is, the fast path
  must be small and well-isolated, not a parallel implementation.

- **OpenDAL version churn.** OpenDAL's API has changed between
  major versions. Pinning a specific version and treating upgrades
  as a maintenance project (not implicit) is the correct posture.
  The S3-compatible service surface is the most stable; browser
  services are newer and may evolve.

- **Conditional-write semantics across S3-compatibles.** AWS S3
  supports `If-Match` on PUT only as of late 2024. Some
  S3-compatible services (especially older or non-AWS
  implementations) may not. SP4's capability check is the gate;
  an S3-compatible service that does not support conditional
  writes is a cache-only provider.

- **WAL group-commit sizing.** The right batching window is
  workload-dependent and provider-dependent. FS wants microseconds;
  S3 wants tens of milliseconds. Defaults must auto-tune from the
  provider's declared latency class, with explicit override.

- **Cost surprises.** A naive port can rack up significant S3 PUT
  bills under chatty commit workloads. SP5's cost characterization
  is the input to default tuning. The configuration schema must
  let users trade durability for cost (e.g., "commit every 100ms"
  vs "commit on every transaction").

- **Cache poisoning across restarts.** SP6's cache lives across
  process restarts. Manifest version checks at startup must
  validate that cached segments still belong to a live manifest;
  otherwise a stale cache could serve bytes from a compacted-out
  segment.

- **IndexedDB transactional semantics vs Strata's transactions.**
  IndexedDB has its own transaction model. SP7 must verify that
  Strata's commit ordering is preserved when the underlying store
  is IndexedDB; in particular, manifest-then-WAL ordering must
  not be reordered by the browser.

- **Zstd on wasm.** The wasm plan resolves zstd by either swapping
  to `ruzstd` or cfg-gating bundle code. SP7 inherits whatever
  decision lands there.

## Acceptance

Per CLAUDE.md's non-regression protocol:

- **Change class:** intentional semantic change at the platform
  layer (new deployment targets), refactor-only at the engine
  layer (no on-disk format change, no public API change beyond
  optional configuration knobs).
- **Assurance class:** S4 overall. Every phase from SP1 through
  SP6 touches the durability path.
- **Benchmarks:** full regression suite (`redb`, `ycsb_compare`,
  `beir`) on local FS at every phase, with results reviewed
  before merge. Remote-storage benchmarks are added in SP5 with
  separate baselines (latency-class workloads, not direct
  throughput comparisons).
- **Crash/restart tests:** required at SP1, SP3, SP5, SP6 per
  S4 protocol.
- **Second reviewer:** required for every S4 phase.
- **Documentation:** at SP4, the supported provider matrix and
  capability table are checked into `docs/storage/`. At SP5,
  the cost/latency characterization document is reviewed.

## Out of Scope, For Reference

- Multi-writer / multi-region active-active deployments
- Geo-replication and cross-region read replicas beyond today's
  follower mode
- Server-side transformation (e.g. S3 Object Lambda)
- Encryption-at-rest beyond what OpenDAL services already provide;
  application-level KMS integration is a separate workstream
- Compaction scheduling policies tuned for cost (vs latency); the
  cost characterization at SP5 is the input to that future work
- A managed cloud service offering. This plan is about making
  Strata deployable against object storage; running it as a
  service is its own product decision.

These should each get their own plan once the storage portability
work is stable.
