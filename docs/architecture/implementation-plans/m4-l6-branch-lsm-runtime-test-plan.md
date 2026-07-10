# M4-L6 Test Plan: Branch LSM Runtime

Status: test-suite plan

Parent plan:
`docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-implementation-plan.md`

## Goal

Prove that storage-next L6 implements branch-isolated MVCC LSM mechanics over
L5 table rows without importing commit-runtime, lifecycle, engine, or product
semantics.

The suite must fail if L6:

1. loses a committed row version from a retained row chain;
2. returns a non-newest visible row for latest/getv/as-of reads;
3. hides tombstones or expired rows inconsistently across own and inherited
   branch state;
4. exposes parent writes after a child fork version;
5. fails to let child-local writes or tombstones shadow inherited rows;
6. rewrites inherited keys incorrectly;
7. removes an inherited layer before replacement materialized tables are
   visible to pinned readers;
8. releases shared table reachability while any branch/layer still references
   it;
9. mutates branch state directly from L7/L8 concepts;
10. imports product DTOs such as old `VersionedValue`, `Value`, `Key`,
    `Namespace`, or `TypeTag`;
11. calls filesystem/backend APIs directly from production `branch/` code;
12. panics on generated branch/fork/materialization/read scripts.

This plan is stricter than current `crates/storage` tests. Current tests are
evidence and regression input; storage-next needs L6-specific model tests that
separate branch state from commit and lifecycle orchestration.

## Testing Principles

1. Test storage mechanics, not product branch workflows.
2. Valid rows are storage-next `StorageRow` values.
3. Test values are opaque bytes. No JSON, graph, vector, search, event, or
   engine payload semantics are valid test or production dependencies.
4. Every read result is compared against an independent row-chain model.
5. Every inherited read is compared after model key rewriting into the child
   branch namespace.
6. Every fork test proves fork-version gating explicitly.
7. Every materialization and compaction test compares reads before and after
   the state transition.
8. Every cleanup/release test proves shared table references remain protected
   while reachable.
9. Fault tests classify branch-state mutation windows without requiring L8 to
   perform recovery.
10. Source guards are part of the test suite, not advisory docs.

## Test Harness Layout

Recommended test locations:

1. `crates/storage-next/src/branch/` for small module-local tests.
2. `crates/storage-next/src/branch/tests/` for larger branch suites once files
   approach engineering thresholds.
3. `crates/storage-next/tests/branch_lsm_properties.rs` for generated L6
   conformance properties.
4. `crates/storage-next/tests/branch_lsm_source_guard.rs` for production L6
   boundary scans.
5. `crates/storage-next/fuzz/fuzz_targets/branch_lsm_reads.rs` for generated
   branch read scripts.
6. `crates/storage-next/fuzz/fuzz_targets/branch_lsm_inheritance.rs` for fork,
   inheritance, and key-rewrite scripts.
7. `crates/storage-next/fuzz/fuzz_targets/branch_lsm_install.rs` for
   materialization, table install, and snapshot row install scripts.

Slice-level implementation and test plans should live under
`docs/architecture/implementation-plans/M4/L6/`, following the grouped M4
slice-doc pattern.

Required regression file:

1. `crates/storage-next/proptest-regressions/branch_lsm.txt`, created only
   when a failing seed is captured.

## Reference Model

Use an independent model, not the production branch implementation.

Suggested model:

```text
ModelBranch {
  branch_id
  own_rows: BTreeMap<physical_key_without_version, Vec<ModelRow newest-first>>
  inherited_layers: Vec<ModelInheritedLayer nearest-first>
}

ModelInheritedLayer {
  source_branch_id
  child_branch_id
  fork_version
  rows: row chains from source snapshot
}
```

The model should:

1. store every retained version as a separate model row;
2. sort row chains by commit version descending;
3. apply latest/version/timestamp bounds independently;
4. apply tombstone and TTL visibility explicitly;
5. rewrite inherited rows to child branch before grouping;
6. search child-local rows before inherited layers;
7. preserve tombstones in history when history requests tombstones;
8. expose insufficient-history and unsafe-pruning conditions as model facts.

## Generators

### Branch Generator

Generate 1 to 16 branches by default, with separate stress cases for deeper
fork chains.

Vary:

1. branch id bytes;
2. root branches with no inheritance;
3. one-level forks;
4. chained forks;
5. sibling forks sharing the same source tables;
6. branches with empty own state;
7. branches with active-only, frozen-only, immutable-only, and mixed state;
8. inherited layers with distinct fork versions;
9. materialized and non-materialized inherited layers.

