# L6B Implementation Plan: Branch Row Identity And Read Bounds

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L6/l6b-branch-row-identity-read-bounds-test-plan.md`

## Objective

Add the pure branch-row helper layer that lets L6 reason about branch-local
storage rows and read bounds without owning mutable state, immutable tables, or
inherited-layer iteration yet.

L6B establishes:

1. branch-local physical-key and row validation helpers;
2. storage-row branch-id rewrite helpers for inherited rows;
3. effective read-bound facts for own-branch and inherited-layer reads;
4. version and timestamp bound comparison helpers;
5. row candidate/visibility facts that preserve tombstone, timestamp, expiry,
   and source metadata without selecting a final value;
6. generated and direct tests for row-chain mechanics;
7. porting-log evidence for the old key-rewrite and MVCC-bound behavior.

L6B should make L6C-L6G smaller by centralizing branch identity and bound
logic. It must not implement branch state mutation, table lookup, final latest
selection, scans, fork creation, or materialization.

## Inputs

1. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
2. `docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-implementation-plan.md`
3. `docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-test-plan.md`
4. `docs/architecture/implementation-plans/M4/L6/l6a-branch-runtime-scaffold-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/L6/m4-l6-porting-log.md`
6. `crates/storage-next/src/branch/{mod.rs,read.rs,error.rs,facts.rs}`
7. `crates/storage-next/src/row/mod.rs`
8. `crates/storage-next/src/table/key.rs`
9. `crates/storage-next/src/format/key.rs`
10. `crates/storage/src/key_encoding.rs`
11. `crates/storage/src/merge_iter.rs`
12. `crates/storage/src/seekable.rs`

## Existing-Code Source Map

| Current file | L6B evidence | L6B action |
|---|---|---|
| `crates/storage/src/key_encoding.rs` | Branch id is the first fixed-width field in the ordered physical/internal key. Old code rewrites branch ids by replacing those bytes. | Preserve the semantic rule using storage-next `PhysicalKey`/`StorageRow` constructors, not byte surgery in L6. |
| `crates/storage/src/merge_iter.rs` | `RewritingIterator` filters source rows by `commit_id <= fork_version` and rewrites source branch id to child branch id before MVCC grouping. | Port the pure rewrite and fork-version cap facts only. Iteration and grouping land in L6D/L6F. |
| `crates/storage/src/seekable.rs` | `RewritingSeekableIter` rewrites seek targets child -> source and output rows source -> child while applying the fork-version gate. | Record seek-bound rewrite requirements, but implement only row/key rewrite helpers in L6B. Seekable inherited reads land later. |
| `crates/storage-next/src/row/mod.rs` | `PhysicalKey` owns branch id, space, storage-space id, and user key; `StorageRow` owns version, timestamp, expiry, tombstone, and value facts. | Use this as the only row model. Do not add a new durable row type. |
| `crates/storage-next/src/table/key.rs` | L5 already has encoded key wrappers and sort/bounds helpers. | Use L5 for ordering facts only when useful; L6B must not import L4/backend/object layout. |

## Scope

L6B implements:

1. a branch row identity module, likely `crates/storage-next/src/branch/identity.rs`;
2. validation that a `PhysicalKey` or `StorageRow` belongs to an expected
   `BranchId`;
3. helpers to rewrite a row from one branch id to another while preserving all
   non-branch row facts;
4. helpers to rewrite physical keys for seek/bounds preparation without
   constructing product keys;
5. an effective read-bound type that can represent version caps, timestamp
   caps, or both;
6. helpers that derive own-branch and inherited-layer effective bounds from
   `BranchReadBound` and a fork version;
7. row bound-axis facts for `commit_version <= max_version` and
   `commit_timestamp <= max_timestamp`;
8. row candidate facts that preserve source, tombstone, expiry, and bound-axis
   decisions;
9. direct tests, generated tests, and source-guard updates;
10. M4-L6 porting-log entries for branch identity and read-bound mechanics.

L6B does not implement:

1. `BranchState` storage;
2. append committed rows;
3. active/frozen rotation;
4. immutable table install or table reader calls;
5. final latest/getv/as-of/history selection;
6. prefix/range scan merge behavior;
7. fork creation or inherited-layer state capture;
8. materialization;
9. TTL/live-value policy beyond preserving expiry facts and comparing a row
   timestamp against a read timestamp;
10. compaction candidate selection;
11. snapshot row install;
12. commit-version allocation;
13. WAL, manifest, object, or backend IO;
14. product DTO conversion such as `VersionedValue`.

## Target Module Shape

Expected production layout after L6B:

```text
crates/storage-next/src/branch/
  mod.rs
  config.rs
  error.rs
  facts.rs
  identity.rs      # new pure row/key identity helpers
  read.rs          # extend read-bound and candidate-fact helpers
  state.rs
  tests.rs
