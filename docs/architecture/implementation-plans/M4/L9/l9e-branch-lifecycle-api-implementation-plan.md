# L9E Implementation Plan: Branch Lifecycle API

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l9-storage-api-boundary-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L9/l9e-branch-lifecycle-api-test-plan.md`

## Objective

Expose storage branch mechanics through L9.

L9E wraps L6/L8 branch lifecycle behavior for engine-next: create, list,
describe, fork, fork at retained history, clear, delete, and generation-guarded
operations. It does not implement product branch names, merge, cherry-pick,
revert, restore, review, publish, or branch UX.

## Inputs

1. L9A-L9D.
2. `crates/storage-next/src/branch/`
3. current L8Y branch lifecycle code under `crates/storage-next/src/lifecycle/`.
4. `crates/storage-next/src/lifecycle/durable/`
5. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
6. `docs/architecture/implementation-plans/M4/L8/l8y-branch-lifecycle-completeness-implementation-plan.md`
7. old engine branch helpers under `crates/engine/src/database/`.

## Scope

L9E implements:

1. branch create;
2. branch list;
3. branch describe;
4. fork from current retained frontier;
5. fork at retained version;
6. fork at retained timestamp via timeline resolution;
7. clear branch;
8. delete branch;
9. generation guards;
10. pinned-reachability cleanup protection mapping;
11. branch cleanup outcome mapping.

L9E does not implement:

1. product branch names beyond opaque labels if needed for storage lookup;
2. branch merge;
3. cherry-pick;
4. revert;
5. restore;
6. publish/review workflows;
7. sync or distributed branch sharing.

## Request Shape

Branch requests should carry:

1. branch ID or selector;
2. expected generation when mutating an existing branch;
3. source branch for fork;
4. retained version or timestamp for historical fork;
5. clear/delete safety options;
6. optional diagnostic tag.

The API should not allow a caller to directly edit inherited layers, table
references, branch-owned levels, or materialization state.

## Outcome Shape

Branch outcomes should expose:

1. branch ID;
2. generation before/after;
3. source branch where relevant;
4. fork version and timestamp where relevant;
5. cleanup/reachability facts for delete/clear;
6. pinned-reachability protected release counts;
7. recovery/maintenance debt if the operation requires follow-up.

## Validation

Reject:

1. duplicate create;
2. unknown branch;
3. generation mismatch;
4. fork from unretained version;
5. fork from timestamp outside retained history;
6. clear/delete attempts that would drop protected cleanup facts for pinned
   reachability;
7. deleting the last required branch if storage policy requires one;
8. operation after runtime close.

## Verification

```bash
cargo fmt --package strata-storage-next --check
cargo test -p strata-storage-next --locked --lib api
cargo test -p strata-storage-next --locked --test api_conformance
cargo test -p strata-storage-next --features testkit --locked --test api_properties
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
```
