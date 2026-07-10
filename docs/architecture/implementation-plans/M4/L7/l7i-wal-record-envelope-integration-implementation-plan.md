# L7I Implementation Plan: WAL Record And Envelope Integration

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L7/l7i-wal-record-envelope-integration-test-plan.md`

## Objective

Implement the durable local commit path for storage-next L7.

L7I takes the commit semantics proven by L7H and adds the WAL durability step:
validated and stamped user rows plus L7G timeline rows are converted into one
row-native `WalRecord`, appended through L4, and only then applied into L6 and
published visible.

L7I must preserve WAL-before-visible ordering. It does not implement full
process-open replay, retained durable repair, checkpoint scheduling, or the
long-lived unresolved-durable write gate. Those belong to L7J/L7K/L8. L7I must
still classify failures at the boundary clearly enough that L7J can add the
gate without changing the successful durable protocol.

## Inputs

1. `docs/architecture/storage/l7-commit-runtime.md`
2. `docs/architecture/storage/l4-log-manifest-snapshot-services.md`
3. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
4. `docs/architecture/storage/commit-timeline-substrate.md`
5. `docs/spec/strata-storage-format-v1.md`
6. `docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`
7. `docs/architecture/implementation-plans/m4-l7-commit-runtime-test-plan.md`
8. `docs/architecture/implementation-plans/M4/L7/l7h-cache-no-wal-commit-path-implementation-plan.md`
9. `crates/storage-next/src/commit/`
10. `crates/storage-next/src/format/wal.rs`
11. `crates/storage-next/src/format/wal/commit_payload.rs`
12. `crates/storage-next/src/service/wal.rs`
13. `crates/storage-next/src/config/mode.rs`
14. `crates/storage-next/src/branch/state.rs`
15. `crates/storage/src/durability/commit_adapter.rs`
16. `crates/storage/src/txn/manager.rs`

## Existing-Code Source Map

| Current file | L7I evidence | L7I action |
|---|---|---|
| `crates/storage/src/durability/commit_adapter.rs` | Old storage wrote durable commit records before storage apply and had ambiguous durability windows. | Port the protocol shape only: WAL before L6 apply, explicit clean/uncertain/post-WAL classification. Do not port old record bytes. |
| `crates/storage/src/durability/payload.rs` | Old storage assembled commit payloads from write batches. | Replace with storage-next row-native `WalCommitPayload::new(rows)`. |
| `crates/storage/src/durability/format/wal_record.rs` | Old WAL record envelope behavior and tests. | Use as behavioral evidence only. Storage-next L3 owns `WalRecord` and `WalRecordEnvelope`. |
| `crates/storage/src/txn/manager.rs` | Old manager ordered branch locks, version allocation, WAL append, storage apply, and visibility. | Preserve ordering and guard lifetime. Retire transaction ids and public transaction sessions. |
| `crates/storage-next/src/commit/cache.rs` | L7H already validates/admit/conflict-checks/allocates/stamps/applies/publishes cache commits. | Reuse the same pre-WAL ordering and row preparation. Extract shared row-preparation helpers only if it keeps code small. |
| `crates/storage-next/src/format/wal.rs` | `WalRecord::new` validates outer branch/version/timestamp against payload rows; `WalRecordEnvelope` is codec-aware WAL framing. | L7I constructs `WalRecord` through the format layer and lets L4 append encode the envelope. L7I must not reimplement payload validation. |
| `crates/storage-next/src/service/wal.rs` | `WalService::append` encodes record frames, rotates segments, appends, and forces durability in `Always` mode. | Use a narrow commit-layer WAL appender adapter over L4. Do not write backend objects directly. |
| `crates/storage-next/src/config/mode.rs` | `DurabilityPolicy::Standard` and `DurabilityPolicy::Always` define local durable policy. | Map `CommitDurabilityMode::{Standard, Always}` to the L4 policy facts and reject mismatches before allocation. |

## Scope

L7I implements:

1. durable local commit orchestration for `CommitDurabilityMode::Standard`;
2. durable local commit orchestration for `CommitDurabilityMode::Always`;
3. a narrow L7-to-L4 WAL append adapter for tests and production;
4. `WalCommitPayload` construction from the exact stamped user+timeline rows;
5. `WalRecord` construction through L3 format validation;
6. `WalService::append` integration through L4;
7. WAL-before-L6-apply ordering;
8. visible durable outcomes after successful WAL append and L6 apply;
9. clean pre-WAL failure classification;
10. WAL append failure classification;
11. `Always` sync/force-durable uncertainty classification;
12. coarse durable-but-not-visible handoff errors for post-WAL L6/visibility failures;
13. direct tests for ordering, format parity, and phase outcomes;
14. generated counters for durable success and failure windows;
15. source-guard updates for the intentional `commit` to `format::wal` and `service::wal` dependency.

L7I does not implement:

1. cache/no-WAL commit behavior beyond sharing helpers with L7H;
2. unresolved durable write gate state;
3. in-process durable repair;
4. WAL replay;
5. allocator catch-up from replay;
6. process-open recovery;
7. manifest checkpointing or flush watermarks;
8. WAL retention;
9. durable transaction ids;
10. public transaction sessions;
11. engine observer hooks;
12. product `as_of` APIs.

## Protocol

The durable mutating commit path is:

```text
validate batch shape
reject cache durability mode
admit target branch and acquire branch guard
ensure target branch state matches the batch branch
ensure no already-applied rows are above current visible version
capture target branch read view
validate read-set/CAS facts at current visible version
allocate one commit version and timestamp
reject allocation if it is not greater than current visible version
stamp user rows
generate two timeline rows
build WalCommitPayload from the combined rows
construct WalRecord through the format layer
append WalRecord through L4 using the selected durable policy
atomically apply user rows + timeline rows into L6
publish visible version
return CommitOutcome { durable: Standard|Always, visible: true }
```

Required ordering:

1. malformed batch rejects before branch guard;
2. missing/deleting/generation-mismatched branch rejects before allocation;
3. conflict rejects before allocation;
4. version/timestamp allocation happens before WAL record construction;
5. `WalRecord::new` is the outer-fact validation boundary;
6. WAL append happens before any L6 mutation;
7. L6 apply happens before visible publication;
8. branch guard remains live through WAL append, L6 apply, and visible publication;
9. guard release happens on every return path by RAII;
10. clean WAL failure leaves no L6 rows and no visible publication;
11. uncertain WAL failure is not treated as a clean retryable failure;
12. post-WAL L6/visibility failure is never reported as visible success.

## Module Layout

Expected production layout after L7I:

```text
crates/storage-next/src/commit/
  cache.rs
  durable.rs       # durable WAL-backed executor and WAL adapter
  tests/
    durable.rs
