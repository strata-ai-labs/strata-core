# M3TC2 Test Implementation Plan: WAL Service

Status: test implementation plan

Suite plan:
`docs/architecture/implementation-plans/m3e2-wal-test-suite-plan.md`

Implementation target:
`crates/storage-next/src/service/wal.rs`

## Goal

Implement the WAL hardening suite as an adversarial durability effort.

The existing M3E2 tests prove the basic service works. M3TC2 must prove the WAL
service is hard to fool: malformed bytes fail closed, append/reporting faults
do not advance the wrong facts, sync uncertainty remains visible, retention does
not delete protected segments, and reopen behavior is precise enough for later
L7/L8 recovery.

Passing tests alone are not enough. Every test group must either fail before a
production fix or demonstrate sensitivity through a local non-committed mutation
that violates the target invariant.

## Non-Negotiable Test Rules

1. Do not count tests that merely append one record and read it back.
2. Every failure test must assert the exact `WalServiceError` variant and the
   load-bearing fields: operation, object, expected/actual sizes, segment id, or
   source kind.
3. Every append-failure test must prove service state after the failure:
   active segment id, active segment size, active metadata, dirty bytes, dirty
   records, and visible backend bytes where relevant.
4. Every corruption test must mutate a named byte boundary and assert whether
   the result is latest-tail recovery or hard `Format` failure.
5. Every retention test must prove both deleted and protected segment lists.
6. Existing passing tests count only after their assertions are made exact and a
   sensitivity probe is recorded.
7. Production code may be rewritten if the suite exposes a weak or ambiguous
   service contract.
8. No local breaking mutation used to prove sensitivity may be committed.

## Red-First Protocol

Each implementation slice follows this loop:

1. Pick the suite-plan case and name the invariant through assertions.
2. Add or tighten the test.
3. If the test fails on current code, verify the failure is meaningful, then
   fix production code.
4. If the test passes on current code, make one local non-committed mutation
   that violates the invariant and rerun the narrow test.
5. Confirm the test fails under the mutation.
6. Revert the mutation and rerun the narrow test.
7. Record the sensitivity probe in the slice closeout note.

Acceptable sensitivity probes include:

1. Remove one required backend capability from the open-time check.
2. Trust backend list order instead of sorting WAL segments by segment id. Valid
   WAL names are fixed-width hex, so lexical path order and numeric segment-id
   order are equivalent for well-formed names; the bug to catch is failing to
   sort listed segments at all.
3. Advance dirty counters before a rejected append, or fail to advance them
   after an `always` sync failure that follows a successful append.
4. Treat any partial tail as recoverable, even in non-latest segments.
5. Collapse database-id mismatch into `Format`, or segment-id mismatch into
   `DatabaseMismatch`.
6. Delete the active segment during retention.

Unacceptable proof:

1. "The test passed on current code."
2. "The test would fail if the code were wrong."
3. `matches!(error, Err(_))` without variant and field assertions.
4. Checking only returned values while ignoring service state and stored bytes.

## Required Test Support

Allowed private test support:

1. WAL record builder with deterministic branch id and timestamps.
2. WAL model tracking segment ids, offsets, object sizes, records, dirty facts,
   and expected active metadata.
3. Byte-boundary mutation routines for WAL segment headers, envelopes, and
   records.
4. Backend fakes for missing capabilities, invalid listing entries, append
   misreports, partial append visibility, sync failure, metadata failure, read
   failure, and delete failure.
5. A stored-byte inspection path through backend object operations.

Rules:

1. Keep support private to `#[cfg(test)]`.
2. Do not expose a second WAL API through testkit unless a fuzz target needs a
   narrower entry point than production services provide.
3. Do not place roadmap labels in test function names or support type names.
4. Support code must not bypass L3 encoders when proving normal append
   behavior. Byte-level mutation is allowed only for corruption tests.

## Implementation Order

### Slice A: Support, Construction, Capability, And Listing

Scope:

1. Add backend fakes for missing individual capabilities.
2. Add invalid WAL-listing object cases.
3. Add list-order, fixed-width segment ordering, segment-gap, and
   segment-id-overflow tests.
4. Tighten existing memory/localfs open tests to assert exact variants and
   fields.

Adversarial checks:

1. Mutate open-time capability checks to skip one required capability. The
   missing-capability matrix must fail.
2. Mutate invalid listed WAL object handling to return `WalServiceError::List`.
   The invalid-object test must fail because the contract is
   `WalServiceError::Backend` with operation `List`.
3. Mutate segment ordering to raw backend order. Backend-order tests must fail.

Closeout command:

```bash
cargo test -p strata-storage-next --locked service::wal
```

### Slice B: Append, Exact Boundaries, Rotation, And Property Model

Scope:

1. Add exact-fit and one-byte-over rotation tests.
2. Add record-too-large rejection.
3. Add append misreport state assertions.
4. Add the 1 to 128 record property test with 1 KiB to 8 KiB segment sizes.
5. Add checked-in proptest regression path when a seed fails.

Adversarial checks:

1. Mutate rotation condition from `>` to `>=`. Exact-fit tests must fail.
2. Mutate record-too-large rejection to append first and reject later. Stored
   bytes and state assertions must fail.
3. Mutate append metadata tracking to skip min/max timestamp or version. The
   model test must fail.

Closeout command:

```bash
PROPTEST_CASES=2048 cargo test -p strata-storage-next --locked wal_append_model
cargo test -p strata-storage-next --locked service::wal
```

### Slice C: Durability Policies And Sync Failure

Scope:

