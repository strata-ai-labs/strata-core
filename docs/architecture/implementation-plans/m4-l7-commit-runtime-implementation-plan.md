# M4-L7 Implementation Plan: Commit Runtime

Status: draft implementation plan

## Objective

Build the storage-next commit runtime.

M4-L7 connects engine-owned semantic write requests to L6 branch state through
one internal storage commit unit. It validates a batch, assigns one commit
version and one commit timestamp, records durability when required, applies the
committed rows into L6, and publishes visibility only after the batch is fully
applied.

M4-L7 must preserve the useful parts of the current transaction machinery
without resurrecting public storage transactions as a V1 product surface.
Users should see product operations and branch workflows through L9 and
engine-next. Storage still needs a precise internal commit runtime so writes
are ordered, durable when required, conflict-checked when requested, and never
partially visible.

M4-L7 is intentionally delivered in three logical parts:

1. **L7-Core: Commit Semantics**
   Defines what a storage commit is and proves cache/no-WAL commits into L6
   without durable-WAL complexity.
2. **L7-Durable: WAL-Before-Visible**
   Adds local durable commit ordering and phase classification.
3. **L7-Replay + Closeout: L8 Handoff**
   Adds replay/catch-up hooks and closes generated, fuzz, fault, source-guard,
   and sensitivity assurance.

The `L7A`, `L7B`, ... slice labels remain the detailed work units. The
three-part structure is the delivery boundary used for planning, review, and
commit grouping.

## Inputs

1. `docs/architecture/storage/l7-commit-runtime.md`
2. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
3. `docs/architecture/storage/l4-log-manifest-snapshot-services.md`
4. `docs/architecture/storage/commit-timeline-substrate.md`
5. `docs/architecture/storage/implementation-patterns.md`
6. `docs/architecture/storage/target-crate-shape-and-test-harness.md`
7. `docs/architecture/implementation-plans/m4-m4t-implementation-plan.md`
8. `docs/architecture/implementation-plans/m4-l7-commit-runtime-test-plan.md`
9. `docs/spec/strata-storage-format-v1.md`
10. `crates/storage-next/src/commit/`
11. `crates/storage-next/src/branch/`
12. `crates/storage-next/src/format/wal.rs`
13. `crates/storage-next/src/service/wal.rs`
14. `crates/storage-next/src/service/manifest.rs`

## Existing-Code Source Map

The current implementation evidence lives across old storage and engine code.
The porting rule is to extract storage commit mechanics, not public transaction
UX or engine product semantics.

| Current file | Relevant L7 evidence | Porting rule |
|---|---|---|
| `crates/storage/src/txn/context.rs` | Staged writes, read-your-writes overlay, read set, CAS set, delete set, TTL map, write modes, apply summary. | Use as evidence for internal commit drafts and validation facts. Do not expose public long-lived storage transactions. |
| `crates/storage/src/txn/manager.rs` | Transaction id/version allocation, branch commit locks, quiesce, visible-version tracking, pending versions, branch deletion barriers, no-WAL commit path. | Port storage-owned commit ordering, guards, visibility, and commit-version allocator catch-up. Retire storage transaction ids for V1 unless a later private optimization reintroduces them. Remove product transaction-session assumptions. |
| `crates/storage/src/txn/validation.rs` | Read-set and CAS validation. | Preserve the internal snapshot-isolation style conflict model. Do not claim serializable transactions. |
| `crates/storage/src/txn/lock_ordering.rs` | Explicit lock-order discipline for commit path. | Rebuild as L7 lock-order guard tests and comments close to the lock acquisition code. |
| `crates/storage/src/durability/commit_adapter.rs` | WAL-before-storage bridge and ambiguous durability classification. | Port the protocol shape: construct a storage-next `WalRecord`, let the format layer validate row facts, append the record through L4, then apply to L6. L4 owns envelope framing. Do not port old WAL bytes. |
| `crates/storage/src/durability/payload.rs` | Current commit payload construction. | Replace with storage-next row-native `WalRecord` construction. The `WalCommitPayload` remains a format-layer detail validated by `WalRecord::new`. |
| `crates/storage/src/durability/format/wal_record.rs` | Old durable commit record envelope. | Use as behavioral evidence only; storage-next already owns WAL record format. |
| `crates/storage/src/segmented/mod.rs` | `apply_writes_atomic`, `apply_recovery_atomic`, version tracking, timestamp preservation. | Split L6 apply mechanics from L7 commit protocol and L8 replay orchestration. |
| `crates/storage/src/traits.rs` | `apply_writes_atomic`, version-bounded reads, write modes. | Use as compatibility evidence for storage-shaped operations. Do not keep product-facing transaction commands by default. |
| `crates/engine/src/database/transaction.rs` | Engine writer health, generation guard validation, backpressure, WAL selection, post-commit observers. | Keep product and observer semantics above L7. L7 may expose storage facts consumed by engine. |
| `crates/engine/src/coordinator.rs` | Current active transaction metrics, timeout checks, GC-safe version tracking, error conversion. | Use as evidence for L7 metrics and version safety, not as storage API shape. |
| `crates/engine/src/transaction/owned.rs` and `crates/engine/src/transaction/pool.rs` | Public/manual transaction handles and pooled contexts. | Treat as optimization and compatibility evidence only. V1 storage-next should not expose public begin/commit/rollback sessions. |

