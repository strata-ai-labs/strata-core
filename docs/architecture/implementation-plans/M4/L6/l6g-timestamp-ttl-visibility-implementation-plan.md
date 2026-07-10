# L6G Implementation Plan: Timestamp Reads And TTL Visibility

Status: implemented in storage-next; direct and generated sensitivity probes are covered

Parent plan:
`docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L6/l6g-timestamp-ttl-visibility-test-plan.md`

## Objective

Complete storage-level timestamp-bounded reads and TTL visibility in the
storage-next L6 branch runtime.

L6G turns the `BranchReadBound::AtTimestamp` scaffolding from L6B-L6F into a
real read mode over branch-local and inherited row sources. It keeps the L6 row
model explicit: each retained storage row carries its commit version, commit
timestamp, expiry timestamp, tombstone bit, and value bytes. L6G does not
rebuild the old `VersionedValue` container. Historical values remain separate
storage rows in descending commit-version order.

L6G establishes:

1. timestamp-bounded point reads over active, frozen, branch-owned immutable,
   and inherited table sources;
2. timestamp-bounded prefix and range scans over the same sources;
3. timestamp eligibility based on row commit timestamps;
4. deterministic "newest" selection after timestamp filtering;
5. TTL visibility evaluated at the requested read timestamp, never wall clock;
6. tombstone visibility at timestamp bounds;
7. inherited timestamp reads that also respect fork-version gates;
8. typed insufficient-history facts when a retained-history coverage proof says
   the requested timestamp is not available;
9. generated branch-LSM model coverage for timestamp and TTL behavior;
10. source guards that permit the new timestamp read paths while continuing to
    reject product DTOs, old storage APIs, backend IO, and wall-clock APIs.

## Inputs

1. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
2. `docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-implementation-plan.md`
3. `docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-test-plan.md`
4. `docs/architecture/implementation-plans/M4/L6/l6d-pinned-own-branch-read-views-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/L6/l6d-pinned-own-branch-read-views-test-plan.md`
6. `docs/architecture/implementation-plans/M4/L6/l6f-fork-inherited-layers-implementation-plan.md`
7. `docs/architecture/implementation-plans/M4/L6/l6f-fork-inherited-layers-test-plan.md`
8. `docs/architecture/implementation-plans/M4/L6/m4-l6-porting-log.md`
9. `crates/storage-next/src/branch/{config.rs,error.rs,facts.rs,identity.rs,read.rs,state.rs}`
10. `crates/storage-next/src/row/mod.rs`
11. `crates/storage-next/src/table/{reader.rs,key.rs,mutable.rs}`
12. `crates/storage/src/segmented/mod.rs`
13. `crates/storage/src/stored_value.rs`
14. `crates/storage/src/merge_iter.rs`
15. `crates/storage/src/seekable.rs`

## Existing-Code Source Map

| Current file | L6G evidence | L6G action |
|---|---|---|
| `crates/storage/src/segmented/mod.rs` | Current storage exposes as-of style reads over versioned row chains and applies branch/fork gates before returning values. | Preserve the branch-visible row-chain behavior, but rebuild it over storage-next `StorageRow` facts and L6 read views. |
| `crates/storage/src/stored_value.rs` | Current storage records tombstone and expiry facts in the stored value wrapper. Some paths evaluate expiry against wall-clock time. | Preserve expiry and tombstone semantics as row facts. Do not port product `Value`, `VersionedValue`, or wall-clock expiry evaluation. |
| `crates/storage/src/merge_iter.rs` | Current storage merges retained versions and chooses a visible value after applying read constraints. | Preserve deterministic row-chain selection after filtering, using storage-next source ordering and L5 row keys. |
| `crates/storage/src/seekable.rs` | Inherited reads rewrite source branch keys into child branch keys and apply a fork-version gate. | Reuse L6F rewrite and fork gates, then add the timestamp cap to inherited effective read bounds. |
| `crates/storage-next/src/row/mod.rs` | `StorageRow` already stores `commit_timestamp`, `expires_at`, and `is_tombstone`. Tombstones use `Timestamp::EPOCH` for expiry. | Use these row facts directly. Treat `Timestamp::EPOCH` expiry on put rows as the no-expiry sentinel. |
| `crates/storage-next/src/branch/read.rs` | `BranchReadBound::AtTimestamp`, `BranchEffectiveReadBound::max_commit_timestamp`, and candidate timestamp facts already exist. `effective_own_read_bound` currently rejects timestamp reads. | Retire the L6G rejection, centralize timestamp/TTL visibility helpers, and apply them consistently to point reads and scans. |
| `crates/storage-next/src/branch/state.rs` | Branch state and read-view facts already track timestamp min/max. | Preserve timestamp facts as diagnostics. Add explicit coverage facts only when they can be proven; do not infer insufficient history solely from min/max observed rows. |
| `crates/storage-next/src/testkit/branch_lsm.rs` | Generated branch-LSM scripts already model own/inherited read behavior and intentionally reject timestamp reads before L6G. | Extend the independent model with timestamp bounds, TTL, tombstones, and insufficient-history cases. |

