# L8V Implementation Plan: Retention-Aware Row Pruning

Status: branch-runtime implementation and proof-bound test suite landed;
durable retained-history manifest extension remains deferred

Parent plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L8/l8v-retention-aware-row-pruning-test-plan.md`

Predecessors:

1. `docs/architecture/implementation-plans/M4/L8/l8k-compaction-materialization-scheduling-implementation-plan.md`
2. `docs/architecture/implementation-plans/M4/L8/l8l-retention-proof-snapshot-pruning-implementation-plan.md`
3. `docs/architecture/implementation-plans/M4/L8/l8t-table-manifest-backed-flush-watermarks-implementation-plan.md`
4. `docs/architecture/implementation-plans/M4/L8/l8u-durable-rewrite-publication-implementation-plan.md`

## Objective

Allow compaction to drop old row history only when storage has a typed proof
that no retained read, snapshot, timestamp lookup, inherited branch, or durable
recovery path can observe a different result.

L5 already accepts a caller-provided compaction policy. L6 already rejects
pruning policies because no proof exists. L8V supplies the missing lifecycle
proof and wires it into compaction/materialization scheduling. The slice makes
row pruning possible without weakening Strata's MVCC, `as_of`, history, TTL,
tombstone, and branch inheritance semantics.

The default remains keep-all. Pruning is opt-in per maintenance request and
must fail closed when any proof input is missing, stale, degraded, or ambiguous.

## Inputs

1. `docs/architecture/storage/l4-log-manifest-snapshot-services.md`
2. `docs/architecture/storage/l5-table-runtime.md`
3. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
4. `docs/architecture/storage/l7-commit-runtime.md`
5. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
6. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`
7. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`
8. `docs/architecture/implementation-plans/M4/L8/l8k-compaction-materialization-scheduling-implementation-plan.md`
9. `docs/architecture/implementation-plans/M4/L8/l8l-retention-proof-snapshot-pruning-implementation-plan.md`
10. `docs/architecture/implementation-plans/M4/L8/l8t-table-manifest-backed-flush-watermarks-implementation-plan.md`
11. `docs/architecture/implementation-plans/M4/L8/l8u-durable-rewrite-publication-implementation-plan.md`
12. `crates/storage-next/src/lifecycle/compaction.rs`
13. `crates/storage-next/src/lifecycle/retention.rs`
14. `crates/storage-next/src/lifecycle/durable/maintenance.rs`
15. `crates/storage-next/src/branch/state.rs`
16. `crates/storage-next/src/branch/read.rs`
17. `crates/storage-next/src/table/compaction.rs`
18. `crates/storage/src/compaction.rs`
19. `crates/storage/src/ttl.rs`
20. `crates/storage/src/segmented/compaction.rs`

## Existing-Code Source Map

| Current file | Evidence | L8V action |
|---|---|---|
| `table/compaction.rs` | Generic compactor already supports a `TableCompactionPolicy` and drop reasons for older versions, tombstones, and expired rows. | Add a storage-owned pruning policy implementation that uses lifecycle/L6 proof facts. Do not embed lifecycle policy into L5. |
| `branch/state.rs` | `BranchCompactionRetentionPolicy::{DropOlderVersions, DropTombstones, DropExpired}` exists but is rejected because proof is absent. | Replace blanket rejection with proof-backed acceptance. Keep rejection for missing/unsafe proofs. |
| `branch/read.rs` | Reads apply version, timestamp, tombstone, TTL, and inherited-layer visibility. | Use the same visibility dimensions in proof validation and reference-model tests. |
| `lifecycle/compaction.rs` | L8K/L8U schedule compaction/materialization and durable rewrite publication. | Extend request/outcome with row-pruning proof, dropped-row summaries, and post-prune timestamp/history coverage. |
| `lifecycle/retention.rs` | L8L/L8S assemble object/snapshot retention proof and recovery-health blockers. | Add row-retention proof facts without mixing object deletion with row pruning. |
| `durable/maintenance.rs` | Durable maintenance runners enqueue/run compaction and materialization. | Route pruning requests only after live retention proof and health freshness are validated. |
| `table/compaction.rs` report | Drop summaries already distinguish `OlderVersion`, `TombstoneElided`, and `Expired`. | Require these summaries in lifecycle outcomes and closeout counters. |

## Old Codebase Porting Map

The old engine had a pruning iterator over segment compaction. L8V ports its
safety rules into storage-next's row/table/branch vocabulary.

| Old source | Behavior to preserve | Rewrite decision | Test focus |
|---|---|---|---|
| `storage/src/compaction.rs::CompactionIterator` | Keeps all versions at or above `prune_floor`, plus one below-floor survivor per key. | Add branch-aware row-pruning policy with an explicit retention floor. | Old-version pruning keeps the floor entry and drops only older rows. |
| `CompactionIterator::with_snapshot_floor` | Active snapshots protect versions from max-version pruning. | Replace old snapshot floor with lifecycle pinned-view/as-of floors and recovered timestamp coverage. | Pinned views and as-of floor prevent pruning. |
| `CompactionIterator::with_is_bottommost` | Non-bottommost compactions preserve tombstones to shadow lower-level values. | Bottommost must mean "no lower owned, inherited, or shared row can be resurrected." | Tombstone elision fails unless lower/inherited safety is proven. |
| `CompactionIterator::with_drop_expired` | Expired TTL rows can be dropped only in bottommost compaction and below the retention floor. | TTL pruning requires timestamp cutoff, version floor, and branch inheritance proof. | Expired rows above floor or needed by `as_of` survive. |
| Issue tests around tombstones and max versions | Tombstones do not count against version caps when they are needed for shadowing. | Preserve tombstone safety ahead of max-version pruning. | Max-version tests cannot drop required tombstones. |
| `ttl.rs::TTLIndex` | Old storage tracked expiry for efficient cleanup. | Do not port a global TTL index in L8V. Use compaction-time proof and row metadata. L8W/L8X can optimize later. | No new unbounded TTL index memory. |

Do not port:

1. direct segment iterators or path handling;
2. product retention configuration;
3. background pruning threads;
4. event-log-specific behavior unless storage-next introduces event rows;
5. pruning from wall-clock "now" without a supplied timestamp cutoff;
6. object deletion/quarantine;
7. primitive/query/vector/graph policy.

## Scope

L8V implements:

1. row-retention proof type for compaction/materialization rewrites;
2. retained commit-version floor and retained timestamp floor validation;
3. active read-view, pinned snapshot, and recovery coverage blockers;
4. branch inheritance blockers for parent rows, child tombstones, and
   materialized replacement rows;
5. tombstone elision proof;
6. TTL-expired row proof;
7. optional max-version-per-key policy with snapshot/as-of protection;
8. L6 acceptance path for proof-backed `DropOlderVersions`,
   `DropTombstones`, and `DropExpired`;
9. L5 policy adapter that decides row drops using L8/L6 proof facts;
10. lifecycle outcome facts for rows kept/dropped by reason;
11. post-prune read/timestamp coverage facts;
12. branch-runtime coverage for as-of/history/TTL/inheritance safety;
13. source guards preventing raw IO, object deletion, product policy, or
    milestone labels in Rust code/test names/fixture bytes.

L8V does not implement:

1. table-object deletion or quarantine;
2. snapshot pruning;
3. WAL truncation or flush-watermark persistence;
4. durable retained-history facts in table-manifest extension bytes;
5. generated/model coverage over durable recovery after pruning;
6. durable rewrite publication mechanics;
7. public API retention configuration;
8. branch delete/clear/fork-at-history completion;
9. memory-budget optimization;
10. lazy reader optimization.

## Safety Model

The pruning proof is a conjunction, not a hint.

Rows may be dropped only if all applicable dimensions prove safety:

1. **Commit-version floor**: the row is older than the retained version floor.
2. **Timestamp floor**: the row cannot satisfy any retained `as_of` timestamp
   query.
3. **Pinned read views**: no active view can observe the row.
4. **Recovery coverage**: WAL/checkpoint/table-manifest facts can recover every
   retained row after pruning.
5. **Branch inheritance**: no child or descendant inherited layer can use the
   row or need a tombstone to shadow it.
6. **Tombstone shadowing**: a tombstone is dropped only if no lower owned,
   inherited, or shared value can be resurrected.
7. **TTL cutoff**: TTL-expired rows are dropped only when their expiry is at or
   below a supplied retention timestamp and the row is below retained history
   floors.
8. **Health freshness**: degraded recovery health or stale proof epochs block
   pruning.

If any fact is unknown, the row is kept.

## Proof Shape

Suggested shape:

```rust
pub(crate) struct LifecycleRowPruningProof {
    branch_id: BranchId,
    proof_epoch: u64,
    recovery_health_epoch: u64,
    retained_version_floor: CommitVersion,
    retained_timestamp_floor: Option<Timestamp>,
    pinned_view_floor: Option<CommitVersion>,
    table_manifest_coverage_floor: CommitVersion,
    allow_tombstone_elision: TombstoneElisionProof,
    allow_ttl_elision: TtlElisionProof,
    max_versions_per_key: Option<usize>,
    inherited_safety: BranchInheritancePruningProof,
}