Storage-next already provides:

1. L3 row-native WAL commit payloads;
2. L4 WAL append/read/retention services;
3. L6 branch-local committed-row install and read visibility;
4. storage-owned `StorageRow` bytes with commit version, timestamp, expiry,
   tombstone flag, and value bytes;
5. branch and table reachability facts consumed later by L8.

## L7 Boundaries

L7 owns:

1. internal `CommitBatch` shape;
2. storage-shaped commit mutations;
3. commit-batch validation;
4. commit-version allocation;
5. commit timestamp allocation;
6. version-clock catch-up from recovered durable rows;
7. optional read-set and CAS conflict validation;
8. per-branch commit ordering;
9. commit quiesce guard;
10. branch-deleting and branch-generation commit barriers;
11. WAL-before-visible discipline through L4;
12. cache/no-WAL commit path;
13. durable `standard` and `always` local commit paths;
14. atomic L6 apply of one committed batch;
15. visible-version publication after L6 apply;
16. durable-but-not-visible classification;
17. commit timeline row construction and install;
18. storage-local commit metrics and diagnostics;
19. lock-order rules for commit-path locks.

L7 must not own:

1. public user transaction sessions;
2. storage transaction ids in V1;
3. user-facing ACID claims;
4. product branch workflows such as merge, cherry-pick, revert, restore,
   review, or publish;
5. JSON path, graph, vector, search, event, embedding, or inference semantics;
6. engine DTO mapping;
7. table byte format;
8. WAL byte format;
9. backend object naming;
10. checkpoint, compaction, retention, quarantine, repair, or recovery
   scheduling;
11. distributed consensus or multi-process writer coordination beyond the L4
    backend writer lock contract;
12. cross-branch atomic product commits unless a later design explicitly adds
    deterministic multi-branch lock ordering.

## Storage Commit Model

The L7 write unit is an internal storage batch:

```text
CommitBatch
  target branch
  mutations
    put physical key + value bytes + expiry/write hint
    delete physical key
  validation facts
    optional read-set versions
    optional CAS facts
  options
    durability mode
    conflict validation mode
    timestamp mode
    operation origin for diagnostics
```

The batch is single-branch by default. A caller may not smuggle rows for
multiple branches into a normal commit. Product workflows that read one branch
and write another should be represented as:

1. engine reads source branches through L9/L6;
2. engine computes the semantic result;
3. engine submits one target-branch `CommitBatch`.

Each mutating commit receives:

1. one commit version;
2. one commit timestamp;
3. one visibility publication point;
4. zero or more storage rows that all carry the same version and timestamp.

## Commit Mutation Model

Target shape:

```text
CommitMutation
  Put {
    physical_key,
    value_bytes,
    expires_at,
    write_mode_or_hint,
  }
  Delete {
    physical_key,
  }
```

L7 stamps each mutation into a `StorageRow` at commit time. Callers do not
pre-stamp storage commit versions.

Rules:

1. Put rows carry value bytes unchanged.
2. Delete rows become tombstone rows with no value bytes.
3. Expiry metadata is copied into the stamped row.
4. Every stamped row must belong to the target branch unless the batch is an
   explicitly storage-internal cross-branch operation. Cross-branch internal
   operations are out of the first V1 slice.
5. Duplicate physical keys inside one batch must have an explicit policy. The
   conservative V1 default should reject ambiguous duplicate keys unless a
   builder normalizes them before validation.
