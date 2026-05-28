# M4-L9 Porting Log

Status: draft

Parent plans:

- `docs/architecture/implementation-plans/m4-l9-storage-api-boundary-implementation-plan.md`
- `docs/architecture/implementation-plans/m4-l9-storage-api-boundary-test-plan.md`

## Closeout Status

Not started.

The canonical slice order is:

1. L9A - API Vocabulary And Visibility Boundary
2. L9B - Open, Runtime Handle, And Close
3. L9C - Reads And Timeline Resolution
4. L9D - Commit API
5. L9E - Branch Lifecycle API
6. L9F - Maintenance API
7. L9G - Diagnostics, Health, And Observability
8. L9H - Engine Testkit And Closeout

Slice labels are planning labels only. They should not appear in production Rust
identifiers, fixture bytes, object names, or user-facing strings.

## L9A - API Vocabulary And Visibility Boundary

Status: planned

### Source Evidence To Read

- `crates/storage-next/src/api/mod.rs`
- `crates/storage-next/src/lib.rs`
- `crates/storage/src/traits.rs`
- `crates/storage-next/src/lifecycle/error.rs`
- `crates/storage-next/src/lifecycle/outcome.rs`
- `docs/architecture/storage-next/target-crate-shape-and-test-harness.md`
- `docs/architecture/implementation-plans/M4/L9/l9a-api-vocabulary-visibility-boundary-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L9/l9a-api-vocabulary-visibility-boundary-test-plan.md`

### Shipped Files

TBD.

### Boundary Decisions

TBD.

### Tests Added

TBD.

### Sensitivity Probes

TBD.

### Verification

TBD.

## L9B - Open, Runtime Handle, And Close

Status: planned

### Source Evidence To Read

- `crates/storage-next/src/lifecycle/cache.rs`
- `crates/storage-next/src/lifecycle/durable/`
- `crates/storage-next/src/lifecycle/outcome.rs`
- `crates/storage-next/src/config/mode.rs`
- `crates/engine/src/database/open.rs`
- `crates/engine/src/database/lifecycle.rs`
- `docs/architecture/implementation-plans/M4/L9/l9b-open-runtime-handle-close-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L9/l9b-open-runtime-handle-close-test-plan.md`

### Shipped Files

TBD.

### Boundary Decisions

TBD.

### Tests Added

TBD.

### Sensitivity Probes

TBD.

### Verification

TBD.

## L9C - Reads And Timeline Resolution

Status: planned

### Source Evidence To Read

- `crates/storage-next/src/branch/read.rs`
- `crates/storage-next/src/branch/state.rs`
- `crates/storage-next/src/commit/timeline.rs`
- `docs/architecture/engine-next/temporal-context-and-timeline-resolver-contract.md`
- `docs/architecture/implementation-plans/M4/L9/l9c-reads-timeline-resolution-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L9/l9c-reads-timeline-resolution-test-plan.md`

### Shipped Files

TBD.

### Boundary Decisions

TBD.

### Tests Added

TBD.

### Sensitivity Probes

TBD.

### Verification

TBD.

## L9D - Commit API

Status: planned

### Source Evidence To Read

- `crates/storage-next/src/commit/batch.rs`
- `crates/storage-next/src/commit/cache.rs`
- `crates/storage-next/src/commit/durable.rs`
- `crates/storage-next/src/commit/outcome.rs`
- `crates/engine/src/database/transaction.rs`
- `docs/architecture/implementation-plans/M4/L9/l9d-commit-api-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L9/l9d-commit-api-test-plan.md`

### Shipped Files

TBD.

### Boundary Decisions

TBD.

### Tests Added

TBD.

### Sensitivity Probes

TBD.

### Verification

TBD.

## L9E - Branch Lifecycle API

Status: planned

### Source Evidence To Read

- `crates/storage-next/src/branch/`
- `crates/storage-next/src/lifecycle/`
- `docs/architecture/implementation-plans/M4/L8/l8y-branch-lifecycle-completeness-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L9/l9e-branch-lifecycle-api-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L9/l9e-branch-lifecycle-api-test-plan.md`

### Shipped Files

TBD.

### Boundary Decisions

TBD.

### Tests Added

TBD.

### Sensitivity Probes

TBD.

### Verification

TBD.

## L9F - Maintenance API

Status: planned

### Source Evidence To Read

- `crates/storage-next/src/lifecycle/maintenance.rs`
- `crates/storage-next/src/lifecycle/flush.rs`
- `crates/storage-next/src/lifecycle/checkpoint.rs`
- `crates/storage-next/src/lifecycle/compaction.rs`
- `crates/storage-next/src/lifecycle/retention.rs`
- `crates/storage-next/src/lifecycle/quarantine.rs`
- `crates/storage-next/src/lifecycle/wal_growth.rs`
- `docs/architecture/implementation-plans/M4/L9/l9f-maintenance-api-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L9/l9f-maintenance-api-test-plan.md`

### Shipped Files

TBD.

### Boundary Decisions

TBD.

### Tests Added

TBD.

### Sensitivity Probes

TBD.

### Verification

TBD.

## L9G - Diagnostics, Health, And Observability

Status: planned

### Source Evidence To Read

- `crates/storage-next/src/observability/`
- `crates/storage-next/src/lifecycle/health.rs`
- `crates/storage-next/src/lifecycle/outcome.rs`
- `crates/storage/src/memory_stats.rs`
- `crates/storage/src/pressure.rs`
- `docs/architecture/implementation-plans/M4/L9/l9g-diagnostics-health-observability-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L9/l9g-diagnostics-health-observability-test-plan.md`

### Shipped Files

TBD.

### Boundary Decisions

TBD.

### Tests Added

TBD.

### Sensitivity Probes

TBD.

### Verification

TBD.

## L9H - Engine Testkit And Closeout

Status: planned

### Source Evidence To Read

- `crates/storage-next/src/testkit/`
- `crates/storage-next/tests/`
- `docs/architecture/engine-next/testing-and-conformance-plan.md`
- `docs/architecture/implementation-plans/M4/L9/l9h-engine-testkit-closeout-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L9/l9h-engine-testkit-closeout-test-plan.md`

### Shipped Files

TBD.

### Boundary Decisions

TBD.

### Tests Added

TBD.

### Sensitivity Probes

TBD.

### Verification

TBD.
