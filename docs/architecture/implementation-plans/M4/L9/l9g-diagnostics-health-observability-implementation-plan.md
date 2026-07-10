# L9G Implementation Plan: Diagnostics, Health, And Observability

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l9-storage-api-boundary-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L9/l9g-diagnostics-health-observability-test-plan.md`

## Objective

Expose raw storage diagnostics through L9.

L9G wraps L8/L7/L6 health, recovery, pressure, budget, table-manifest,
branch-lifecycle, lazy-read, and maintenance facts into product-neutral
diagnostics consumed by engine-next.

## Inputs

1. L9A-L9F.
2. `crates/storage-next/src/observability/`
3. `crates/storage-next/src/lifecycle/health.rs`
4. `crates/storage-next/src/lifecycle/outcome.rs`
5. `crates/storage-next/src/lifecycle/facts.rs`
6. `crates/storage-next/src/lifecycle/retention.rs`
7. `crates/storage-next/src/lifecycle/quarantine.rs`
8. `crates/storage-next/src/lifecycle/budget.rs` or current L8W module.
9. `crates/storage/src/memory_stats.rs`
10. `crates/storage/src/pressure.rs`

## Scope

L9G implements:

1. storage health report;
2. recovery report;
3. maintenance status report;
4. memory/cache budget report;
5. storage pressure report;
6. table-manifest reachability summary;
7. quarantine/retention summary;
8. branch lifecycle summary;
9. lazy-read/cache counters;
10. commit timeline bounds summary;
11. source-chain summary access.

L9G does not implement:

1. product telemetry transport;
2. user-facing advice;
3. automatic remediation;
4. primitive-aware diagnostics;
5. fleet or StrataHub health reporting.

## Diagnostic Shape

Diagnostics should be snapshots of storage facts:

1. mode;
2. open/closed state;
3. recovery health;
4. visible version;
5. max commit version;
6. maintenance queue facts;
7. budget usage and limits;
8. cache hit/miss/load facts where available;
9. table-object reachability counts;
10. quarantine counts;
11. WAL growth/checkpoint policy facts;
12. branch count and pressure facts.

Fields can be `Option` when unsupported by mode, but unsupported must be
distinguishable from unknown.

## Product-Neutrality

Diagnostics must not contain:

1. product recommendations;
2. primitive names;
3. JSON/event/vector/graph/search terms;
4. user/workspace/project/account facts;
5. StrataHub fleet facts;
6. inference/model facts.

## Verification

```bash
cargo fmt --package strata-storage-next --check
cargo test -p strata-storage-next --locked --lib api
cargo test -p strata-storage-next --locked --test api_conformance
cargo test -p strata-storage-next --locked --test api_source_guard
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
```