6. Timeline rows are storage-owned mutations generated by L7 and installed in
   the same logical commit unit as user rows.

## Commit Facts

L7 should distinguish these facts even when the first implementation stores
some of them in the same value:

1. `allocated_version`: highest version reserved by L7;
2. `durable_version`: highest version known durable in WAL or equivalent;
3. `applied_version`: highest version applied into L6 branch state;
4. `visible_version`: highest version safe for new snapshots;
5. `timeline_version`: highest version whose timestamp mapping is installed;
6. `recovered_version`: highest version observed from L8 replay/catch-up.

The implementation may collapse facts only when the failure model remains
sound. Durable and visible must remain distinguishable.

V1 storage-next does not keep a durable storage transaction id. Recovery
allocator catch-up therefore applies to commit versions only. Any future
private transaction-id allocator must land with an explicit recovery catch-up
test and a deferred-map update.

Commit timestamps come from a storage-owned `CommitTimestampSource`
abstraction. Tests should use deterministic/manual sources. A production source
may wrap wall-clock time, but it must include a monotonic guard so one runtime
does not move commit timestamps backward. Equal timestamps are still valid
because replayed data or an explicit source can produce them; the timeline
tiebreaker is commit version.

## Delivery Parts

### Part 1: L7-Core

L7-Core establishes the storage commit semantics without durable WAL behavior.

It includes:

1. `CommitBatch` and `CommitMutation`;
2. batch validation and duplicate-key policy;
3. commit-version allocation;
4. commit timestamp allocation;
5. commit outcomes and visibility facts;
6. branch registry, branch write guard, and quiesce skeleton;
7. read-only diagnostic path;
8. conflict validation over L6 read facts;
9. commit timeline row construction and install;
10. cache/no-WAL commit path into L6.

Exit gate:

1. cache/no-WAL commits atomically install user rows plus timeline rows into
   L6;
2. every mutating batch has exactly one version and timestamp;
3. conflicts reject before version allocation;
4. read-only batches allocate no version;
5. branch guards reject unsafe targets before mutation;
6. visible-version facts move only after full L6 apply.

Timeline belongs in Core because timeline facts are part of the commit unit.
Durable mode later writes the same stamped rows to WAL, but the timeline model
must already be correct before WAL integration.

### Part 2: L7-Durable

L7-Durable adds local durability to the already-defined commit semantics.

It includes:

1. `WalRecord` construction from stamped storage rows;
2. `WalRecord` append through L4, where L4 owns envelope framing;
3. `standard` and `always` durability modes;
4. WAL-before-visible ordering;
5. clean WAL failure classification;
6. uncertain WAL failure classification;
7. durable-but-not-visible classification;
8. write gate for unresolved durable-but-not-visible facts.

Exit gate:

1. durable commits are never visible before L4 accepts the WAL record;
2. clean WAL failure leaves no visible rows;
3. uncertain WAL outcome is distinct from clean failure;
4. durable-but-not-visible is explicit and blocks unsafe forward progress;
5. `standard` and `always` outcomes are distinguishable.

### Part 3: L7-Replay + Closeout

L7-Replay + Closeout makes the durable protocol consumable by L8 and closes
assurance.

It includes:

1. replay entrypoints for already-durable WAL records;
2. allocator catch-up;
3. replay idempotency;
4. replay mismatch rejection;
5. visible publication after replay;
6. quiesce hardening and lock-order assertions;
7. generated/property/fuzz/fault harnesses;
8. source guards;
9. porting log;
10. sensitivity ledger;
11. closeout inventory.

Exit gate:

1. L8 can replay durable commits safely into L6;
2. the commit-version allocator catches up above recovered versions;
3. replay is idempotent when facts match and fails closed when they do not;
4. generated/fuzz/fault tests cover Core and Durable behavior;
5. closeout proves source boundaries and command matrix.

## Commit Protocols

### Read-Only Batch

Read-only batches are compatibility and diagnostics helpers. Normal reads
should use L9/L6. They are not part of the L7 V1 minimum product surface; they
remain in this plan only as a thin internal helper because the old storage
context had a no-allocation read-only path and it is useful for diagnostics.

```text
validate no mutations
return current visible snapshot fact
do not allocate a version
```

### Cache / No-WAL Commit

Cache mode has no crash durability claim.

