# L9A Implementation Plan: API Vocabulary And Visibility Boundary

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l9-storage-api-boundary-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L9/l9a-api-vocabulary-visibility-boundary-test-plan.md`

## Objective

Create the storage-next API boundary scaffold.

L9A establishes the public storage vocabulary, export policy, error shape, and
source guards that make L9 the only engine-facing storage surface. It should not
implement open, reads, commits, branch operations, maintenance, diagnostics, or
testkit behavior beyond shells and validation helpers.

The slice must make later L9 work easier while keeping all lower storage modules
private and product-neutral.

## Inputs

1. `docs/architecture/implementation-plans/m4-l9-storage-api-boundary-implementation-plan.md`
2. `docs/architecture/implementation-plans/m4-l9-storage-api-boundary-test-plan.md`
3. `docs/architecture/storage/target-crate-shape-and-test-harness.md`
4. `docs/architecture/strata-v1-architecture.md`
5. `crates/storage-next/src/api/mod.rs`
6. `crates/storage-next/src/lib.rs`
7. `crates/storage-next/src/lifecycle/error.rs`
8. `crates/storage-next/src/lifecycle/outcome.rs`
9. `crates/storage-next/src/commit/outcome.rs`
10. `crates/storage-next/src/branch/error.rs`
11. `crates/storage/src/traits.rs`

## Existing-Code Source Map

| Current file | L9A evidence | L9A action |
|---|---|---|
| `crates/storage-next/src/api/mod.rs` | One-line placeholder for engine-facing boundary. | Expand into module skeleton and crate-public re-export owner. |
| `crates/storage-next/src/lib.rs` | Lower modules are private today. | Keep lower modules private; expose only `api` intentionally. |
| `crates/storage/src/traits.rs` | Old synchronous storage trait. | Preserve synchronous shape but avoid old product value types. |
| `crates/storage-next/src/lifecycle/error.rs` | Stable code/source-chain pattern. | Mirror structured code/source-chain behavior at the API boundary. |
| `docs/architecture/storage/target-crate-shape-and-test-harness.md` | Public API must be sync; `api` owns engine-facing DTOs. | Encode source guards and public surface rules. |

## Scope

L9A implements scaffolding only:

1. `crates/storage-next/src/api/` module split;
2. public storage result and error shells;
3. storage atom wrappers or re-exports approved for the boundary;
4. open/read/commit/branch/maintenance/diagnostic request and outcome shells;
5. `#[non_exhaustive]` public enums where future growth is expected;
6. error `code()` accessor with class/area/detail format;
7. source-chain wrapper fields without leaking lower-layer concrete types;
8. `lib.rs` public export policy;
9. source guard tests for public visibility and forbidden vocabulary;
10. initial `docs/architecture/implementation-plans/M4/L9/m4-l9-porting-log.md`.

L9A does not implement:

1. opening storage;
2. committing rows;
3. reading branch state;
4. resolving timelines;
5. branch lifecycle behavior;
6. maintenance task execution;
7. diagnostics collection;
8. fake or faulting persistence;
9. engine-next integration.

## Target Module Layout

```text
crates/storage-next/src/api/
  mod.rs
  error.rs
  result.rs
  atoms.rs
  options.rs
  outcome.rs
  read.rs
  commit.rs
  branch.rs
  maintenance.rs
  diagnostics.rs
```

The exact split can evolve, but `mod.rs` should stay a small export and
documentation hub.

## Boundary Vocabulary

Define storage-shaped types for:

1. storage mode;
2. durability policy;
3. open disposition;
4. branch selector and branch generation;
5. storage-space selector;
6. key selector;
7. value bytes;
8. read bound;
9. scan bound and limit;
10. commit mutation;
11. commit outcome summary;
12. recovery health summary;
13. maintenance request and outcome summary;
14. storage diagnostics summary.

Do not define product concepts such as primitive type, object class, property,
event, graph edge, vector embedding, model, prompt, conversation, workspace,
project, user, or StrataHub account.

## Error Model

`StorageApiError` should be the public boundary error. It should carry:

1. stable `code()`;
2. coarse class;
3. storage area;
4. structured storage fields;
5. optional source-chain summary.

Initial variants:

1. invalid argument;
2. unsupported capability;
3. invalid lifecycle state;
4. lower layer failure;
5. conflict;
6. retained history unavailable;
7. branch not found;
8. branch already exists;
9. branch generation mismatch;
10. durable uncertainty;
11. recovery degraded;
12. maintenance rejected.

L9A can construct these variants without wiring lower-layer behavior.

## Export Policy

`crates/storage-next/src/lib.rs` should expose the L9 API intentionally. Lower
modules should remain private production modules. If testkit needs public
helpers, they stay behind `cfg(test)` or `feature = "testkit"` and are not the
production boundary.

## Source Guard Policy

Add `crates/storage-next/tests/api_source_guard.rs` or extend the existing
source guard suite so it fails if:

1. lower modules are publicly exported by accident;
2. `src/api/**` imports engine/intelligence/inference/executor/CLI/SDK crates;
3. `src/api/**` contains async runtime types;
4. `src/api/**` contains product vocabulary;
5. lower modules import `crate::api`;
6. L9 public signatures mention lower-layer concrete types.

## Porting Log

Create `docs/architecture/implementation-plans/M4/L9/m4-l9-porting-log.md`
with an L9A section recording:

1. source evidence read;
2. shipped files;
3. boundary decisions;
4. tests added;
5. deferred behavior;
6. sensitivity probes;
7. verification commands.

## Verification

Expected slice commands:

```bash
cargo fmt --package strata-storage-next --check
cargo test -p strata-storage-next --locked --lib api
cargo test -p strata-storage-next --locked --test api_source_guard
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
```
