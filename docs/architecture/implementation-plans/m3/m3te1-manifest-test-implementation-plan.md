# M3TE1 Test Implementation Plan: Manifest Services

Status: test implementation plan

Suite plan:
`docs/architecture/implementation-plans/m3e1-manifest-test-suite-plan.md`

Implementation target:
`crates/storage-next/src/service/manifest.rs`

## Goal

Implement the manifest service hardening suite as an adversarial test effort,
not as a pass-count exercise.

The M3E1 implementation already has useful focused tests. M3TE1 raises the
bar: tests must prove that manifest services reject bad state, preserve old
durable bytes across failed publication, keep role/object facts precise, and
never infer table or branch semantics from opaque table manifest bytes.

If a required test passes immediately because the current code is already
correct, the slice must still include a local sensitivity probe that breaks the
target invariant and proves the test fails for the intended reason.

## Non-Negotiable Test Rules

1. A test that only proves "this call returns `Ok`" does not count toward M3TE1.
2. Every error-path test must assert the exact `ManifestServiceError` variant
   and the load-bearing fields: role, object, field name, expected/actual codec,
   or `PublishFailureKind`.
3. Every publication-failure test must prove the authoritative stored bytes
   after the failure, not only the returned error.
4. Every preservation test must check both the returned write fact and a fresh
   load from the backend.
5. Every table-manifest opacity test must use payloads that would be dangerous
   if decoded: corrupt database manifest bytes, WAL-looking bytes, NUL bytes,
   and arbitrary binary bytes.
6. Existing tests are evidence. They count only after they are tightened to the
   exact contract and have a sensitivity probe recorded.
7. Production code may be rewritten if the suite reveals that the current shape
   cannot express the V1 contract cleanly.
8. No local breaking mutation used to prove sensitivity may be committed.

## Red-First Protocol

Each implementation slice follows this loop:

1. Pick the suite-plan case and name the invariant in the test body through
   assertions, not comments.
2. Add or tighten the test.
3. If the test fails on current code, confirm the failure is the intended
   contract failure, then fix production code.
4. If the test passes on current code, make one local non-committed mutation
   that violates the invariant and rerun the narrow test.
5. Confirm the test fails on that mutation.
6. Revert the mutation and rerun the narrow test.
7. Record the sensitivity probe in the slice closeout note.

Acceptable sensitivity probes include:

1. Collapse a typed error into a generic variant.
2. Drop or alter a preserved manifest fact.
3. Publish before validating invalid recovery facts.
4. Replace old bytes when a simulated publish failure says visibility did not
   happen.
5. Decode table manifest payloads and reject bytes that should remain opaque.

Unacceptable proof:

1. "The test passed on the current implementation."
2. "The test would fail if the code were wrong."
3. "The test checks the same field the implementation just wrote, without a
   fresh load."
4. "The test asserts `is_err()` without the exact variant and facts."

## Required Test Support

Build private test support only where it reduces duplicated mechanics.

Allowed test support:

1. A deterministic manifest model containing database id, codec id, active WAL
   segment, snapshot facts, and flush watermark.
2. A byte-recording backend that exposes exact stored object bytes to tests.
3. A publish-fault backend that can return every `PublishFailureKind` for create
   and replace.
4. A read-fault backend that can return selected non-`NotFound` read errors.
5. Byte mutation routines for named manifest fields.
6. Table payload generators for arbitrary byte vectors.

Rules:

1. Keep support private to `#[cfg(test)]`.
2. Do not expose a second manifest service API for tests.
3. Do not place roadmap labels in test function names or support type names.
4. Prefer precise fixture constructors over broad "make valid manifest" calls
   that hide the facts under test.

## File-Size Decision

`crates/storage-next/src/service/manifest.rs` intentionally keeps the M3TE1
tests module-local because the manifest services are crate-private and the
tests need direct access to service-local error variants and fake backends. This
is an explicit exception to the V1 unit-test-module size threshold for this
slice only.

Do not use this as precedent for later durable services. If more manifest
service tests are added after M3TE1, split the test module into
`crates/storage-next/src/service/manifest/tests.rs` before adding new cases.

## Implementation Order

### Slice A: Support, Load, And Capability

Scope:

1. Add the byte-recording backend if existing backend APIs cannot prove stored
   bytes exactly.
2. Add the publish-fault backend skeleton with all five failure kinds.
3. Tighten load-current, load-required, codec-load, memory-backend, and localfs
   publication tests.

Adversarial checks:

1. Mutate missing-object handling so optional load returns an error. The
   optional-load test must fail.
2. Mutate non-`NotFound` backend read errors into missing. The backend-failure
   distinction tests must fail.
3. Mutate durable-publish unsupported into apparent success on memory. The
   memory durable-publish tests must fail.

Closeout command:

```bash
cargo test -p strata-storage-next --locked service::manifest
```

### Slice B: Create, Replace, And State Machine

Scope:

1. Add the manifest model.
2. Add create-initial exact-byte and returned-write tests.
3. Add current-state update tests for active WAL, snapshot facts, and flush
   watermark.
4. Add the state-machine property test with 1 to 64 operations and checked-in
   regression seeds.

Adversarial checks:

1. Mutate one update path to drop codec id or database id. The preservation
   tests and property test must fail.
2. Mutate one update path to overwrite snapshot facts while updating flush
   watermark. The property test must fail.
3. Mutate rejected zero facts to publish first and reject later. The old-bytes
   preservation assertion must fail.

Closeout command:

```bash
PROPTEST_CASES=2048 cargo test -p strata-storage-next --locked manifest_state
cargo test -p strata-storage-next --locked service::manifest
```

### Slice C: Recovery Facts And Codec Validation

Scope:

1. Add invalid recovery-fact cases.
2. Add valid boundary cases using `1` and `u64::MAX`.
3. Add invalid codec-id cases.
4. Add codec mismatch and codec-load backend-failure distinction tests.

Adversarial checks:

1. Mutate invalid codec handling to return `InvalidRecoveryFact`. The invalid
   codec tests must fail.
2. Mutate codec-load corrupt bytes to return `CodecMismatch`. The corrupt-byte
   routing test must fail.
3. Mutate valid `u64::MAX` recovery facts to reject as overflow. The boundary
   tests must fail.

Closeout command:

```bash
cargo test -p strata-storage-next --locked service::manifest
```

### Slice D: Publish Failure Matrix

Scope:

1. Cover database/table roles.
2. Cover create/replace intents.
3. Cover all five `PublishFailureKind` values.
4. Assert prior stored bytes after failure kinds that mean no visible
   replacement happened.
5. Assert visibility-unknown and durability-unconfirmed kinds are not collapsed.

Adversarial checks:

1. Mutate the service to map every publish failure to `Unsupported`. The matrix
   must fail.
2. Mutate replace failure to write new bytes before returning
   `FailedBeforeVisibility`. Old-byte preservation must fail.
3. Mutate table publish errors to database role. Role precision tests must fail.

Closeout command:

```bash
cargo test -p strata-storage-next --locked service::manifest publish
```

### Slice E: Table Opacity And Error Precision

Scope:

1. Add the table-manifest opacity matrix.
2. Add both overlong branch-id layout cases.
3. Add role/object precision cases for every error family.
4. Tighten existing table-manifest tests so they prove payload bytes, not only
   publish success.

Adversarial checks:

1. Mutate table manifest load to decode payload as a database manifest. The
   opacity tests must fail.
2. Mutate table layout errors to drop table role. Role precision tests must
   fail.
3. Mutate overlong assembled object-name handling to return component
   `InvalidComponent`. The assembled-name case must fail.

Closeout command:

```bash
cargo test -p strata-storage-next --locked table
cargo test -p strata-storage-next --locked service::manifest
```

## Slice Closeout Record

Each slice closeout note must include:

| Field | Required content |
|---|---|
| Suite cases covered | Exact section/case references from the suite plan. |
| Narrow command | The exact test command run for the slice. |
| Sensitivity probe | The local mutation used to prove the tests can fail. |
| Failure observed | The test name or filter that failed under the mutation. |
| Revert proof | Confirmation that the mutation was reverted before closeout. |
| Broad command | The full storage-next command set run before marking complete. |

For M3TE1, the closeout records are recorded as rows in
`docs/architecture/v1-progress-tracker.md`. Separate per-slice closeout files
are not required unless a later milestone changes the tracker convention.

Do not mark a slice complete without a sensitivity probe unless the test failed
against the unmodified implementation first and required a production fix.

## Full Closeout Gate

M3TE1 closes only after:

1. Every required suite-plan case is covered or deferred to a named owner.
2. Every slice has a closeout record with a sensitivity probe or an initial
   red failure.
3. Property tests have checked-in regression files if any seed fails. If no
   seed fails during development, no empty regression file is required.
4. No test names, support names, or production names contain roadmap labels.
5. The full storage-next verification matrix from the suite plan passes.