```text
validate batch
acquire commit guard
allocate commit version
allocate commit timestamp
stamp rows
apply rows atomically through L6
publish visible version
release guard
return CommitOutcome { durable: false, visible: true }
```

If L6 apply fails before visibility, the commit is not durable and not visible.
A version gap may remain.

### Durable Local Commit

Durable local mode must append the WAL record before L6 visibility.

```text
validate batch
acquire commit guard
allocate commit version
allocate commit timestamp
stamp rows
construct WalRecord through the format layer
append WalRecord through L4 using selected durability policy
apply rows atomically through L6
publish visible version
release guard
return CommitOutcome { durable: true, visible: true }
```

L7 must not reimplement WAL payload validation. The format layer already
constructs the row-native payload and validates the WAL record's outer branch,
version, and timestamp against every payload row. L7's responsibility is to pass
stamped rows into `WalRecord::new` and append that record through L4
`WalService::append`, which owns record encoding and envelope framing.

Durability policies:

1. `standard`: WAL append succeeds and the configured sync policy is
   responsible for forcing durability within the bounded window.
2. `always`: WAL append succeeds and the per-commit durability barrier has
   completed before the commit is acknowledged.
3. `cache`: no WAL path and no crash durability claim.

### Durable But Not Visible

If WAL durability succeeds and L6 apply or visibility publication fails, L7
must return a typed durable-but-not-visible outcome or error. The caller must
not treat the commit as visible in the current process. L8 recovery must later
replay or reconcile the durable WAL record before normal writes continue.

## Conflict Model

V1 should preserve the current internal conflict model:

1. read-set validation detects changed observed versions when requested;
2. CAS validation detects mismatched expected versions;
3. blind writes do not conflict by default;
4. write skew is possible and is not a serializable transaction guarantee;
5. conflict validation happens before version allocation.

Conflict facts are storage facts:

```text
ReadFact {
  physical_key,
  observed_version: Option<CommitVersion>,
}

CasFact {
  physical_key,
  expected_version: Option<CommitVersion>,
}
```

The exact type names may change, but they must remain storage-shaped and
branch-aware.

## Commit Guards

The commit path must document and test lock ordering.

Target order:

```text
1. commit quiesce read guard
2. per-branch commit guard
3. branch deletion/generation barrier when required
4. WAL append guard when durable mode requires WAL
5. L6 branch-state apply guard
6. visible-version publication guard
```

Rules:

1. A mutating commit cannot start while quiesce is active.
2. V1 quiesce is nonblocking: it returns a typed unavailable error while
   in-flight mutating commits hold branch guards, and L8 owns retry/deadline
   policy.
3. Same-branch commits are ordered deterministically.
4. Different-branch commits must not corrupt global visible-version facts.
5. Branch deletion blocks new commits before version allocation.
6. Branch generation mismatch rejects before visibility.

Branch-id reuse after deletion/recreation is not owned by L7 unless L9 supplies
a stable generation fact in the commit request. When such a fact is present,
L7 must treat reuse-after-delete the same as any other stale generation.

## Commit Timeline

L7 owns the generic timestamp-to-version substrate.

For each mutating commit, L7 installs storage-owned timeline facts:

```text
branch id + commit timestamp + commit version -> commit version
branch id + commit version                    -> commit timestamp
```

The physical representation is defined by
`docs/architecture/storage/commit-timeline-substrate.md`. Timeline rows
are ordinary storage-owned rows in the reserved commit-timeline family. They
must be written with the same commit version and timestamp as the user rows
they describe.

L7 does not implement product `as_of` selectors. It provides the substrate that
L9/engine can use to resolve selectors.

Timestamp lookup must return the greatest retained commit version at or before
the requested timestamp. When multiple commits have the same timestamp in one
branch, the greatest commit version wins. The timestamp index must therefore
include commit version in the key, matching
`docs/architecture/storage/commit-timeline-substrate.md`.

## Recovery Handoff

L8 owns recovery orchestration. L7 owns replay/catch-up rules.

L7 must expose hooks for L8 to:

1. stamp no new versions during replay;
2. apply recovered rows with the WAL record's original version and timestamp;
3. bypass normal read-set/CAS conflict validation during replay;
4. catch up the version allocator above every recovered commit version;
5. publish visible version only after replayed rows are installed into L6;
6. classify duplicate replay as idempotent when the durable row is already
   installed with the same facts;
7. reject replay when installed row facts disagree with the durable WAL record.

