# L9H Implementation Plan: Engine Testkit And Closeout

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l9-storage-api-boundary-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L9/l9h-engine-testkit-closeout-test-plan.md`

## Objective

Close L9 with conformance helpers, fake/faulting persistence, source guards,
generated tests, and closeout evidence.

L9H makes L9 usable by engine-next tests without letting testkit become a
second production API. It also records final L9 assurance and verifies that
engine-facing code imports storage-next only through L9.

## Inputs

1. L9A-L9G.
2. `docs/architecture/engine/testing-and-conformance-plan.md`
3. `docs/architecture/engine/temporal-context-and-timeline-resolver-contract.md`
4. `crates/storage-next/src/testkit/`
5. `crates/storage-next/tests/`
6. `docs/architecture/implementation-plans/M4/L9/m4-l9-porting-log.md`

## Scope

L9H implements:

1. fake L9-compatible persistence;
2. faulting L9 wrapper;
3. shared API conformance harness;
4. generated operation script harness;
5. source guards for engine imports;
6. public API snapshot or equivalent signature guard;
7. fuzz target scaffolding if byte-decoded scripts are ready;
8. closeout inventory test;
9. sensitivity probe ledger;
10. porting log completion.

L9H does not implement:

1. engine product semantics;
2. product primitive fake behavior;
3. distributed/object-store durability;
4. external SDK tests;
5. format-freeze compatibility.

## Fake Persistence Requirements

The fake must implement the same public L9 contract used by engine-next tests.
It should support:

1. deterministic branches;
2. deterministic commit versions;
3. deterministic timestamps;
4. latest/version/timestamp/history reads;
5. prefix/range scans;
6. branch create/fork/delete/clear;
7. configurable conflicts;
8. configurable retained-history misses;
9. configurable recovery health;
10. configurable capability facts.

It must not expose shortcuts that engine code could depend on outside the real
L9 contract.

## Faulting Wrapper Requirements

Faults:

1. open failure;
2. read failure;
3. validation failure;
4. conflict;
5. failure before commit mutation;
6. durable uncertainty;
7. applied-not-visible;
8. recovery degradation;
9. maintenance failure;
10. close failure.

Faults should be selectable by deterministic scripts.

## Closeout Evidence

The final L9 closeout should record:

1. shipped API files;
2. public surface snapshot;
3. source guard results;
4. conformance command results;
5. generated/fault command results;
6. feature matrix command results;
7. sensitivity probes;
8. deferred work.

## Verification

```bash
cargo fmt --package strata-storage-next --check
cargo test -p strata-storage-next --locked --test api_source_guard
cargo test -p strata-storage-next --locked --test api_conformance
cargo test -p strata-storage-next --features testkit --locked --test api_properties
cargo test -p strata-storage-next --features fault-injection,testkit --locked --test api_faults
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo hack check -p strata-storage-next --feature-powerset --depth 2
```

Add engine-next boundary tests when the engine-next crate exists.
