# L6F Implementation Plan: Fork And Inherited Layers

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L6/l6f-fork-inherited-layers-test-plan.md`

## Objective

Add storage-level fork and inherited-layer read mechanics to storage-next L6.

L6F extends the L6E branch runtime from branch-owned active, frozen, and
immutable sources to copy-on-write inherited immutable layers. A child branch
can read source-branch immutable table state at a fork version without copying
rows. Reads rewrite inherited rows into the child branch namespace before MVCC
grouping, apply the inherited layer's fork-version gate, and let child-local
rows or tombstones shadow inherited rows.

L6F establishes:

1. inherited layer state backed by L5 table readers;
2. storage-level fork capture over a source branch's immutable table state;
3. inherited read-view pinning;
4. source-to-child row-key rewriting for inherited candidates;
5. fork-version gates for inherited rows;
6. nearest-ancestor-first inherited layer ordering;
7. child-local shadowing of inherited values and tombstones;
8. read parity for latest, getv, history, prefix, and range reads;
9. explicit deferral of timestamp/as-of policy, materialization, reachability
   publication, and durable branch manifest integration.

L6F should make L6G, L6H, L6I, and L6J smaller by centralizing the in-memory
inheritance read model before timestamp policy, materialization, reachability,
and branch compaction are layered on top.

## Inputs

1. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
2. `docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-implementation-plan.md`
3. `docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-test-plan.md`
4. `docs/architecture/implementation-plans/M4/L6/l6e-branch-owned-immutable-levels-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/L6/l6e-branch-owned-immutable-levels-test-plan.md`
6. `docs/architecture/implementation-plans/M4/L6/m4-l6-porting-log.md`
7. `crates/storage-next/src/branch/{config.rs,error.rs,facts.rs,identity.rs,read.rs,state.rs}`
8. `crates/storage-next/src/table/{builder.rs,reader.rs,facts.rs,key.rs,mutable.rs}`
9. `crates/storage-next/src/row/mod.rs`
10. `crates/storage/src/segmented/mod.rs`
11. `crates/storage/src/seekable.rs`
12. `crates/storage/src/key_encoding.rs`
13. `crates/storage/src/manifest.rs`
14. `crates/storage/src/segmented/ref_registry.rs`

## Existing-Code Source Map

| Current file | L6F evidence | L6F action |
|---|---|---|
| `crates/storage/src/segmented/mod.rs` | `fork_branch` flushes source mutable state, captures source immutable levels, prepends the source-owned layer, preserves existing inherited layers nearest-first, records fork version from the source max applied version, and rejects self-fork. Point reads and scans search child-local sources before inherited layers. | Port the storage mechanics into storage-next in-memory L6 types. Keep durable manifest publication, refcount barriers, filesystem cleanup, and recovery outside L6F. |
| `crates/storage/src/seekable.rs` | `RewritingSeekableIter` applies a fork-version gate and rewrites source branch id to child branch id so inherited and child-local rows group under one MVCC key. | Implement row-level inherited rewrite for L5 `StorageRow` candidates, not old `InternalKey`/`MemtableEntry` iterators. |
| `crates/storage/src/key_encoding.rs` | Branch-id rewrite helpers prove the old design rewrites only branch identity and preserves the remaining logical key bytes. | Use storage-next `rewrite_row_branch`/physical-key helpers and test that space, storage-space id, user key, version, timestamp, expiry, tombstone bit, and value bytes are preserved. |
| `crates/storage/src/manifest.rs` | Branch manifests persisted inherited layer descriptors with source branch, fork version, segment references, and status. | Use this as evidence for descriptor fields only. Durable manifest format and publication are L8/L4 work. |
| `crates/storage/src/segmented/ref_registry.rs` | Old inherited layers protect shared segment files via runtime refcounts plus durable manifest facts. | Defer shared-table reachability and release facts to L6I. L6F can expose counts/facts but must not release tables. |
| `crates/storage-next/src/branch/facts.rs` | `InheritedLayerDescriptor`, `InheritedLayerStatus`, inherited counts, and inherited runtime stats already exist as L6A scaffolding. | Promote the descriptor from shell fact to actual inherited read-view state. |
| `crates/storage-next/src/branch/read.rs` | `BranchRowSource::Inherited`, `BranchEffectiveReadBound::for_inherited_layer`, and `source_order` are present, but `BranchReadView` currently rejects inherited facts. | Add inherited layer storage to read views and collect rewritten inherited candidates for point/history/scans. |
| `crates/storage-next/src/branch/state.rs` | `BranchLocalState` owns active/frozen/owned immutable levels and captures pinned read views. | Add fork construction and inherited layer attachment without importing backends or durable services. |
| `crates/storage-next/src/table/reader.rs` | `ImmutableTableReader` exposes decoded rows and table facts. | Reuse L5 readers for inherited table references. Do not duplicate table decoding in L6. |

## Scope

L6F implements:

1. an inherited immutable table wrapper that records source branch, source
   level/table index, fork version, and L5 reader facts;
2. an inherited layer state containing source branch id, fork version, status,
   and immutable levels nearest to farthest;
3. validation that active inherited layers contain only rows from their source
   branch;
4. validation that a child branch cannot directly inherit from itself;
5. validation that inherited layer counts respect
   `BranchRuntimeConfig::max_inherited_layers`;
6. fork capture from a source branch read/state snapshot into a destination
   branch with empty own state and inherited layers;
7. preservation of source inherited layers in deterministic nearest-first
   ancestry order;
8. reset of copied active/materializing inherited-layer status to `Active`
   in the child;
9. propagation of the fork version into destination branch facts so later forks
   from the child know inherited data exists up to that version;
10. read-view pinning for inherited layers;
11. inherited point-read candidate collection;
12. inherited history candidate collection;
13. inherited prefix/range scan candidate collection;
14. source-to-child row-key rewriting before grouping or source comparison;
15. fork-version gating through
   `BranchEffectiveReadBound::for_inherited_layer`;
16. child-local source precedence for equal internal keys;
17. nearest-inherited-layer precedence for equal rewritten internal keys;
18. source attribution as `BranchRowSource::Inherited { source_branch_id,
   layer_index }`;
19. runtime stats/facts for inherited layers examined;
20. direct tests, generated tests, source-guard updates, and porting-log notes.

L6F does not implement:

1. durable branch manifest publication;
2. object layout, backend IO, or service table loading;
3. WAL-before-visible discipline for fork publication;
4. shared-table reachability registry, release facts, or retention proofs;
5. materialization of inherited rows into child-owned immutable tables;
6. timestamp/as-of reads or TTL-at-read-time policy;
7. branch compaction, tombstone pruning, or inherited retention policy;
8. snapshot row install;
9. product branch workflow, product branch names, or public API exposure;
10. commit-version allocation or commit-runtime coordination;
11. automatic source flush scheduling. If an upper layer needs fork to include
    mutable source rows, it must flush/install those rows into immutable source
    state before calling the L6F capture primitive.

## Target Module Shape

Expected production layout after L6F:

```text
crates/storage-next/src/branch/
  mod.rs
  config.rs
  error.rs
  facts.rs          # extend inherited descriptor/facts
  identity.rs
  read.rs           # inherited candidate collection and rewrite
  state.rs          # fork capture and inherited layer attachment
  inheritance.rs    # optional if read/state would otherwise grow too large
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

