# M4P-L6 Test Plan: Branch-Isolated LSM Runtime Parity

Status: draft test plan

Implementation plan:
`docs/architecture/implementation-plans/M4P/m4p-l6-branch-lsm-runtime-parity-implementation-plan.md`

Parent test methodology:
`docs/architecture/implementation-plans/m4p-storage-next-parity-restoration-test-plan.md`

## Goal

Prove that storage-next L6 restores old branch-isolated LSM mechanics while
preserving storage-next boundaries and correctness.

The tests must prove both behavior and mechanical shape:

1. latest, version, timestamp, history, prefix, and range reads return the same
   visible rows as the independent branch model;
2. point reads over nonzero levels probe at most one table per level per
   readable layer;
3. scans over nonzero levels create lazy level cursors rather than one cursor
   per table;
4. history for one key does not scan unrelated physical keys;
5. read-view capture does not clone table rows;
6. branch compaction and materialization preserve semantics while using bounded
   source preparation;
7. L6 code does not cross into L5, L7, L8, L9, backend, durable-service, or
   product boundaries.

## Audit Finding References

Primary sources:

1. `docs/architecture/perf-tuning/storage-next-mechanics-parity-audit.md`
   - `L6. Branch-Isolated LSM Runtime`
   - `9. Differential Tests And Perf Counters`
   - `10. Final Parity Matrix And Architecture-Aligned Gap Plan`
2. `docs/architecture/perf-tuning/storage-next-serving-path-parity-plan.md`
   - `Old Invariants To Restore`
   - `PERF-T3: Read Snapshot Pinning`
   - `PERF-T4: Point Read Seek Over Existing Internal Keys`
   - `PERF-T5: Lazy Range Scan With Limit Pushdown`
   - `Hot-path-specific gates`
3. `docs/architecture/implementation-plans/m4p-storage-next-parity-restoration-test-plan.md`
   - `Layer Test Matrix`
   - `Differential Testing`
   - `Performance Testing Methodology`
   - `Fail-fast performance invariants`
   - `Source Guards`
   - `Fuzz And Generated Testing`

Old regression anchors:

1. `crates/storage/src/segmented/mod.rs`
2. `crates/storage/src/segment.rs`
3. `crates/storage/src/seekable.rs`
4. `crates/storage/src/merge_iter.rs`
5. `crates/storage/src/segmented/compaction.rs`
6. `crates/storage/src/compaction.rs`
7. `crates/storage/src/segmented/tests/fork.rs`
8. `crates/storage/src/segmented/tests/leveled.rs`
9. `crates/storage/src/segmented/tests/materialize.rs`
10. `crates/storage/src/segmented/tests/resurrection.rs`
11. `crates/storage/src/segmented/tests/post_restart_branch.rs`

Storage-next test targets:

1. `crates/storage-next/src/branch/tests/read_view.rs`
2. `crates/storage-next/src/branch/tests/immutable_reads.rs`
3. `crates/storage-next/src/branch/tests/owned_compaction.rs`
4. `crates/storage-next/src/branch/tests/inheritance_materialization/`
5. `crates/storage-next/src/branch/tests/row_pruning/`
6. `crates/storage-next/src/branch/tests/snapshot_install.rs`
7. `crates/storage-next/src/testkit/branch_lsm/`
8. `crates/storage-next/tests/branch_lsm_properties.rs`
9. `crates/storage-next/tests/branch_lsm_source_guard.rs`
10. `crates/storage-next/tests/branch_lsm_closeout.rs`
11. `crates/storage-next/src/observability/perf_trace.rs`

## Test Matrix

