# L6H Implementation Plan: Materialization Mechanics

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L6/l6h-materialization-mechanics-test-plan.md`

## Objective

Add storage-level inherited-layer materialization to storage-next L6.

L6H converts retained rows that are currently reachable through an inherited
layer into child-owned L5 immutable table rows. The operation changes physical
ownership: after a successful materialization, the child branch can read the
materialized rows from its own immutable levels instead of from the inherited
layer. It must not change the visible result of latest, version-bounded,
timestamp-bounded, history, prefix scan, or range scan reads.

L6H is an in-memory/state-transition slice. It may build L5 table artifacts and
install L6 branch-owned table descriptors. It must not publish manifests, write
objects, update durable reachability, decrement shared references, schedule
compaction, or perform backend IO. L6I and L8 consume the facts produced here
to make durable reachability and cleanup decisions.

L6H establishes:

1. a materialization request/outcome model over one child inherited layer;
2. collection of all retained rows from that layer that are valid at the layer
   fork-version gate;
3. source-to-child branch-key rewriting for materialized rows;
4. exact-precedence duplicate suppression for byte-identical rewritten rows
   already represented by a higher-precedence child-local or nearer inherited
   source;
5. L5 table building for rewritten child-owned rows;
6. atomic reader-perspective installation of replacement child-owned tables
   before removing the inherited layer from new read views;
7. pinned read-view isolation across materialization;
8. idempotent no-op behavior when a layer has already been materialized;
9. typed recovery facts for upper layers that publish or replay the operation;
10. generated model coverage and source guards.

## Inputs

1. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
2. `docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-implementation-plan.md`
3. `docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-test-plan.md`
4. `docs/architecture/implementation-plans/M4/L6/l6e-branch-owned-immutable-levels-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/L6/l6f-fork-inherited-layers-implementation-plan.md`
6. `docs/architecture/implementation-plans/M4/L6/l6g-timestamp-ttl-visibility-implementation-plan.md`
7. `docs/architecture/implementation-plans/M4/L6/m4-l6-porting-log.md`
8. `crates/storage-next/src/branch/{config.rs,error.rs,facts.rs,identity.rs,read.rs,state.rs}`
9. `crates/storage-next/src/table/{builder.rs,facts.rs,reader.rs,key.rs,mutable.rs}`
10. `crates/storage-next/src/row/mod.rs`
11. `crates/storage/src/segmented/mod.rs`
12. `crates/storage/src/segmented/tests/materialize.rs`
13. `crates/storage/src/segmented/tests/concurrency.rs`
14. `crates/storage/src/manifest.rs`
15. `crates/storage/src/segmented/ref_registry.rs`

## Existing-Code Source Map

| Current file | L6H evidence | L6H action |
|---|---|---|
| `crates/storage/src/segmented/mod.rs` | `materialize_layer` rewrites inherited rows into child-owned segment rows, publishes a materializing status, installs replacement segments, removes the inherited layer, and treats failures around publication/recovery conservatively. | Port the storage-level state-transition semantics, but rebuild over storage-next rows and L5 table builders. Keep disk files, manifest publication, refcount decrements, and GC outside L6H. |
| `crates/storage/src/segmented/tests/materialize.rs` | Tests cover collapse, commit-id preservation, post-fork filtering, empty layers, read parity, deepest-first materialization, and crash/recovery status handling. | Convert these into L6 direct and generated tests without old storage `Value`, filesystem paths, or segment files. |
| `crates/storage/src/segmented/tests/concurrency.rs` | Regression tests prove materialization shadow detection must see child active/frozen state and must serialize same-branch materialization. | Preserve the reader-result invariant. In storage-next L6, serialization is represented by explicit staged facts and state validation; actual async locking is above L6. |
| `crates/storage/src/manifest.rs` | Inherited layer status persisted as Active/Materializing/Materialized in old manifests. | Reuse `InheritedLayerStatus` as an L6 recovery fact. Do not write or parse durable manifests in this slice. |
| `crates/storage/src/segmented/ref_registry.rs` | Old materialization decremented shared segment refs after replacement install. | Defer durable shared-table release to L6I/L8. L6H only reports which inherited tables were replaced and which replacement tables were installed. |
| `crates/storage-next/src/branch/read.rs` | L6F/L6G read inherited rows by rewriting source branch keys to child branch keys, applying fork-version and timestamp gates, and respecting source precedence. | Reuse the same rewrite/effective-bound vocabulary for materialization row collection. |
| `crates/storage-next/src/branch/state.rs` | `BranchLocalState` already owns active, frozen, branch-owned immutable levels, and inherited layers. | Add materialization planning and commit helpers that build child-owned tables and update state in one reader-perspective transition. |
| `crates/storage-next/src/table/builder.rs` | L5 can build immutable table artifacts from sorted storage rows and returns facts/readers without backend IO. | Use L5 builders for replacement child-owned tables. Split large materialization outputs before exceeding L5 bounds. |

## Scope

L6H implements:

1. `BranchMaterializationRequest` or equivalent request validation for a child
   branch and inherited layer index;
2. a `BranchMaterializationOutcome` or equivalent fact record with branch id,
   source branch id, fork version, layer index, rows materialized, tables
   created, inherited layers remaining, and recovery classification;
3. typed errors for missing layer, unavailable layer, wrong branch, invalid
   layer status, stale staged facts, duplicate unresolvable internal keys,
   table build failures, and invalid output identities;
4. collection of retained layer rows with `row.commit_version <= fork_version`;
5. rejection of inherited rows whose physical branch id does not match the
   layer source branch;
6. rewriting of retained source rows into the child branch namespace;
7. preservation of commit version, commit timestamp, expiry timestamp,
   tombstone bit, value bytes, storage space, logical space, and user key;
8. sorting rewritten rows by L5 internal key before table building;
9. exact duplicate handling that preserves the same precedence as normal L6
   reads:
   - child-local byte-identical rewritten row wins over the inherited copy;
   - nearer inherited byte-identical rewritten row wins over the farther
     inherited copy;
   - same physical key at different commit versions is not a duplicate and
     must be retained unless a later retention proof says otherwise;
   - same internal key already present in a higher-precedence child-local or
     nearer inherited source rejects materialization unless the rewritten row
     facts are byte-identical;
10. L5 table building for the retained rewritten rows;
11. output splitting when the retained row set would exceed L5 table limits;
12. child-owned replacement table installation into L0;
13. removal or status transition of the materialized inherited layer only
    after replacement tables are visible to new read views;
14. no-op materialization of an empty layer that only removes the layer or
    marks it materialized according to the shipped state model;
15. idempotent retry when a layer is already `Materialized` or absent due to a
    completed prior materialization;
16. reset of stale `Materializing` facts to a retryable active/materializing
    state according to the documented recovery mode;
17. pinned read-view isolation: old views keep reading their captured inherited
    layers while new views see the post-materialization state;
18. read parity before and after materialization for latest, getv, as-of,
    history, prefix scan, and range scan;
19. generated branch-LSM model counters for materialization plans;
20. source-guard updates and porting-log notes.

L6H does not implement:

1. durable branch manifest publication;
2. object layout, backend IO, L4 table publication, or filesystem cleanup;
3. WAL-before-visible orchestration;
4. durable branch/table reachability payloads;
5. shared-table refcount decrements or release proofs;
6. compaction scheduling or retention cleanup;
7. TTL physical deletion;
8. tombstone pruning;
9. commit-version allocation;
10. snapshot row install;
11. product branch workflow, product branch names, public API, or StrataHub
    behavior.

## Core Rule: Materialization Is Not Cleanup

Materialization changes ownership. It must not decide that a historical row is
safe to discard merely because another row is newer.

The materializer must keep every inherited storage row that can still be
observed by any supported L6 read bound. This includes:

1. older versions that are hidden from latest reads but visible to `AtVersion`
   reads;
2. rows hidden by a newer child row for latest reads but visible before that
   child row's commit version;
3. rows hidden by timestamp filtering for one as-of timestamp but visible at a
   later as-of timestamp;
4. tombstones, because they are observable in storage history and can suppress
   older rows;
5. expired rows, because TTL cleanup requires a separate retention proof.

Rows may be skipped only when L6 can prove they are not a distinct retained row
after applying materialization mechanics:

1. the source row is above the inherited layer fork-version gate;
2. the source row is invalid for the layer source branch;
3. the byte-identical rewritten storage row is already represented by a
   higher-precedence child-local source;
4. the byte-identical rewritten storage row is already represented by a nearer
   inherited layer;
5. a staged retry proves the row was already installed by the same completed
   materialization.

Broad cleanup such as "drop all inherited rows for keys that currently have a
newer child value" belongs to L6J/L8 after retained version, timestamp,
tombstone, TTL, branch, snapshot, and reachability proofs exist.

## Semantic Rules

### Source Layer Selection

The request names one inherited layer by current layer index or by a stable
staged fact containing:

```text
child_branch_id
source_branch_id
fork_version
layer_index_at_plan_time
source_table_count
```

Index-based requests are valid for direct in-memory calls. Replay should prefer
the stable source branch plus fork-version identity because layer indexes can
shift after a different layer is materialized.

Readable statuses:

1. `Active` can be planned and materialized.
2. `Materializing` remains readable and can be retried.
3. `Materialized` returns an idempotent no-op or confirms the staged outcome.
4. `Unavailable` fails closed.

### Row Collection

The materializer collects rows only from the target layer. For each row:

1. require `row.physical_key().branch_id() == source_branch_id`;
2. require `row.commit_version <= layer.fork_version`;
3. rewrite the physical key from `source_branch_id` to `child_branch_id`;
4. preserve every non-branch row fact exactly;
5. compute the rewritten L5 internal key;
6. apply exact-precedence duplicate suppression;
7. sort by rewritten L5 internal key.

The target layer's rows are not filtered by one read bound. L6H must preserve
all retained row-chain facts needed by later latest, version, timestamp, and
history reads.

### Duplicate And Precedence Handling

L6H must preserve normal read precedence after materialization:

```text
child-local sources
then materialized replacement rows
then remaining inherited layers nearest-first
```

An exact rewritten storage-row duplicate is safe to skip when a higher
precedence source already owns the same internal key and the same row facts.
Different commit versions for the same physical key are not duplicates when
their L5 internal keys remain distinct.

If a higher-precedence child-local or nearer inherited source already owns the
same rewritten internal key with different row facts, L6H rejects the
materialization without mutation. Storage-next branch-owned immutable state
preserves the L5 invariant that duplicate internal keys cannot coexist in the
same branch-owned table set; retaining both rows would make later compaction
and reachability ambiguous. Exact rewritten duplicates remain safe to skip.

If two rows inside the same target layer rewrite to the same internal key, the
operation must fail with a typed invalid inherited layer error. That case means
the inherited layer is internally inconsistent.

### Table Building

Replacement rows become child-owned L5 immutable tables:

1. table rows must be sorted and unique before calling the L5 builder;
2. large outputs are split into multiple L0 tables before L5 row/block limits
   are exceeded;
3. each output table receives an opaque `TableIdentity` supplied by the caller
   or derived from an L6 test-only deterministic seed;
4. table identities are not filesystem paths or object names;
5. built table facts are preserved in the installed `BranchOwnedTable`
   descriptors;
6. empty retained row sets do not build an empty L5 table.

### State Transition

The reader-perspective transition is:

```text
validate request
capture source layer and higher-precedence exact-key facts
mark target layer Materializing, or create equivalent staged fact
build replacement child-owned tables outside the visible state mutation
install replacement tables into child L0
remove target inherited layer, or mark it Materialized if the state model keeps ledger entries
refresh branch facts
return outcome/recovery facts
```

New read views must never observe a state where the inherited layer is gone and
the replacement tables are absent. If table building fails, the inherited layer
must remain readable. If replacement tables are installed but a later durable
publish step fails above L6, L6 must surface enough facts for L8 to retry or
reconcile without losing the inherited source.

Pinned read views captured before the transition keep their cloned inherited
layer references. They may continue reading inherited rows. Read views captured
after the transition read replacement child-owned tables.

### Recovery Facts

L6H should expose storage-owned facts sufficient for L8 reconciliation:

```text
BranchMaterializationRecovery =
  ReplacementVisibleLayerRemoved
  ReplacementAlreadyVisibleLayerRemoved
  LayerAlreadyMaterialized
