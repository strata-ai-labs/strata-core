# V1 Boundary Baseline

Status: M0 boundary baseline

## Purpose

This document records the current crate graph and boundary debt before V1
implementation starts. It is evidence, not target architecture.

The target architecture remains defined by:

1. `docs/architecture/strata-v1-architecture.md`
2. `docs/architecture/core-next-architecture.md`
3. `docs/architecture/storage-next-architecture.md`
4. `docs/architecture/engine-next-architecture.md`
5. `docs/architecture/inference-next-architecture.md`
6. `docs/architecture/intelligence-next-architecture.md`

## Baseline Provenance

This baseline was captured from:

| Field | Value |
|---|---|
| Captured at | 2026-05-11 21:06:25 CDT |
| Git branch | `v1` |
| Git HEAD | `35650d9b` |
| Worktree state | Dirty M0 documentation workspace |

The dirty worktree is intentional: this baseline was captured during M0 after
M0TA and M0TB document cleanup had already landed in the working tree. At
capture time, `git status --short -M --untracked-files=all` included M0
documentation edits, archived core cleanup documents, this new baseline file,
and a pre-existing `.gitignore` modification.

The provenance was gathered with:

```sh
date '+%Y-%m-%d %H:%M:%S %Z'
git branch --show-current
git rev-parse --short HEAD
git status --short -M --untracked-files=all
```

## Baseline Commands

The crate graph was gathered from both `cargo tree` and `cargo metadata`.
`cargo tree --edges normal` shows enabled normal dependencies; `cargo metadata`
is needed to capture optional normal dependency declarations that are not active
under default features.

```sh
cargo tree -p <package> --edges normal --depth 1

cargo metadata --format-version 1 --no-deps |
  jq -r '
    .packages[]
    | select(.name|test("^strata|^stratadb"))
    | .name as $p
    | "\n"+$p,
      (.dependencies[]?
       | select(.source == null)
       | "  \(.kind // "normal") \(.name) optional=\(.optional) features=\(.features|join(","))")
  '
```

The source-size table was gathered with:

```sh
for d in crates/core crates/storage crates/engine crates/inference \
         crates/intelligence crates/executor crates/cli .
do
  printf '%s ' "$d"
  find "$d/src" -name '*.rs' -type f | wc -l | tr -d ' '
  printf ' '
  find "$d/src" -name '*.rs' -type f -print0 |
    xargs -0 wc -l |
    tail -n 1 |
    awk '{print $1}'
done
```

The import-boundary facts were gathered with:

```sh
rg -l '^use strata_storage|strata_storage::|^pub use strata_storage' crates src tests -g '*.rs'
rg -l '^use strata_engine|strata_engine::|^pub use strata_engine' crates/storage/src crates/core/src crates/inference/src -g '*.rs'
rg -l '^use strata_inference|strata_inference::|^pub use strata_inference' crates/engine/src crates/executor/src crates/cli/src src tests -g '*.rs'
```

These commands are not the final V1 guard suite. They are the M0 factual
baseline that later milestone guards should tighten.

## Workspace Members

Current workspace packages:

| Package | Path | Current role |
|---|---|---|
| `strata-core` | `crates/core` | Shared foundational and product-shaped vocabulary. |
| `strata-storage` | `crates/storage` | Current storage, transaction, segmented LSM, WAL, manifest, snapshot, recovery, and maintenance runtime. |
| `strata-engine` | `crates/engine` | Current database product kernel plus primitives, graph, vector, search, branch operations, product open, recovery orchestration, and configuration. |
| `strata-inference` | `crates/inference` | Model provider and local inference runtime. |
| `strata-intelligence` | `crates/intelligence` | Embedding, expansion, reranking, RAG, and intelligence orchestration over engine plus inference. |
| `strata-executor` | `crates/executor` | Public command boundary, IPC, compatibility handle, import/export helpers, and typed command outputs. |
| `strata-cli` | `crates/cli` | CLI shell over executor, plus current local open/init/up UX. |
| `stratadb` | repository root | Facade package over executor plus root integration tests and benchmark feature plumbing. |

Approximate current source size:

| Crate | Rust source files under `src` | Lines under `src` |
|---|---:|---:|
| `strata-core` | 13 | 4,634 |
| `strata-storage` | 82 | 66,799 |
| `strata-engine` | 149 | 138,783 |
| `strata-inference` | 17 | 10,177 |
| `strata-intelligence` | 13 | 4,684 |
| `strata-executor` | 44 | 21,891 |
| `strata-cli` | 10 | 5,068 |
| `stratadb` | 1 | 67 |

## Normal Dependency Graph

Current normal Strata-package edges:

```text
strata-core

strata-storage
  -> strata-core

strata-engine
  -> strata-core
  -> strata-storage

strata-inference

strata-intelligence
  -> strata-core
  -> strata-engine
  -> strata-inference (optional, via embed/provider features)

strata-executor
  -> strata-core
  -> strata-engine
  -> strata-intelligence

strata-cli
  -> strata-executor
  -> strata-intelligence (optional, via embed feature)

stratadb
  -> strata-executor
```

Current dev/test Strata-package edges:

```text
strata-engine dev
  -> strata-storage with engine-internal, fault-injection, test-utils

strata-executor dev
  -> strata-engine with test-support

stratadb dev
  -> strata-core
  -> strata-storage
  -> strata-engine with test-support
  -> strata-intelligence
```

## Healthy Current Boundaries

These facts are useful because they should not regress while the V1 rewrite is
in progress:

1. `strata-core` has no normal dependency on another Strata crate.
2. `strata-storage` has no dependency on `strata-engine`.
3. `strata-inference` has no dependency on Strata database crates.
4. `strata-engine` has no dependency on `strata-intelligence` or
   `strata-inference`.
5. `strata-executor` has no normal dependency on `strata-storage`.
6. `strata-cli` has no normal direct dependency on `strata-engine` or
   `strata-storage`.
7. The root `stratadb` package has only `strata-executor` as a normal Strata
   dependency.
8. Source scan found no production `strata_engine` imports under
   `crates/storage/src`, `crates/core/src`, or `crates/inference/src`.
9. Source scan found no direct upper-layer `strata_inference` imports outside
   `crates/intelligence`; self-references inside `crates/inference` docs and
   examples are expected.

## Boundary Debt

### Core Owns Too Much Product Vocabulary

Current `strata-core` still exports product-shaped types:

1. `Value`
2. `EntityRef`
3. `PrimitiveType`
4. `Version`
5. `Versioned<T>`
6. `VersionedValue`
7. `VersionedHistory<T>`
8. `BranchName`
9. `TxnId`

Target ownership is defined by `docs/architecture/core-next-architecture.md`:

- core keeps only true shared atoms such as `BranchId`, `CommitVersion`, and the
  timestamp representation;
- engine owns product values, entity references, primitive taxonomy, product
  version DTOs, branch naming policy, and public read-result shapes;
- storage owns transaction/runtime identifiers.

### Storage Exposes Product-Shaped Rows

Current storage still imports and exposes product-shaped core types in
production code. Representative files:

1. `crates/storage/src/traits.rs`
2. `crates/storage/src/stored_value.rs`
3. `crates/storage/src/memtable.rs`
4. `crates/storage/src/segment.rs`
5. `crates/storage/src/segmented/mod.rs`
6. `crates/storage/src/txn/context.rs`
7. `crates/storage/src/txn/manager.rs`
8. `crates/storage/src/durability/payload.rs`
9. `crates/storage/src/durability/format/writeset.rs`
10. `crates/storage/src/durability/decoded_snapshot_install.rs`

Examples of current leakage:

- `Storage` returns `VersionedValue`.
- storage rows carry `Value`, `Version`, and `Timestamp` as product-shaped DTOs.
- WAL/write-set encoding still names `EntityRef`.
- `TxnId` is still imported from core.

V1 storage-next must replace this with row-native storage DTOs and opaque row
bytes at the storage boundary.

### Engine Is The Current Consolidation Hub

The current engine is structurally the right dependency hub, but internally it
is still too broad. It contains:

1. database open, lifecycle, recovery, config, product-open, and IPC-adjacent
   policy;
