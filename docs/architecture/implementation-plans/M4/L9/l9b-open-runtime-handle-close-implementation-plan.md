# L9B Implementation Plan: Open, Runtime Handle, And Close

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l9-storage-api-boundary-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L9/l9b-open-runtime-handle-close-test-plan.md`

## Objective

Implement the L9 runtime handle and open/close boundary.

L9B wraps L8 cache and durable local lifecycle runtimes behind a single
storage-facing handle. It maps L9 open options into L8 open plans, returns
storage-shaped open outcomes, rejects unsupported V1 modes, and exposes close
without leaking lifecycle shell internals.

## Inputs

1. L9A API vocabulary.
2. `crates/storage-next/src/lifecycle/cache.rs`
3. `crates/storage-next/src/lifecycle/durable/`
4. `crates/storage-next/src/lifecycle/config.rs`
5. `crates/storage-next/src/lifecycle/outcome.rs`
6. `crates/storage-next/src/lifecycle/error.rs`
7. `crates/storage-next/src/config/mode.rs`
8. `docs/architecture/storage/target-crate-shape-and-test-harness.md`
9. `crates/engine/src/database/open.rs`
10. `crates/engine/src/database/lifecycle.rs`

## Scope

L9B implements:

1. `StorageOpenOptions`;
2. cache open/create;
3. durable local standard open/create;
4. durable local always open/create;
5. unsupported object-durable/distributed mode rejection;
6. `StorageRuntime` handle owning one lower runtime variant;
7. `StorageOpenOutcome` mapping from L8;
8. `StorageCloseOptions`;
9. `StorageCloseOutcome` mapping from L8;
10. idempotent close after closed;
11. post-close operation rejection helper.

L9B does not implement:

1. reads;
2. commits;
3. branch operations;
4. maintenance operations;
5. diagnostics beyond open/close facts;
6. background checkpoint policy beyond whatever L8 performs internally.

## Runtime Handle

The runtime handle should internally distinguish:

1. cache runtime;
2. durable local standard runtime;
3. durable local always runtime;
4. closed runtime state.

The public type should not expose the enum variants directly if that would leak
lower-layer runtime types.

Methods added in this slice:

1. `open`;
2. `create`;
3. `open_or_create` if useful;
4. `close`;
5. `is_open` or equivalent diagnostic helper only if needed by tests.

## Option Mapping

L9 open options should include:

1. storage mode;
2. durability policy;
3. local path or backend selector where applicable;
4. cache-only limits;
5. strict/lossy recovery policy;
6. memory/budget options as already supported by L8;
7. checkpoint/WAL-growth policy knobs as supported by L8Z.

L9 should reject:

1. cache mode with durable-local path requirements;
2. durable mode without required local backend details;
3. object-durable production mode;
4. distributed writer mode;
5. unsupported feature combinations;
6. invalid numeric limits.

## Outcome Mapping

Map L8 open outcome into L9 facts:

1. storage mode;
2. opened vs created disposition;
3. recovered visible version;
4. recovered max commit version;
5. recovery health;
6. database/codec IDs where available;
7. backend capabilities used;
8. raw stats;
9. maintenance readiness.

Cache mode must not report durable recovery facts.

## Close Mapping

Close should:

1. call L8 close exactly once for an open runtime;
2. map maintenance drain/sync/writer release facts;
3. store closed state so a second close returns idempotent outcome;
4. reject future operations with typed closed-state errors;
5. not require async/background threads.

## Source Guard Additions

Extend source guards to ensure:

1. `api` does not expose lifecycle runtime concrete types;
2. cache open path has no durable-service object names in public facts;
3. object-durable candidate text remains unsupported, not production-ready.

## Verification

```bash
cargo fmt --package strata-storage-next --check
cargo test -p strata-storage-next --locked --lib api
cargo test -p strata-storage-next --locked --test api_conformance
cargo test -p strata-storage-next --locked --test api_source_guard
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
```
