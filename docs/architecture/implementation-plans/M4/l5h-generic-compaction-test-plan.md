# L5H Test Plan: Generic Table Compaction

Status: draft test plan

Parent plan:
`docs/architecture/implementation-plans/M4/l5h-generic-compaction-implementation-plan.md`

## Goal

Prove that L5H is a deterministic, policy-free compaction executor over L5
table rows and M3G artifacts.

The suite must fail if L5H:

1. drops a row without an explicit policy decision;
2. keeps or drops rows based on hidden branch, snapshot, TTL, tombstone, or
   product-family rules;
3. changes row bytes, value bytes, commit versions, timestamps, tombstone bits,
   branch bytes, or storage-space ids;
4. emits unsorted output;
5. silently resolves exact duplicate internal keys by source order;
6. builds non-M3G table bytes;
7. exceeds `max_output_tables`;
8. publishes objects, builds object names, reads paths, or calls backend/L4
   services;
9. makes cache or accelerator state part of compaction correctness;
10. regresses old compaction edge cases that are now caller-policy cases.

## Test Locations

Use these locations:

1. `crates/storage-next/src/table/tests/compaction.rs` for module-local
   compaction tests.
2. `crates/storage-next/src/table/tests/mod.rs` to register the test module.
3. `crates/storage-next/src/testkit/table_runtime.rs` for generated compaction
   cases and model checks.
4. `crates/storage-next/tests/table_runtime_properties.rs` for generated L5H
   property tests behind the `testkit` feature.
5. `crates/storage-next/tests/table_runtime_source_guard.rs` for import and
   vocabulary guards.
6. `docs/architecture/implementation-plans/M4/m4-l5-porting-log.md` for the
   old compaction behavior classification.

Tests should use behavior-based names, not milestone names. For example, prefer
`keep_all_policy_preserves_only_tombstone_and_expired_fixtures` over names containing
implementation-plan labels.

## Reference Model

Use an independent sorted-vector model.

Inputs:

```text
sources: Vec<Vec<ModelRow>>
policy: Fn(ModelRowContext, ModelRow) -> ModelDecision
target_output_bytes: u64
max_output_tables: usize
```

Model algorithm:

```text
validate every source is strictly sorted by encoded internal key
merge all rows by encoded internal key
reject exact duplicate internal keys
for each merged row:
  input_rows += 1
  decision = policy(context, row)
  if decision == keep:
    append row to kept_rows
  else:
    record drop reason
split kept_rows by the documented split rule
reject output_table_count > max_output_tables
```

The model must not use L5H code. It may use existing storage-next row/key
encoders because those are the contract under test, but merge, policy, and
split expectations should be computed separately.

## Required Unit Tests

### 1. Request And Config Validation

1. Valid request with one nonempty source is accepted.
2. Valid request with multiple sources is accepted.
3. Empty source set follows the documented contract.
4. Source with zero rows follows the documented contract.
5. All sources empty returns zero artifacts or a typed error according to the
   documented contract.
6. `target_output_bytes = 0` is rejected through config validation.
7. `max_output_tables = 0` is rejected through config validation.
8. Output identity seed is validated.
9. Source ids are bounded for display.
10. Invalid estimated byte counts are rejected if the API accepts estimates.
11. Request validation happens before policy is called.
12. Request validation happens before any table artifact is built.

### 2. Source Ordering

1. One sorted source compacts successfully.
2. One source with descending adjacent keys returns `InvalidRowOrder`.
3. One source with equal adjacent keys returns `DuplicateInternalKey`.
4. Out-of-order row after several valid rows is caught.
5. Multiple individually sorted sources compact successfully.
6. A later source with local disorder is caught.
7. Source ordering checks do not require materializing branch or table-object
   identity.
8. Ordering is based on encoded internal-key bytes, not physical key only.
9. Different commit versions for the same physical key remain ordered by the
   internal-key encoding.
10. Embedded zero bytes in physical keys do not break ordering.

### 3. Keep-All Identity

1. One source with keep-all policy returns the same rows in the same order.
2. Multiple disjoint sources with keep-all policy return sorted union.
3. Multiple interleaved sources with keep-all policy return sorted union.
4. Empty physical-key prefixes are preserved if valid row construction allows
   them.
5. Large value bytes are preserved byte-for-byte.
6. Tombstone rows are preserved.
7. Expired-looking rows are preserved.
8. Old commit versions are preserved.
9. Multiple versions of one physical key are preserved.
10. Storage-space ids are preserved.
11. Branch bytes are preserved as opaque physical-key bytes.
12. Commit timestamps and expiry timestamps are preserved.
13. Output rows match the model exactly.

### 4. Cross-Source Duplicates

1. Exact duplicate internal keys across two sources are rejected.
2. Exact duplicate internal keys across more than two sources are rejected.
3. Duplicate physical keys with different commit versions are accepted.
4. Duplicate user-key bytes in different storage-space ids are accepted.
5. Duplicate-looking prefixes are accepted when encoded internal keys differ.
6. Duplicate rejection occurs before output artifact build.
7. Duplicate rejection does not call row-retention policy to resolve priority
   unless the implementation deliberately exposes explicit duplicate policy.

