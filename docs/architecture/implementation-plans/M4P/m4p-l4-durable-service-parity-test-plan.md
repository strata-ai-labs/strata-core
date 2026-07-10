# M4P-L4 Test Plan: Durable Service Parity

Status: draft

Companion implementation plan:
`docs/architecture/implementation-plans/M4P/m4p-l4-durable-service-parity-implementation-plan.md`

## Goal

Prove that storage-next L4 durable services provide the same safety envelope as the
old durable service layer while preserving the L1-L9 architecture boundaries.

The test suite should not prove performance through special fast paths. It should
prove that ordinary L4 publication, loading, cleanup, and recovery behavior is
correct under normal operation, cache-mode absence, malformed durable state, and
crash-window simulations.

## Audit Inputs

This plan must be checked against the audit findings before implementation starts:

- `docs/architecture/perf-tuning/storage-next-mechanics-parity-audit.md`
  - `L4. Log / Manifest / Snapshot Services`
  - `Audit Matrix / Durability, Manifest, WAL, And Recovery Mechanics`
  - `Final Parity Matrix And Architecture-Aligned Gap Plan`
  - `GAP-L4/L8: Recovery And Reference Accounting`
- `docs/architecture/implementation-plans/m4p-storage-next-parity-restoration-implementation-plan.md`
- `docs/architecture/implementation-plans/M4P/m4p-l1-backend-io-parity-implementation-plan.md`
- `docs/architecture/implementation-plans/M4P/m4p-l2-object-layout-parity-implementation-plan.md`
- `docs/architecture/implementation-plans/M4P/m4p-l3-durable-format-parity-implementation-plan.md`

Current-status correction: L1 durable delete outcomes now exist. L4 tests must
verify that services preserve and act on those outcomes instead of treating the
original audit as still describing a missing L1 primitive.

## Coverage Boundary

In scope:

- L4 durable publication and load behavior for WAL segments, manifests,
  snapshots, checkpoints, table objects, sidecars, and quarantine objects.
- L4 cleanup reports that preserve object id, object family, deletion status,
  deletion durability, and failure reason.
- Capability preflight for flows that require durable visibility.
- Failure windows around publish, replace, prune, quarantine, and delete
  operations.
- Restart proof for the storage-next durable topology.
- Cache-mode absence behavior for all durable service families.
- Source guards that prevent L4 bypasses and format reimplementation.

Out of scope:

- L5 format encoding semantics beyond using L5 parsers to validate L4 I/O.
- L6 manifest planning and reference graph policy.
- L7 compaction policy.
- L8 recovery orchestration decisions beyond the durable facts L4 returns.
- L9 public API behavior.
- Benchmark tuning or benchmark-only fast paths.

## Old-Code Regression Sources

Use these old implementation files as behavior references when writing parity
tests:

- `crates/storage/src/durability/wal/mod.rs`
- `crates/storage/src/durability/wal/writer.rs`
- `crates/storage/src/durability/wal/reader.rs`
- `crates/storage/src/durability/wal/config.rs`
- `crates/storage/src/durability/wal/mode.rs`
- `crates/storage/src/durability/format/wal_record.rs`
- `crates/storage/src/durability/format/segment_meta.rs`
- `crates/storage/src/durability/format/watermark.rs`
- `crates/storage/src/durability/payload.rs`
- `crates/storage/src/durability/format/manifest.rs`
- `crates/storage/src/manifest.rs`
- `crates/storage/src/durability/commit_adapter.rs`
- `crates/storage/src/durability/checkpoint_runtime.rs`
- `crates/storage/src/durability/disk_snapshot/mod.rs`
- `crates/storage/src/durability/disk_snapshot/writer.rs`
- `crates/storage/src/durability/disk_snapshot/reader.rs`
- `crates/storage/src/segment_builder.rs`
- `crates/storage/src/segmented/mod.rs`
- `crates/storage/src/segmented/compaction.rs`
- `crates/storage/src/segmented/tests/publish_failures.rs`
- `crates/storage/src/quarantine.rs`
- `crates/storage/src/segmented/quarantine_protocol.rs`
- `crates/storage/src/durability/recovery.rs`
- `crates/storage/src/durability/recovery_bootstrap.rs`
- `crates/storage/src/segmented/recovery.rs`

The tests should not copy old internals wholesale. They should preserve old
observable behavior where that behavior belongs in L4: durable publication order,
safe cleanup after partial failure, idempotent missing-object cleanup, and
recoverable object families.