There is no transaction-id catch-up hook in V1 because storage-next L7 does not
persist transaction ids.

Full open/recovery sequencing belongs to L8.

## Implementation Slices

### Part 1: L7-Core

| Slice | Title | Implementation scope | Test scope | Exit gate |
|---|---|---|---|---|
| `L7A` | Source map and module scaffold | Create `commit` module structure, error/config/fact/result shells, crate-private exports, source guards, and porting log. Detailed plans: `docs/architecture/implementation-plans/M4/L7/l7a-commit-runtime-scaffold-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L7/l7a-commit-runtime-scaffold-test-plan.md`. | Compile-only tests, error display/source-chain tests, source-guard tests. | Commit module compiles without behavior and without public transaction API leakage. |
| `L7B` | Commit batch and mutation model | Add `CommitBatch`, `CommitMutation`, options, limits, validation facts, duplicate-key policy, and storage-row stamping helpers. Detailed plans: `docs/architecture/implementation-plans/M4/L7/l7b-commit-batch-mutation-model-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L7/l7b-commit-batch-mutation-model-test-plan.md`. | Batch validation, malformed batch rejection, branch mismatch rejection from Storage Commit Model rule 4, duplicate mutation policy, row stamping invariants. | L7 can build valid stamped rows but cannot yet allocate or apply them. |
| `L7C` | Version and timestamp clocks | Add monotonic version allocator, timestamp provider abstraction, timestamp guard, version-gap policy, overflow errors, and catch-up helper shape. Detailed plans: `docs/architecture/implementation-plans/M4/L7/l7c-version-and-timestamp-clocks-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L7/l7c-version-and-timestamp-clocks-test-plan.md`. | Allocation monotonicity, gaps, overflow, timestamp consistency, catch-up boundary checks, read-only no-allocation. | L7 can produce ordered commit facts without touching L6 or WAL. |
| `L7D` | Commit outcomes, visibility, and read-only path | Add `CommitOutcome`, visible-version tracker, read-only fast path, and snapshot visibility facts. Detailed plans: `docs/architecture/implementation-plans/M4/L7/l7d-outcomes-visibility-read-only-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L7/l7d-outcomes-visibility-read-only-test-plan.md`. | Read-only does not allocate, visible version starts/catches up correctly, impossible fact ordering rejected. | L7 can report visibility facts without mutating branches. |
| `L7E` | Branch registry and commit guards | Add branch registration/lookup model, per-branch commit guard, branch-deleting marker, branch generation guard, and quiesce skeleton. Detailed plans: `docs/architecture/implementation-plans/M4/L7/l7e-branch-registry-commit-guards-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L7/l7e-branch-registry-commit-guards-test-plan.md`. | Same-branch ordering, branch deleting rejection, generation mismatch rejection, quiesce blocks new commits. | L7 can safely admit or reject a target-branch commit before mutation. |
| `L7F` | Conflict validation | Implement read-set and CAS validation over L6 read views before version allocation. Preserve blind-write no-conflict behavior. Detailed plans: `docs/architecture/implementation-plans/M4/L7/l7f-conflict-validation-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L7/l7f-conflict-validation-test-plan.md`. | Read-set conflict, CAS conflict, delete-vs-put conflicts, blind write acceptance, no version allocation on conflict. | Optional optimistic validation works without public transactions. |
| `L7G` | Commit timeline substrate | Generate storage-owned timeline rows and expose timeline query helpers/facts. Keep installation with user rows for L7H. Detailed plans: `docs/architecture/implementation-plans/M4/L7/l7g-commit-timeline-substrate-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L7/l7g-commit-timeline-substrate-test-plan.md`. | timestamp-to-version with version tiebreak, version-to-timestamp, branch isolation, duplicate timestamp ordering, timeline rows share commit facts. | L7 can construct and query the generic timeline needed for later `as_of` and branch-from-time support. |
| `L7H` | Cache/no-WAL commit path | Validate, allocate, stamp user and timeline rows, atomically apply into L6, publish visibility, and return non-durable visible outcome for cache mode. Detailed plans: `docs/architecture/implementation-plans/M4/L7/l7h-cache-no-wal-commit-path-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L7/l7h-cache-no-wal-commit-path-test-plan.md`. | Put/delete/timeline atomicity, one version per batch, version gaps on pre-visible apply failure, L6 read parity after commit. | Cache mode can commit to L6 without WAL and without claiming crash durability. |

