# L5J Implementation Plan: L5 Conformance Closeout

Status: draft implementation plan

Parent plans:

1. `docs/architecture/implementation-plans/m4-m4t-implementation-plan.md`
2. `docs/architecture/implementation-plans/m4-l5-table-runtime-implementation-plan.md`
3. `docs/architecture/implementation-plans/m4-l5-table-runtime-test-plan.md`
4. `docs/architecture/implementation-plans/M4/l5a-table-runtime-scaffold-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/l5b-row-key-adapters-implementation-plan.md`
6. `docs/architecture/implementation-plans/M4/l5c-mutable-frozen-tables-implementation-plan.md`
7. `docs/architecture/implementation-plans/M4/l5d-raw-cursors-merge-cursor-implementation-plan.md`
8. `docs/architecture/implementation-plans/M4/l5e-immutable-table-builder-implementation-plan.md`
9. `docs/architecture/implementation-plans/M4/l5f-immutable-table-reader-implementation-plan.md`
10. `docs/architecture/implementation-plans/M4/l5g-block-cache-accelerators-implementation-plan.md`
11. `docs/architecture/implementation-plans/M4/l5h-generic-compaction-implementation-plan.md`
12. `docs/architecture/implementation-plans/M4/l5i-object-backed-table-access-implementation-plan.md`
13. `docs/architecture/implementation-plans/M4/l5j-l5-conformance-closeout-test-plan.md`

## Goal

Close M4-L5 as a coherent, policy-free table runtime layer.

L5J is not a new table feature slice. It is the conformance, audit,
documentation, and hardening pass that proves the pieces from L5A through L5I
work together and are ready for L6 branch table state.

L5J must answer these questions with code, tests, or explicit deferrals:

1. Does `crates/storage-next/src/table/` stay pure L5?
2. Do all L5 mechanics use storage-next rows and M3G table bytes only?
3. Are mutable, frozen, cursor, merge, builder, reader, cache, compaction, and
   object-backed access covered by direct examples and generated models?
4. Do cache and accelerator paths remain non-authoritative?
5. Does compaction drop rows only through caller-supplied policy decisions?
6. Are old `crates/storage` behaviors either ported, retired, or deferred to a
   named future layer?
7. Can a lower layer or L6 consumer run a small, stable conformance command set
   and trust the result?

## Inputs

1. `docs/architecture/storage/l5-table-runtime.md`
2. `docs/architecture/storage/implementation-patterns.md`
3. `docs/architecture/storage/target-crate-shape-and-test-harness.md`
4. `docs/spec/strata-storage-format-v1.md`
5. `docs/architecture/implementation-plans/m4-l5-table-runtime-implementation-plan.md`
6. `docs/architecture/implementation-plans/m4-l5-table-runtime-test-plan.md`
7. `docs/architecture/implementation-plans/M4/m4-l5-porting-log.md`
8. all L5A through L5I implementation and test plans
9. `crates/storage-next/src/table/`
10. `crates/storage-next/src/service/table.rs`
11. `crates/storage-next/src/testkit/table_runtime.rs`
12. `crates/storage-next/tests/table_runtime_properties.rs`
13. `crates/storage-next/tests/table_runtime_source_guard.rs`
14. `crates/storage-next/fuzz/`
15. relevant old-code evidence under `crates/storage/src/`

## Scope

L5J implements closeout work only:

1. conformance inventory for every L5 module and every L5A-L5I exit gate;
2. source-boundary guard consolidation for L5 purity;
3. generated table-runtime harness completeness checks;
4. fuzz-target inventory and smoke coverage for table runtime paths;
5. command-level conformance scripts or documentation for memory, no-default,
   wasm, localfs-enabled, and all-features modes;
6. porting-log closeout entries for behavior preserved, intentionally changed,
   retired, or deferred;
7. test naming cleanup where milestone labels leaked into test names;
8. small test holes or assertion gaps found during the closeout audit;
9. explicit deferred-work ledger for L6/L8/post-V1 behavior;
10. final M4-L5 exit-gate checklist.