### Row Chain Generator

Generate row chains over storage-next `StorageRow`.

Vary:

1. storage space id, including storage-owned and engine-owned valid ids;
2. space names;
3. user key bytes, including empty, long shared prefixes, embedded zero bytes,
   and high-bit bytes;
4. repeated physical keys with multiple commit versions;
5. branch-local and inherited versions for the same logical key;
6. equal timestamps at distinct versions;
7. non-monotonic timestamps relative to commit version, when explicitly testing
   timestamp behavior;
8. tombstones;
9. put rows with empty values;
10. put rows with expiry at epoch, before read timestamp, at read timestamp,
    and after read timestamp.

### Operation Generator

Generate scripts containing:

1. create branch;
2. append committed put row;
3. append committed tombstone;
4. rotate active mutable table;
5. install branch-owned table;
6. latest read;
7. getv read;
8. as-of read;
9. history read;
10. prefix/range scan;
11. fork branch;
12. materialize inherited layer;
13. compact branch-owned tables;
14. clear/delete branch;
15. install snapshot rows;
16. capture pinned read view;
17. read through pinned view after mutation.

The model must compute the expected result for every read operation.

### Fault Generator

Generate branch mutation faults:

1. table build failure before branch install;
2. table publish failure before branch install;
3. table publish visible but durability unconfirmed;
4. branch reachability publish failure before visibility;
5. branch reachability visible but durability unconfirmed;
6. inherited reference registration failure;
7. materialization intent conflict;
8. snapshot row preflight failure;
9. branch clear racing with table install;
10. branch delete racing with fork.

L6 should expose typed facts/errors. L8 will later consume those facts for
recovery.

## Required Cases

### 1. Module And Boundary Guards

1. `branch` module compiles under default features.
2. `branch` module compiles under no-default features.
3. `branch` module compiles under all features.
4. Production `branch/` does not import `crate::commit`, `crate::lifecycle`,
   `crate::api`, or engine crates.
5. Production `branch/` does not import old product DTOs: `VersionedValue`,
   `Value`, `Key`, `Namespace`, `TypeTag`, `EntityRef`, JSON, graph, vector,
   search, or transaction types.
6. Production `branch/` does not call `std::fs`, `Path`, `File`, mmap, pread,
   backend APIs, or environment APIs.
7. Branch errors are typed and preserve L5/L4 source errors where useful.
8. Public crate surface remains unchanged unless L9 approves exposure.
9. Branch files stay within engineering thresholds or split into submodules.

### 2. Branch Row Identity And Rewriting

Detailed slice test plan:
`docs/architecture/implementation-plans/M4/L6/l6b-branch-row-identity-read-bounds-test-plan.md`

1. A row whose physical key branch id matches the target branch is accepted.
2. A row whose physical key branch id differs from the target branch is
   rejected for own-branch install.
3. Rewriting an inherited row from source branch to child branch preserves
   space, storage-space id, user key, commit version, timestamp, expiry,
   tombstone flag, and value bytes.
4. Rewriting is reversible for valid branch ids.
5. Rewritten rows sort in the same logical position as child-local rows for the
   same logical key.
6. Invalid physical keys return typed errors before state mutation.
7. Branch id bytes are opaque; no test relies on names or product branch UX.

### 3. Branch Creation And Empty Reads

1. Empty branch creation succeeds.
2. Duplicate branch creation returns typed already-exists error.
3. Missing branch read returns typed not-found or empty according to documented
   API contract.
4. Empty latest/getv/as-of/history/prefix/range reads return empty results.
5. Empty branch facts report zero rows, zero tables, no max version, and no
   timestamp range.
6. Creating one branch does not affect another branch.

### 4. Branch-Local Mutable And Frozen State

1. Committed put row appends to active table.
2. Committed tombstone appends to active table.
3. Duplicate exact internal key is rejected.
4. Active rotation moves rows into frozen state without changing reads.
5. Frozen tables are searched newest first.
6. Branch-local max version updates only from installed rows.
7. Branch-local timestamp range updates from row commit timestamps.
8. Branch-local active/frozen state preserves tombstones and expired-looking
   rows until read visibility decides their result.
9. Applying rows to one branch cannot mutate another branch.

### 5. Own-Branch Latest Reads

Detailed active/frozen-only slice test plan:
`docs/architecture/implementation-plans/M4/L6/l6d-pinned-own-branch-read-views-test-plan.md`

L6D closes the latest/getv/history/prefix/range subset over own active/frozen
state and records timestamp/TTL policy as deferred. The timestamp/TTL cases in
this parent section close when L6G lands.

