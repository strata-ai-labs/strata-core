# #3070 — Multi-branch bundle import (clone reconstitution)

**Status:** design. **Severity:** S1 (StrataHub cannot publish a dataset with
more than one branch — the `raw`/`cleaned`/`train`/`test` view pattern is a core
part of the pitch). **Touches invariant:** MVCC-007 (must be preserved).

## Problem

A bundle carrying more than one branch exports and serves fine, but
`strata clone` cannot reconstitute it. Single-branch bundles are unaffected.
Reported on v1.1.1 (both ends); the StrataHub ingest side (empty branch list)
was fixed separately, leaving this as the remaining blocker.

```
failed_precondition.executor.hub_clone: reconstitution failed:
  staged database failed to materialize: invalid_argument.engine.persistence
```

## Root cause (confirmed)

`crates/hub/src/import.rs::materialize` imports each branch's artifact
**sequentially** into one staging `Database`:

```rust
for artifact in artifacts {
    db.import_branch_artifact(artifact)?;   // one branch at a time
}
```

`import_branch` (`crates/engine/src/artifact/import.rs`) replays a branch's
records at their **original commit timestamps** through the explicit-timestamp
replay seam. The commit-timestamp floor (`CommitTimestampGuard::last_allocated`,
`crates/storage/src/commit/allocator.rs`) is a single **per-database** value:

- Importing branch 1 (`default`) raises the floor to branch 1's **max**
  timestamp.
- Importing branch 2 then replays *its* history, whose timestamps are typically
  **below** branch 1's max — a fork shares `default`'s timestamps, and even the
  structural branch/space creates replay at the branch's *minimum* content
  timestamp. `preview_explicit` (`allocator.rs:129`) rejects any explicit
  timestamp below the floor:

```
InvalidTimestampPolicy { reason: "explicit commit timestamp is before the monotonic floor" }
```

flattened by the adapter to `invalid_argument.engine.persistence`.

A minimal engine-level reproduction (durable target; passes on cache because
cache does not enforce the floor): write two rising timestamps to `default`,
create a second branch below default's max, export both, import both — the
second import fails with the reason above. (This becomes the red test for the
implementation slice.)

### Why the obvious fixes are wrong

- **Relax / lower the floor per branch.** Violates **MVCC-007** ("Commit
  timestamps are monotonically floored: … an explicit timestamp below the floor
  is rejected …"). The floor is a real durability contract, not an accident.
- **Rebase branch 2's timestamps upward.** Breaks re-export byte-identity (the
  HB6b round-trip property StrataHub conformance depends on) and corrupts
  per-branch time-travel (`as_of`).

## Design: reconstruct the true global commit order

The branches were originally written into a single commit stream, interleaved in
real time; the export captured each branch's rows at those original timestamps.
The floor is global precisely because the stream is global. So the import must
replay **all branches together in global timestamp order**, exactly as they were
first written — which keeps every commit ≥ the floor and preserves MVCC-007 with
zero change to the floor's semantics.

New engine entry point — `import_branches(db, &[BranchArtifact])` (generalizing
`import_branch`, which becomes the 1-element case):

1. **Structural facts first, globally ordered.** For every branch: create it if
   absent (`ensure_empty_target_branch`) and create its spaces. These carry no
   payload timestamp; today they "replay at the enclosing content's minimum
   timestamp." Schedule them into the same global order as content so a
   branch/space create never lands below the floor set by another branch's
   later content. (Concretely: emit structural work items tagged with their
   branch at that branch's min content timestamp, and merge them into the global
   schedule below.)
2. **One global schedule.** Build each branch's `WorkItem`s (the existing
   `build_schedule`), tag each with its branch, and merge-sort all of them by
   `(timestamp, branch, section, record)` — a stable, deterministic total order.
3. **Single replay pass.** Replay each item against its tagged branch. Because
   the schedule is globally non-decreasing in timestamp, `preview_explicit`
   never sees a regression.

Emptiness check stays per branch (`ensure_empty_target_branch` still refuses a
target branch that already holds content — `conflict.engine.artifact_import`).
`materialize` calls `import_branches` once instead of looping `import_branch`.

### Preserved properties

- **MVCC-007** — every replayed commit timestamp is ≥ the floor (global order).
- **Re-export byte-identity** — each branch's rows replay at their original
  timestamps in their original per-branch order; a re-export of any branch is
  byte-identical to its source artifact (HB6b).
- **Single-branch path unchanged** — one artifact ⇒ the schedule is exactly
  today's; existing tests must stay green.

## Ask #2: stop flattening the error

`materialize` maps failures to `error.code().to_owned()`, so an operator gets a
bare `invalid_argument.engine.persistence` with no branch, space, or reason —
which is what made this take a while to isolate. Propagate the underlying
`EngineError` (code + message + the `field`/`reason` details, and which branch)
through `BundleImportError` instead of collapsing to a code string. Small,
independent, and worth doing regardless of the ordering fix.

## Implementation plan

- **S1 — error propagation.** `materialize`/`BundleImportError` carry the
  underlying `EngineError` (branch + reason), not a flattened code. Test: a
  forced import failure surfaces the reason. *(Independent; could land first.)*
- **S2 — `import_branches` global schedule.** Add the multi-branch entry point:
  create all branches/spaces, build one globally-sorted schedule (content +
  structural), single replay pass. `import_branch` delegates (1-element case).
  Red test: the 2-branch durable reproduction above → green.
- **S3 — hub `materialize` uses it.** Replace the per-branch loop with one
  `import_branches` call; add a hub-level `import_bundle` round-trip test with
  a two-branch bundle (`crates/hub/tests/import_bundle.rs`).
- **F — conformance.** Re-export byte-identity across a multi-branch round trip;
  invariant check for MVCC-007; a durable reopen (`materialize`'s §6 verify).

## Testing

- Engine: `import_branches` for 2+ branches with overlapping and inverted
  timestamps (fork case + independent-root case), asserting both branches import
  and re-export byte-identical; single-branch unchanged.
- Storage/invariant: MVCC-007 holds after a multi-branch import + durable reopen.
- Hub: two-branch bundle round trip through `import_bundle`.

## Risks / open questions

- **Structural-fact timestamp placement.** Creating a branch/space at its min
  content timestamp within the global order must remain invisible to re-export
  (as today). Verify the branch/space create seam accepts the armed timestamp in
  the interleaved order.
- **A branch with no content** (structural only) still needs its create
  scheduled at a floor-safe timestamp.
- **Determinism.** The global sort key includes branch identity so equal
  timestamps across branches order deterministically (byte-identity across
  targets — the existing `import_is_deterministic_across_targets` property,
  generalized).