### Inherited Table Reference

Add an inherited source wrapper equivalent to:

```text
BranchInheritedTable {
    source_branch_id: BranchId,
    fork_version: CommitVersion,
    source_level: BranchLevel,
    source_table_index: usize,
    reader: ImmutableTableReader,
}
```

Rules:

1. every row in the reader must physically belong to `source_branch_id`;
2. source level and table index are diagnostic/source facts, not sort keys for
   MVCC selection;
3. table identity remains opaque and is not interpreted as a path;
4. reader facts must not be recomputed by L6;
5. construction errors must not include row value bytes;
6. table rows are not copied or rewritten during construction.

### Inherited Layer

Add inherited layer state equivalent to:

```text
BranchInheritedLayer {
    descriptor: InheritedLayerDescriptor,
    owned_levels: Vec<Vec<BranchInheritedTable>>,
}
```

Rules:

1. `descriptor.source_branch_id != child_branch_id`;
2. `descriptor.status == Active` participates in reads;
3. `Materializing` remains readable until L6H completes materialization;
4. `Materialized` is skipped by inherited reads;
5. `Unavailable` fails closed for new read-view construction unless the caller
   explicitly requests a diagnostic-only view;
6. `descriptor.table_count` must equal the number of inherited tables;
7. layer count must be bounded by branch runtime config;
8. layers are stored nearest ancestor first;
9. copied active/materializing inherited layers preserve their readable status
   in the child, while materialized layers are skipped because their
   replacement child-owned state is already the readable source;
10. an empty source-owned layer is allowed only when it exists to preserve a
    valid fork-version boundary; otherwise prefer zero inherited layers.

### Fork Capture

Add a storage-level fork helper equivalent to:

```text
BranchForkRequest {
    source_branch_id: BranchId,
    destination_branch_id: BranchId,
    requested_fork_version: Option<CommitVersion>,
}

BranchForkOutcome {
    source_branch_id: BranchId,
    destination_branch_id: BranchId,
    fork_version: CommitVersion,
    inherited_layer_count: usize,
    inherited_table_count: usize,
}
```