Named coverage should include
`global_duplicate_rejection_runs_before_policy`.

### 5. Policy Decisions

1. Drop-exact-key policy drops only the selected key.
2. Drop-by-row-ordinal policy drops only selected ordinals.
3. Drop-tombstone policy drops only tombstone rows selected by the policy.
4. Drop-expired-looking policy drops only rows selected by the policy.
5. Drop-old-version policy drops only rows selected by the policy.
6. Drop-put policy drops only selected non-tombstone rows.
7. Mixed keep/drop decisions produce expected report counts.
8. Drop reasons are counted by reason.
9. Policy sees rows in merged order.
10. Policy context source ids match the selected source.
11. Policy error aborts compaction with `CompactionPolicy`.
12. Policy error returns no partial output.
13. Policy is not called for rows that fail input validation before merge.
14. Keep-all policy is the default for model tests unless a drop case requires
    otherwise.

Named coverage should include
`compaction_policy_can_drop_older_physical_key_versions_explicitly`.

### 6. No Hidden Retention Semantics

1. Below-floor-looking old rows are kept under keep-all policy.
2. Above-floor-looking tombstones are kept under keep-all policy.
3. Below-floor-looking tombstones are kept under keep-all policy.
4. Expired-looking rows are kept under keep-all policy.
5. Snapshot-floor-looking rows are not special-cased.
6. Max-version-looking groups are not pruned.
7. Event-shaped rows are not special-cased.
8. Product-family-shaped payload bytes are not inspected.
9. Branch-local level facts are not required by the API.
10. Bottommost/non-bottommost is not inferred by L5H.

Named coverage should include
`keep_all_policy_preserves_only_tombstone_and_expired_fixtures`.

### 7. Artifact Build

1. Keep-all output builds one valid M3G artifact when under target size.
2. Policy-dropped output builds a valid M3G artifact with only kept rows.
3. All-drop output follows the documented zero-output or typed-error contract.
4. Output artifact bytes decode through `decode_immutable_table`.
5. Output artifact opens through `ImmutableTableReader`.
6. Reader rows equal model rows.
7. Output artifact facts row count matches kept rows.
8. Output artifact key range matches first and last kept rows.
9. Output artifact commit range matches kept rows.
10. Output artifact byte count matches actual bytes length.
11. Uncompressed builder config is honored.
12. Zstd builder config is honored.
13. Builder format errors are routed as table-runtime errors.
14. Compaction never emits old `STRAKV` bytes.

### 8. Output Splitting

1. Target larger than all rows produces one artifact.
2. Small target produces multiple artifacts.
3. No artifact is empty.
4. Every artifact individually has sorted unique rows.
5. Concatenating artifact rows equals model kept rows.
6. Split count is reported.
7. Output table count is reported.
8. Actual output bytes are reported as the sum of artifact bytes.
9. Single row larger than target produces one oversized artifact.
10. Oversized physical-key group behavior matches the documented rule.
11. Splits happen only between rows.
12. If physical-key boundary preservation is implemented, all versions of a
    physical key stay in one artifact when possible.
13. If pure row-boundary splitting is chosen instead, tests assert that exact
    documented behavior.
14. `max_output_tables = 1` rejects any compaction needing two outputs.
15. `max_output_tables = N` accepts exactly N outputs.
16. Exceeding `max_output_tables` returns a typed error and no partial result.
17. Split output identities are deterministic and distinct.
18. Re-running the same compaction yields byte-identical artifacts.

### 9. Report And Stats

1. `input_sources` matches request source count.
2. `input_rows` counts every valid merged input row.
3. `kept_rows` counts every output row.
4. `dropped_rows = input_rows - kept_rows`.
5. Drop reason summaries add up to `dropped_rows`.
6. `output_tables` matches artifact count.
7. `output_bytes` matches actual artifact byte sum.
8. `split_count` matches `output_tables - 1` when output is nonempty.
9. Empty/all-drop report follows the documented convention.
10. Existing `TableRuntimeStats` compaction fields, if used, match the report.
11. Report display/debug output is bounded for large row counts and ids.
12. Report contains no object names, paths, branch ids as policy facts, or
    product value previews.

### 10. Cursor Error Propagation

1. Source cursor failure during `seek_to_first` aborts compaction.
2. Source cursor failure during `advance` aborts compaction.
3. Source cursor failure after some kept rows returns no partial output.
4. Source cursor failure is not rewritten as format corruption.
5. Policy is not called after a cursor error.
6. Builder is not called after a cursor error.

### 11. Cache And Accelerator Neutrality

1. Compaction output is identical with cache enabled and disabled in readers
   used to build sources.
2. Compaction does not require a `TableBlockCache`.
3. Compaction does not mutate cache stats except through ordinary reader
   construction if reader-backed sources are used.
4. Bloom/filter accelerators do not decide row drops.
5. Missing accelerators do not change compaction output.
6. Corrupt optional accelerators surface through reader/source errors before
   L5H sees rows, or fall back conservatively according to reader contract.