```

Supporting testkit changes:

```text
crates/storage-next/src/testkit/branch_lsm.rs
crates/storage-next/tests/branch_lsm_properties.rs
crates/storage-next/tests/branch_lsm_source_guard.rs
docs/architecture/implementation-plans/M4/L6/m4-l6-porting-log.md
```

All production items remain `pub(crate)`.

## Proposed Type Surface

Names may change if the responsibilities stay intact.

### Branch Row Identity

Add a small identity/fact wrapper for a row under an expected branch:

```text
BranchRowIdentity {
    branch_id: BranchId,
    physical_key: PhysicalKey,
    commit_version: CommitVersion,
    commit_timestamp: Timestamp,
}
```

Responsibilities:

1. construct from a `StorageRow` only after branch id validation;
2. expose branch id, physical key, commit version, and commit timestamp;
3. never expose or clone value bytes for diagnostics;
4. return `BranchRuntimeError::InvalidBranchRow` for branch mismatches or
   impossible row facts;
5. treat branch id bytes as opaque storage atoms.

This wrapper is optional if helper functions can preserve the same facts
clearly, but the tests should still pin the behavior.

### Branch-Local Validation Helpers

Add helpers equivalent to:

```text
require_physical_key_branch(expected_branch_id, physical_key)
require_row_branch(expected_branch_id, row)
row_matches_branch(expected_branch_id, row) -> bool
```

Rules:

1. own-branch install helpers fail on branch mismatch;
2. checks use `PhysicalKey::branch_id()`, not encoded byte offsets;
3. checks do not inspect space names, storage-space ids, user key bytes, row
   values, tombstone flags, or expiry except when preserving facts for
   diagnostics;
4. mismatches are typed as branch-row errors, not table decode errors.

### Branch Rewrite Helpers

Add helpers equivalent to:

```text
rewrite_physical_key_branch(key, target_branch_id) -> PhysicalKey
rewrite_row_branch(row, source_branch_id, target_branch_id) -> BranchRuntimeResult<StorageRow>
```

Rules:

1. the source row must match `source_branch_id` before rewrite;
2. rewritten row uses `target_branch_id`;
3. space, storage-space id, user key, commit version, commit timestamp,
   expiry, tombstone flag, and value bytes are preserved exactly;
4. put rows remain put rows, including empty-value puts;
5. tombstone rows remain tombstones with no value bytes;
6. rewriting to the same branch is accepted and returns an equivalent row;
7. rewrite is reversible for valid source/target branch ids;
8. implementation uses `PhysicalKey::new` and `StorageRow::{put,tombstone}`,
   not ad hoc byte splicing.

L6F later uses these helpers for inherited layers. L6H later uses them for
materialization.

### Effective Read Bounds

`BranchReadBound` from L6A can represent the caller request:

```text
Latest
AtVersion(CommitVersion)
AtTimestamp(Timestamp)
```

L6B should add an effective bound shape able to represent combined inherited
constraints:

```text
BranchEffectiveReadBound {
    max_commit_version: Option<CommitVersion>,
    max_commit_timestamp: Option<Timestamp>,
}
```

Rules:

1. own-branch latest has no version cap and no timestamp cap;
2. own-branch `AtVersion(v)` has `max_commit_version = v`;
3. own-branch `AtTimestamp(t)` has `max_commit_timestamp = t`;
4. inherited latest has `max_commit_version = fork_version`;
5. inherited `AtVersion(v)` has
   `max_commit_version = min(v, fork_version)`;
6. inherited `AtTimestamp(t)` has both
   `max_commit_version = fork_version` and `max_commit_timestamp = t`;
7. a bound with both caps requires a row to satisfy both caps;
8. `CommitVersion::new(0)` and `Timestamp::EPOCH` are valid exact caps unless
   an existing lower layer forbids them.

This avoids losing the fork-version gate for timestamp-bounded inherited reads.

### Bound And Candidate Facts

Add row candidate facts equivalent to:

```text
BranchRowCandidateFacts {
    source: BranchRowSource,
    physical_key: PhysicalKey,
    commit_version: CommitVersion,
    commit_timestamp: Timestamp,
    expires_at: Timestamp,
    is_tombstone: bool,
    version_in_bound: bool,
    timestamp_in_bound: bool,
    matches_effective_bound: bool,
}
```

Responsibilities:

1. preserve row metadata exactly;
2. classify whether a row is eligible under the effective read bound;
3. avoid selecting the final visible row from a row chain;
4. avoid TTL/live-value policy, except preserving `expires_at` for L6G;
5. avoid old `VersionedValue` or product DTO vocabulary.

`BranchVisibleRow` remains a result shell from L6A. L6B should not turn
candidate facts into final visible rows; that belongs to L6D/L6G.

## Source Guards

The existing L6 source guards still apply:

1. no upper-layer imports;
2. no product DTOs;
3. no backend, filesystem, WAL, checkpoint, or object layout APIs;
4. no public production branch API.

L6B may add pure helper functions in new branch files. The L6A-only premature
behavior guard is scoped to scaffold files and should remain that way. Do not
add function names such as `read_latest`, `as_of`, `prefix_scan`, or
`compact_branch` in L6B.

## Implementation Steps

### L6B-A: Porting Log And Source Audit

1. Add an `L6B` entry to `m4-l6-porting-log.md`.
2. Record old-code evidence from `key_encoding.rs`, `merge_iter.rs`, and
   `seekable.rs`.
3. Record explicit deferrals for final read selection, inherited iteration,
   TTL policy, and branch state mutation.

Exit: the porting log names preserved mechanics and deferred behavior before
production code changes.

### L6B-B: Module Wire-Up

1. Add `identity.rs` or equivalent under `src/branch/`.
2. Wire it from `branch/mod.rs`.
3. Re-export only crate-private helper types/functions needed by later L6
   slices.

Exit: the module compiles with no behavior beyond pure row/key helpers.

### L6B-C: Branch-Local Validation

1. Implement physical-key branch matching.
2. Implement row branch matching.
3. Return typed branch-row errors for mismatches.
4. Add direct tests for matching and mismatching rows.

Exit: L6 can reject wrong-branch rows before state mutation.

### L6B-D: Row And Physical-Key Rewrite

1. Implement physical-key branch rewrite.
2. Implement storage-row branch rewrite with source-branch preflight.
3. Preserve put/tombstone shape and all metadata.
4. Add round-trip/reversibility tests.

Exit: inherited rows can be mechanically projected into a child branch
namespace without table/state code.

### L6B-E: Effective Read Bounds

1. Add `BranchEffectiveReadBound` or equivalent.
2. Add own-branch effective bound construction.
3. Add inherited-layer effective bound construction with fork-version cap.
4. Add bound comparison helpers for rows.

Exit: version and timestamp caps are represented without losing combined
constraints.

### L6B-F: Candidate Facts

1. Add row bound-axis facts.
2. Add candidate fact construction from row, source, and effective bound.
3. Keep candidate facts separate from final `BranchVisibleRow` selection.
4. Add tests proving tombstone and expiry facts are preserved, not interpreted.

Exit: later read code has one mechanical fact surface for row-chain
evaluation.

### L6B-G: Testkit Generated Route

1. Extend `check_branch_lsm_scaffold_contract` or add a second hidden testkit
   route for row identity/read-bound scripts.
2. Generate opaque branch ids, storage rows, requested bounds, fork versions,
   and rewrite targets.
3. Count nonzero coverage across branch match, mismatch, rewrite, own bounds,
   inherited bounds, candidate facts, storage-owned edge rows, encoded-key
   grouping after branch rewrite, row-chain filtering, and fork-boundary caps.

Exit: property tests exercise real L6B behavior instead of only constructors.

### L6B-H: Documentation Closeout

1. Update this plan and the L6B test plan if names changed.
2. Update parent plan links if needed.
3. Record tests, sensitivity probes, and commands in the porting log.
4. Run closeout commands.

Exit: L6B is ready for L6C branch-local mutable/frozen state.

## Verification Commands

Minimum L6B commands:

```bash
cargo test -p strata-storage-next --locked --lib branch
cargo test -p strata-storage-next --locked --test branch_lsm_source_guard
cargo test -p strata-storage-next --features testkit --locked --test branch_lsm_properties
cargo test -p strata-storage-next --no-default-features --features testkit --locked --test branch_lsm_properties
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