## Scope

L6G implements:

1. `BranchReadBound::AtTimestamp` for own-branch point reads;
2. `BranchReadBound::AtTimestamp` for inherited point reads with effective
   bound `(commit_version <= fork_version) && (commit_timestamp <= T)`;
3. timestamp-bounded prefix scans;
4. timestamp-bounded range scans;
5. shared row-visibility helpers for timestamp eligibility, tombstone
   shadowing, and TTL expiry;
6. exact expiry-boundary semantics: a put row is visible at timestamp `T` only
   when `expires_at == Timestamp::EPOCH || T < expires_at`;
7. selected tombstones at or before `T` shadow older puts;
8. rows with `commit_timestamp > T` are ineligible and cannot shadow older
   rows;
9. deterministic selection of the highest commit version among rows eligible
   for the effective timestamp/version bound;
10. source-order tie breaks unchanged for exact duplicate internal keys;
11. non-monotonic commit timestamp support: timestamp bounds filter rows, but
    the retained row chain remains ordered by commit version;
12. visible-row source facts for timestamp reads;
13. candidate facts that record whether a row matched timestamp and version
    bounds;
14. read-view facts that can carry timestamp-history coverage status;
15. typed insufficient-history errors/facts when a coverage proof marks a
    requested timestamp as outside retained history;
16. history result preservation of timestamp, expiry, tombstone, source, and
    value facts;
17. generated model counters for timestamp point reads, timestamp scans, TTL
    before/at/after expiry, `Timestamp::MAX` expiry, tombstone-at-timestamp
    shadowing, scan boundaries, scan key-space isolation, empty scans,
    non-monotonic timestamps, inherited timestamp fork gates, child-local
    inherited shadowing, and nearest inherited exact ties;
18. source-guard updates that remove the "timestamp reads are premature"
    assertion and add wall-clock and product DTO probes.

L6G does not implement:

1. a public product API;
2. old `VersionedValue`, product `Value`, old `Key`, `Namespace`, or `TypeTag`;
3. wall-clock `now` based TTL evaluation;
4. automatic TTL cleanup or physical deletion;
5. branch compaction policies for dropping TTL-expired rows;
6. durable retained-history proof publication;
7. commit timeline lookup from arbitrary application timestamp to branch
   frontier;
8. remote/hub timestamp synchronization;
9. backend IO, table loading, WAL, manifest publication, or lifecycle
   orchestration;
10. materialization or reachability changes.

## Semantic Rules

### Timestamp Eligibility

For a timestamp read at `T`, a row is timestamp-eligible when:

```text
row.commit_timestamp <= T
```

For inherited layers, the existing fork-version gate is also applied:

```text
row.commit_version <= layer.fork_version
row.commit_timestamp <= T
```

The row timestamp is a visibility filter, not the primary sort key. After the
filter is applied, L6 keeps the retained row-chain rule and chooses the newest
eligible row by descending commit version.

This matters when commit timestamps are non-monotonic. If version 9 has
timestamp 100 and version 8 has timestamp 120, an as-of read at 130 selects
version 9 because version 9 is the newest retained row and both rows are
timestamp-eligible. Direct and generated tests must pin this behavior.

### TTL Visibility

TTL is evaluated only when the read bound carries a timestamp. L6G must not
read wall-clock time.

For put rows:

```text
live_at(T) =
    expires_at == Timestamp::EPOCH || T < expires_at
```

`Timestamp::EPOCH` is the no-expiry sentinel for put rows. A row expiring
exactly at `T` is not visible at `T`.

For tombstones:

1. expiry is ignored;
2. a selected tombstone hides older put rows;
3. tombstones do not return visible value bytes;
4. tombstone rows remain available to history when the caller asks to include
   tombstones.

For `Latest` and `AtVersion` reads without a timestamp, L6G must not invent a
clock. Higher layers that want "current time" TTL behavior must pass an
explicit timestamp read bound.

### Insufficient Timestamp History

Timestamp reads are only unsafe when a caller or retained-history proof says
the requested timestamp predates the retained coverage window. Observed
`timestamp_min` and `timestamp_max` facts are not enough by themselves to prove
history is unavailable, because an empty result before the first write can be a
valid answer.

Add or extend a storage-owned coverage fact equivalent to:

```text
BranchTimestampCoverage =
    Unknown
  | Complete
  | CompleteSince { earliest_timestamp }
```

Rules:

1. `Unknown` allows best-effort timestamp reads and records that no durable
   coverage proof was available.