1. Latest returns newest live put row.
2. Newer tombstone makes latest return none.
3. Newer expired row makes latest return none at a read timestamp after expiry.
4. Older live row is not returned when a newer visible tombstone shadows it.
5. Duplicate physical keys at many versions return the newest visible row.
6. Latest reads search active, frozen, L0, and L1+ in documented order.
7. Latest read result includes commit version and timestamp facts.
8. Latest read never constructs old `VersionedValue` inside L6.

### 6. Version-Bounded Reads

1. `getv(V)` returns newest row with commit version `<= V`.
2. Rows with commit version greater than `V` are ignored.
3. Tombstone at or below `V` hides older put rows.
4. Tombstone above `V` does not hide older put rows visible at `V`.
5. Empty result is returned when all matching rows are above `V`.
6. Version bounds work identically over active, frozen, and immutable sources.
7. Version bounds work over inherited layers using
   `min(requested_version, fork_version)`.

### 7. Timestamp-Bounded Reads

Detailed slice test plan:
`docs/architecture/implementation-plans/M4/L6/l6g-timestamp-ttl-visibility-test-plan.md`

1. `as_of(T)` returns newest row whose commit timestamp is `<= T`.
2. Rows with timestamp greater than `T` are ignored.
3. Tombstone at or before `T` hides older put rows.
4. Tombstone after `T` does not hide older put rows visible at `T`.
5. TTL is evaluated at the requested timestamp, not wall-clock now.
6. A row expiring exactly at `T` follows the documented inclusive/exclusive
   expiry rule.
7. Timestamp reads over inherited layers also respect fork-version gates.
8. If retained history is insufficient for `T`, L6 returns typed
   insufficient-history facts rather than inventing a value.

### 8. History Reads

1. History returns retained versions newest first.
2. History can include tombstones when requested.
3. History can exclude tombstones only through documented caller option.
4. Limit `N` returns at most `N` rows.
5. `before_version` excludes rows at or above that version.
6. History over inherited state applies fork-version gates.
7. History preserves commit timestamp, expiry, tombstone, and value facts.
8. History does not collapse multiple versions into one result.

### 9. Prefix And Range Scans

1. Prefix scan returns one visible row per logical key under the prefix.
2. Range scan respects inclusive/exclusive bounds.
3. Scans merge active, frozen, L0, L1+, and inherited sources.
4. Scans rewrite inherited keys into child namespace before grouping.
5. Child-local row shadows inherited row with same logical key.
6. Child-local tombstone shadows inherited put row.
7. Parent row after fork is invisible to child scan.
8. Scan output is sorted by branch-local physical key.
9. Scan does not cross branch id, space, or storage-space boundaries unless the
   API explicitly asks for those boundaries.

### 10. Pinned Read Views

L6D first proves pinned views across active/frozen append and rotation. Later
slices extend the same invariant to immutable table install, compaction,
materialization, and clear/delete transitions.

1. Captured read view remains valid after active rotation.
2. Captured read view remains valid after frozen flush/table install.
3. Captured read view remains valid after compaction install.
4. Captured read view remains valid after materialization.
5. Captured read view remains valid after branch clear/delete according to
   documented lifetime rules.
6. A reader sees old view or new view, never a mix.
7. No row disappears before its replacement table is visible in the branch
   state.

### 11. Branch-Owned Immutable Levels

Detailed slice test plan:
`docs/architecture/implementation-plans/M4/L6/l6e-branch-owned-immutable-levels-test-plan.md`

1. Installing one L0 table preserves reads.
2. Installing multiple overlapping L0 tables searches newest first.
3. L1+ tables are non-overlapping and searched by key range.
4. Overlapping L1+ install is rejected.
5. Table facts must match branch id and key range expectations.
6. Table object identities are opaque facts, not filesystem paths.
7. L6 reads immutable tables through L5 table readers or L4/L5 object-backed
   adapters, not backend APIs.
8. Installing an empty table is rejected.

### 12. Fork And Inherited Layers

Detailed slice test plan:
`docs/architecture/implementation-plans/M4/L6/l6f-fork-inherited-layers-test-plan.md`

1. Fork creates destination branch without copying rows.
2. Destination branch starts with empty own state and inherited layers.
3. Fork captures source max applied version as fork version.
4. Parent writes after fork are invisible to child.
5. Parent writes before or at fork version are visible to child if not
   shadowed.
