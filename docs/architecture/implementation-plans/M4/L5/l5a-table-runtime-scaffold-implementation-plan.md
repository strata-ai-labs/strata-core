# L5A Implementation Plan: Table Runtime Scaffold

Status: draft implementation plan

Parent plans:

1. `docs/architecture/implementation-plans/m4-m4t-implementation-plan.md`
2. `docs/architecture/implementation-plans/m4-l5-table-runtime-implementation-plan.md`
3. `docs/architecture/implementation-plans/m4-l5-table-runtime-test-plan.md`

## Goal

Create the storage-next L5 table-runtime module scaffold and guardrails before
porting table behavior.

L5A is intentionally small. It should establish names, module boundaries, error
vocabulary, table facts, config surfaces, source guards, and porting records so
later L5 slices can add behavior without rediscovering the layer boundary.

## Inputs

1. `docs/architecture/storage/l5-table-runtime.md`
2. `docs/architecture/storage/implementation-patterns.md`
3. `docs/architecture/storage/target-crate-shape-and-test-harness.md`
4. `docs/spec/strata-storage-format-v1.md`
5. `crates/storage-next/src/table/mod.rs`
6. `crates/storage-next/src/format/table/`
7. `crates/storage-next/src/service/table.rs`
8. `crates/storage/src/memtable.rs`
9. `crates/storage/src/segment_builder.rs`
10. `crates/storage/src/segment.rs`
11. `crates/storage/src/block_cache.rs`
12. `crates/storage/src/merge_iter.rs`
13. `crates/storage/src/seekable.rs`
14. `crates/storage/src/compaction.rs`

## Scope

L5A implements scaffolding only:

1. module tree;
2. table-local result and error types;
3. table config structs;
4. table identity and table fact structs;
5. table stats structs;
6. placeholder submodules for later L5 slices;
7. source/dependency guards;
8. compile-only smoke tests;
9. M4-L5 porting-log entry.

L5A does not implement:

1. mutable table behavior;
2. frozen table behavior;
3. cursors;
4. immutable table building;
5. immutable table reading;
6. block cache behavior;
7. compaction;
8. object-backed reading.

## Proposed Module Shape

Create or prepare this structure under `crates/storage-next/src/table/`:

```text
table/
  mod.rs
  error.rs
  config.rs
  facts.rs
  key.rs
  mutable.rs
  cursor.rs
  builder.rs
  reader.rs
  cache.rs
  compaction.rs
  tests/
    mod.rs
```

Initial module responsibilities:

| Module | L5A responsibility | Later slice |
|---|---|---|
| `mod.rs` | internal re-exports and module docs | all L5 slices |
| `error.rs` | `TableRuntimeError`, `TableRuntimeResult` | all L5 slices |
| `config.rs` | builder/cache/reader/compaction config shells | `L5E`, `L5G`, `L5H` |
| `facts.rs` | table identity, table facts, stats shells | `L5E`, `L5F`, `L5H` |
| `key.rs` | placeholder for row/key adapters | `L5B` |
| `mutable.rs` | placeholder for mutable/frozen table types | `L5C` |
| `cursor.rs` | placeholder for raw cursor contracts | `L5D` |
| `builder.rs` | placeholder for immutable builder | `L5E` |
| `reader.rs` | placeholder for immutable reader/source contracts | `L5F`, `L5I` |
| `cache.rs` | placeholder for database-owned block cache | `L5G` |
| `compaction.rs` | placeholder for policy-driven compaction | `L5H` |

Keep every new type `pub(crate)`. L9 owns any later public boundary.

## Type Shells

L5A should add small, behavior-light types only where they reduce churn in
later slices.

Recommended initial types:

1. `TableRuntimeResult<T> = Result<T, TableRuntimeError>`.
2. `TableRuntimeError` with variants for:
   - invalid configuration;
   - invalid row order;
   - duplicate internal key;
   - invalid range;
   - table build failure wrapping `FormatError`;
   - table decode failure wrapping `FormatError`;
   - table source read failure;
   - cache failure;
   - compaction policy failure.