2. storage orchestration and direct storage imports across many modules;
3. KV, JSON, event, vector, graph, branch, and search capability
   implementation;
4. branch merge/diff/cherry-pick/revert machinery;
5. bundle import/export;
6. public transaction handles;
7. manual maintenance commands such as flush and compact.

Representative current public surfaces that V1 must retire or redesign:

1. `crates/engine/src/lib.rs` re-exports `Transaction`, `ScopedTransaction`,
   `TransactionPool`, `TransactionOps`, `StorageIterator`, and bundle types.
2. `crates/engine/src/database/transaction.rs` exposes
   `Database::begin_transaction` and `Database::commit_transaction`.
3. `crates/engine/src/database/compaction.rs` exposes `flush` and `compact`.
4. `crates/engine/src/database/spec.rs` still defines follower mode.
5. `crates/engine/src/bundle/` still implements branch bundles.

V1 engine-next should keep engine as the product semantic owner, but reorganize
it around persistence, data capabilities, branch/time, control plane,
retrieval, command boundary, clone artifacts, and diagnostics.

### Removed Product Surfaces Still Exist In Current Code

The current code still contains several surfaces already removed or redesigned
by the V1 product/architecture documents:

| Surface | Current evidence | V1 disposition |
|---|---|---|
| Follower mode | `OpenOptions::follower`, `DatabaseMode::Follower`, CLI `--follower`, `Database::refresh`, follower tests and recovery paths | Remove before or during engine-next cutover. |
| Public transaction sessions | `Database::begin_transaction`, engine `Transaction`, executor transaction outputs/errors | Replace with per-command commits and capability-local batch APIs. |
| Manual maintenance workflow | executor/compat `flush` and `compact`, engine `Database::flush` and `Database::compact` | Keep maintenance internal/diagnostic, not a default product workflow. |
| Branch bundles | `crates/engine/src/bundle`, executor branch bundle handlers | Remove in favor of Arrow support and clone artifacts. |
| Product-shaped storage APIs | storage `Storage`, `VersionedValue`, `TransactionContext` exposed upward | Replace with storage-next L9 boundary and engine persistence adapter. |

### Executor And CLI Still Mirror Old Surfaces

Executor currently has no direct storage dependency, which is good. It still
contains old product command shapes that later milestones must remove:

1. transaction output/error vocabulary in `crates/executor/src/output.rs` and
   `crates/executor/src/error.rs`;
2. transaction-backed handlers such as `crates/executor/src/handlers/kv.rs`;
3. branch bundle import/export/validate handlers;
4. compatibility `flush` and `compact` methods;
5. direct use of core-owned `Value`, `EntityRef`, `PrimitiveType`, `Version`,
   and `Versioned` product DTOs.

CLI currently depends normally on executor and optionally on intelligence for
embed features. It still contains follower plumbing in `crates/cli/src/open.rs`
and `crates/cli/src/app.rs`.

### Intelligence And Inference Are Lower Risk

Inference has a small normal dependency graph and no Strata database
dependencies. Intelligence currently depends on core and engine, with optional
inference via embed/provider features.

This aligns with the V1 direction closely enough that M7/M8 should be
harden-and-contract work rather than an invasive rewrite.

Known intelligence boundary debt:

1. Intelligence still consumes core-owned product DTOs directly because those
   DTOs have not moved to engine-next.
2. Intelligence-next must consume inference through task contracts and engine
   through named engine surfaces, not through provider internals or storage.

### Root Package Carries Test And Benchmark Edges

The root `stratadb` package depends normally on `strata-executor`. It also has
dev-dependencies on core, storage, engine, and intelligence for current root
integration tests and guard tests.

This is acceptable as baseline evidence, but M9/M10 should decide which root
tests remain product-path tests and which lower-layer tests move into their
own crates or testkit harnesses.

## M0TC Closure

`M0TC` is closed when this document remains true enough for M0 decision
closure:

1. current workspace packages are listed;
2. normal and dev Strata dependency edges are captured;
3. healthy current boundaries are identified;
4. known boundary debt is assigned to later architecture work;
5. the next M0 step can use this baseline to close or assign open decisions.

After M0A and M0B, the next M0 closure target is `M0C` standards alignment.