```

If row preparation is shared with L7H, prefer one of these narrow shapes:

```text
commit/rows.rs
  PreparedCommitRows
```

or a small helper inside `durable.rs` that reuses `CacheCommitRows` without
making cache semantics part of durable names. Do not create a broad transaction
manager abstraction.

All new production items remain `pub(crate)`.

## Proposed Type Surface

Names may change if responsibilities stay intact.

### `CommitDurableRuntime`

Suggested shape:

```text
CommitDurableRuntime<'a, S, W> {
    config: &'a CommitRuntimeConfig,
    registry: &'a CommitBranchRegistry,
    guard_set: &'a CommitBranchGuardSet,
    allocator: &'a mut CommitFactAllocator<S>,
    branch: &'a mut BranchLocalState,
    visible: &'a mut VisibleVersionTracker,
    wal: &'a mut W,
}
```

Suggested entrypoint:

```text
execute(
    &mut self,
    batch: CommitBatch,
    generation_guard: CommitBranchGenerationGuard,
) -> CommitRuntimeResult<CommitOutcome>
```

Rules:

1. accepts only `CommitDurabilityMode::Standard` and `CommitDurabilityMode::Always`;
2. rejects `CommitDurabilityMode::Cache` before allocation;
3. validates branch state branch id against the batch branch;
4. admits the branch before conflict validation;
5. validates conflicts before allocation;
6. allocates exactly one version and timestamp;
7. constructs user rows and timeline rows once;
8. appends the same row set to WAL that will be applied to L6;
9. publishes visibility only after L6 apply succeeds;
10. returns durable visible outcomes only after L4 append success.

### `CommitWalAppender`

Suggested testable adapter:

```text
trait CommitWalAppender {
    fn durability_policy(&self) -> DurabilityPolicy;
    fn append_commit_record(&mut self, record: &WalRecord)
        -> CommitRuntimeResult<CommitWalAppendFacts>;
}