```

L6 uses an in-memory atomic stage-and-swap: replacement installation and source
layer removal commit together from the reader perspective. That means L6 does
not expose durable in-flight recovery states. The key requirement is that
callers can distinguish:

1. replacement tables were installed and the inherited layer was removed;
2. replacement tables were already visible from a previous attempt and the
   inherited layer was removed by this retry;
3. the request is stale because another materialization already completed.

L6H does not write durable recovery records. It returns facts that L8 can
durably publish or replay.

## Target Module Shape

Expected production layout after L6H:

```text
crates/storage-next/src/branch/
  mod.rs
  config.rs
  error.rs
  facts.rs
  identity.rs
  read.rs
  state.rs
  materialize.rs    # optional if state.rs/read.rs would grow too large
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

### Request

```text
BranchMaterializationRequest {
    child_branch_id: BranchId,
    layer_index: usize,
    output_identity_seed: TableIdentitySeed,
    target_level: BranchLevel, // first implementation should use L0
}
```

Rules:

1. child branch id must match the state being mutated;
2. layer index must name an existing inherited layer unless replay uses a
   stable staged fact;
3. target level should be L0 for the first implementation;
4. output identity seed must produce valid opaque table identities and must not
   encode paths or object layout strings.

### Staged Plan

```text
BranchMaterializationPlan {
    child_branch_id: BranchId,
    source_branch_id: BranchId,
    fork_version: CommitVersion,
    layer_index_at_plan_time: usize,
    retained_row_count: u64,
    skipped_exact_duplicate_count: u64,
    skipped_post_fork_count: u64,
}
```

