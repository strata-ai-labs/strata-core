# L5D Test Plan: Raw Cursors And Merge Cursor

Status: draft test plan

Parent plan:
`docs/architecture/implementation-plans/M4/l5d-raw-cursors-merge-cursor-implementation-plan.md`

## Goal

Prove that L5D implements raw table cursor mechanics over storage-next rows
without importing MVCC latest selection, branch rewriting, TTL filtering,
tombstone hiding, immutable-reader behavior, backend IO, or product semantics.

The suite must fail if L5D:

1. returns rows out of encoded internal-key order;
2. seeks to the previous row instead of the first row greater than or equal to
   the target;
3. panics during ordinary empty, missing, or exhausted cursor movement;
4. skips tombstones;
5. skips expired rows;
6. collapses duplicate physical keys at different commit versions;
7. deduplicates exact internal keys across sources;
8. treats source-index tie break as visibility or latest-selection policy;
9. rewrites branch ids;
10. applies snapshot/as-of/fork-version filters;
11. mutates child cursor state incorrectly during merge seek or advance;
12. only tests the linear merge path or only tests the heap merge path;
13. depends on old `crates/storage` row/key/value types;
14. imports backend, filesystem, lifecycle, branch, commit, or engine APIs.

## Test Locations

Use these locations:

1. `crates/storage-next/src/table/tests/cursor.rs` for module-local L5D unit
   tests, with `mod cursor;` from `src/table/tests/mod.rs`.
2. `crates/storage-next/src/testkit/table_runtime.rs` for generated L5D cursor
   and merge model checks.
3. `crates/storage-next/tests/table_runtime_properties.rs` for generated L5D
   property tests behind the `testkit` feature.
4. `crates/storage-next/tests/table_runtime_source_guard.rs` for source-boundary
   scans and executable guard probes.
5. `docs/architecture/implementation-plans/M4/m4-l5-porting-log.md` for the
   old-cursor porting record.

Do not add immutable table reader tests here. L5F owns immutable-reader cursor
parity. L5D tests should use `MutableTable`, `FrozenTable`, and small test-only
row sources where needed.

## Reference Model

All cursor and merge tests compare against an independent sorted-vector model.

For one source:

```text
model_rows = source rows sorted by encoded internal key
seek(target) = first model row where key >= target
advance() = next model row after current index
```

For bounded cursors:

```text
bounded_model_rows = model_rows filtered by TableKeyBounds::contains_key
seek(target) = first bounded row where key >= target
```

For merge:

```text
model_items = all child rows annotated with source_index and row_index
sort by (encoded_internal_key ASC, source_index ASC, row_index ASC)
```

The model intentionally preserves every row. It does not collapse duplicate
physical keys, duplicate exact keys across sources, tombstones, or expired
rows.

## Required Unit Tests

### 1. Cursor State Contract

1. Empty mutable cursor starts exhausted after `seek_to_first`.
2. Empty frozen cursor starts exhausted after `seek_to_first`.
3. Empty cursor remains exhausted after repeated `advance`.
4. Empty cursor remains exhausted after `seek` before any possible row.
5. `current()` returns `None` while exhausted.
6. `current_key()` returns `None` while exhausted.
7. A one-row cursor returns that row once.
8. A one-row cursor becomes exhausted after one `advance`.
9. Repeated `advance` after exhaustion remains exhausted.
10. Calling `seek_to_first` after exhaustion repositions at the first row.
11. Calling `seek` after exhaustion repositions according to the target.
12. No ordinary state transition panics.

### 2. Mutable And Frozen Cursor Parity

1. Mutable and frozen cursors over the same empty table match.
2. Mutable and frozen cursors over one row match.
3. Mutable and frozen cursors over many rows match.
4. Mutable and frozen cursors preserve row metadata exactly.
5. Mutable and frozen cursors return rows in the same encoded-key order.
6. Mutable and frozen cursors handle tombstones identically.
7. Mutable and frozen cursors handle expired-looking rows identically.
8. Mutable and frozen cursors handle duplicate physical keys at distinct commit
   versions identically.

### 3. Seek Boundaries

1. Seek before the first row positions at the first row.
2. Seek exactly to the first row positions at the first row.
3. Seek exactly to a middle row positions at that row.
4. Seek exactly to the last row positions at the last row.
5. Seek into a gap positions at the first greater row.
6. Seek after the last row exhausts the cursor.
7. Seek to a missing commit version for an existing physical key follows raw
   encoded-key ordering.
8. Seek to a row with embedded zero bytes in the user key follows encoded-key
   ordering.
9. Seek is independent of commit timestamp.
10. Seek is independent of expiry timestamp.
11. Seek is independent of tombstone flag.
12. Repeated seeks are idempotent.
13. Seek after partial iteration forgets the previous position and uses the
   target.

### 4. Bounds And Prefixes