6. Source inherited layers are preserved in deterministic ancestry order.
7. Chained fork reads search nearest ancestor first.
8. Fork fails if source branch is missing.
9. Fork-at-history fails if requested version is not retained.
10. Fork does not expose destination branch until reachability facts are safe
    enough for the documented mode.

### 13. Child-Local Shadowing

1. Child put shadows inherited put for latest reads.
2. Child tombstone shadows inherited put for latest reads.
3. Child older version does not shadow inherited newer version if visibility
   rules choose the inherited version at the read bound.
4. Child row above requested version does not shadow inherited row visible at
   requested version.
5. Child row after requested timestamp does not shadow inherited row visible at
   requested timestamp.
6. Shadowing works in point reads, history reads, prefix scans, and range scans.

### 14. Materialization

Detailed slice test plan:
`docs/architecture/implementation-plans/M4/L6/l6h-materialization-mechanics-test-plan.md`

1. Materializing one inherited layer preserves latest reads.
2. Materializing one inherited layer preserves getv reads.
3. Materializing one inherited layer preserves as-of reads.
4. Materializing one inherited layer preserves prefix/range scans.
5. Materialization rewrites source branch id to child branch id.
6. Materialization excludes only exact duplicate internal keys already
   represented by child-local rows or nearer inherited layers; broad row
   pruning is deferred to compaction/retention proof work.
7. Materialization does not remove inherited layer before replacement tables
   are visible to readers.
8. Materialization is idempotent when replayed from staged facts.
9. Materialization failure before replacement install leaves old reads intact.
10. Materialization visible-but-not-durable state is reported with typed facts
    for L8.

### 15. Reachability And Shared Table References

Detailed slice test plan:
`docs/architecture/implementation-plans/M4/L6/l6i-reachability-shared-table-refs-test-plan.md`

1. Branch-owned table is reachable from its owning branch.
2. Inherited table is reachable from every branch/layer that references it.
3. Shared table registry can be rebuilt from durable reachability facts.
4. Removing one branch does not release a table inherited by another branch.
5. Removing an inherited layer releases only tables no longer referenced.
6. Materialization release facts are emitted only after replacement
   reachability is safe.
7. Branch clear/delete reports tables that may be released and tables still
   protected.
8. Reachability facts are deterministic and sorted.

### 16. Branch Compaction Integration

Detailed slice test plan:
`docs/architecture/implementation-plans/M4/L6/l6j-branch-compaction-integration-test-plan.md`

1. Candidate selection is branch-local.
2. L6 supplies L5 with explicit keep/drop policy.
3. Keep-all branch compaction preserves all reads.
4. Dropping old versions is rejected unless retention facts prove safety.
5. Dropping tombstones is rejected unless inherited/lower rows cannot be
   resurrected.
6. Dropping TTL-expired rows is rejected unless timestamp/as-of retention facts
   prove safety.
7. Compaction output install preserves pinned old read views.
8. Compaction output install publishes one atomic branch-level state change.
9. Old table release facts are produced only after replacement reachability is
   safe.

### 17. Snapshot Row Install

Detailed plan:
`docs/architecture/implementation-plans/M4/L6/l6k-snapshot-row-install-test-plan.md`

1. Empty install is a no-op or typed error according to documented contract.
2. Valid single-branch install creates readable rows.
3. Valid multi-branch install creates readable rows in each branch.
4. Row targeting missing branch follows documented create-or-reject rule.
5. Row whose physical key branch id disagrees with install target is rejected.
6. Duplicate internal key in install plan is rejected before mutation.
7. Invalid row ordering is rejected before mutation.
8. Table build/staging failure leaves no partial branch-state install visible.
9. Durable publication failure remains L8; L6K must expose only staged
   in-memory install facts until L8 completes publication.
10. Install result reports table and branch facts needed by L8.

### 18. Fault Windows

1. Branch table install failure before state publish leaves old state intact.
2. Branch table install visible but durability unconfirmed reports typed
   uncertain facts.
3. Fork reachability failure before destination visibility leaves no visible
   destination branch.
4. Fork visibility unknown reports enough facts for L8 classification.
5. Materialization intent conflict returns typed error.
6. Clear/delete racing with flush cannot resurrect a branch.
7. Corrupt inherited layer facts fail closed.
8. L5 table read/decode errors remain source errors, not branch not-found.

### 19. Generated Properties

Generated L6 properties must cover:

1. latest/getv/history consistency over one row chain;
2. timestamp/as-of consistency over one row chain;
3. prefix/range scan consistency over many row chains;
4. fork-version gates;
5. child-local shadowing;
6. inherited key rewriting;
7. materialization read parity;
8. branch compaction read parity;
9. snapshot install all-or-nothing behavior;
10. reachability/shared reference safety.