The first implementation may expose narrower helpers, for example:

```text
BranchLocalState::fork_into_empty_child(destination_branch_id)
BranchLocalState::attach_inherited_layers(layers)
```

That is acceptable if the same invariants are enforced and the test plan names
the shipped surface.

Fork rules:

1. source and destination branch ids must differ;
2. destination own state starts empty;
3. source active and frozen rows are not implicitly inherited by L6F;
4. source branch-owned immutable levels become the first inherited layer;
5. source inherited layers are appended after the source-owned layer;
6. the shipped L6F helper uses the source max applied commit version as the
   fork version;
7. retained historical fork-version requests are deferred until a caller-owned
   retained-history proof API exists;
8. once retained fork requests exist, the requested historical fork version
   must not exceed the source max applied version;
9. retained-history proof for old fork versions is deferred to L6I/L8;
10. destination facts include the inherited max version/timestamp facts;
11. failure leaves destination state unchanged.

### Branch Read View

Extend `BranchReadView` equivalent to:

```text
BranchReadView {
    branch_id
    active
    frozen
    owned_levels
    inherited_layers
    facts
}
```

Capture rules:

1. capture clones the inherited layer descriptors and L5 reader handles
   visible at capture time;
2. later source branch writes or installs do not affect an already captured
   child view;
3. later child branch writes or installs do not affect an already captured
   child view;
4. view validation accepts `inherited_layer_count > 0` once L6F lands;
5. view validation rejects stale inherited counts or mismatched table counts;
6. view validation rejects inherited rows whose physical branch id does not
   match the layer source branch;
7. view validation rejects direct self-inheritance.

### Inherited Candidate Rewrite

Inherited read collection should use a helper equivalent to:

```text
rewrite_inherited_candidate(
    child_branch_id,
    source_branch_id,
    source_row,
) -> BranchRuntimeResult<StorageRow>
```

Rules:

1. the source row must physically belong to `source_branch_id`;
2. the rewritten row must physically belong to `child_branch_id`;
3. storage space, logical space, user key, commit version, commit timestamp,
   expiry timestamp, tombstone flag, and value bytes are preserved;
4. rewrite occurs before scan grouping;
5. rewrite occurs before comparing child-local and inherited candidates;
6. rewrite failures are typed `BranchRuntimeError::InvalidInheritedLayer`;
7. error messages must not include row value bytes.

### Read Semantics

For child-local sources:

```text
effective own bound = requested version/timestamp bound
```

For inherited sources:

```text
effective inherited bound =
  min(requested version bound or latest, layer.fork_version)
  plus requested timestamp bound when L6G enables timestamp reads
```

L6F keeps timestamp reads rejected as L6G-owned behavior.

Candidate ordering rules:

1. collect child-local active, frozen, and branch-owned immutable candidates;
2. collect inherited candidates from active/materializing layers nearest
   ancestor first;
3. rewrite inherited candidates to the child branch before grouping;
4. group by rewritten physical key;
5. sort each row chain by commit version descending;
6. for exact equal internal keys, prefer child-local sources over inherited
   sources;
7. for exact equal inherited internal keys, prefer lower `layer_index`
   (nearest ancestor);
8. apply the effective bound for each source;
9. a selected tombstone returns no visible latest/getv result and suppresses
   older rows;
10. history preserves tombstones according to existing history options;
11. prefix/range scans return at most one visible row per physical key.

## Implementation Steps

### L6F-A: Source Map And Porting Log

1. Read old fork, inherited read, and rewriting iterator code.
2. Record the exact source files and preserved semantics in
   `m4-l6-porting-log.md`.
3. Record explicit V1 deferrals for durable reachability, materialization,
   timestamp reads, and source mutable flush scheduling.

### L6F-B: Inherited Layer Types

1. Add or extend inherited table/layer structs.
2. Validate source branch ownership and table counts.
3. Reuse existing `InheritedLayerDescriptor` and `InheritedLayerStatus` where
   possible.
4. Add state/read-view accessors needed by tests and later slices.

### L6F-C: Fork Capture

1. Add a source-state capture helper over branch-owned immutable levels.
2. Build destination inherited layers as `[source_own, ...source_inherited]`.
3. Reset copied active/materializing inherited status to `Active` and skip
   materialized layers.
4. Reject source/destination equality.
5. Reject attaching inherited layers to a destination that already has own
   rows or inherited layers unless the helper is explicitly an attach API.
6. Return fork outcome facts without publishing any durable object.

### L6F-D: Read View Validation

1. Add inherited layer field to `BranchReadView`.
2. Make `BranchLocalState::facts` include inherited layer count and inherited
   row version/timestamp ranges.