| Slice | Required proof | Failure caught |
| --- | --- | --- |
| M4P-L6A | Source-layout facts and source-class counters are accurate for active, frozen, owned L0, owned nonzero levels, inherited L0, and inherited nonzero levels. | Later benchmarks cannot explain whether work scales by rows, tables, or levels. |
| M4P-L6B | Point reads return correct rows and nonzero probes are level-count bounded. | New storage continues probing every nonzero table for one key. |
| M4P-L6C | Prefix/range scans return correct rows and nonzero cursor setup is level-count bounded. | Scan setup opens every nonzero table before producing rows. |
| M4P-L6D | History/timestamp/facts paths avoid unrelated row scans or record typed deferrals. | Single-key history or facts calls hide retained-row scaling. |
| M4P-L6E | Read views pin stable sources without row cloning and remain isolated after mutation. | Snapshot isolation regresses or read-view capture remains row-scaled. |
| M4P-L6F | Branch compaction uses bounded source preparation and preserves install/pruning semantics. | Compaction clones full input tables or drops visible/history rows. |
| M4P-L6G | Materialization uses bounded source preparation and preserves fork/shadow/retry/reachability semantics. | Materialization diverges from inherited reads or remains full-layer eager. |
| M4P-L6H | Fork preconditions and higher-layer handoff are explicit and tested. | L6 absorbs L7/L8 work or silently loses fork-frontier correctness. |
| M4P-L6I | Model, generated, source-guard, and benchmark closeout all pass. | A local fix works only for one hand-written case or one benchmark. |

## Source-Layout And Counter Tests

Add source-layout tests that construct these branch shapes directly through L6
test helpers:

1. active-only branch;
2. active plus frozen tables;
3. owned L0-only branch with overlapping physical ranges;
4. owned nonzero levels with many non-overlapping tables;
5. owned L0 plus owned nonzero levels;
6. inherited L0-only layer;
7. inherited nonzero levels with key rewriting and fork-version bounds;
8. multiple inherited layers nearest ancestor first;
9. materializing, materialized, and unavailable inherited-layer statuses;
10. mixed owned and inherited source layouts.

Assertions:

1. source-layout facts match constructed topology;
2. source-layout facts do not require row scans on the normal path;
3. point counters increment the correct source classes;
4. scan counters distinguish L0 table cursors from nonzero level cursors;
5. history counters distinguish key-local work from unrelated-row scans;
6. perf-trace reset/snapshot behavior is deterministic in tests.

## Point-Read Tests

Correctness cases:

1. latest hit in active table shadows every older source;
2. frozen hit shadows owned and inherited tables;
3. owned L0 hit shadows owned nonzero and inherited sources;
4. owned nonzero hit returns the newest visible row in that level/table;
5. inherited hit is rewritten back to the child branch key;
6. child-local tombstone hides inherited rows;
7. source tombstone is visible in history and hidden in latest reads according
   to current documented rules;
8. version-bounded reads ignore rows after the bound;
9. timestamp-bounded reads map through the current timestamp facts contract;
10. TTL-expired rows are filtered according to current documented rules;
11. missing keys return no row without probing unrelated nonzero tables;
12. first-key, last-key, and out-of-range keys are covered.

Mechanical cases:

1. with 100 nonzero tables in one level, a point read probes at most one table
   from that level;
2. with multiple nonzero levels, a point read probes at most one table per
   level;
3. with inherited nonzero levels, the same bound applies per readable inherited
   layer after key rewrite;
4. L0 remains per-table because L0 ranges overlap by design;
5. active and frozen sources remain probed according to current source
   precedence;
6. false-positive table filter results do not make a row authoritative;
7. absent/disabled filters preserve correctness and source-pruning counters.

Required counter assertions:

1. `point_owned_nonzero_table_probes <= owned_nonzero_level_searches`.
2. `point_inherited_nonzero_table_probes <= inherited_nonzero_level_searches`.
3. `point_rows_visited` is bounded by selected table seeks and the key's
   version chain, not total retained rows.
4. no counter indicates table-count-scaled nonzero probing for one key.

## Scan Tests

Correctness cases:

1. prefix scans over active/frozen/owned/inherited sources preserve key order
   and source precedence;
