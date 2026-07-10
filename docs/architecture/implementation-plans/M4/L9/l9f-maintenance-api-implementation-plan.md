# L9F Implementation Plan: Maintenance API

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l9-storage-api-boundary-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L9/l9f-maintenance-api-test-plan.md`

## Objective

Expose explicit storage maintenance through L9.

L9F maps checkpoint, flush, compaction, materialization, retention, quarantine,
purge, repair, WAL-growth, and maintenance-queue operations onto L8. It returns
storage-shaped maintenance facts and source chains without exposing L4/L5/L6/L8
concrete services.

## Inputs

1. L9A-L9E.
2. `crates/storage-next/src/lifecycle/maintenance.rs`
3. `crates/storage-next/src/lifecycle/flush.rs`
4. `crates/storage-next/src/lifecycle/checkpoint.rs`
5. `crates/storage-next/src/lifecycle/compaction.rs`
6. `crates/storage-next/src/lifecycle/retention.rs`
7. `crates/storage-next/src/lifecycle/quarantine.rs`
8. `crates/storage-next/src/lifecycle/wal_growth.rs`
9. `docs/architecture/implementation-plans/M4/L8/l8h-maintenance-task-executor-implementation-plan.md`
10. `docs/architecture/implementation-plans/M4/L8/l8z-commit-hardening-pre-l9-readiness-implementation-plan.md`

## Scope

L9F implements:

1. explicit checkpoint;
2. explicit flush;
3. compaction request;
4. materialization request;
5. retention request;
6. snapshot pruning request;
7. quarantine/reclaim request;
8. purge request;
9. repair request;
10. WAL-growth policy status and trigger;
11. maintenance queue status;
12. deterministic maintenance drain for tests;
13. cache-mode unsupported/deferred maintenance mapping.

L9F does not implement:

1. product scheduling policy;
2. background thread ownership;
3. direct object deletion APIs;
4. direct manifest editing APIs;
5. table-object service exposure;
6. checkpoint format changes.

## Request Shape

Maintenance requests should carry:

1. target branch or global scope;
2. operation kind;
3. safety/proof options;
4. dry-run option where supported;
5. deterministic drain option for tests;
6. explicit limits.

Cache mode should reject or defer durable-only operations with typed outcomes
instead of silently stranding queued tasks.

## Outcome Shape

Maintenance outcomes should expose:

1. status;
2. reason class;
3. affected object names where safe and storage-shaped;
4. bytes reclaimed;
5. checkpoint required/debt facts;
6. recovery health debt;
7. source error summary;
8. retryability.

Do not expose L4 service handles, table IDs as mutable handles, branch LSM
internals, or manifest editors.

## Safety Rules

1. Retention and purge require current proof.
2. Flush does not imply WAL truncation unless L8 proves it.
3. Checkpoint success is distinct from truncation follow-up failure.
4. Materialization must use stable handle/intent semantics, not naked layer
   index from the API.
5. Compaction/materialization durable output debt must be visible to callers.
6. Cache maintenance must not claim durable effects.

## Verification

```bash
cargo fmt --package strata-storage-next --check
cargo test -p strata-storage-next --locked --lib api
cargo test -p strata-storage-next --locked --test api_conformance
cargo test -p strata-storage-next --features fault-injection,testkit --locked --test api_faults
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
```