L5J may add small helper functions, source guards, testkit counters, or tests.
It should not add a new production table subsystem unless the audit proves a
previous L5 slice left an incomplete contract.

## Non-Goals

L5J must not implement:

1. branch table manifests;
2. level ownership;
3. table installation or manifest replacement;
4. inherited table lookup;
5. MVCC latest-row selection;
6. commit allocation;
7. WAL coordination;
8. checkpoint scheduling;
9. retention, garbage collection, or quarantine policy;
10. lazy block reads after whole-object validation;
11. durable object-store fences;
12. public API stabilization.

If a gap belongs to one of these areas, L5J documents it as a named L6, L8, or
post-V1 deferral and adds a guard only when the current L5 boundary can be
regressed accidentally.

## Current Surface To Close

The closeout covers these storage-next surfaces:

| Surface | Files | L5J question |
|---|---|---|
| module scaffold | `src/table/mod.rs`, `config.rs`, `error.rs`, `facts.rs`, `stats.rs` | Are crate-private exports, configs, facts, stats, and errors complete enough for L6 without public leakage? |
| row/key adapters | `src/table/key.rs` | Are ordering, bounds, physical-prefix, and row-size rules directly tested and product-neutral? |
| mutable/frozen tables | `src/table/mutable.rs` | Are mutation, duplicate rejection, freeze, facts, memory accounting, and cursors model-checked? |
| raw cursors and merge | `src/table/cursor.rs` | Are cursor state transitions, seek semantics, bounds, linear merge, heap merge, and tie behavior model-checked? |
| immutable builder | `src/table/builder.rs` | Does L5 produce only valid M3G bytes from sorted unique rows and reject invalid inputs? |
| immutable reader | `src/table/reader.rs` | Do byte-backed and source-backed readers validate M3G bytes before trusting facts? |
| cache/accelerators | `src/table/cache.rs` | Are cache identity, eviction, stats, bloom no-false-negative behavior, and non-authoritative semantics covered? |
| compaction | `src/table/compaction.rs` | Are row drops fully policy-provided, output splitting bounded, and artifacts valid M3G tables? |
| object-backed access | `src/service/table.rs` | Does L4/L5 handoff read known objects without moving object names or backend calls into L5? |
| generated harness | `src/testkit/table_runtime.rs` | Does one bounded generated route exercise every L5 category? |
| external guards | `tests/table_runtime_*.rs` | Do source scans and property harnesses fail when L5 regresses? |

## Closeout Rules

1. Prefer strengthening tests over changing production code.
2. Keep new production changes mechanical and local.
3. Do not move L4 service adapters into `src/table/`.
4. Do not introduce new durable table bytes or alter M3G.
5. Do not add product semantics, visibility semantics, branch semantics, or
   retention rules.
6. Do not add public exports outside the existing hidden testkit surface.
7. Do not rely on old `crates/storage` tests as proof.
8. Every newly found gap is either fixed in L5J or recorded with an owner layer.

## Implementation Steps

### L5J-A: Inventory Existing Coverage

Build a coverage matrix from the current code and tests.

Rows:

1. L5A scaffold;
2. L5B row/key adapters;
3. L5C mutable/frozen;
4. L5D raw cursors and merge cursor;
5. L5E immutable builder;
6. L5F immutable reader;
7. L5G cache and accelerators;
8. L5H generic compaction;
9. L5I object-backed access.

Columns:

1. direct unit tests;
2. generated/property route;
3. source guard;
4. cross-feature/no-default coverage;
5. wasm check coverage;
6. fuzz or fuzz-adjacent coverage;
7. old-code behavior mapped;
8. known deferrals.

Output:

1. an L5J section in `m4-l5-porting-log.md`;
2. a list of missing tests or docs to close in later L5J steps.

### L5J-B: Strengthen Source Guards

Review `tests/table_runtime_source_guard.rs` and add probes only where gaps
remain.

Guard categories:

1. upper-layer imports;
2. engine/product vocabulary;
3. filesystem/backend/service usage inside production `src/table/`;
4. object layout literals inside production `src/table/`;
5. old table format vocabulary;
6. old path-hash/process-global cache identity;
7. compaction retention-policy vocabulary;
8. bare public surface leaks;
9. testkit leakage into production table code.