pub(crate) enum TombstoneElisionProof {
    Disabled,
    BottommostOwnedAndInheritedSafe,
}

pub(crate) enum TtlElisionProof {
    Disabled,
    ExpiredAtOrBefore { timestamp: Timestamp },
}
```

Exact names can change. Required properties:

1. proof is branch-scoped;
2. proof is bound to current recovery health;
3. proof is bound to current L6 reachability/materialization facts;
4. proof exposes retained version and timestamp floors;
5. stale proof fails before output build.

## Pruning Policy

Rules for logical-key row chains sorted by physical key and descending commit
version:

1. Keep all rows at or above the retained version floor.
2. Keep all rows with commit timestamp at or after the retained timestamp floor.
3. Keep the newest below-floor non-tombstone row per logical key unless a
   stronger proof says no retained read can use it.
4. Keep below-floor tombstones unless tombstone elision proof says the compaction
   is bottommost across owned, inherited, and shared sources.
5. Keep tombstones needed to shadow inherited/lower values even when
   `max_versions_per_key` is exhausted.
6. Drop TTL-expired rows only when TTL proof is enabled, the row is below all
   retained floors, and no inherited/tombstone safety rule keeps it.
7. Never drop rows whose branch id does not match the proof branch.
8. Never drop timeline rows unless timeline retention proof is added in this
   slice. V1 should keep timeline rows by default.
9. Never drop metadata rows required to recover branch table manifests,
   materialization provenance, or checkpoint coverage.

## Protocol

Target lifecycle sequence:

```text
require durable or cache open runtime
capture current recovery health, visible version, timestamp coverage, and L6 reachability
build row-pruning proof for the requested branch and scope
validate proof against current branch state and active/pinned views
build L6 compaction request with proof-backed retention policy
run L5 compaction policy adapter
validate output read parity against retained model bounds
install/publish through existing L8K/L8U rewrite path
record drop summaries and new coverage floors
```

Rules:

1. Proof validation happens before output build.
2. Candidate freshness is rechecked after output build and before install.
3. Durable publication order remains owned by L8U.
4. Cache mode may run proof-backed pruning but cannot claim durable object
   deletion or WAL-shortening side effects.
5. If pruning drops any row, the outcome must report the drop reason counts and
   the retained floors used.

## Recovery And Coverage

Pruning changes what historical reads can be served. L8V must update coverage
facts rather than silently narrowing history.

Rules:

1. `as_of` below the retained timestamp floor returns a typed insufficient
   history error, not a misleading absence.
2. `getv`/history below the retained version floor returns a typed retained
   history boundary.
3. Timeline lookup must not point to a version whose row history was pruned
   unless the row result is intentionally unavailable with typed history debt.
4. Recovery from table manifests must preserve the same coverage floors.
5. Durable rewrite manifest must record enough pruning facts to recover the
   coverage boundary.

## Error And Health Vocabulary

Add typed lifecycle/branch errors for:

1. row-pruning proof missing;
2. row-pruning proof stale;
3. row-pruning proof unsafe recovery health;
4. row-pruning proof branch mismatch;
5. retained version floor above visible version;
6. retained timestamp floor above known timestamp coverage;
7. tombstone elision unsafe;
8. TTL elision unsafe;
9. inherited-layer pruning unsafe;
10. pinned view blocks pruning;
11. timeline retention proof missing;
12. row-pruned history requested.

Every error must expose a stable code and preserve source chains.

## Source Boundaries

L8V may import:

1. L6 branch compaction requests and reachability facts;
2. L5 table compaction policy traits;
3. L7 visible/timeline coverage facts;
4. L8 recovery health, retention proof, and maintenance routing;
5. L8U durable rewrite publication outcomes.

L8V must not import:

1. raw filesystem APIs;
2. backend delete APIs;
3. quarantine or purge mutation APIs;
4. object-retention deletion services;
5. product/engine retention policy crates;
6. StrataHub code;
7. primitive/query/vector/graph modules.

Rust code, test names, fixture bytes, and user-facing error strings must not
include milestone labels.

## Implementation Steps

1. Define row-pruning proof and policy request types.
2. Add proof builder over recovery health, visible version, timestamp coverage,
   L6 reachability, pinned views, and inherited-layer facts.
3. Add L5 policy adapter for old-version, tombstone, TTL, and max-version
   decisions.
4. Replace L6 blanket rejection of pruning retention policies with proof-backed
   validation.
5. Thread pruning proof through lifecycle compaction/materialization requests.
6. Record drop summaries, retained floors, and coverage updates in outcomes.
7. Add typed insufficient-history behavior for reads below retained floors.
8. Wire durable rewrite publication to include pruning coverage facts in table
   manifests.
9. Add direct, generated, source-guard, and porting-log coverage.

## Deferred Behavior

Deferred to L8W:

1. memory-optimized pruning scans;
2. bounded TTL candidate indexes;
3. memory-budget admission for large retained-history proofs.

Deferred to L8X:

1. lazy object-backed pruning scans;
2. block-cache-aware pruning.

Deferred to L8Y:

1. positive branch-absence facts as pruning proof;
2. branch delete/clear generation-based pruning.

Deferred to L9:

1. public retention policy configuration;
2. user-facing retention API and diagnostics.

## Exit Gate

L8V is complete when:

1. pruning remains disabled without explicit proof;
2. old-version pruning preserves retained `getv`, history, and `as_of` bounds;
3. tombstone elision cannot resurrect lower or inherited values;
4. TTL pruning cannot remove rows visible to retained timestamp reads;
5. inherited-layer and child-local precedence are preserved;
6. active/pinned views block unsafe pruning;
7. durable table manifests record retained-history coverage;
8. recovery enforces pruned-history boundaries with typed errors;
9. generated tests cover compaction, materialization, TTL, tombstone, and
   inheritance interactions;
10. source guards block raw IO, object deletion, product imports, and milestone
    labels in Rust code.