1. Unbounded cursor returns all rows.
2. Exact bound returns the matching row when present.
3. Exact bound returns empty when absent.
4. Closed range includes lower and upper endpoints.
5. Open range excludes lower and upper endpoints.
6. Lower-unbounded range starts at the first row and stops at the upper bound.
7. Upper-unbounded range starts after the lower bound and continues to
   exhaustion.
8. Equal inclusive bounds return a singleton when present.
9. Equal exclusive bounds return empty.
10. Prefix bound returns every version for a physical key.
11. Prefix bound stops before adjacent user keys with the same byte prefix only
    when those bytes are outside the encoded physical-key prefix.
12. Prefix bound does not cross branch-id bytes.
13. Prefix bound does not cross storage-space-id bytes.
14. Bounds return tombstones.
15. Bounds return expired rows.
16. Bounds do not perform latest-version selection.

### 5. Raw Semantics Guard Tests

1. Multiple versions for one physical key are all emitted.
2. Versions for one physical key appear newest commit first because of V1
   encoded-key order.
3. A tombstone version is emitted as a row.
4. An expired-looking put row is emitted as a row.
5. A row from another branch id is emitted if it is present in the source and
   within the raw bounds.
6. No cursor API accepts a snapshot commit version.
7. No cursor API accepts a fork version.
8. No cursor API accepts a branch rewrite target.

### 6. Merge Basics

1. Merge of zero sources is exhausted.
2. Merge of one empty source is exhausted.
3. Merge of one nonempty source is identity.
4. Merge of two empty sources is exhausted.
5. Merge of one empty and one nonempty source is identity.
6. Merge of two disjoint sources returns global sorted order.
7. Merge of many disjoint sources returns global sorted order.
8. Merge preserves source rows exactly.
9. Merge preserves tombstones.
10. Merge preserves expired rows.
11. Merge preserves duplicate physical-key versions.
12. Merge does not deduplicate exact internal keys across sources.

### 7. Merge Equal-Key Tie Break

1. Same encoded internal key in sources 0 and 1 emits source 0 first.
2. Same encoded internal key in sources 1 and 0 emits source 0 first regardless
   of constructor input row payload.
3. Same encoded internal key across five or more sources emits rows in source
   index order.
4. Equal-key tie break is stable across repeated seeks.
5. Equal-key tie break is stable after partial iteration and re-seek.
6. Equal-key tie break preserves all payload bytes.
7. Equal-key tie break does not inspect timestamps.
8. Equal-key tie break does not inspect tombstone state.

### 8. Merge Seek And Advance

1. `seek_to_first` positions at the global first row.
2. `seek` before all sources positions at the global first row.
3. `seek` to a key in one child positions at that child when it is the minimum.
4. `seek` to a key missing from every child positions at the first greater
   global row.
5. `seek` after all sources exhausts the merge cursor.
6. `advance` moves only the previously selected child.
7. Advancing one child does not skip a smaller row still current in another
   child.
8. Repeated `advance` drains every source exactly once.
9. Re-seek after partial drain repositions all children.
10. Re-seek after exhaustion repositions all children.
11. Repeated `seek` to the same target gives the same output.
12. Merge current row remains readable until `advance` is called.

### 9. Linear And Heap Paths

1. Zero-source path is covered.
2. One-source path is covered.
3. Two-source linear path is covered.
4. Four-source linear threshold path is covered.
5. Five-source heap threshold path is covered.
6. Sixteen-source heap stress path is covered.
7. Linear and heap paths return the same model output for equivalent source
   sets.
8. The many-source regression with shared equal keys across all sources is
   covered.

### 10. Invalid Inputs And Invariants

1. Test-only source constructors reject unsorted rows, if such constructors
   exist.
2. Test-only source constructors reject duplicate keys within one source, if
   they promise per-source uniqueness.
3. Invalid `TableKeyBounds` construction remains rejected before cursor use.
4. Merge constructor accepts empty source lists.
5. Merge constructor accepts empty child cursors.
6. Merge cursor reports typed source errors once L5F source-read cursors exist;
   until then, in-memory L5D cursors should not fabricate source errors.

## Required Generated Tests

Extend `check_table_runtime_scaffold_contract` or add a neighboring table
runtime check so the property harness must exercise L5D.

For each generated script:

1. generate 0 to 16 bounded row sources;
2. force at least one case at 0 sources;
3. force at least one case at 1 source;
4. force at least one case at 4 sources;
5. force at least one case at 5 or more sources;
6. include empty sources;
7. include one-row sources;
8. include disjoint source ranges;
9. include overlapping source ranges;
10. include duplicate exact internal keys across sources;
11. include duplicate physical keys at distinct commit versions;
12. include tombstones;
13. include expired-looking rows;
14. include user keys with embedded zero bytes;
15. include different branch id bytes and storage-space ids;
16. compare mutable and frozen cursor output to the one-source model;
17. compare bounded cursor output to filtered model rows;
18. compare merge cursor output to the stable merge model;
19. run generated seek, advance, re-seek, and collect operations;
20. enforce a fixed operation budget.

