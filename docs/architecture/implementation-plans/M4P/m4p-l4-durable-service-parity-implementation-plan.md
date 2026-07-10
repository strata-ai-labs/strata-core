# M4P-L4 Implementation Plan: Durable Service Parity

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4p-storage-next-parity-restoration-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4P/m4p-l4-durable-service-parity-test-plan.md`

## Objective

Close the L4 durable-service parity gaps without changing the L1-L9
architecture or moving lifecycle policy into service code.

L4 is not missing wholesale. Storage-next already has durable services for WAL,
manifests, snapshots, checkpointing, immutable table objects, WAL sidecars, and
quarantine. The remaining work is to make those services production-grade
against old-storage fault windows:

1. preserve delete durability and already-missing facts through cleanup reports;
2. prove the changed durable topology across crash/restart windows;
3. document every implemented manifest/service role;
4. add reusable durable-service conformance instead of relying only on
   service-local tests;
5. record the object-durable fencing decision before any future object-store
   mode is accepted.

This is a correctness and proof slice. It should not add table-read fast paths,
LSM pruning, compaction scoring, checkpoint cadence, public L9 APIs, or
object-store support.

## Audit Finding References

Primary audit source:
`docs/architecture/perf-tuning/storage-next-mechanics-parity-audit.md`

Relevant sections:

1. `L4. Log / Manifest / Snapshot Services`
2. `Audit Matrix / Durability, Manifest, WAL, And Recovery Mechanics`
3. `Final Parity Matrix And Architecture-Aligned Gap Plan`
4. `GAP-L4/L8: close durable topology and recovery parity`

Findings covered by this plan:

1. L4 cleanup paths need typed reports that distinguish deleted,
   already-missing, failed, and delete-durability-uncertain objects.
2. L4 architecture documentation has drifted behind implemented manifest
   services.
3. L4 conformance is broad but mostly service-local.
4. WAL durability-policy behavior must remain explicit as modes evolve.
5. Object-store fencing is deferred for V1, but must be documented as a hard
   gate before object-durable mode.
6. Storage-next changed the durable topology source of truth from old
   branch-local `segments.manifest` files to database manifest, branch catalog,
   per-branch table manifests, table-object facts, WAL, checkpoint rows, and
   quarantine facts. That topology needs direct restart proof.

Current-status correction to the audit:

1. The older audit said L1 lacked delete outcome/durability facts. That is no
   longer true. `crates/storage-next/src/backend/delete.rs` now exposes
   `DeleteStatus`, `DeleteDurability`, `DeleteOutcome`, and delete failure
   windows. The remaining L4 work is to preserve and enforce those facts in
   service reports.
2. The older audit said table-object reference recovery was rejected. Current
   recovery now routes table manifests through
   `crates/storage-next/src/lifecycle/table_manifest.rs` and opens table
   objects through `TableObjectReaderService`. The remaining gap is restart and
   fault-window proof across the full publication topology, not a simple missing
   table-reference implementation.

Supporting architecture:

1. `docs/architecture/storage/l1-backend-io.md`
2. `docs/architecture/storage/l2-object-layout.md`
3. `docs/architecture/storage/l3-durable-format-codec.md`
4. `docs/architecture/storage/l4-log-manifest-snapshot-services.md`
5. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
6. `docs/architecture/storage/target-crate-shape-and-test-harness.md`
7. `docs/spec/strata-storage-format-v1.md`

## Predecessors

Required:

1. M4P-L1 durable-delete outcome vocabulary exists in L1.
2. M4P-L2 object-role helpers exist for object families used by L4/L8 cleanup
   and recovery.
3. M4P-L3 durable format helpers and specs exist for manifest, snapshot row,
   retained-history extension, branch catalog, pending releases, table, WAL,
   and sidecar bytes.

Useful but not required before L4A:

1. L5/L6/L8 serving-path counters and source-shape work.
2. L9 diagnostics API work.

L4 may add small lifecycle test harness hooks when restart proof requires them,
but durable-service behavior remains owned by L4.

## Layer Ownership Check

L4 owns durable service mechanics:

1. publish durable objects through L1 backend primitives;
2. create, replace, load, list, append, repair, and delete service-owned object
   families;
3. map backend/layout/format errors into typed service errors;
4. classify publish and cleanup failure windows;
5. expose service facts needed by L8 recovery, retention, checkpoint, and
   maintenance.

L4 does not own:

1. backend implementation details, filesystem paths, or direct fsync calls
   above L1;
2. object-name grammar beyond consuming L2 helpers;
3. durable byte layout beyond consuming L3 codecs;
4. table building, table seek, table cursor, or compaction algorithms;
5. branch visibility, inherited-layer semantics, materialization policy, or
   retention policy;
6. commit validation, version allocation, WAL-before-visible admission, or
   public API behavior;
7. checkpoint cadence, maintenance scheduling, or branch delete/clear policy.

## Existing-Code Source Map

| Current file | Current behavior | L4 action |
| --- | --- | --- |
| `crates/storage-next/src/service/publish.rs` | Central durable/non-durable object publication service. | Keep as L4 publication core; add fenced-publish decision documentation, not implementation, unless object-durable mode is enabled. |
| `crates/storage-next/src/service/wal.rs` | WAL open/create/append/sync/rotate/read/repair/delete. | Preserve delete durability facts in `WalDeleteReport`; keep active-segment protection and sidecar cleanup best-effort. |
| `crates/storage-next/src/service/manifest.rs` | Database, table, branch catalog, and pending releases manifest services. | Update L4 docs; add conformance coverage for each role. |
| `crates/storage-next/src/service/snapshot.rs` and `snapshot/listing.rs` | Snapshot publish/load/list/prune mechanics. | Preserve delete outcome/durability in prune reports; prove live/newest protection and malformed-object rejection. |
| `crates/storage-next/src/service/checkpoint.rs` | Mechanical checkpoint publication sequencing. | Add restart/fault proof for active-WAL manifest, snapshot, final manifest, and optional table-manifest records. |
| `crates/storage-next/src/service/table.rs` | Immutable table object publication and object-backed reader handoff. | Conformance coverage for publish/open/fact validation; restart proof through L8 table-manifest recovery. |
| `crates/storage-next/src/service/sidecar.rs` | Optional WAL metadata sidecar publish/load/delete. | Preserve delete status/durability as optional cleanup facts without failing authoritative WAL cleanup. |
| `crates/storage-next/src/service/quarantine.rs` and submodules | Quarantine inventory, object mutation, purge, and reconciliation. | Preserve source/quarantine delete outcome facts and prove inventory rewrite retry behavior. |
| `crates/storage-next/src/service/cache_mode_absence_tests.rs` | Durable L4 services reject cache backends before durable mutation. | Expand conformance coverage if new L4 paths are added. |
| `crates/storage-next/src/lifecycle/table_manifest.rs` | L8 consumer of L4 table manifest and table-object reader services. | Use as restart-proof target; do not move recovery policy into L4. |
| `crates/storage-next/src/lifecycle/rewrite_publication.rs` | Durable rewrite publication and table-manifest debt classification. | Prove restart behavior for manifest-debt windows; keep branch install policy in L8/L6. |
| `crates/storage-next/src/lifecycle/checkpoint.rs` | L8 checkpoint retention and WAL truncation consumer. | Prove checkpoint/restart and flush-watermark retention windows through L4 services. |
| `crates/storage-next/src/lifecycle/quarantine.rs` | L8 orchestration around L4 quarantine service. | Prove active, deleted, forked, materialized, and inherited branches participate in recovery/reconciliation. |

## Old-Code Source Map

Old storage evidence to preserve as invariants, not implementation shape:

| Old source | Behavior to preserve | Storage-next target |
| --- | --- | --- |
| `crates/storage/src/durability/wal/mod.rs`, `writer.rs`, `reader.rs`, `config.rs`, `mode.rs` | WAL segment creation, append, sync policy, replay, repair, rotation, and safe deletion. | `service/wal.rs`; L7/L8 call it but do not own WAL mechanics. |
| `crates/storage/src/durability/format/wal_record.rs`, `segment_meta.rs`, `watermark.rs`, and `crates/storage/src/durability/payload.rs` | WAL and sidecar service bytes consumed by L4-like services. | L3 codecs plus L4 WAL/sidecar services. |
| `crates/storage/src/durability/format/manifest.rs`, `crates/storage/src/manifest.rs`, `crates/storage/src/durability/commit_adapter.rs` | Manifest loading, durable publication, and commit-visible durability boundary. | `service/manifest.rs`; L7 owns commit visibility; L4 owns durable manifest mechanics. |
| `crates/storage/src/durability/checkpoint_runtime.rs` | Checkpoint order: manifest active-WAL fact, snapshot, final manifest watermark; WAL deletion after durable proof. | `service/checkpoint.rs`, `lifecycle/checkpoint.rs`, `service/wal.rs`. |
| `crates/storage/src/durability/disk_snapshot/*` and `format/snapshot.rs` | Snapshot temp/publish/read/list/prune mechanics and corruption classification. | `service/snapshot.rs` and `service/snapshot/listing.rs`. |
| `crates/storage/src/segment_builder.rs`, `segmented/mod.rs`, `segmented/compaction.rs`, `segmented/tests/publish_failures.rs` | Table object publish, branch manifest publish, and failure-window health. | `service/table.rs`, `service/manifest.rs`, `lifecycle/rewrite_publication.rs`, `lifecycle/table_manifest.rs`. |
| `crates/storage/src/quarantine.rs` and `segmented/quarantine_protocol.rs` | Quarantine copy-before-delete, inventory rewrite, purge retry, and cleanup safety. | `service/quarantine.rs` and `lifecycle/quarantine.rs`. |
| `crates/storage/src/durability/recovery.rs`, `recovery_bootstrap.rs`, `segmented/recovery.rs` | Restart classifies durable objects, manifests, sidecars, and branch topology. | L4 services plus L8 recovery orchestration. |

Do not port:

1. direct filesystem paths or temp-file sequences above L1;
2. old manifest byte formats;
3. old primitive snapshot DTOs;
4. old mixed lifecycle policy from `checkpoint_runtime.rs`;
5. object-store conditional fencing before object-durable mode is accepted;
6. table serving/compaction mechanics into L4.

## Scope

M4P-L4 implements:

1. L4 service documentation parity for every implemented service role;
2. delete outcome and durability propagation in L4 cleanup reports;
3. reusable durable-service conformance harnesses for L4 service families;
4. crash/fault/restart proof for table publication, table manifest publication,
   rewrite publication, checkpoint, WAL retention, snapshot pruning,
   quarantine, branch clear/delete cleanup, and purge windows touched by L4;
5. source guards that prevent direct backend/L2/L3 bypasses in higher layers
   where L4 service use is required;
6. a recorded object-durable fencing decision and stop condition.

M4P-L4 does not implement:

1. new storage object formats;
2. table seek/cursor/read-path performance fixes;
3. LSM level-shape or compaction scheduling changes;
4. public L9 diagnostics or maintenance APIs;
5. object-store support;
6. background WAL sync scheduling unless a later L8 policy slice requires a
   narrow L4 force/sync proof;
7. branch policy, retention policy, or checkpoint cadence.

## Execution Plan

The plan is split into executable sub-slices. `M4P-L4A` is the first slice from
the parent M4P queue. Later sub-slices can be implemented as separate commits.

### M4P-L4A. Publication And Recovery Window Inventory

Goal: create the proof map before changing service behavior.

Steps:

1. Inventory every L4 service transition that can leave durable state in an
   intermediate state:
   - WAL segment create;
   - WAL append with `standard` and `always`;
   - WAL repair by durable replace;
   - WAL retention delete and optional sidecar delete;
   - database manifest create/replace;
   - table manifest replace;
   - branch catalog manifest replace;
   - pending releases manifest replace;
   - snapshot publish/load/prune;
   - checkpoint active-WAL fact, snapshot, final manifest, and optional
     table-manifest record;
   - table object publish/open;
   - rewrite publication and table-manifest debt;
   - quarantine publish, source delete, inventory rewrite, reconcile, purge.
2. For each transition, record:
   - authoritative object;
   - optional objects;
   - required lower-layer capabilities;
   - visible-but-not-durable window;
   - already-missing behavior;
   - recovery classification;
   - owning test suite.
3. Add or update docs in `l4-log-manifest-snapshot-services.md` so the
   architecture doc names every implemented service role.
4. Mark which transitions are purely L4 and which require L8 restart proof.

Exit gate:

1. the L4 architecture doc no longer lags implemented service families;
2. the test plan has a transition matrix with one owner per transition;
3. no Rust behavior changes are made unless the inventory exposes an obvious
   missing assertion.

Stop condition:

If the inventory shows a durable byte format gap, stop and create an M4P-L3
decision before continuing.

### M4P-L4B. Cleanup Report And Delete-Durability Propagation

Goal: make L4 cleanup reports preserve L1 delete facts.

Steps:

1. Define an L4 cleanup fact shape that can be reused or mirrored by service
   reports:
   - object name;
   - delete status: deleted or already missing;
   - delete durability: durable or non-durable;
   - failure kind/source when deletion fails;
   - authoritative vs optional object classification.
2. Update WAL retention:
   - replace segment-id-only deleted/failed facts with typed per-object facts,
     while preserving segment-id helpers for existing callers if needed;
   - treat non-durable delete outcomes on durable WAL objects as failed or
     durability-uncertain, not successful durable cleanup;
   - keep optional sidecar cleanup best-effort.
3. Update snapshot pruning:
   - preserve per-snapshot delete outcome/durability;
   - keep live and newest-retained protection unchanged;
   - fail or report uncertainty for non-durable delete outcomes on durable
     snapshot objects.
4. Update sidecar cleanup:
   - include delete status and durability in `WalSegmentMetadataSidecarDelete`;
   - keep sidecar cleanup optional and non-authoritative.
5. Update quarantine cleanup:
   - preserve delete status and durability for quarantine object purge;
   - preserve source-delete outcome and durability after quarantine publish;
   - keep retry inventory semantics for failed or durability-uncertain deletes.
6. Update lifecycle projections of cleanup reports without hiding new facts.

Exit gate:

1. every durable L4 cleanup path distinguishes deleted, already missing,
   failed-before-removal, removal-unknown, and
   removed-durability-unconfirmed where L1 exposes those facts;
2. durable services do not count non-durable delete outcomes as durable cleanup;
3. existing high-level behavior remains compatible through helper accessors or
   documented changes.

Stop condition:

If report changes would force public L9 API shape changes, stop and split the
public exposure into an M4P-L9 diagnostics slice. L4 may keep crate-private
facts.

### M4P-L4C. Durable Service Conformance Harness

Goal: consolidate service-local tests into reusable L4 conformance.

Steps:

1. Add a service conformance module under either:
   - `crates/storage-next/src/service/conformance.rs`, for crate-private unit
     conformance; or
   - `crates/storage-next/tests/service_conformance.rs`, for testkit-backed
     backend selection.
2. Build fixtures that can run against memory/cache and localfs with expected
   capability differences:
   - memory must reject durable service mutation before object-family mutation;
   - localfs must satisfy durable publish/sync/delete where configured;
   - faulting wrappers must classify publish/delete windows.
3. Cover service families:
   - manifest create/replace/load/list where applicable;
   - WAL append/read/repair/delete;
   - snapshot publish/load/list/prune;
   - checkpoint publication order;
   - table publish/open/fact validation;
   - sidecar present/missing/corrupt/delete;
   - quarantine inventory/mutation/reconcile/purge;
   - cache-mode absence.
4. Keep conformance assertions service-level. Do not assert L8 policy decisions
   except where a service explicitly rejects unsupported capability.

Exit gate:

1. one command can run the core L4 service conformance suite;
2. localfs and memory/cache expected differences are explicit;
3. conformance catches missing capability preflight and publish/delete fact
   collapse.

### M4P-L4D. Restart Proof For Durable Topology

Goal: prove storage-next's changed durable topology is correct after restart.

Steps:

1. Add restart harness cases for table-object and table-manifest topology:
   - table object published, table manifest absent;
   - table object published, table manifest publication uncertain;
   - table manifest names missing table object;
   - table manifest names corrupt table object;
   - table manifest names object with mismatched facts;
   - table manifest recovered into active branch;
   - inherited layer manifest recovered without deleting shared table objects.
2. Add rewrite publication restart cases:
   - compaction output published before branch install;
   - compaction output installed before table manifest publication;
   - materialization replacement output with manifest debt;
   - retry after manifest debt records consistent table facts.
3. Add checkpoint/restart cases:
   - active-WAL fact published, snapshot absent;
   - snapshot published, final manifest absent;
   - final manifest points to missing snapshot;
   - final manifest points to corrupt snapshot;
   - table-manifest flush proof present or incomplete;
   - WAL truncation after durable flush proof.
4. Add quarantine/recovery cases:
   - quarantine object published, source delete failed;
   - source deleted, inventory rewrite failed;
   - purge deleted some objects and failed others;
   - all active/deleted/forked/materialized/inherited branches participate in
     reachability and quarantine reconciliation where applicable.
5. Add branch clear/delete cleanup cases only to the extent they exercise L4
   object cleanup and durable service facts. Branch lifecycle policy remains
   L8/L9.

Exit gate:

1. restart never loses reachable rows;
2. restart never resurrects unmanifested table objects into reads;
3. orphaned, missing, corrupt, quarantined, and optional objects are classified
   distinctly;
4. WAL retention never deletes active or uncovered segments;
5. table objects reachable from owned or inherited manifests are not deleted.

Stop condition:

If restart proof requires changing branch visibility, compaction install, or
maintenance scheduling, stop and split that work into L6/L8 slices. L4 should
only change service facts and service mechanics.

### M4P-L4E. WAL Policy And Close/Sync Proof

Goal: keep WAL durability policy behavior explicit as modes evolve.

Steps:

1. Reconfirm `DurabilityPolicy::Standard` and `DurabilityPolicy::Always`
   semantics in `WalService` tests:
   - `always` forces durability before append success;
   - `standard` may report dirty WAL state until force/close/maintenance;
   - neither policy changes record visibility ordering required by L7.
2. Add close/force tests for dirty WAL state if existing coverage is only in
   lifecycle code.
3. Record that background sync scheduling is L8 policy unless a future plan
   adds an L4 background-sync API.
4. Keep cache mode WAL-free.

Exit gate:

1. WAL policy tests fail if `always` stops forcing durability;
2. `standard` and `always` replay the same complete committed records;
3. cache mode still cannot create durable WAL objects.

### M4P-L4F. Object-Durable Fencing Decision

Goal: keep object-durable mode blocked until L4 has publish fencing semantics.

Steps:

1. Add a decision section to `future-object-durable-guardrails.md` or the L4
   doc:
   - durable-local V1 can rely on single-writer localfs locking;
   - object-durable mode needs conditional publish/generation fencing;
   - manifest, checkpoint, table, snapshot, WAL, and quarantine services must
     use those fences before mode admission.
2. Add source/API guard tests that rejected object-durable modes stay rejected.
3. Do not implement compare-and-swap publish in this slice unless a separate
   object-durable plan accepts that scope.

Exit gate:

1. object-durable mode remains rejected before runtime construction;
2. docs state exactly what L1/L4 fencing is required before enabling it.

## Rollout Order

Recommended order:

1. `M4P-L4A`: inventory + docs + transition matrix.
2. `M4P-L4B`: delete durability propagation.
3. `M4P-L4C`: service conformance harness.
4. `M4P-L4D`: restart proof for changed durable topology.
5. `M4P-L4E`: WAL policy closeout.
6. `M4P-L4F`: object-durable fencing decision.

`M4P-L4D` may be split by transition family if the test surface becomes too
large:

1. table manifest/table object;
2. rewrite publication;
3. checkpoint/WAL retention;
4. quarantine/reclaim/purge.

## Source Guards

Add or extend source guards so:

1. L5/L6/L7/L8 do not publish, append, delete, or list durable L4 object
   families directly when an L4 service exists;
2. L4 services do not parse canonical object names through raw string slicing
   when L2 role helpers exist;
3. L4 services do not reimplement L3 durable bytes;
4. cache-mode code does not import durable manifest, WAL, snapshot, table
   object, checkpoint, sidecar, or quarantine mutation services except in
   absence tests;
5. production code outside L1 does not use filesystem APIs.

Source guards should be targeted. Do not ban all `delete_object` or
`publish_object` calls globally until adapters are in place; first identify the
service-owned object families and the allowed files.

## Documentation Updates

Update:

1. `docs/architecture/storage/l4-log-manifest-snapshot-services.md`;
2. `docs/architecture/storage/future-object-durable-guardrails.md` if
   fencing decision text belongs there;
3. `docs/architecture/storage/target-crate-shape-and-test-harness.md` if
   a new conformance harness is added;
4. M4P README index after plans are written.

Documentation must include:

1. all manifest roles: database, table, branch catalog, pending releases;
2. durable cleanup report semantics;
3. optional sidecar policy;
4. cache-mode absence expectations;
5. restart proof ownership between L4 and L8;
6. object-durable fencing stop condition.

## Verification Commands

Minimum commands after implementation:

```sh
cargo fmt -p strata-storage-next
cargo test -p strata-storage-next --locked --lib service
cargo test -p strata-storage-next --locked --lib lifecycle::tests::recovery
cargo test -p strata-storage-next --locked --lib lifecycle::tests::table_manifest_recovery
cargo test -p strata-storage-next --locked --lib lifecycle::tests::checkpoint
cargo test -p strata-storage-next --locked --lib lifecycle::tests::retention
cargo test -p strata-storage-next --locked --lib lifecycle::tests::quarantine
cargo test -p strata-storage-next --locked --test format_layer_source_guard
cargo test -p strata-storage-next --locked --test table_format_source_guard
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo clippy -p strata-storage-next --lib --features perf-trace -- -D warnings
cargo check -p strata-storage-next --no-default-features --target wasm32-unknown-unknown --all-targets --locked
git diff --check
```

Add conformance-specific commands once `M4P-L4C` creates the harness.

## Benchmark Gate

M4P-L4 is not expected to materially improve point-read, scan, or load
throughput. Its benchmark gate is regression-only:

1. run the current 100K storage-next profile after L4B/L4D if behavior changes
   touch normal write/flush/recovery paths;
2. compare against the latest stored trace only to catch obvious regressions;
3. do not block L4 on closing the known 2x+ serving-path gap, which remains
   owned by L5/L6/L8.

## Exit Criteria

M4P-L4 is complete when:

1. L4 docs match implemented service families;
2. cleanup reports preserve delete status, delete durability, and failure
   windows for authoritative and optional objects;
3. reusable service conformance exists and runs against expected backend
   capability profiles;
4. crash/restart proof covers the durable topology transition families listed
   in `M4P-L4D`;
5. WAL policy tests prove `standard`, `always`, and cache-mode behavior;
6. object-durable mode remains blocked with documented fencing prerequisites;
7. source guards prevent new L4 bypasses without blocking legitimate lower-layer
   tests;
8. all verification commands pass.
