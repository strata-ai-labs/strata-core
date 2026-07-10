# L9D Implementation Plan: Commit API

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l9-storage-api-boundary-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L9/l9d-commit-api-test-plan.md`

## Objective

Expose L7/L8 commit behavior through L9 without public transaction sessions.

L9D provides a storage commit batch API for engine-next. It validates mutations,
maps storage commit options to L7/L8, preserves conflicts and durable
uncertainty, and returns storage-shaped commit outcomes.

## Inputs

1. L9A-L9C.
2. `crates/storage-next/src/commit/batch.rs`
3. `crates/storage-next/src/commit/cache.rs`
4. `crates/storage-next/src/commit/durable.rs`
5. `crates/storage-next/src/commit/outcome.rs`
6. `crates/storage-next/src/lifecycle/cache.rs`
7. `crates/storage-next/src/lifecycle/durable/`
8. `docs/architecture/storage/l7-commit-runtime.md`
9. `crates/engine/src/database/transaction.rs`

## Scope

L9D implements:

1. commit batch builder;
2. put mutation;
3. delete/tombstone mutation;
4. optional TTL metadata;
5. branch/storage-space/key validation;
6. duplicate mutation rejection;
7. conflict/CAS selectors supported by L7;
8. commit durability options;
9. commit outcome mapping;
10. durable uncertainty error/outcome mapping;
11. applied-not-visible mapping;
12. closed-runtime and wrong-mode rejection.

L9D does not implement:

1. public transaction sessions;
2. durable transaction IDs;
3. serializable isolation claims;
4. cross-branch atomic commits;
5. product side effects;
6. derived index/search/vector updates.

## Batch Shape

The commit request should include:

1. target branch;
2. expected branch generation if supplied;
3. mutations;
4. read-set/CAS predicates if supplied;
5. commit durability preference if allowed by runtime mode;
6. optional caller-provided timestamp only if L7 policy permits it;
7. opaque client tag for diagnostics only if needed.

All mutations in one L9 commit are single-branch V1.

## Validation

Reject before allocation:

1. empty batch;
2. duplicate internal mutation key;
3. malformed key;
4. unknown branch;
5. branch generation mismatch;
6. cross-branch mutation;
7. unsupported durability request for mode;
8. transaction/session IDs;
9. product DTO metadata.

## Outcome Mapping

Map lower outcome into:

1. commit version;
2. commit timestamp;
3. target branch;
4. mutation counts;
5. durability class;
6. visibility facts;
7. timeline facts;
8. unresolved durable or applied-not-visible facts;
9. source-chain facts for lower-layer failures.

The API should not expose WAL record bytes, WAL segment IDs, L6 row details, or
commit-runtime guard internals.

## Verification

```bash
cargo fmt --package strata-storage-next --check
cargo test -p strata-storage-next --locked --lib api
cargo test -p strata-storage-next --locked --test api_conformance
cargo test -p strata-storage-next --features fault-injection,testkit --locked --test api_faults
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
```