Default generated runs should be bounded for normal CI. Larger inherited-depth,
table-count, and row-count stress runs may be separate ignored or manual
stress commands.

### 20. Fuzz Targets

Detailed closeout test plan:
`docs/architecture/implementation-plans/M4/L6/l6l-l6-conformance-closeout-test-plan.md`

Required L6 fuzz targets:

1. `branch_lsm_reads`: arbitrary operation scripts over generated branches and
   row chains, checking model parity for latest/getv/history/scans.
2. `branch_lsm_inheritance`: arbitrary fork, key rewrite, shadowing, and
   materialization scripts.
3. `branch_lsm_install`: arbitrary table install, compaction install, and
   snapshot row install scripts with typed rejection on invalid plans.

Each target must have a small checked-in seed corpus and a hidden testkit
contract function. Closeout tests should verify target registration and that
each target calls its dedicated contract.

### 21. Sensitivity Probes

Before closing L6, temporarily introduce each mutation and confirm a targeted
test or guard fails:

1. sort commit versions ascending for one row chain;
2. ignore tombstones in latest reads;
3. evaluate TTL against wall-clock now instead of requested timestamp;
4. omit fork-version gate for inherited rows;
5. forget to rewrite inherited branch id;
6. search inherited layers before child-local state;
7. release shared table refs on branch delete without checking other branches;
8. materialize inherited layer before applying child-local shadowing;
9. expose `VersionedValue` in production branch code;
10. import `crate::commit` or `crate::lifecycle` from production branch code.

## Cross-Feature Matrix

Mandatory modes:

| Mode | Purpose | Command |
|---|---|---|
| branch unit | fast branch mechanics check | `cargo test -p strata-storage-next --locked --lib branch::tests` |
| branch generated | generated branch model check | `cargo test -p strata-storage-next --features testkit --locked --test branch_lsm_properties` |
| no-default generated | prove no accidental localfs/default dependency | `cargo test -p strata-storage-next --no-default-features --features testkit --locked --test branch_lsm_properties` |
| source guards | L6 purity | `cargo test -p strata-storage-next --locked --test branch_lsm_source_guard` |
| closeout inventory | generated/fuzz/doc inventory | `cargo test -p strata-storage-next --locked --test branch_lsm_closeout` |
| wasm/no-default | browser-compatible branch mechanics | `cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked` |
| lint | all-target/all-feature lint surface | `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings` |
| full package | regression safety net | `cargo test -p strata-storage-next --locked` |
| format | rustfmt stability | `cargo fmt --package strata-storage-next --check` |
| whitespace | patch hygiene | `git diff --check` |

Optional/manual modes:

1. localfs explicit feature if branch/table object handoff uses localfs tests;
2. short fuzz smoke commands for all `branch_lsm_*` fuzz targets;
3. longer branch stress commands for inherited-depth and materialization
   scripts.

## Deferred Behavior Map

Not L6 test gaps:

1. commit-version allocation;
2. WAL-before-visible discipline;
3. commit conflict validation;
4. checkpoint scheduling;
5. recovery orchestration;
6. compaction scheduling;
7. retention scheduling;
8. quarantine/repair orchestration;
9. branch-registry workflows such as duplicate branch create, fork on a
   missing source branch, and fork-at-history requests;
10. branch clear/delete APIs and pinned-view behavior across those public
    lifecycle operations;
11. product branch merge/cherry-pick/revert/restore semantics;
12. public branch naming UX;
13. materialization durability uncertainty, visible-but-not-durable publish
    windows, durable recovery records, and backend fault reconciliation;
14. materialization provenance diagnostics across re-forked materialized
    replacement tables;
15. query planner or secondary index semantics;
16. engine-facing DTO mapping.

These belong to L7, L8, L9, or engine-next.

## Exit Gate

M4-L6 test coverage is complete when:

1. direct tests cover every branch module surface;
2. generated model tests cover latest/getv/as-of/history/scans, fork,
   inheritance, materialization, compaction install, and snapshot install;
3. fuzz targets exist with checked-in seed corpora;
4. source guards prevent commit/lifecycle/engine/product/backend leakage;
5. sensitivity probes have been run and recorded in the porting log;
6. memory/object-backed branch table reads are covered through lower-layer
   APIs;
7. the implementation plan and porting log identify old storage mechanics that
   were ported, rewritten, retired, or deferred;
8. all mandatory cross-feature commands pass.