2. range scans over the same sources preserve key order and source precedence;
3. L0 overlapping tables merge correctly;
4. nonzero levels merge correctly with active/frozen/L0 sources;
5. inherited rows are rewritten to child branch keys;
6. fork-version caps suppress source rows after the fork;
7. child-local rows shadow inherited rows;
8. tombstones and TTL filtering match current documented rules;
9. scan limits stop after the requested visible rows;
10. empty ranges, single-key ranges, prefix boundary ranges, first-key ranges,
    and last-key ranges are covered.

Mechanical cases:

1. nonzero scan setup creates one lazy level cursor per nonzero level, not one
   table cursor per table;
2. prefix/range overlap pruning happens before cursor creation;
3. lazy level cursors open table cursors only when the scan advances into that
   table's range;
4. L0 remains per-table because L0 ranges overlap by design;
5. inherited nonzero levels use lazy level cursors after key rewriting;
6. limit-10 scans do not collect all visible rows before applying the limit.

Required counter assertions:

1. `scan_owned_nonzero_level_cursors` is bounded by owned nonzero level count.
2. `scan_inherited_nonzero_level_cursors` is bounded by inherited nonzero
   level count.
3. `scan_owned_nonzero_table_cursors_opened` is bounded by tables actually
   reached by the scan.
4. `scan_rows_visited_per_row_returned` remains explainable by MVCC versions
   and skipped rows, not total table count.

## History, Timestamp, And Facts Tests

History tests:

1. many unrelated keys plus many versions of one target key;
2. history returns only the target key's versions;
3. history ordering is newest to oldest where currently documented;
4. history limits stop after the requested number of versions;
5. inherited history respects fork-version bounds and child shadowing;
6. tombstones and TTL are handled according to current documented behavior.

Counter assertions:

1. unrelated row scans for single-key history are zero;
2. selected source probes are bounded by active/frozen/L0 plus nonzero levels;
3. rows visited are bounded by the target key's visible/version chain and
   selected table seeks.

Timestamp tests:

1. timestamp lookup finds the newest commit version at or before the timestamp;
2. timestamp lookup handles no matching timestamp;
3. timestamp lookup handles inherited layers and fork-version caps;
4. timestamp lookup after flush, compaction, materialization, and snapshot
   install uses maintained/recovered facts.

If L7 timeline facts are not yet available, tests must assert the current
counter-visible row scan and mark the exact deferral owner in the implementation
plan. Do not silently accept hidden row scans.

Facts tests:

1. normal branch facts calls report source layout, version, timestamp, put, and
   tombstone facts without scanning table rows;
2. recovery/rebuild paths may scan rows, but only through explicit validation
   or rebuild APIs;
3. facts remain correct after append, rotation, flush install, compaction
   install, materialization install, and snapshot replacement.

## Read-View Pinning Tests

Required behavior:

1. a read view captured before a later commit does not observe the later commit;
2. a read view captured before active-table rotation remains stable;
3. a read view captured before flush installation remains stable;
4. a read view captured before compaction replacement remains stable;
5. a read view captured before materialization replacement remains stable;
6. a read view captured before snapshot branch replacement remains stable;
7. cleanup/reachability cannot reclaim a pinned source while the read view can
   still read it.

Required counters:

1. read-view captures increment once per capture;
2. source handles cloned are bounded by source count;
3. table rows cloned are zero on ordinary read-view capture;
4. row clone bytes are zero on ordinary read-view capture.

Failure cases:

1. rejected branch states do not produce partial read views;
2. unavailable inherited layers are rejected or skipped according to current
   branch status rules;
3. stale table handles after replacement remain readable through the pinned
   view or are protected by typed reachability facts.

## Branch Compaction Tests

Correctness cases:

1. L0 compaction preserves visible rows;
2. L0-to-L1 compaction preserves visible rows;
3. nonzero-level compaction preserves sorted non-overlapping level invariants;
4. overlapping target-level selection is correct;
5. stale-candidate validation rejects changed inputs;
6. pruning proof validation prevents unsafe row drops;
7. tombstones, TTL, retained history, and duplicate internal keys follow
   current documented policies;