Rules:

1. the plan contains no value bytes;
2. the plan is diagnostic/recovery metadata, not a durable manifest format;
3. replay validates source branch plus fork version, not only layer index.

### Outcome

```text
BranchMaterializationOutcome {
    child_branch_id: BranchId,
    source_branch_id: BranchId,
    fork_version: CommitVersion,
    layer_index: usize,
    rows_materialized: u64,
    tables_created: usize,
    inherited_layers_remaining: usize,
    replacement_owned_table_count: usize,
    recovery: BranchMaterializationRecovery,
}
```

Rules:

1. `rows_materialized` counts retained rewritten rows installed into new
   child-owned tables;
2. `tables_created` is zero for an empty materialization;
3. the outcome does not claim durable publication;
4. release of inherited shared tables is not implied by this outcome.

## Implementation Steps

### L6H-A: Add Materialization Types And Errors

1. Add request/outcome/recovery fact types in `branch/facts.rs`, `state.rs`, or
   `materialize.rs`.
2. Add typed errors for missing layer, unavailable layer, stale staged fact,
   duplicate rewritten internal key, invalid output identity, and table build
   failure.
3. Re-export the new crate-local types from `branch/mod.rs`.
4. Add display/source-chain tests that avoid value bytes in errors.

Exit: L6 can express materialization requests and outcomes without performing
the state transition.

