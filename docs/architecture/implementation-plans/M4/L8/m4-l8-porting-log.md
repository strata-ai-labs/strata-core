# M4-L8 Porting Log

Status: active

Parent plans:

- `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`
- `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`

## L8A - Lifecycle Scaffold

Status: implemented

### Source Evidence Read

- `crates/storage-next/src/lifecycle/mod.rs`
- `crates/storage-next/src/commit/config.rs`
- `crates/storage-next/src/commit/error.rs`
- `crates/storage-next/src/commit/mod.rs`
- `crates/storage-next/src/testkit/mod.rs`
- `crates/storage-next/tests/commit_runtime_properties.rs`
- `crates/storage-next/tests/commit_runtime_source_guard.rs`
- `crates/storage-next/tests/common/mod.rs`
- `docs/architecture/storage-next/l8-lifecycle-recovery-maintenance.md`
- `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`
- `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8a-lifecycle-scaffold-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8a-lifecycle-scaffold-test-plan.md`

### Shipped Files

- `crates/storage-next/src/lifecycle/mod.rs`
- `crates/storage-next/src/lifecycle/config.rs`
- `crates/storage-next/src/lifecycle/error.rs`
- `crates/storage-next/src/lifecycle/facts.rs`
- `crates/storage-next/src/lifecycle/health.rs`
- `crates/storage-next/src/lifecycle/outcome.rs`
- `crates/storage-next/src/lifecycle/result.rs`
- `crates/storage-next/src/lifecycle/tests.rs`
- `crates/storage-next/src/testkit/lifecycle.rs`
- `crates/storage-next/tests/lifecycle_properties.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`

### Preserved As Storage Vocabulary

- Lifecycle state vocabulary: new, opening, recovering, open, closing, closed,
  and failed.
- Storage mode vocabulary: cache, durable local standard, durable local always,
  and object durable candidate.
- Storage open plan and open outcome facts.
- Recovery health shape: healthy, degraded, and failed.
- Recovery fault categories for manifest, snapshot, WAL, table object, inherited
  layer, IO, quarantine inventory, and timeline mismatch facts.
- Maintenance task vocabulary for flush, checkpoint, WAL truncation,
  compaction, materialization, snapshot pruning, retention, quarantine, purge,
  repair, and health collection.
- Retention, quarantine, and close fact shells.
- Lower-layer source-chain preservation through `LifecycleError::source()`.

### Intentional Changes

- The scaffold is crate-private. The crate root remains `mod lifecycle;`.
- Config uses explicit enums for close timeout and lossy recovery policy.
- Lossy recovery is disabled by default and must be explicit before an open plan
  can request lossy fallback.
- Cache-mode open plans cannot request durable recovery fallback.
- Cache-mode open outcomes cannot claim a recovered durable visible version.
- The generated lifecycle property route is a scaffold contract only; it does
  not open storage or mutate lower layers.

### Retired From V1 L8

- Product open policy and public open wording.
- Public maintenance command vocabulary.
- Primitive reconstruction callbacks.
- Product recovery advice.
- Follower refresh behavior.
- IPC or multi-process product behavior.
- StrataHub behavior.
- Product value, graph, vector, search, embedding, or inference DTOs.

### Deferred By Owner Slice

- Lifecycle state transition validation: L8B.
- Backend and service capability validation: L8C.
- Cache-mode open and close baseline: L8D.
- Durable local open/create service assembly: L8E.
- Recovery orchestration and L7 replay/bootstrap: L8F-L8G.
- Maintenance executor and task queue behavior: L8H.
- Flush, checkpoint, WAL truncation, compaction, and materialization scheduling:
  L8I-L8K.
- Retention, quarantine, purge, and repair orchestration: L8L-L8M.
- Close ordering, drain, sync, and guard release: L8N.
- Fault, crash, fuzz, sensitivity, and closeout inventory: L8O-L8P.

### Tests Added

- Module-local scaffold tests in `src/lifecycle/tests.rs`.
- Source-boundary guard in `tests/lifecycle_source_guard.rs`.
- Generated scaffold property harness in `tests/lifecycle_properties.rs`.
- Hidden testkit route `check_lifecycle_scaffold_contract`.

### Sensitivity Probes Planned

- Product/engine import into production lifecycle source must trip
  `lifecycle_source_guard`.
- Raw filesystem or environment access in production lifecycle source must trip
  `lifecycle_source_guard`.
- Lower layers importing `crate::lifecycle` must trip
  `lifecycle_source_guard`.
- Bare public lifecycle items must trip `lifecycle_source_guard`.
- Config zero limits must fail module-local tests and generated scaffold tests.
- Lower-layer source-chain collapse must fail module-local tests and generated
  scaffold tests.

### Verification

Commands to run for L8A:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --no-default-features --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```