CommitWalAppendFacts {
    segment_id,
    start_offset,
    bytes_written,
    forced_durable,
}
```

Production implementation:

```text
impl CommitWalAppender for WalService<'_> {
    append_commit_record -> WalService::append
}
```

Rules:

1. the adapter exists for L7 tests and error mapping, not as a public API;
2. it must not expose backend object names as commit API requirements;
3. it must map L4 append errors into storage-shaped commit errors;
4. it must preserve source chains for L4 errors where available;
5. it must identify `Always` sync failure as durability-uncertain, not clean failure;
6. `forced_durable` must be true for a successful `Always` commit;
7. `forced_durable` may be false for a successful `Standard` commit.

If adding a trait is heavier than needed, use a small `CommitWalServiceAdapter`
wrapper and keep tests around a fake wrapper with the same shape.

### `CommitDurabilityMode` Mapping

Mapping rules:

| Batch mode | WAL policy requirement | Outcome durability |
|---|---|---|
| `Cache` | rejected by L7I | none |
| `Standard` | exact `DurabilityPolicy::Standard` unless explicitly documented as satisfied by `Always` | `CommitDurabilityClass::Standard` |
| `Always` | `DurabilityPolicy::Always`, with `forced_durable == true` | `CommitDurabilityClass::Always` |

Prefer exact policy matching for V1. If a later slice allows an `Always` WAL
service to satisfy a `Standard` batch, it must document whether the outcome
reports requested durability or effective durability.

### WAL Record Construction

Suggested helper:

```text
build_wal_record(stamp, branch_id, rows) -> CommitRuntimeResult<WalRecord>
```

Implementation rules:

1. rows are the same combined user+timeline rows sent to L6;
2. `WalCommitPayload::new(rows)` is used for payload construction;
3. `WalRecord::new(stamp.version, branch, stamp.timestamp, payload)` is used
   for outer-fact validation;
4. L7I does not call `encode_wal_commit_payload` directly;
5. L7I does not manually bypass `WalRecordEnvelope`;
6. production append calls L4 `WalService::append(&record)`, which owns record
   encoding and envelope framing;
7. tests may decode the appended WAL record through L4/L3 helpers to assert
   parity, but production L7 must not hand-write frame bytes.

## Failure Classification

L7I must distinguish these boundaries:

| Boundary | Durable? | Visible? | Expected classification |
|---|---:|---:|---|
| invalid batch / branch rejection / conflict | no | no | rejected before allocation |
| version overflow / timestamp unavailable | no | no | rejected before durable write |
| row stamping or WAL record construction failure | no | no | allocated-not-durable; version gap allowed |
| WAL append clean failure before bytes accepted | no | no | lower-layer WAL failure; no L6 mutation |
| WAL record too large / segment id overflow / rotation failure | no | no | lower-layer WAL failure; no L6 mutation |
| `Always` sync failure after append bytes may exist | uncertain | no | durability-uncertain; no L6 mutation |
| WAL append success, L6 apply failure | yes | no | durable-but-not-visible handoff |
| WAL append success, L6 apply success, visible publish failure | yes | not published | applied-but-not-visible handoff |
| WAL append success, L6 apply success, visible publish success | yes | yes | visible durable outcome |

L7J will add the persistent unresolved-durable write gate. L7I must avoid
making that future gate impossible by collapsing post-WAL failures into generic
errors.

## Source Guard Policy

L7I adds the following intentional production imports:

1. `crate::format::wal::{WalCommitPayload, WalRecord}`;
2. `crate::service::wal::{WalAppend, WalService, WalServiceError}` or a
   narrower adapter wrapper;
3. `crate::config::mode::DurabilityPolicy`.

L7I must still not import:

1. `crate::backend` directly;
2. `crate::layout` directly;
3. `crate::object` directly;
4. `std::fs`, `Path`, `File`, mmap, environment variables, or process-global
   mutable state;
5. engine/product modules;
6. JSON, graph, vector, search, event, embedding, remote, hub, or dataset code.

## Implementation Steps

### L7I-A: Shared Row Preparation

1. Decide whether to extract `CacheCommitRows` into a shared
   `PreparedCommitRows`.
2. Preserve L7H public behavior and tests.
3. Keep row order deterministic: user rows first, timeline rows after.
4. Add no WAL imports to cache-only code.

Exit gate: L7H tests remain green.

### L7I-B: WAL Adapter

1. Add a narrow adapter over L4 `WalService::append`.
2. Expose only durability policy and append facts needed by L7.
3. Map `WalServiceError` to `CommitRuntimeError`.
4. Preserve L4 source chains where possible.
5. Classify `Always` sync failure as uncertain.

Exit gate: unit tests can fake append success, clean failure, and uncertain
failure without a backend.

### L7I-C: WAL Record Builder

1. Build `WalCommitPayload` from prepared rows.
2. Build `WalRecord` through `WalRecord::new`.
3. Add tests proving mismatched row facts are caught by L3 format validation.
4. Add tests that decode appended records and compare rows to L6-applied rows.

Exit gate: L7I relies on L3 validation instead of reimplementing it.

### L7I-D: Durable Runtime

1. Implement `CommitDurableRuntime`.
2. Mirror L7H validation/admission/conflict/allocation ordering.
3. Append WAL record before L6 apply.
4. Apply rows atomically into L6 after append success.
5. Publish visible version after L6 apply.
6. Return `CommitOutcome::visible` with `Standard` or `Always` durability.

Exit gate: successful standard and always commits are visible only after WAL
append success.

### L7I-E: Failure Windows

1. Add clean WAL failure tests.
2. Add `Always` uncertain failure tests.
3. Add record-too-large/segment-overflow classification tests where easy with
   fakes.
4. Add post-WAL L6 failure handoff tests if the runtime already exposes the
   handoff error; otherwise document the exact L7J follow-up.

Exit gate: no WAL failure makes rows visible.

### L7I-F: Generated Coverage And Source Guards

1. Add durable counters to the commit-runtime generated harness.
2. Add source-guard allowances only for L7I's intended L3/L4 imports.
3. Add negative guard checks for backend/layout/object/fs imports.
4. Update the L7 parent plan row to link this plan and test plan.

Exit gate: generated property harness exercises standard/always success and
WAL failure categories.

## Porting Log Requirements

Update `docs/architecture/implementation-plans/M4/L7/m4-l7-porting-log.md`
with:

1. old durability adapter behavior preserved;
2. old WAL byte formats retired;
3. transaction-id behavior intentionally omitted;
4. `standard` and `always` policy mapping;
5. clean vs uncertain WAL failure classification;
6. durable-but-not-visible handoff deferred to L7J gate/recovery;
7. command evidence.

## Acceptance Criteria

L7I is complete when:

1. `CommitDurabilityMode::Standard` and `Always` have durable local commit paths;
2. cache mode remains owned by L7H and rejected by L7I;
3. L7I constructs `WalRecord` through L3 format types;
4. L4 appends the WAL record before L6 apply;
5. successful durable commits publish visible version only after L6 apply;
6. standard and always outcome durability classes are distinct;
7. clean WAL append failure leaves no L6 rows and no visible publication;
8. uncertain WAL failure is distinct from clean failure;
9. post-WAL failures are surfaced as durable/applied-not-visible handoff facts;
10. source guards prevent direct backend/object/layout/filesystem access;
11. direct and generated L7I tests pass under default and all-features builds;
12. L7H cache tests continue to pass unchanged.

## Verification Commands

Minimum commands for this slice:

```bash
cargo fmt --package strata-storage-next --check
cargo test -p strata-storage-next --locked --lib commit
cargo test -p strata-storage-next --locked --test commit_runtime_source_guard
cargo test -p strata-storage-next --all-features --locked --test commit_runtime_properties
cargo test -p strata-storage-next --all-features --locked --test commit_runtime_faults
cargo test -p strata-storage-next --no-default-features --locked --lib commit
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
git diff --check
```

If `commit_runtime_faults.rs` does not exist before implementation, create it
only for behavioral fault tests. Do not add tests that merely assert planning
documents exist.