2. `Complete` means every timestamp is covered by retained history.
3. `CompleteSince { earliest_timestamp }` rejects `AtTimestamp(T)` when
   `T < earliest_timestamp` with a typed insufficient-history result.
4. inherited read coverage is the intersection of child-local coverage and
   readable inherited-layer coverage once inherited coverage facts exist.
5. full durable proof generation is deferred to later retention/lifecycle
   slices, but L6G must define and test the local enforcement point.

The error or outcome must carry:

1. requested timestamp;
2. earliest proven available timestamp when known;
3. branch id;
4. whether the insufficiency came from own state, inherited state, or combined
   coverage;
5. no user value bytes.

## Target Module Shape

Expected production layout after L6G:

```text
crates/storage-next/src/branch/
  mod.rs
  config.rs
  error.rs          # add insufficient timestamp history error/facts if needed
  facts.rs          # timestamp coverage facts if facts.rs owns them
  identity.rs
  read.rs           # timestamp read enablement and visibility helpers
  state.rs          # coverage facts carried into pinned read views
  tests.rs
```

Supporting testkit and guard updates:

```text
crates/storage-next/src/testkit/branch_lsm.rs
crates/storage-next/tests/branch_lsm_properties.rs
crates/storage-next/tests/branch_lsm_source_guard.rs
docs/architecture/implementation-plans/M4/L6/m4-l6-porting-log.md
```

All production items remain `pub(crate)`.

## Proposed Type Surface

Names may change if the responsibilities stay intact.

### Timestamp Coverage Facts

Add a fact type equivalent to:

```text
BranchTimestampCoverage {
    kind: Unknown | Complete | CompleteSince,
    earliest_timestamp: Option<Timestamp>,
}
```

Validation:

1. `Unknown` has no earliest timestamp.
2. `Complete` has no earliest timestamp.
3. `CompleteSince` must have an earliest timestamp.
4. coverage facts are facts, not scheduling commands;
5. coverage facts contain no backend paths or object names.

### Insufficient History Result

Add a typed error or read outcome equivalent to:

```text
BranchRuntimeError::InsufficientTimestampHistory {
    branch_id: BranchId,
    requested_timestamp: Timestamp,
    earliest_available_timestamp: Option<Timestamp>,
    source: BranchTimestampCoverageSource,
}
```

If the existing result surface makes an error too disruptive, an equivalent
typed read status is acceptable, but `None` without a status is not acceptable
when coverage is known insufficient.

### Visibility Helpers

Centralize helpers equivalent to:

```text
row_matches_effective_bound(row, effective_bound)
row_is_live_put_at(row, read_timestamp)
candidate_is_visible_for_bound(candidate, effective_bound)
selected_candidate_for_bound(candidates, effective_bound)
```

Rules:

1. no helper may read wall-clock time;
2. no helper may inspect product payloads;
3. TTL is evaluated only when `effective_bound.max_commit_timestamp` is
   present;
4. tombstone handling stays separate from TTL handling;
5. point and scan reads must call the same selection helper.

### Read View Surface

The existing read methods remain the primary surface:

```text
BranchReadView::read_point(key, BranchReadBound::AtTimestamp(T))
BranchReadView::scan_prefix(bounds, BranchReadBound::AtTimestamp(T))
BranchReadView::scan_range(bounds, BranchReadBound::AtTimestamp(T))
```

Convenience wrappers such as `at_timestamp` or `as_of` are acceptable if they
delegate to the same path and do not create public API exposure.

History remains storage-history by default. It should preserve expired and
tombstone rows when requested; product-facing "visible history as of T" belongs
above L6 unless L6G adds an explicit storage-owned timestamp-history option.

## Implementation Steps

### L6G-A: Enable Timestamp Read Bounds

1. Remove the `AtTimestamp` rejection from `effective_own_read_bound`.
2. Use `BranchEffectiveReadBound::for_own_branch` for all three read bounds.
3. Keep wrong-branch validation before candidate collection.
4. Preserve source-order and commit-version ordering for selected candidates.
5. Add direct tests proving `AtTimestamp` no longer returns
   `InvalidReadBound`.

### L6G-B: Add Central Visibility Helpers

1. Move point and scan selection through one helper.
2. Filter candidates by effective version and timestamp bounds.
3. After filtering, sort newest-first by commit version and source order.
4. Apply tombstone and TTL visibility to the selected candidate.
5. Return `None` for selected tombstone or selected expired put.
6. Do not fall through selected tombstones or expired selected rows to older
   rows unless a documented future retention policy explicitly changes that
   rule. The parent plan treats "newer expired row makes latest return none at
   a read timestamp after expiry" as the expected behavior.

### L6G-C: Apply TTL At Requested Timestamp

