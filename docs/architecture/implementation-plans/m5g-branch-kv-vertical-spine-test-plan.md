# M5G Test Plan: Engine-Next Branch And KV Vertical Spine

Status: draft test plan

Planning labels such as `M5G` are documentation metadata only. Test names,
production comments, error codes, metrics, and module names must use permanent
domain vocabulary instead.

## Goal

Prove that engine-next can open cache and durable-local databases, bootstrap the
minimal control plane, create branches, and perform KV put/get/delete through
the persistence adapter only.

The tests must catch boundary regressions early:

1. Direct storage-next imports outside persistence.
2. Executor-facing engine APIs exposing storage-next types.
3. Cache and durable-local semantic drift.
4. Half-created branches.
5. Control-plane corruption being accepted as healthy.
6. KV data leaking across branches.

## Test Targets

Expected test targets:

```text
crates/engine-next/tests/dependency_guards.rs
crates/engine-next/tests/persistence_adapter.rs
crates/engine-next/tests/control_plane.rs
crates/engine-next/tests/branch_and_kv.rs
crates/engine-next/tests/common/mod.rs
```

Unit tests may live near source modules when they are pure and do not need the
integration harness. End-to-end behavior should live in integration tests.

## Test Data Policy

1. Use deterministic branch names, spaces, keys, and values.
2. Use temp directories for durable-local tests.
3. Remove temp directories before each durable test if the path already exists.
4. Do not share one database across tests.
5. Use fake persistence for fault windows rather than inducing real filesystem
   corruption when the behavior under test is the engine boundary.
6. Use real storage-next L9 only for end-to-end cache/durable conformance.

## Required Test Harness Helpers

Add small helpers under `tests/common` or `src/test_support`:

1. `open_cache_database()`.
2. `open_durable_database(temp_name)`.
3. `branch_name(name)`.
4. `space_name(name)`.
5. `kv_key(bytes)`.
6. `kv_value(bytes)`.
7. `assert_no_storage_type_in_engine_error(error)`.
8. `assert_default_branch_exists(db)`.
9. `assert_branch_value(db, branch, space, key, value)`.
10. Source guard file collectors and Rust source scanners.

Helpers must not hide assertions that belong in tests. They should keep setup
small and deterministic.

## Unit Test Matrix

### API Shape

Test cases:

1. Cache open options are explicit.
2. Durable-local open options are explicit.
3. Open options do not implement an implicit default mode.
4. Executor-facing database handle can close idempotently or returns a stable closed
   state according to the implemented contract.
5. Branch name accepts ordinary names.
6. Branch name rejects empty names.
7. Branch name rejects `_system_`.
8. Branch name rejects reserved internal spellings.
9. Product space accepts ordinary names.
10. Product space rejects `_system_` for normal KV APIs.
11. KV key rejects empty keys if empty keys are not part of the product
    contract.
12. KV value permits empty bytes only if the product contract allows it.
13. Engine DTOs do not contain storage-next type names in their type
    signatures.

Expected result:

1. Invalid input returns stable engine invalid-input errors.
2. No test imports storage-next outside source guard code or persistence tests
   designed to inspect the boundary.

### Row Encoding

Test cases:

1. KV row class resolves to engine storage-space ID `0x20`.
2. Branch catalog row class resolves to control ID `0x30`.
3. Branch lifecycle row class resolves to control ID `0x30`.
4. Registry row class resolves to control ID `0x32`.
5. Database identity row class resolves to dataset ID `0x34`.
6. KV key encoding is deterministic for ordinary ASCII bytes.
7. KV key encoding is deterministic for binary bytes.
8. KV key encoding preserves prefix-scan ordering if prefix scans are exposed
   later.
9. Control row key encoding is deterministic for database identity.
10. Control row key encoding is deterministic for branch catalog entries.
11. Control row payload decode rejects unknown version.
12. Control row payload decode rejects truncated bytes.

Expected result:

1. Checked fixtures lock down row key and payload stability.
2. Capability code uses symbolic row classes, not raw storage-next IDs.

### Persistence Adapter With Fake Persistence

Test cases:

1. Successful fake commit returns committed version and timestamp.
2. Fake storage conflict maps to engine conflict error.
3. Fake stale branch generation maps to engine conflict or branch-stale error.
4. Fake unavailable backend maps to retryable storage-unavailable error.
5. Fake corruption maps to non-retryable corruption error.
6. Fake ambiguous commit maps to ambiguous-commit error.
7. Fake closed runtime maps to closed-runtime error.
8. Fake branch-create success maps to engine branch create outcome.
9. Fake branch-create duplicate maps to branch-already-exists error.
10. Fake branch-create storage failure does not activate branch catalog.
11. Fake diagnostic facts map to engine diagnostics without storage enum names.

Expected result:

1. Engine error class, code, retryability, and source preservation are tested.
2. Engine messages/codes do not expose storage-next enum/type names.

### Control-Plane Bootstrap

Test cases:

1. New cache open writes database identity.
2. New cache open writes storage-space registry version.
3. New cache open writes KV capability support marker.
4. New cache open writes default branch catalog row.
5. New durable-local open writes the same required rows.
6. Durable reopen validates matching rows.
7. Durable reopen reports reopened, not created.
8. Missing identity row fails closed or follows an explicitly documented repair
   path.
9. Missing registry row fails closed or follows an explicitly documented repair
   path.
10. Corrupt identity row fails with incompatible/corrupt layout error.
11. Corrupt registry version fails with incompatible layout error.
12. Unknown future registry version fails closed.
13. System branch is not returned by normal branch list.
14. System space is not accepted by normal KV API.

Expected result:

1. Existing corrupt control rows are never accepted as healthy.
2. Repair behavior, if implemented, is explicit and covered.

### Branch MVP

Test cases:

1. Default branch exists after cache open.
2. Default branch exists after durable open.
3. Default branch exists after durable reopen.
4. Creating `feature` from default succeeds.
5. Creating duplicate `feature` fails.
6. Creating branch from missing source fails.
7. Creating branch with invalid name fails.
8. Branch list includes default and created branch.
9. Branch list excludes system branch.
10. Branch lookup returns stable branch identity and generation.
11. Pending branch lifecycle row on open fails closed or repairs according to
    the implemented strategy.
12. Corrupt branch catalog row fails closed.

Expected result:

1. Product branch state comes from engine catalog, not raw storage branch list.
2. A half-created branch is not silently treated as healthy.

### KV MVP

Test cases:

1. Put then get returns the committed value.
2. Missing key returns none.
3. Delete hides a committed key.
4. Delete missing key succeeds or returns a stable not-found outcome according
   to the product decision.
5. Overwrite returns the latest value.
6. Put on missing branch fails.
7. Get on missing branch fails.
8. Delete on missing branch fails.
9. Put in reserved system space fails.
10. Get in reserved system space fails.
11. Binary key and value round-trip.
12. Large value within configured test limits round-trips.

Expected result:

1. KV operations always resolve branch through the engine branch catalog.
2. KV operations never construct storage-next commit batches outside
   persistence.

## Integration Test Matrix

### Cache End To End

Workflow:

1. Open cache database.
2. Assert open outcome is cache/non-durable.
3. Assert default branch exists.
4. Put `default/default/key-a = value-a`.
5. Get `default/default/key-a`.
6. Delete `default/default/key-a`.
7. Get `default/default/key-a` and observe missing.
8. Put `default/default/shared = base`.
9. Create branch `feature` from default.
10. Get `feature/default/shared` and observe branch-create semantics.
11. Put `feature/default/shared = feature`.
12. Get `default/default/shared` and observe `base`.
13. Get `feature/default/shared` and observe `feature`.
14. Close and assert non-durable close facts.

Assertions:

1. Branch and KV behavior works without durable backend.
2. Branch isolation is preserved.
3. System branch/space remains hidden.

### Durable-Local End To End

Workflow:

1. Open durable-local database in temp dir.
2. Assert open outcome is created.
3. Assert default branch exists.
4. Put `default/default/key-a = value-a`.
5. Create branch `feature` from default.
6. Put `feature/default/key-b = value-b`.
7. Close and assert durable close facts.
8. Reopen same temp dir.
9. Assert open outcome is reopened.
10. Assert branch list contains default and `feature`.
11. Assert default key is preserved.
12. Assert feature key is preserved.
13. Assert branch isolation is preserved after reopen.
14. Close again.

Assertions:

1. Control-plane rows and KV rows survive durable reopen.
2. Durable-local does not fall back to cache.
3. Close reports durable facts.

### Branch Create Failure Windows

Use fake persistence with injected failures.

Cases:

1. Fail before storage branch operation.
2. Fail during storage branch operation.
3. Fail after storage branch operation before pending lifecycle row is cleared.
4. Fail after pending lifecycle row is written before catalog activation.
5. Fail after catalog activation before final outcome is returned.
6. Reopen after each persisted partial state.

Assertions:

1. No partial state lists as a healthy branch unless the implemented repair path
   completed it.
2. Reopen either repairs, tombstones, or fails closed with stable diagnostics.
3. Duplicate retry behavior is deterministic.

### Mode Boundary

Cases:

1. Cache open succeeds without localfs.
2. Durable-local open requires localfs/backend support.
3. Durable-local open never calls cache fallback.
4. Cache close reports non-durable.
5. Durable close reports durable sync when storage-next can prove it.
6. Cache and durable execute the same branch/KV workflow.

Assertions:

1. Durability is the only semantic difference in this slice.
2. Durable failure does not produce a cache database.

## Source And Dependency Guards

### Storage Dependency Guard

Scan `crates/engine-next/src`.

Allowed:

1. `src/persistence/**`
2. `src/test_support/**` only if explicitly testing persistence boundary.
3. Integration tests that are source guards or persistence adapter tests.

Forbidden elsewhere:

1. `strata_storage_next`
2. `strata-storage-next`
3. `StorageRuntime`
4. `StorageOpenOptions`
5. `CommitBatch`
6. `CommitMutation`
7. `StorageSpaceId`
8. `StorageKey`
9. `StorageValue`
10. `BranchRequest`

### Engine Type Guard

Scan executor-facing signatures in `src/api`, `src/branch`, and `src/data/kv`.

Forbidden engine API type names:

1. Storage-next API DTOs.
2. WAL, manifest, table, lifecycle, backend service, and storage branch
   operation concrete types.
3. Old engine transaction context/session names.

### Planning Label Guard

Scan production sources and tests for planning labels. Documentation files are
excluded.

Forbidden examples:

1. `M5`
2. `M5G`
3. `M5T`
4. `vertical spine`
5. `next slice`

Use permanent domain names in code and tests.

The guard implementation may need to name or construct the forbidden tokens.
Exclude the guard source itself from its scan, or construct those tokens in a
way that does not cause the guard to fail on its own implementation.

### Product Scope Guard

Scan `crates/engine-next/src` for out-of-scope module behavior.

Forbidden in this slice:

1. JSON behavior.
2. Event behavior.
3. Vector behavior.
4. Graph behavior.
5. Retrieval/search behavior.
6. IPC behavior.
7. Clone/export behavior.
8. Merge/diff/restore/revert/cherry-pick behavior.
9. Public transaction sessions.

Empty placeholder modules are allowed only if required by crate-shape policy and
must not expose behavior.

## Error Contract Tests

Every engine error in this slice must assert:

1. Stable code.
2. Stable class.
3. Retryability.
4. No storage-next type names in executor-facing message.
5. Source chain retained internally.

Required error cases:

1. Invalid branch name.
2. Reserved branch name.
3. Missing branch.
4. Duplicate branch.
5. Invalid product space.
6. Missing KV key where operation expects existence, if applicable.
7. Storage unavailable.
8. Storage conflict.
9. Ambiguous commit.
10. Incompatible layout.
11. Corrupt control plane.
12. Closed database.

## Conformance Checklist

The slice cannot close unless these pass:

1. Cache branch/KV workflow.
2. Durable-local branch/KV workflow.
3. Durable reopen persistence proof.
4. Branch isolation proof.
5. Control-plane bootstrap and corruption tests.
6. Branch create failure-window tests.
7. Persistence fake error mapping tests.
8. Dependency guards.
9. Engine type guards.
10. Planning label guards.
11. Product scope guards.

## Verification Commands

```bash
cargo fmt --all --check
cargo check -p strata-engine-next --all-features
cargo test -p strata-engine-next --test dependency_guards --all-features
cargo test -p strata-engine-next --test persistence_adapter --all-features
cargo test -p strata-engine-next --test control_plane --all-features
cargo test -p strata-engine-next --test branch_and_kv --all-features
cargo test -p strata-engine-next --all-features
```

## Exit Gate

Testing is complete when:

1. Every verification command passes.
2. Real cache and durable-local workflows pass through executor-facing engine APIs.
3. Fake persistence tests cover all planned failure classes.
4. Durable reopen proves control-plane and KV persistence.
5. Source guards prove dependency direction.
6. No out-of-scope capability behavior lands.