8. shared-table reachability and table-manifest coverage remain valid.

Mechanical cases:

1. L6 source preparation does not clone full input tables into row vectors on
   the standard path;
2. source-open counts match selected compaction inputs;
3. peak buffered rows are bounded by streaming mechanics and output flush
   thresholds;
4. L5 owns table output construction; L6 owns candidate/level install logic.

Fault cases:

1. output build failure leaves branch state unchanged;
2. install validation failure leaves branch state unchanged;
3. stale candidate after concurrent mutation leaves branch state unchanged;
4. failed pruning proof leaves branch state unchanged.

## Materialization Tests

Correctness cases:

1. materialized reads match inherited reads before materialization;
2. materialized scans match inherited scans before materialization;
3. fork-version filtering excludes source rows after the fork;
4. child-local rows shadow inherited rows;
5. inherited tombstones and TTL behavior are preserved;
6. duplicate replacement rows are rejected;
7. higher-precedence collisions are rejected;
8. materialized layer removal and replacement table install are atomic at the
   L6 state boundary.

Retry/recovery cases:

1. materialization can retry after prepared work fails before install;
2. materialization can retry after a stale source state is detected;
3. reachability is bound before replacement is installed;
4. materialized/unavailable statuses behave according to current documented
   branch status rules.

Mechanical cases:

1. materialization source preparation does not collect the whole inherited
   layer into one row vector on the standard path;
2. rows rewritten, rows skipped by fork-version, rows skipped by child
   shadowing, output tables, and peak buffered rows are counted;
3. replacement output uses the same streaming artifact path as branch
   compaction once that path exists.

## Fork Contract Tests

Required cases:

1. empty-child fork captures readable source topology with correct
   fork-version facts;
2. self-fork is rejected;
3. unavailable inherited layers are rejected or skipped according to current
   documented rules;
4. active/frozen source preconditions return typed errors where L6 requires a
   flushed source;
5. child-local put shadows inherited parent row;
6. child-local tombstone hides inherited parent row;
7. parent compaction after fork does not change child visibility;
8. materialization after fork does not change child visibility.

Boundary assertions:

1. L6 tests prove safe fork mechanics and typed preconditions.
2. L7/L8 tests own any public quiesce/flush/retry orchestration.
3. L9 tests own public API wording and mode behavior.

## Independent Model And Generated Tests

Extend the branch LSM model so generated scripts can express:

1. append rows to active table;
2. rotate active to frozen;
3. install L0 tables;
4. install nonzero-level tables;
5. build many tables per nonzero level;
6. fork branches;
7. apply child-local writes and tombstones;
8. materialize inherited layers;
9. compact owned levels;
10. replace branch state from a snapshot;
11. run latest, version, timestamp, history, prefix, and range reads.

Generated scripts must record:

1. seed;
2. operation list;
3. source layout before every read;
4. expected visible rows from the independent model;
5. source-shape counters for performance-sensitive operations;
6. any semantic decision that changes old-storage comparison rules.

Corpus seeds:

1. many L1+ tables and one point miss;
2. many L1+ tables and one point hit near the first table;
3. many L1+ tables and one point hit near the last table;
4. prefix scan over a narrow range in many L1+ tables;
5. range scan with limit over many L1+ tables;
6. inherited chain with nearest-ancestor shadowing;
7. inherited tombstone plus child resurrection;
8. materialization retry after source mutation;
9. compaction after fork;
10. timestamp reads after compaction and materialization.

## Differential Tests

Where old storage remains executable, compare storage-next to old storage for:

1. blind puts and deletes;
2. put/delete/put resurrection;
3. latest reads;
4. version-bounded reads;
5. timestamp-bounded reads where semantics are comparable;
6. history reads;
7. prefix scans;
8. range scans;
9. branch fork and child-local shadowing;
10. materialization;
11. compaction after sustained load;
12. restart after branch-visible transitions where L4/L8 provide the durable
    harness.

Comparison rules:

1. compare visible rows and ordering first;
2. compare retained history only where both engines document the same
   retention/TTL behavior;
3. compare source-shape counters separately from throughput;
4. record every skipped or reinterpreted old behavior in the semantic decision
   register before weakening a test.

## Source Guards

Add or extend source guards proving L6 production Rust does not:

1. import L7 commit runtime modules;
2. import L8 lifecycle scheduler or recovery modules;
3. import L9 API modules;
4. import backend or filesystem modules;
5. construct object names or durable paths directly;
6. import product DTOs;
7. add roadmap labels to Rust identifiers, comments, fixture bytes, panic
   messages, or user-visible text;
8. inspect table bytes or duplicate L5 table-local seek/cache/filter logic.

LSM level terms such as `L0` through `L7` may appear only when they refer to
actual LSM levels, not architecture roadmap layers.

## Benchmark Gates

Benchmark rules:

1. run old and new engines serially;
2. use the L9 benchmark surface;
3. include perf-trace state and source-shape counters;
4. compare source-shape counters before wall-clock throughput;
5. record machine, build profile, git revision, mode, durability policy,
   backend, feature state, key count, value size, scan samples, scan limit, and
   maintenance policy.

Required scales:

1. 100K keys after every slice that changes point, scan, history, read-view,
   compaction, or materialization behavior;
2. 1M keys after point pruning, scan planning, and bounded history/facts;
3. 5M keys after point pruning, scan planning, and bounded history/facts;
4. 10M keys as the L6 parity gate;
5. 50M and 100M keys only after 10M source-shape counters are clean.

Required derived metrics:

1. `point_source_probes_per_read`;
2. `point_nonzero_table_probes_per_read`;
3. `scan_source_cursors_per_call`;
4. `scan_table_cursors_opened_per_call`;
5. `scan_rows_visited_per_row_returned`;
6. `l0_tables_per_million_rows_after_load`;
7. old-to-new throughput ratio for load, point latest, point throughput, scan
   prefix, and scan range.

Fail-fast invariants:

1. point reads over nonzero levels must probe at most one table per nonzero
   level per readable layer;
2. scan setup over nonzero levels must create lazy level cursors rather than
   one eager cursor per table;
3. history for a single key must not scan unrelated physical keys;
4. ordinary read-view capture must not clone table rows;
5. cache and durable modes must use the same branch-local serving algorithms
   after durable-only persistence facts are ignored.

## Required Verification Commands

Run focused commands after the relevant slice, then the broader commands at
closeout:

1. `cargo fmt --package strata-storage-next --check`
2. `cargo clippy -p strata-storage-next --lib --features perf-trace -- -D warnings`
3. `cargo test -p strata-storage-next --features perf-trace branch`
4. `cargo test -p strata-storage-next --features perf-trace branch_lsm`
5. `cargo test -p strata-storage-next --all-features`
6. L9 old-vs-new benchmark matrix for the required scales.

If a command is too broad for the local environment, record the exact narrower
command used, the missing gate, and why it could not run.

## Closeout Requirements

M4P-L6 is complete only when:

1. every L6 audit finding is closed or explicitly deferred with owner layer,
   reason, and replacement proof;
2. source-layout and source-class counters are tested;
3. point, scan, history, timestamp, facts, read-view, compaction,
   materialization, and fork tests pass;
4. generated branch scripts cover source topology and inherited-layer cases;
5. source guards pass;
6. cache and durable modes show identical branch-local source layout for the
   same workload after durable-only facts are ignored;
7. 10M source-shape counters are clean;
8. old-vs-new throughput comparisons are explainable by counters;
9. all deferred L7/L8/L9 handoffs are documented in the implementation plan.