### 12. Determinism

1. Same rows, policy, and config produce byte-identical artifacts.
2. Different source grouping with same global row set produces the same output.
3. Different source order with disjoint keys produces the same output.
4. Interleaved source order still produces the same output.
5. Drop reason ordering is deterministic.
6. Split output identities are deterministic.
7. No timestamps, random ids, process ids, pointer addresses, or filesystem
   paths enter output bytes or reports.

## Required Generated Tests

Extend the table-runtime testkit with compaction scripts.

For each generated case:

1. generate 1 to 16 sources;
2. generate 0 to 4096 total rows;
3. vary source counts across empty, single, small, and heap-merge paths;
4. generate disjoint, interleaved, and same-physical-key sources;
5. generate tombstones and non-tombstones;
6. generate expired-looking and non-expired-looking timestamps;
7. generate many versions of the same physical key;
8. generate storage-space ids across reserved and engine-owned allowed ranges;
9. generate value sizes across empty, small, block-boundary, and large cases;
10. generate keep-all, drop-by-key-set, drop-by-ordinal, and mixed policies;
11. generate target output bytes across tiny, exact-fit, and roomy cases;
12. generate `max_output_tables` both below and above needed output count;
13. optionally inject cursor errors and policy errors;
14. compare row output and reports to the independent model;
15. decode every produced artifact and read it back through
    `ImmutableTableReader`;
16. assert no hidden row pruning under keep-all policy.

Generated tests should run with fixed deterministic seeds and bounded case
counts suitable for normal CI. Larger soak counts can be left as ignored or
documented local stress commands.

## Regression Map From Old Compaction

Recreate these old behaviors as caller-policy tests, not as built-in L5H
rules:

1. keep every row when pruning floor is zero;
2. keep all rows above a floor;
3. keep one older version only when policy selects that behavior;
4. drop a below-floor tombstone only when policy selects that behavior;
5. preserve below-floor tombstone when policy selects non-bottommost behavior;
6. protect snapshot-floor rows when policy selects that behavior;
7. max-version pruning only by policy;
8. above-floor tombstones preserved unless policy drops them;
9. TTL rows dropped only by policy;
10. event-shaped rows kept unless policy drops them;
11. multiple physical keys tracked independently by policy;
12. empty input behavior documented and tested.

Recreate these old splitting behaviors mechanically:

1. split at target output size;
2. do not split before the first row;
3. avoid empty outputs;
4. keep output sorted across split artifacts;
5. handle one row larger than target.

Defer these old segmented behaviors to L6/L8 tests:

1. level score computation;
2. L0-to-L1 overlap selection;
3. grandparent-overlap split predicates;
4. branch manifest swaps;
5. concurrent flush preservation;
6. old segment quarantine and purge;
7. branch deletion during compaction;
8. rate limiting;
9. checkpoint and WAL truncation coordination.

## Source Guards

Extend `table_runtime_source_guard.rs` so L5H production code fails if it
contains:

1. imports from `service`, `backend`, `layout`, `branch`, `lifecycle`, `commit`,
   or engine crates;
2. `std::fs`, `Path`, `PathBuf`, `File`, `rename`, `remove_file`, `pread`,
   `mmap`, or backend object APIs;
3. object-name strings such as `tables/`, `manifest`, `wal/`, or `snapshots/`;
4. old format strings such as `STRAKV`;
5. product payload vocabulary such as `Value`, `EntityRef`, MessagePack, or
   engine capabilities;
6. hidden policy words in production logic such as `bottommost`,
   `snapshot_floor`, `prune_floor`, `max_versions`, or `drop_expired`, unless
   they appear only in caller-policy test helpers or docs;
7. process-global mutable state.

The guard should allow neutral words in docs/tests where they classify deferred
policy, but production `table/compaction.rs` must stay policy-free.

## Verification Commands

Run at least:

```sh
cargo test -p strata-storage-next --locked --lib table::tests::compaction
cargo test -p strata-storage-next --locked --lib table::tests
cargo test -p strata-storage-next --features testkit --locked --test table_runtime_properties
cargo test -p strata-storage-next --no-default-features --features testkit --locked --test table_runtime_properties
cargo test -p strata-storage-next --locked --test table_runtime_source_guard
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

If compaction tests add generated large cases, also provide a bounded local
stress command and keep it outside default CI unless runtime is proven small.

## Exit Gate

The L5H test suite is complete when:

1. unit tests cover request validation, ordering, policy decisions, duplicate
   handling, output artifact build, splitting, reports, and errors;
2. generated tests compare L5H against an independent model;
3. every successful output artifact decodes and reads through the M3G reader;
4. keep-all policy preserves tombstones, expired-looking rows, old versions,
   event-shaped rows, branch bytes, storage-space ids, timestamps, and values;
5. every drop is explained by explicit policy;
6. no hidden L6/L8 policy is present in L5H production code;
7. source guards prevent object, backend, filesystem, upper-layer, and product
   vocabulary leaks;
8. no-default, wasm testkit, clippy, fmt, and diff checks pass.