The guard should distinguish production L5 code from service boundary adapters
and tests. L4/L5 object-backed adapters may mention backend/object vocabulary;
pure L5 may not.

### L5J-C: Consolidate Generated Harness Counters

Audit `TableRuntimeScaffoldOutcome`.

Every L5 category must have:

1. a counter;
2. a nonzero assertion in `tests/table_runtime_properties.rs`;
3. a direct unit test assertion in `src/testkit/table_runtime.rs`;
4. at least one generated check that can fail independently.

Expected categories:

1. valid configs;
2. invalid configs;
3. valid facts;
4. invalid facts;
5. row/key adapters;
6. invalid row/key sequences;
7. key bounds;
8. size accounting;
9. mutable/frozen tables;
10. raw cursors;
11. immutable builder artifacts;
12. immutable table readers;
13. object-backed table readers;
14. table block cache;
15. bloom/filter accelerators;
16. generic table compaction;
17. error source chains;
18. stats.

### L5J-D: Add Missing Direct Tests

From L5J-A, add small direct tests only for material coverage gaps.

Candidate checks:

1. error display/source-chain assertions for each L5 error family;
2. source guard regression probes for newly added forbidden terms;
3. table-runtime property harness self-test counters;
4. no-default object-backed generated route if it is missing;
5. cache disabled/cold/warm/polluted neutrality if not already explicit;
6. compaction artifact reader validation if not already explicit;
7. cursor-state invariants not covered by generated scripts.

Do not add broad rewrites or duplicate existing tests.

### L5J-E: Fuzz Inventory And Smoke Targets

Inventory the current fuzz targets under `crates/storage-next/fuzz/`.

Confirm coverage for:

1. M3G table artifact bytes;
2. table block bytes;
3. table runtime reader byte input;
4. cursor movement over generated valid table sources;
5. compaction generated models.

The delivered L5 runtime fuzz targets are:

1. `table_runtime_reader`, which calls
   `check_table_runtime_reader_contract`;
2. `table_runtime_cursor`, which calls
   `check_table_runtime_cursor_contract`;
3. `table_runtime_compaction`, which calls
   `check_table_runtime_compaction_contract`.

`tests/table_runtime_closeout.rs` must enforce the fuzz inventory and the
structural rule that each runtime target calls its dedicated contract instead
of only calling the shared scaffold route.

### L5J-F: Cross-Feature Conformance Commands

Record and verify a stable command set:

1. focused L5 unit tests;
2. generated table runtime properties with `testkit`;
3. generated table runtime properties with `--no-default-features`;
4. source guards;
5. wasm/no-default check;
6. clippy all-targets/all-features;
7. full `strata-storage-next` package tests.

If any command is too slow for default CI, split it into default closeout,
nightly, or manual stress categories and document the difference.

### L5J-G: Porting Log Closeout

Append an M4-L5J section to `m4-l5-porting-log.md`.

Record:

1. files read during closeout;
2. L5 behavior preserved from old storage;
3. intentional V1 changes;
4. behavior retired;
5. behavior deferred to L6;
6. behavior deferred to L8;
7. behavior deferred to post-V1;
8. tests added or strengthened;
9. commands run.

This section should make it possible to start L6 without re-auditing the old
storage table code from scratch.

### L5J-H: Documentation Consistency Pass

Review and update:

1. `m4-l5-table-runtime-implementation-plan.md`;
2. `m4-l5-table-runtime-test-plan.md`;
3. all L5A-L5I detailed plans if they contain stale follow-up statements;
4. `m4-l5-porting-log.md`.

Only make factual consistency edits. Do not rewrite completed slice plans into
after-the-fact implementation reports.

### L5J-I: Final Gap Ledger

Create a short final ledger in the porting log.

Each remaining gap must have:

1. owner layer;
2. reason it is not L5;
3. current guard or test that prevents accidental L5 regression;
4. first expected consumer.