1. Add `standard` dirty-counter and close-failure tests.
2. Add `always` success and sync-failure tests.
3. Assert post-sync-failure state: dirty counters non-zero, active size
   advanced, active metadata advanced, bytes readable, and later append works.
4. Add sync error-kind routing cases.

Adversarial checks:

1. Mutate `always` append to clear dirty counters even when sync fails. The
   sync-failure tests must fail.
2. Mutate `always` append to update active metadata only after sync. The
   post-failure metadata assertion must fail.
3. Mutate `force_durable` failure to clear dirty counters. The standard-policy
   failure tests must fail.

Closeout command:

```bash
cargo test -p strata-storage-next --locked service::wal::tests::durability
cargo test -p strata-storage-next --locked service::wal
```

### Slice D: Corruption Matrix

Scope:

1. Add named byte-boundary mutation routines.
2. Cover every corruption matrix row from the suite plan.
3. Split latest-tail recovery from non-latest and mid-segment corruption.
4. Verify database-id mismatch uses a valid header checksum for another
   database id.

Adversarial checks:

1. Mutate non-latest partial tail handling to recover instead of fail. The
   non-latest corruption test must fail.
2. Mutate database-id mismatch to report `Format`. The mismatch test must fail.
3. Mutate segment-id mismatch to report `DatabaseMismatch`. The segment-id test
   must fail.
4. Mutate payload checksum verification to skip payload bytes. Payload mutation
   tests must fail.

Closeout command:

```bash
cargo test -p strata-storage-next --locked service::wal::tests::corruption
cargo test -p strata-storage-next --locked service::wal
```

### Slice E: Backend Fault Windows And Partial Visibility

Scope:

1. Add fault cases for publish/create, metadata before append, list, read, and
   delete. Sync fault cases are owned by Slice C.
2. Add a partial-append backend that can expose a prefix before returning
   failure or misleading metadata.
3. Assert the visible-prefix behavior on reopen.
4. Prove each operation reports the correct typed failure.

Adversarial checks:

1. Mutate append backend errors to advance service state. State assertions must
   fail.
2. Mutate non-latest partial tails to be recoverable. Reopen tests must fail.
3. Mutate delete failures to disappear from the delete report. Fault-window
   tests must fail.

Closeout command:

```bash
cargo test -p strata-storage-next --locked service::wal
cargo test -p strata-storage-next --features testkit,fault-injection --locked service_fault
```

### Slice F: Retention, Deletion, And Reopen

Scope:

1. Add delete report tests for protected, deleted, and failed segments.
2. Add active-segment and newer-segment protection cases.
3. Add deterministic reopen cases after clean close, dirty standard append,
   always append, rotation, latest partial tail, non-latest partial tail, and
   corrupt header.
4. Assert retention requires `DeleteObject` capability before list/delete work.
5. Confirm process-kill windows remain deferred to L7/L8.

Adversarial checks:

1. Mutate retention to delete the active segment. Active protection tests must
   fail.
2. Mutate retention to delete segments with records above the covered-through
   watermark. Protected-list tests must fail.
3. Mutate reopen to ignore latest partial tails. Reopen refusal tests must fail.
4. Remove the `DeleteObject` capability gate from retention. The capability
   test must fail.

Closeout command:

```bash
cargo test -p strata-storage-next --locked service::wal
```

### Slice G: Read, Watermark, And Review Closure

Scope:

1. Add explicit `read_after_commit_version` boundary tests for zero and max
   watermarks.
2. Add duplicate-version, out-of-order-version, and mixed-branch-id read tests.
3. Add the shortened-envelope-length corruption mutation that keeps the
   envelope CRC valid but frames an impossible inner record.
4. Add review-gap assertions for open-time metadata failure, empty existing
   segment objects, clean close without sync, and property payload spread.

Adversarial checks:

1. Mutate `read_after_commit_version` to sort records by commit version before
   filtering. The out-of-order append-order test must fail.
2. Mutate `read_after_commit_version` to use `>=` instead of `>`. The duplicate
   version test must fail.
3. Mutate clean `close` to sync unconditionally. The clean-close test must fail.

Closeout command:

```bash
cargo test -p strata-storage-next --locked service::wal
PROPTEST_CASES=2048 cargo test -p strata-storage-next --locked wal_append_model
```

### Optional Slice: Service-Level WAL Fuzz

Scope:

1. Add `service_wal_segment` only if unit/property tests expose a narrow
   service-level byte surface that L3 fuzzing does not cover.
2. Keep the target focused on object bytes, object-name lists, and trailing
   garbage.
3. Do not treat fuzzing as a substitute for the corruption matrix.

Adversarial checks:

1. Seed the corpus with known corrupt headers, latest partial tails,
   non-latest partial tails, mutated payload bytes, and invalid object names.
2. Verify the target reaches typed `WalServiceError` variants rather than
   swallowing all failures.

Manual command:

```bash
cargo fuzz run service_wal_segment --manifest-path crates/storage-next/fuzz/Cargo.toml
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

Do not mark a slice complete without a sensitivity probe unless the test failed
against the unmodified implementation first and required a production fix.

## Full Closeout Gate

M3TC2 closes only after:

1. Every required suite-plan case is covered or deferred to a named owner.
2. Every slice has a closeout record with a sensitivity probe or an initial
   red failure.
3. Property tests have checked-in regression files if any seed fails.
4. Fuzz target deferral or implementation is explicitly recorded.
5. No test names, support names, or production names contain roadmap labels.
6. The full storage-next verification matrix from the suite plan passes.