### Part 2: L7-Durable

| Slice | Title | Implementation scope | Test scope | Exit gate |
|---|---|---|---|---|
| `L7I` | WAL record and envelope integration | Construct `WalRecord` through the format layer, append through L4 WAL service envelope framing, select `standard`/`always`, and only then call L6 apply. Detailed plans: `docs/architecture/implementation-plans/M4/L7/l7i-wal-record-envelope-integration-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L7/l7i-wal-record-envelope-integration-test-plan.md`. | WAL outer-fact parity through the existing format validator, append failure leaves no visible rows, always-vs-standard outcome facts, WAL-before-visible ordering. | Durable local commits are WAL-backed and visible only after L4 success. |
| `L7J` | Durable-but-not-visible classification | Add typed phase errors/outcomes for failures after WAL durability and before L6 visibility. Add write gate state for unresolved durable commits. Detailed plans: `docs/architecture/implementation-plans/M4/L7/l7j-durable-but-not-visible-classification-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L7/l7j-durable-but-not-visible-classification-test-plan.md`. | Inject L6 apply failure after WAL success, visibility publish failure, unresolved durable commit blocks new writes until L8/reconcile hook. | Ambiguous durable commits are explicit and cannot be silently retried as normal writes. |

### Part 3: L7-Replay + Closeout

| Slice | Title | Implementation scope | Test scope | Exit gate |
|---|---|---|---|---|
| `L7K` | Recovery replay and allocator catch-up hooks | Add replay entrypoints for already-durable rows, idempotent duplicate handling, conflict bypass, commit-version allocator catch-up, and visible publication after replay. Detailed plans: `docs/architecture/implementation-plans/M4/L7/l7k-recovery-replay-allocator-catch-up-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L7/l7k-recovery-replay-allocator-catch-up-test-plan.md`. | Replay idempotency, version allocator catch-up, timestamp preservation, fact mismatch rejection, duplicate durable record handling. | L8 can replay WAL records into L6 without normal commit validation. |
| `L7L` | Concurrency and quiesce hardening | Complete in-process quiesce semantics, lock-order assertions, deterministic same-branch ordering, cross-branch visible-version safety, and deterministic scheduler-style guard tests. Detailed plans: `docs/architecture/implementation-plans/M4/L7/l7l-concurrency-quiesce-hardening-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L7/l7l-concurrency-quiesce-hardening-test-plan.md`. | Same-branch guard contention, quiesce fast-fail/block/release, guard release after cache/durable failures, cross-branch visible safety, deterministic guard interleavings. | Commit guards are strong enough for L8 checkpoint/recovery gates. |
| `L7M` | Generated harness, fuzz, and fault scripts | Add commit-runtime testkit model, generated commit scripts, fuzz targets, enriched corpora, and backend fault windows. Detailed plans: `docs/architecture/implementation-plans/M4/L7/l7m-generated-fuzz-fault-assurance-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L7/l7m-generated-fuzz-fault-assurance-test-plan.md`. | Property/fuzz coverage for cache, durable, conflict, timeline, quiesce, and phase failures. | Generated assurance covers the full commit protocol, not only unit examples. |
| `L7N` | Conformance closeout | Consolidate source guards, command matrix, old-code behavior ledger, sensitivity probes, deferred map, and closeout inventory tests. Detailed plans: `docs/architecture/implementation-plans/M4/L7/l7n-l7-conformance-closeout-implementation-plan.md` and `docs/architecture/implementation-plans/M4/L7/l7n-l7-conformance-closeout-test-plan.md`. | Closeout inventory, source guard, fuzz inventory, sensitivity-probe ledger, full command set. | M4-L7 closes and L8 can recover durable commits. |

## Implementation Budget

Each slice should stay under the engineering-standard 1,500 LOC review budget.
Expected scope:

| Slice group | Expected LOC | Split trigger |
|---|---:|---|
| `L7A` to `L7C` | 300-900 LOC each | Split if scaffold, clocks, or tests exceed one focused module. |
| `L7D` to `L7H` | 700-1,400 LOC each | Split if branch guards, conflict validation, timeline, or cache commit tests become mixed in one file. |
| `L7I` to `L7J` | 700-1,400 LOC each | Split if WAL service fakes, phase errors, and protocol code cannot be reviewed independently. |
| `L7K` to `L7N` | 500-1,400 LOC each | Split generated harnesses and closeout/source guards into separate modules before they cross the limit. |