1. Treat `Timestamp::EPOCH` expiry as no expiry for put rows.
2. Treat `expires_at <= T` as expired for put rows.
3. Ignore expiry on tombstones.
4. Preserve value bytes for live rows, including empty values.
5. Add boundary tests for before expiry, exact expiry, after expiry,
   `Timestamp::EPOCH`, and `Timestamp::MAX`.

### L6G-D: Extend Timestamp Scans

1. Apply the same effective bound and selected-candidate visibility helper to
   prefix scans.
2. Apply the same helper to range scans.
3. Keep scan grouping by rewritten child physical key.
4. Ensure invisible selected rows suppress output for that key instead of
   falling through to older rows.
5. Preserve scan output order by branch-local physical key.

### L6G-E: Inherited Timestamp Reads

1. Keep L6F source-to-child branch rewrite before grouping.
2. Keep L6F inherited effective bound for timestamps:
   `(commit_version <= fork_version) && (commit_timestamp <= T)`.
3. Preserve nearest-ancestor source ordering for exact internal-key ties.
4. Add tests where a source row after the fork has an old timestamp and must
   still be hidden by the fork-version gate.
5. Add tests where a source row before the fork has a future timestamp and
   must be hidden by the timestamp gate.

### L6G-F: Timestamp Coverage Facts

1. Add local timestamp coverage facts to read views or state facts.
2. Default new branch-local states to `Unknown` unless a caller supplies a
   stronger proof.
3. Reject known-insufficient timestamp reads before candidate selection.
4. Preserve coverage source facts in errors.
5. Add tests that prove `timestamp_min` alone does not create an
   insufficient-history error.

### L6G-G: Generated Model And Source Guards

1. Extend the branch-LSM generated model with timestamp-bound reads.
2. Extend generated scripts with TTL before/at/after expiry.
3. Extend generated scripts with non-monotonic commit timestamps.
4. Extend generated scripts with inherited timestamp plus fork-version gates.
5. Retire the L6F generated expectation that timestamp reads are rejected.
6. Update source guards to forbid `Timestamp::now`, `SystemTime`,
   `Instant::now`, `std::time`, old storage `VersionedValue`, product `Value`,
   old `Key`, `Namespace`, and `TypeTag`.

### L6G-H: Porting Log And Verification

1. Add an L6G porting-log entry before branch code changes.
2. Record old storage TTL behavior that was intentionally not ported because
   it depends on wall clock and product value wrappers.
3. Record all new tests and sensitivity probes.
4. Run the L6 verification command set from the test plan.

## Edge Cases To Pin

1. requested timestamp before every retained row;
2. requested timestamp exactly equal to a row commit timestamp;
3. requested timestamp between two retained row timestamps;
4. requested timestamp after every retained row;
5. selected row is a tombstone;
6. selected row is expired exactly at requested timestamp;
7. selected row expires after requested timestamp;
8. newest row has timestamp above bound, older row is eligible;
9. higher commit version has lower timestamp than lower commit version;
10. inherited row commit version above fork but timestamp below bound;
11. inherited row commit version below fork but timestamp above bound;
12. child tombstone at timestamp shadows inherited put;
13. child expired put at timestamp suppresses inherited put for the same key;
14. nearest inherited layer wins exact ties after timestamp filtering;
15. prefix and range scans suppress keys whose selected row is expired;
16. empty user keys and high-bit user keys still compare correctly;
17. wrong-branch timestamp read fails before payload inspection;
18. insufficient-history errors do not include value bytes;
19. pinned timestamp read views are stable after later appends, freezes, owned
    table installs, or source branch mutations.

## Verification Commands

Required before closing L6G:

```bash
cargo test -p strata-storage-next --locked --lib branch
cargo test -p strata-storage-next --locked --test branch_lsm_source_guard
cargo test -p strata-storage-next --features testkit --locked --test branch_lsm_properties
cargo test -p strata-storage-next --no-default-features --features testkit --locked --test branch_lsm_properties
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

Run the broader crate test set if L6G changes shared row, table, or branch
helpers:

```bash
cargo test -p strata-storage-next --locked
```

## Exit Criteria

L6G is complete when:

1. timestamp point reads work over active, frozen, owned immutable, and
   inherited rows;
2. timestamp prefix and range scans work over the same sources;
3. TTL is evaluated only against the requested timestamp;
4. exact-expiry behavior is documented and tested;
5. tombstone-at-timestamp behavior is documented and tested;
6. inherited timestamp reads respect fork-version gates;
7. non-monotonic timestamps are handled deterministically;
8. known-insufficient timestamp history returns typed facts;
9. generated tests exercise every timestamp/TTL/inherited-shadow category at
   least once;
10. source guards reject wall-clock and product DTO drift;
11. porting log records preserved behavior, intentional V1 changes, tests, and
    retirement status;
12. all verification commands pass.