## New Test Locations

Prefer colocated unit tests for narrow service behavior and lifecycle tests for
multi-service restart windows:

- `crates/storage-next/src/service/wal.rs`
- `crates/storage-next/src/service/manifest.rs`
- `crates/storage-next/src/service/snapshot.rs`
- `crates/storage-next/src/service/checkpoint.rs`
- `crates/storage-next/src/service/table.rs`
- `crates/storage-next/src/service/sidecar.rs`
- `crates/storage-next/src/service/quarantine.rs`
- `crates/storage-next/src/service/cache_mode_absence_tests.rs`
- `crates/storage-next/src/lifecycle/tests/`
- `crates/storage-next/tests/service_fault_windows.rs`

If multiple service families need the same matrix, add a reusable test harness
under `crates/storage-next/tests/` or `crates/storage-next/src/service/tests/`
instead of duplicating backend setup and fault injection.

## L4A Transition Ownership Matrix

The architecture inventory in
`docs/architecture/storage/l4-log-manifest-snapshot-services.md` is the
canonical transition map. This matrix names the test owner for each transition
and the L4 sub-slice that must close any remaining gap.

| Transition | Current test owner | Closing slice |
| --- | --- | --- |
| Durable publisher create/replace | `crates/storage-next/src/service/publish.rs` tests | L4C conformance expands backend matrix |
| WAL segment create/open | `crates/storage-next/src/service/wal/tests/append.rs`, `localfs.rs`, `read.rs` | L4E |
| WAL append with `standard` policy | `crates/storage-next/src/service/wal/tests/append.rs`, `durability.rs`, `read.rs`, `corruption.rs` | L4E |
| WAL append with `always` policy | `crates/storage-next/src/service/wal/tests/durability.rs`, `fault_windows.rs` | L4E |
| WAL latest-tail repair | `crates/storage-next/src/service/wal/tests/retention_reopen.rs` | L4E |
| WAL retention delete | `crates/storage-next/src/service/wal/tests/retention_reopen.rs` | L4B |
| Database manifest create/replace | `crates/storage-next/src/service/manifest.rs` tests, `crates/storage-next/src/lifecycle/tests/durable.rs` | L4C |
| Table manifest replace | `crates/storage-next/src/service/manifest.rs` tests, `crates/storage-next/src/lifecycle/tests/table_manifest_recovery.rs` | L4D |
| Branch catalog manifest replace | `crates/storage-next/src/service/manifest.rs` tests, `crates/storage-next/src/lifecycle/tests/branch_lifecycle/catalog.rs` | L4D |
| Pending releases manifest replace | `crates/storage-next/src/service/manifest.rs` tests; add lifecycle owner when restart windows are implemented | L4D |
| Snapshot publish | `crates/storage-next/src/service/snapshot/publish_load_tests.rs`, `publish_fault_tests.rs` | L4D |
| Snapshot load/list/latest | `crates/storage-next/src/service/snapshot/listing_tests.rs`, `listing_property_tests.rs` | L4D |
| Snapshot prune | `crates/storage-next/src/service/snapshot/listing_tests.rs` | L4B |
| Checkpoint active-WAL fact | `crates/storage-next/src/service/checkpoint/tests/sequencing.rs`, `crates/storage-next/src/lifecycle/tests/checkpoint.rs` | L4D |
| Checkpoint snapshot/final manifest | `crates/storage-next/src/service/checkpoint/tests/sequencing.rs`, `crates/storage-next/src/lifecycle/tests/checkpoint/*` | L4D |
| Table object publish/open | `crates/storage-next/src/service/table.rs` tests, `crates/storage-next/src/lifecycle/tests/table_manifest_recovery.rs` | L4D |
| Rewrite publication and table-manifest debt | `crates/storage-next/src/lifecycle/rewrite_publication.rs` tests, `crates/storage-next/src/lifecycle/tests/compaction/*` | L4D |
| WAL sidecar publish/load/delete | `crates/storage-next/src/service/sidecar/tests/*`, WAL retention tests | L4B |
| Quarantine inventory publish | `crates/storage-next/src/service/quarantine/tests/inventory.rs`, `mutation.rs`, `reconcile.rs` | L4D |
| Quarantine source copy/delete | `crates/storage-next/src/service/quarantine/tests/mutation.rs`, `crates/storage-next/src/lifecycle/tests/quarantine.rs` | L4B and L4D |
| Quarantine reconcile/purge | `crates/storage-next/src/service/quarantine/tests/reconcile.rs`, `crates/storage-next/src/lifecycle/tests/quarantine.rs` | L4B and L4D |
| Cache-mode durable service absence | `crates/storage-next/src/service/cache_mode_absence_tests.rs`, `crates/storage-next/src/lifecycle/tests/cache.rs` | L4C and L4F |