Default generated budget:

1. 0 to 16 sources;
2. 0 to 64 rows per source;
3. 0 to 256 total rows per generated merge case;
4. 0 to 128 cursor operations per generated case;
5. value bytes capped at 1024 by default;
6. user-key bytes capped at 256 by default.

Generated tests must terminate under the fixed budget and must not use random
wall-clock time.

## Operation Generator

Generate cursor scripts using these operations:

1. open unbounded cursor;
2. open exact-bound cursor;
3. open closed-range cursor;
4. open open-range cursor;
5. open physical-prefix cursor;
6. seek to first;
7. seek to a present key;
8. seek to a missing key before the first row;
9. seek to a missing key after the last row;
10. seek to a missing key in a gap;
11. advance once;
12. collect the rest;
13. re-seek after partial consumption;
14. re-seek after exhaustion;
15. drop and recreate cursor over the same source.

The model must compute expected output independently for each operation. Do not
reuse production cursor code in the model.

## Source Guards

The existing `table_runtime_source_guard` applies to production files under
`crates/storage-next/src/table/`. L5D must keep that guard passing and should
extend probes for cursor-specific risks.

Forbidden in production L5D code:

1. `crates/storage`;
2. `crate::key_encoding`;
3. old `Key`, `Namespace`, `TypeTag`, `Value`, `VersionedValue`, or
   `MemtableEntry`;
4. `Mvcc`;
5. `snapshot`;
6. `as_of`;
7. `fork`;
8. `inherit`;
9. `rewrite`;
10. `visible_at`;
11. `latest`;
12. `crate::branch`;
13. `crate::commit`;
14. `crate::lifecycle`;
15. `crate::backend`;
16. filesystem or path APIs;
17. environment-variable reads.

Tests may mention forbidden terms inside guard probes and historical comments.
Production code should not.

## Fuzz Plan

L5D should land the generated operation model first. If the cursor API is stable
enough after implementation, add:

```text
crates/storage-next/fuzz/fuzz_targets/table_runtime_cursor.rs
```

Fuzz target shape:

1. accept arbitrary operation bytes;
2. generate bounded in-memory table sources;
3. run mutable cursor, frozen cursor, and merge cursor operation scripts;
4. compare successful output to the sorted-vector model;
5. reject or cap oversized source and operation counts before allocation;
6. assert no panic;
7. assert all successful output remains sorted and within bounds.

If adding the fuzz target would pull focus away from the L5D API landing, defer
the target to L5J but keep the same model and operation generator in the
property harness.

## Sensitivity Probes

Before marking L5D complete, run temporary local mutations and confirm targeted
tests fail:

1. make `seek` choose the previous row instead of first greater-or-equal;
2. make `advance` skip two rows;
3. make exhausted `current()` panic;
4. reverse encoded-key ordering;
5. drop tombstones from cursor output;
6. drop expired-looking rows from cursor output;
7. collapse duplicate physical-key versions;
8. collapse duplicate exact keys across sources;
9. remove source-index tie break;
10. reverse source-index tie break;
11. force all merges through the linear path and disable heap-specific coverage;
12. force all merges through the heap path and disable linear-specific coverage;
13. make merge `advance` advance every child instead of only the selected child;
14. make merge `seek` re-seek only the previously selected child;
15. add a production `Mvcc` reference and verify the source guard fails;
16. add a production backend or filesystem reference and verify the source
    guard fails.

Record sensitivity probe results in the L5D closeout notes or M4-L5 closeout
notes.

## Verification

Minimum L5D closeout commands:

1. `cargo test -p strata-storage-next --locked table::tests::cursor`
2. `cargo test -p strata-storage-next --locked --test table_runtime_source_guard`
3. `cargo test -p strata-storage-next --features testkit --locked --test table_runtime_properties`
4. `cargo test -p strata-storage-next --no-default-features --features testkit --locked --test table_runtime_properties`
5. `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings`
6. `cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked`
7. `cargo fmt --package strata-storage-next --check`
8. `git diff --check`

Run the full storage-next all-features suite before closing the slice if L5D
changes shared `TableKeyBounds`, `TableRow`, testkit generation, or source
guards.

## Exit Gate

L5D test coverage is complete when:

1. every cursor state transition has direct unit coverage;
2. mutable and frozen cursors have parity coverage;
3. seek boundary behavior is compared against an independent model;
4. exact, range, and physical-prefix bounds are covered;
5. tombstones, expired rows, duplicate versions, and duplicate exact keys
   across sources are preserved by tests;
6. merge cursor tests cover zero, one, linear, threshold, heap, and many-source
   paths;
7. merge seek and re-seek behavior is covered after partial consumption and
   exhaustion;
8. generated tests compare cursor and merge output to independent models;
9. source guards reject upper-layer, product, old-storage, backend, and
   filesystem leakage;
10. sensitivity probes prove the suite catches the likely cursor and merge
    regressions.