3. `TableRuntimeConfig` for top-level L5 defaults.
4. `TableBuilderConfig` for target block size, rows-per-block, and compression.
5. `TableReaderConfig` for the reader validation policy.
6. `TableCacheConfig` for capacity and enablement.
7. `TableCompactionConfig` for target output size and split limits.
8. `TableIdentity` as an L5-stable identity shell that does not construct
   object names.
9. `TableRuntimeFacts` for row count, data-block count, key range, commit
   range, and byte count.
10. `TableRuntimeStats` for reads, bytes, cache hits/misses, and compaction
    counters.

The exact field set may stay minimal in L5A. Avoid adding fields that need
branch, commit, lifecycle, or product semantics to explain them.

## Source Guards

L5A must add `crates/storage-next/tests/table_runtime_source_guard.rs`.

The guard should fail if production files under `crates/storage-next/src/table/`
contain:

1. `crate::branch`;
2. `crate::commit`;
3. `crate::lifecycle`;
4. current or future upper runtime module imports;
5. engine crate imports;
6. product primitive imports or vocabulary such as `Value`, `EntityRef`, JSON,
   graph, vector, search, transaction, primitive payloads;
7. `std::fs`;
8. `std::path::Path`;
9. `std::fs::File` or `File`;
10. `std::os::unix::fs::FileExt`;
11. direct backend method calls;
12. `std::env` or environment variable reads;
13. process-global cache singletons.

The guard may allow test modules to mention forbidden words when the mention is
explicitly about rejecting leakage. Keep the allowlist narrow and documented.

## Implementation Steps

1. Expand `crates/storage-next/src/table/mod.rs` from placeholder docs into the
   module root.
2. Add empty/typed submodules listed in the proposed module shape.
3. Add `TableRuntimeError` and `TableRuntimeResult`.
4. Add config and facts shells with constructor validation where useful.
5. Add compile-only smoke tests for constructing default config/facts.
6. Add source guard integration test.
7. Add a porting-log section `M4-L5A: Table Runtime Scaffold`.
8. Update the M4-L5 implementation plan if the final module names differ from
   this plan.

## Required Tests

1. `table` module compiles under default features.
2. `table` module compiles under no-default features.
3. `table` module compiles under all features.
4. Default table config construction succeeds.
5. Invalid config values fail with typed errors.
6. Table facts construction rejects impossible counts where fields are present.
7. Error display and source chains compile and route wrapped `FormatError`.
8. Source guard rejects upper-layer imports.
9. Source guard rejects product primitive vocabulary.
10. Source guard rejects filesystem/path APIs.
11. Source guard rejects direct backend calls.
12. Source guard rejects process-global cache singleton vocabulary.
13. No public crate API is added.

## Verification

Minimum L5A closeout commands:

1. `cargo test -p strata-storage-next --locked table`
2. `cargo test -p strata-storage-next --no-default-features --locked table`
3. `cargo test -p strata-storage-next --all-features --locked table`
4. `cargo test -p strata-storage-next --locked --test table_runtime_source_guard`
5. `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings`
6. `cargo check -p strata-storage-next --no-default-features --target wasm32-unknown-unknown --all-targets --locked`
7. `cargo doc -p strata-storage-next --no-deps --locked`
8. `cargo fmt --package strata-storage-next --check`
9. `git diff --check`

## Sensitivity Probes

Before marking L5A complete, temporarily introduce each local mutation and
confirm the targeted guard fails:

1. add `use crate::branch;` to a production table module;
2. add product `Value` vocabulary to a production table module;
3. add `std::fs` to a production table module;
4. add `Path` to a production table module;
5. add a direct backend call marker to a production table module;
6. expose a table type as `pub` instead of `pub(crate)` if an API guard exists.

Record the probes in the porting log.

## Exit Gate

L5A is complete when:

1. the table module scaffold exists;
2. config, error, facts, and stats shells compile;
3. no behavior beyond scaffold/type validation has been added;
4. source guards enforce the L5 boundary;
5. no public API has leaked;
6. verification commands pass;
7. the porting log records the old-code source map and the slices that will port
   each behavior family.