No transition may advance to behavior changes without a matching owner row here
or an explicit plan update that explains why the transition moved to another
layer.

## L4A Test Pass: Publication And Recovery Inventory

This pass is mostly inspection plus targeted coverage for already-implemented
paths.

Required tests:

- Every L4 durable object family has at least one publish-load-roundtrip test.
- Every durable object family has a missing-object test that returns the expected
  typed error or empty result.
- Every durable object family rejects cache-mode publication through L4, not by
  accidentally falling into an in-memory default.
- Table-object service tests prove that the service validates durable bytes using
  L5 parsing before returning a usable object.
- Manifest service tests cover database manifest, branch catalog, table manifest,
  pending-release manifest, checkpoint manifest, and snapshot manifest shapes.

Exit condition:

- A source map exists in the implementation plan.
- Each source-map row either has a named test or an explicit later L4 pass.

## L4B Test Pass: Cleanup Report And Delete-Durability Propagation

This is the highest-risk L4 behavior because L1 now exposes stronger delete
facts than most existing L4 reports return.

Required tests:

- WAL cleanup reports include segment id, object path or object name, deletion
  status, deletion durability, and optional error.
- Snapshot prune reports include snapshot id, manifest object, data objects,
  deletion status, deletion durability, and optional error.
- Checkpoint cleanup reports preserve each object deletion outcome.
- Sidecar cleanup reports distinguish required sidecar failure from optional
  sidecar absence.
- Quarantine purge reports distinguish quarantine-copy deletion from source-object
  deletion.
- Missing object deletion is idempotent and reported as already absent, not as a
  hard failure.
- Durable delete unsupported by the backend is not reported as durable success.
- A partial cleanup failure preserves enough facts for a retry to target only
  the unresolved objects.

Negative tests:

- A service must not collapse a non-durable delete into a successful durable
  cleanup.
- A service must not hide object-family identity in a generic count-only report.
- A service must not proceed with source-object removal after a required
  quarantine copy failed durable publication.

## L4C Test Pass: Durable Service Conformance Harness

Add reusable conformance coverage for backend capability differences.

Backend matrix:

- Memory/cache backend:
  - accepts non-durable operations where appropriate.
  - rejects durable-only service flows.
  - preserves idempotent missing-object behavior.
- Local durable backend:
  - supports durable write/replace/delete where the platform allows it.
  - returns confirmed or explicitly unconfirmed durability facts.
- Faulting backend:
  - injects write failure before visibility.
  - injects write failure after temporary object creation.
  - injects delete failure.
  - injects non-durable delete acknowledgement.
  - injects list/load inconsistency.

Conformance assertions:

- Services must preflight required capabilities before starting irreversible
  multi-object workflows.
- Services must return typed failures that preserve object identity.
- Services must leave no new visible durable object when a preflight fails.
- Services must support idempotent retry after every injected cleanup failure.

## L4D Test Pass: Durable Topology Restart Proof

These tests prove the storage-next topology, not the old topology. Old storage
used a simpler manifest source of truth. Storage-next uses database manifest,
branch catalog, branch state, table manifests, table-object facts, WAL segments,
checkpoints, snapshots, sidecars, and quarantine records.

Required restart-window tests:

- A table object exists but its table manifest entry is missing.
- A table manifest entry exists but the object is missing.
- A table manifest entry points at an object whose L5 facts do not match.
- A shared inherited table object is referenced by multiple branches and one
  branch is deleted.
- A branch rewrite publishes replacement objects before branch manifest
  replacement.
- A branch rewrite replaces the branch manifest before old-object cleanup.
- A checkpoint publishes checkpoint objects before checkpoint manifest
  replacement.
- A checkpoint manifest exists but a referenced object is missing.
- A snapshot manifest exists but one referenced object is missing.
- A WAL segment exists after the branch state says it should have been
  checkpointed.