### L6H-B: Collect And Rewrite Target Layer Rows

1. Add a helper that snapshots the selected inherited layer and closer
   inherited layers.
2. Collect only rows with `commit_version <= fork_version`.
3. Rewrite each retained row from source branch id to child branch id.
4. Preserve timestamp, expiry, tombstone, value, space, storage-space id, and
   user-key bytes.
5. Reject wrong-source rows and same-target-layer duplicate rewritten internal
   keys.
6. Return sorted `StorageRow` or `TableRow` values plus skipped-row counters.

Exit: row collection is deterministic and side-effect free.

### L6H-C: Apply Exact-Precedence Duplicate Suppression

1. Build exact internal-key sets from child active, frozen, and owned tables.
2. Build exact internal-key sets from inherited layers nearer than the target
   layer after rewriting them to the child branch.
3. Skip only exact rewritten internal-key duplicates already represented in a
   higher-precedence source.
4. Do not skip same-key rows at different commit versions.
5. Do not apply TTL, tombstone, latest-read, or timestamp cleanup.

Exit: materialization preserves every retained historical row needed by
supported read bounds.

### L6H-D: Build Replacement L5 Tables

1. Split retained rows into sorted chunks that satisfy L5 table builder limits.
2. Build each chunk with `ImmutableTableBuilder`.
3. Decode/open the built artifact through `ImmutableTableReader`.
4. Wrap each reader in a child-owned `BranchOwnedTable` at L0 with matching
   descriptor facts.