3. Remove the L6E rejection of nonzero inherited facts.
4. Validate inherited table rows against layer source branch.
5. Validate direct self-inheritance and stale counts.
6. Keep timestamp-bounded reads rejected until L6G.

### L6F-E: Inherited Point Reads

1. For point reads, rewrite the requested child physical key to each source
   branch for lookup or scan.
2. Read source rows from inherited L5 readers.
3. Apply the inherited fork-version gate.
4. Rewrite accepted source rows back to the child branch.
5. Merge with child-local candidates through the same row-chain selector.

### L6F-F: History And Scans

1. Add inherited candidates to retained history reads.
2. Preserve tombstones in history according to existing options.
3. Add inherited candidates to prefix/range scans.
4. Rewrite inherited rows before `BTreeMap` grouping.
5. Preserve deterministic source facts.

### L6F-G: Generated Harness And Guards

1. Extend branch-LSM generated scripts with fork/inherited cases.
2. Add counters for fork capture, inherited latest, inherited getv, inherited
   history, inherited scans, key rewrite, child put shadowing, child tombstone
   shadowing, post-fork parent invisibility, and chained ancestry.
3. Update source guards to allow inherited/fork production entrypoints while
   still rejecting backend, lifecycle, commit-runtime, materialization,
   snapshot, compaction, and product DTO imports.

### L6F-H: Verification And Closeout

1. Run the branch unit, generated, guard, no-default, wasm, clippy, fmt, and
   full package checks listed in this plan.
2. Update the porting log with shipped behavior, tests, sensitivity probes,
   and remaining deferrals.
3. Leave L6G/L6H/L6I/L6J/L6K rows untouched except for cross-links when
   needed.

## Crash Safety And Publication Boundary

L6F is in-memory L6 mechanics. It must not claim durable fork publication.

The old implementation used manifest publication, refcount increments, and
rollback paths around fork visibility. Storage-next keeps those facts out of
L6F:

1. L6F may return what would need to be protected.
2. L6F may expose inherited table counts and source descriptors.
3. L6F must not publish manifests.
4. L6F must not delete or release source tables.
5. L6F must not read or write backend objects.

L8/L4 will later make fork visibility durable and crash-safe using the L6F
facts.

## Sensitivity Probes

Before closing L6F, temporarily introduce each mutation and confirm a targeted
test fails:

1. omit the fork-version gate for inherited rows;
2. rewrite inherited rows after scan grouping instead of before grouping;
3. forget to rewrite the branch id at all;
4. inherit child-local active/frozen rows implicitly;
5. search inherited layers before child-local sources;
6. search farther ancestors before nearer ancestors for exact ties;
7. allow parent writes after fork to appear in a child read;
8. let a child tombstone fall through to an inherited put;
9. copy `Materialized` status as readable inherited state;
10. accept direct self-inheritance;
11. include row value bytes in inherited-layer validation errors;
12. import backend, lifecycle, commit-runtime, or product DTO code from
    production `branch/`.

## Verification

Required after implementation:

1. `cargo test -p strata-storage-next --locked --lib branch`
2. `cargo test -p strata-storage-next --locked --test branch_lsm_source_guard`
3. `cargo test -p strata-storage-next --features testkit --locked --test branch_lsm_properties`
4. `cargo test -p strata-storage-next --no-default-features --features testkit --locked --test branch_lsm_properties`
5. `cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked`
6. `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings`
7. `cargo test -p strata-storage-next --locked`
8. `cargo fmt --package strata-storage-next --check`
9. `git diff --check`

Optional/manual after fuzz target registration:

1. `cargo +nightly fuzz run branch_lsm_inheritance -- -max_total_time=30`

## Completion Criteria

L6F is complete when:

1. a child branch can inherit source immutable levels without row copy;
2. child reads see source rows at or before the fork version;
3. child reads do not see source rows after the fork version;
4. inherited rows are rewritten into the child namespace before MVCC grouping;
5. child-local puts and tombstones shadow inherited rows correctly;
6. chained inherited layers read in deterministic nearest-first order;
7. pinned read views are isolated from later child/source mutations;
8. generated branch-LSM tests exercise every L6F category;
9. source guards allow only L6F-owned fork/inheritance vocabulary;
10. no production `branch/` code imports backend, lifecycle, commit runtime,
    engine, product DTO, or old storage APIs;
11. all required verification commands pass.

## Deferred

1. L6G owns timestamp/as-of reads and TTL visibility over inherited layers.
2. L6H owns materialization and read parity before/after materialization.
3. L6I owns durable reachability, shared table refs, retention, and
   branch/delete release facts.
4. L6J owns compaction safety across inherited tombstones and lower levels.
5. L6K owns snapshot row install.
6. L8 owns durable branch manifest publication and recovery.
7. L7 owns commit-version allocation and any fork-with-source-flush protocol.