- A WAL segment is partially published or truncated.
- A quarantine copy exists but the source object still exists.
- A quarantine copy exists but the source object was already deleted.
- A quarantine record references an inherited or shared object.

Expected behavior:

- L4 must return enough typed facts for L8 to choose repair, retry, quarantine,
  or hard failure.
- L4 must not silently discard partially visible durable state.
- L4 must not delete any object that may still be live through another branch,
  table manifest, checkpoint, snapshot, or quarantine record.

## L4E Test Pass: WAL Policy And Close/Sync Proof

Required tests:

- Standard WAL append and close produce a loadable segment after restart.
- Force-durable WAL append produces durable visibility or an explicit failure.
- Segment close is idempotent.
- Active WAL segment cleanup is rejected.
- Older closed WAL segment cleanup is allowed only after checkpoint proof exists.
- Corrupt latest WAL segment returns a repairable typed error.
- Corrupt historical WAL segment returns a hard error unless policy explicitly
  allows quarantine.
- Cache-mode WAL service creation or durable append fails with the documented
  cache-mode absence error.

## L4F Test Pass: Object-Durable Fencing Decision

M4P-L4 does not need distributed object-store fencing unless the implementation
plan explicitly admits object-store durable backends.

Required tests/documentation:

- Local-durable backend remains the only durable backend exercised by M4P-L4.
- Object-store durable backend creation is rejected or gated if no fencing
  contract exists.
- The rejection message points at the documented follow-up rather than silently
  running without fencing.

If object-store support is enabled later, this test pass must expand to include
generation checks, compare-and-swap semantics, or an equivalent fencing proof.

## Source Guards

Add or extend source guards so the test suite protects the architecture:

- L4 services may use L1 object APIs and L2 object layout helpers.
- L4 services may use L3 codecs and L5 parsers for durable validation.
- L4 services must not directly parse object names with ad hoc string slicing
  where an L2 helper exists.
- L4 services must not perform direct filesystem calls.
- L4 services must not implement L6 branch policy or L8 recovery policy.
- Higher layers must not bypass L4 service APIs to publish, list, or delete L4
  object families.

## Generated And Fuzz Coverage

Fuzzing is optional for M4P-L4 unless the implementation introduces a new parser.
Prefer deterministic generated fault-window tests over broad fuzzing.

Useful generated cases:

- all object-family cleanup permutations of `removed`, `already_absent`,
  `failed`, and `durability_unknown`;
- checkpoint manifest and object publication order permutations;
- table-manifest reference graph permutations for active, inherited, deleted,
  and quarantined objects;
- WAL segment close/truncate permutations.

## Benchmark And Performance Regression Gate

This plan is not a performance-tuning plan. However, L4 changes must not undo the
recent storage-next performance repairs.

Run the existing 100K benchmark profile only as a regression check if L4 code
changes touch hot read, scan, or load paths.

Do not add dedicated benchmark fast paths as part of M4P-L4.

## Verification Commands

Run narrow tests first, then the package-level checks:

```bash
cargo test -p strata-storage-next --locked --lib service
cargo test -p strata-storage-next --locked --lib lifecycle
cargo test -p strata-storage-next --locked --test service_fault_windows
cargo test -p strata-storage-next --locked --test backend_io_boundary
cargo test -p strata-storage-next --locked --test format_layer_source_guard
cargo test -p strata-storage-next --locked --test table_format_source_guard
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo clippy -p strata-storage-next --lib --features perf-trace -- -D warnings
cargo test -p strata-storage-next --locked --target wasm32-unknown-unknown --no-default-features --lib
git diff --check
```

If the conformance harness is added as an integration test, also run:

```bash
cargo test -p strata-storage-next --locked --test service_conformance
```

If L4 touches old-vs-new benchmark code, also run the existing redb benchmark
profile for 100K keys and record the result in the perf-tuning notes.

## Exit Criteria

M4P-L4 is complete when:

- Every L4 service family has direct publication/load/cleanup tests.
- Cleanup reports preserve L1 delete status and durability facts.
- Durable service conformance tests cover local, cache, and faulting backends.
- Restart-window lifecycle tests cover table objects, manifests, WAL,
  checkpoints, snapshots, sidecars, and quarantine.
- Cache-mode durable service absence is explicit and tested.
- Object-store fencing is either implemented and tested or explicitly gated out.
- Source guards prevent architecture bypasses.
- The verification commands pass.