Expected entries include:

1. L6 branch table manifests and reachable table selection;
2. L6 MVCC/latest-visible selection;
3. L6 inherited table lookup and fork gates;
4. L8 flush scheduling and manifest installation;
5. L8 table retention and garbage collection;
6. L8 checkpoint/table/WAL coordination;
7. post-V1 lazy block reads after whole-object validation;
8. post-V1 caller-provided compaction split boundaries;
9. post-V1 durable object-store fences.

### L5J-J: Final Exit Gate

M4-L5 closes when:

1. all L5J test-plan required commands pass;
2. all L5A-L5I exit gates are either satisfied or have explicit owner-layer
   deferrals;
3. generated harness counters cover every L5 category;
4. source guards prove L5 purity;
5. no new public API leaked;
6. no old `STRAKV`, `KVSegment`, path, `EntityRef`, MessagePack, or product DTO
   behavior entered storage-next L5;
7. the porting log has a final L5J section;
8. `git diff --check` and formatting pass.

## Expected File Changes

Likely touched files:

1. `crates/storage-next/src/testkit/table_runtime.rs`
2. `crates/storage-next/tests/table_runtime_properties.rs`
3. `crates/storage-next/tests/table_runtime_source_guard.rs`
4. `crates/storage-next/tests/table_runtime_closeout.rs`
5. `docs/architecture/implementation-plans/M4/m4-l5-porting-log.md`
6. `docs/architecture/implementation-plans/m4-l5-table-runtime-test-plan.md`

Possible files if gaps are found:

1. `crates/storage-next/src/table/tests/*.rs`
2. `crates/storage-next/src/table/*.rs`
3. `crates/storage-next/fuzz/fuzz_targets/table_runtime_reader.rs`
4. `crates/storage-next/fuzz/fuzz_targets/table_runtime_cursor.rs`
5. `crates/storage-next/fuzz/fuzz_targets/table_runtime_compaction.rs`
6. `crates/storage-next/fuzz/corpus/table_runtime_*/`

Do not expect new production modules unless the audit finds a real missing L5
mechanic.

## Verification Commands

Minimum closeout command set:

```sh
cargo test -p strata-storage-next --locked --lib table::tests
cargo test -p strata-storage-next --locked --lib testkit::table_runtime
cargo test -p strata-storage-next --features testkit --locked --test table_runtime_properties
cargo test -p strata-storage-next --no-default-features --features testkit --locked --test table_runtime_properties
cargo test -p strata-storage-next --locked --test table_runtime_source_guard
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo test -p strata-storage-next --locked
cargo fmt --package strata-storage-next --check
git diff --check
```

Optional closeout commands:

```sh
cargo test -p strata-storage-next --features testkit,localfs --locked
cd crates/storage-next/fuzz && cargo +nightly fuzz run format_table_artifact -- -max_total_time=60
cd crates/storage-next/fuzz && cargo +nightly fuzz run format_table_block -- -max_total_time=60
cd crates/storage-next/fuzz && cargo +nightly fuzz run table_runtime_reader -- -max_total_time=60
cd crates/storage-next/fuzz && cargo +nightly fuzz run table_runtime_cursor -- -max_total_time=60
cd crates/storage-next/fuzz && cargo +nightly fuzz run table_runtime_compaction -- -max_total_time=60
```

Only include optional fuzz commands in the final done criteria if cargo-fuzz
and the nightly toolchain are available. Runtime fuzz targets must remain
documented in `crates/storage-next/fuzz/README.md` and covered by
`tests/table_runtime_closeout.rs`.

## Exit Criteria

L5J is complete when:

1. the coverage inventory shows every L5A-L5I slice represented by direct tests,
   generated tests, source guards, or explicit deferrals;
2. all mandatory verification commands pass;
3. the final porting log identifies old behavior as ported, retired, changed,
   or deferred;
4. L5 production code remains free of upper-layer imports, backend calls,
   object layout strings, filesystem paths, product vocabulary, and public API
   leaks;
5. L6 can depend on L5 table mechanics without first adding new L5 conformance
   work.