5. Reject empty table builds by returning an empty replacement list.
6. Surface L5 build/decode errors through `BranchRuntimeError` source chains.

Exit: L6H can produce child-owned table objects from inherited rows without
backend IO.

### L6H-E: Commit The Reader-Perspective Transition

1. Validate that the selected layer still matches the planned source branch
   and fork version.
2. Mark the layer `Materializing` or record an equivalent staged fact before
   replacement install.
3. Install replacement L0 tables before removing or marking the inherited
   layer materialized.
4. Remove by stable identity `(source_branch_id, fork_version)` rather than by
   index when committing a staged plan.
5. Refresh branch facts after install/removal.
6. Preserve old pinned read views by cloning state before mutation.

Exit: new read views observe either the old inherited layer or the replacement
child-owned tables, never a gap.

### L6H-F: Idempotency And Recovery Classification

1. Return no-op when the layer index is already absent and the staged source
   identity is known complete.
2. Treat an already `Materialized` layer as stale successful replay.
3. Treat stale `Materializing` with no replacement as retryable.
4. Treat replacement visible plus inherited layer still present as retryable
   reconciliation, not data loss.
5. Return facts that L8 can use to publish durable manifests and reachability
   updates.

Exit: repeated materialization requests are safe and explicitly classified.

### L6H-G: Tests, Generated Model, Guards, And Porting Log

1. Add direct tests for every semantic rule above.
2. Extend `BranchLsmScaffoldOutcome` with materialization counters.
3. Add generated operation scripts that fork, mutate child/source branches,
   materialize layers, and compare read parity before/after.
4. Update `branch_lsm_source_guard.rs` so production branch materialization
   cannot import backend, service, lifecycle, old storage, product DTO, or
   wall-clock APIs.
5. Update the L6 porting log with preserved old behavior, changed boundaries,
   deferred durable work, and sensitivity probes.

Exit: L6H passes direct, generated, source-guard, no-default-features, wasm,
clippy, and formatting gates.

## Deferred To Later Slices

1. L6I owns durable reachability payloads and shared table release facts.
2. L6J owns branch compaction and pruning based on retention safety proofs.
3. L6K owns snapshot row install.
4. L6L owns consolidated fuzz inventory and closeout sensitivity runs.
5. L8 owns durable materialization orchestration, manifest publication,
   ambiguous publish-window reconciliation, and recovery from backend failures.

## Verification Commands

Run at least:

```bash
cargo test -p strata-storage-next --locked --lib branch
cargo test -p strata-storage-next --features testkit --locked --test branch_lsm_properties
cargo test -p strata-storage-next --no-default-features --features testkit --locked --test branch_lsm_properties
cargo test -p strata-storage-next --locked --test branch_lsm_source_guard
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

If L6H touches table chunking or shared L5 helpers, also run the table runtime
tests that cover builder and reader behavior.

## Exit Criteria

L6H is complete when:

1. materialization has no backend, service, WAL, manifest, lifecycle, old
   storage, product DTO, or public API dependency;
2. inherited rows are rewritten into child-owned L5 table rows with all row
   facts preserved;
3. materialization preserves latest, getv, as-of, history, prefix scan, and
   range scan results before and after the transition;
4. materialization does not perform retention cleanup;
5. replacement tables are visible before inherited layers are removed from new
   read views;
6. pinned read views remain stable;
7. empty, stale, retry, and invalid materialization requests are typed;
8. generated tests cover fork/materialize/read parity scripts;
9. source guards and porting log are updated;
10. the verification commands pass.