Run the full package test if L6B changes shared row/table helpers or testkit
exports:

```bash
cargo test -p strata-storage-next --locked
```

Run the wasm no-default check if feature gates or testkit routing changes:

```bash
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
```

## Sensitivity Probes

Before marking L6B complete, temporarily introduce local mutations and verify
tests fail:

1. accept a wrong-branch own row;
2. rewrite only the branch id but drop expiry;
3. rewrite a tombstone into a put row;
4. forget to preflight source branch before rewrite;
5. inherited `AtTimestamp` drops the fork-version cap;
6. version bound uses `<` instead of `<=`;
7. timestamp bound uses `<` instead of `<=`;
8. candidate facts hide tombstones or expired-looking rows;
9. introduce a product DTO term such as `VersionedValue`;
10. introduce a backend call such as `read_object(...)`.

Record the probe results in the L6B porting-log entry.

## Exit Criteria

L6B is complete when:

1. branch-local row/key validation is implemented and tested;
2. row and physical-key branch rewrites are implemented and tested;
3. own and inherited effective read bounds are represented without losing
   combined version/timestamp caps;
4. row bound-axis/candidate facts preserve all row metadata;
5. generated testkit coverage exercises all L6B helper categories;
6. source guards still enforce L6 boundaries;
7. no branch state, table IO, backend IO, lifecycle, commit runtime, or product
   DTO behavior is introduced;
8. closeout commands pass;
9. the L6B porting-log entry records preserved, changed, deferred, and
   sensitivity-probe outcomes.