If a slice approaches the limit, create a narrower sub-slice before coding
rather than accepting a large mixed patch.

## Error And Outcome Shape

L7 errors must be phase-specific.

Required categories:

1. invalid batch;
2. invalid mutation;
3. branch not found;
4. branch not writable;
5. branch deleting;
6. branch generation mismatch;
7. read-set conflict;
8. CAS conflict;
9. commit version overflow;
10. commit timestamp unavailable or invalid;
11. commit quiesce unavailable;
12. unsupported durability mode;
13. WAL append failure;
14. WAL writer halted;
15. WAL segment id overflow or segment-roll failure;
16. WAL durable outcome uncertain;
17. L6 apply failure before durability;
18. durable but not visible;
19. visibility publication failure;
20. timeline install failure;
21. replay fact mismatch;
22. allocator catch-up failure.

`CommitOutcome` should report:

1. branch id;
2. commit version;
3. commit timestamp;
4. put count;
5. delete count;
6. timeline row count;
7. durable status;
8. visible status;
9. validation mode;
10. durability mode;
11. optional WAL segment/object facts;
12. optional recovery/replay facts.

## Source Guard Policy

Production `commit/` code may import:

1. `crate::branch`;
2. `crate::format::wal`;
3. `crate::row`;
4. `crate::service::wal`;
5. `crate::service::manifest` only for explicit commit watermarks/facts;
6. `strata_core_next::{BranchId, CommitVersion, Timestamp}`;
7. standard library synchronization primitives.

Production `commit/` code must not import:

1. engine crates;
2. product DTOs;
3. JSON, graph, vector, search, event, or embedding modules;
4. `crate::table` internals except through L6 or explicit row facts;
5. `crate::backend` directly;
6. `crate::layout` or object-name builders directly;
7. `std::fs`, `Path`, `File`, mmap, environment variables, or process-global
   mutable state.

All production commit APIs default to `pub(crate)`. L7 should not expose `pub`
commit runtime types at the crate root; L9 owns the future public storage API
boundary.

## Porting Log

Create `docs/architecture/implementation-plans/M4/L7/m4-l7-porting-log.md`
before behavior lands. This follows the grouped `M4/L6` slice-doc convention
while the parent plan remains flat at
`docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`.
Every L7 slice must record:

1. old code mapped to storage-next code;
2. behavior preserved;
3. behavior intentionally changed;
4. behavior retired;
5. behavior deferred to L8, L9, engine-next, or post-V1;
6. command evidence;
7. sensitivity probes or structural guards.

## Deferred Behavior Map

Not L7 implementation gaps:

1. public user transaction sessions;
2. durable storage transaction ids and transaction-id allocator catch-up;
3. serializable isolation claims;
4. product branch merge/cherry-pick/revert/restore;
5. cross-branch atomic commits;
6. branch-id reuse and deletion/recreation generation ownership, unless L9
   supplies generation facts to L7;
7. checkpoint scheduling;
8. compaction scheduling;
9. WAL retention scheduling;
10. snapshot creation scheduling;
11. process open/recovery orchestration;
12. backend repair/quarantine orchestration;
13. object-store multi-writer fencing beyond current L4 capabilities;
14. engine observer side effects;
15. query/index/search side effects;
16. storage API mapping and public response DTOs.

These belong to L8, L9, engine-next, or post-V1.

## Exit Gate

M4-L7 is complete when:

1. cache/no-WAL commits validate, allocate, apply, and publish visibility;
2. durable local `standard` and `always` commits append WAL before L6 apply;
3. durable-but-not-visible outcomes are typed and block unsafe forward progress;
4. read-set and CAS validation preserve the current internal conflict model;
5. commit versions are monotonic, gaps are accepted, and overflow is typed;
6. one timestamp is assigned per mutating commit and timeline facts are
   installed;
7. read-only batches do not allocate versions;
8. branch guards and quiesce guards prevent unsafe commits;
9. recovery replay hooks are sufficient for L8 to install WAL records
   idempotently;
10. source guards prevent product, engine, backend, layout, and filesystem
    leakage;
11. generated/property/fuzz/fault tests cover every commit phase;
12. the porting log records preserved, changed, retired, and deferred behavior;
13. closeout commands pass under default, no-default, all-features, and wasm
    where applicable.
