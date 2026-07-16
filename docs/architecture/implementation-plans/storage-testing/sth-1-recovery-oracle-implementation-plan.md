# STH-1 Implementation Plan: Recovery Oracle (prefix-of-acknowledged-history)

Status: implemented (2026-06-18; header repaired 2026-07-16, TCP1.6 — the code
landed with STH-2 but this status line was never updated: the exact drift the
charter guard now catches). As built: `src/testkit/recovery_oracle/{model,
verify,workload,driver}.rs` + `tests/crash_recovery_oracle.rs` — shadow
expected-state model, prefix-of-acknowledged-history verifier with typed
`LostAck`/`Phantom`/`TornBatch`/`Gap` violations, bounded-exhaustive kill
points for short durable sequences, seed-scaled soak.
Charter class: 4 — Silent data loss / recovery holes (❌ Missing → ✅)
Companion: `docs/architecture/v1-storage-testing-taxonomy-and-gaps.md`
Depends on: none. **Reused by STH-2, STH-3, STH-4, STH-5.**

## Objective

After a crash at *any* point in a durable workload, prove the recovered database
is a **prefix of acknowledged commit history**: every acknowledged commit that
the durability contract promised to keep is present and exactly correct, no
acknowledged commit is silently dropped out of order, no un-acknowledged or torn
write appears, and no commit batch is applied partially. This is the one bug
class where "it reopened" hides real data loss, and storage-next has zero
coverage of the defining technique today.

## Why this matters (blog beat)

Most databases test recovery by reopening and reading a few keys. That catches
crashes that corrupt the file; it does not catch the crash that quietly loses the
last three acknowledged writes, or resurrects a write the caller never saw
succeed. The only instrument that catches those is an oracle: an independent
model of what the database *promised*, checked against what it *kept*. RocksDB's
db_stress found an undetected recovery hole exactly this way. This plan gives
StrataDB the same instrument — and makes it the reusable post-condition for every
fault test that follows.

## Seams to build on (verified 2026-06-17)

- Crash harness: `run_localfs_crash_recovery_harness` + `CrashRecoveryHarnessOutcome`
  (`src/testkit/integration_harness.rs:576`), and `src/testkit/lifecycle/crash.rs`.
  Crash = `drop(runtime)`; reopen = `open_local(root)`.
- Acknowledged-commit surface: `CommitSummary { branch_id, commit_version,
  commit_timestamp }` returned from `StorageRuntime::commit`. An *acknowledged*
  commit is one whose `CommitSummary` was returned to the caller.
- Read-back surface: `read_point` / `scan_prefix` (see the
  `assert_background_closed_loop_reads` pattern in `src/api/tests/mod.rs`) for
  full-state enumeration after reopen.
- Durability contract: `CommitDurability::{RuntimeDefault, Standard, Always}`;
  WAL writer halts on fsync failure, recovery via explicit resume. The oracle's
  tolerance for a lost suffix is mode-dependent (below).
- Precedent to generalize: `src/testkit/commit_runtime_model.rs` is a *commit-time*
  shadow model. This plan builds the *recovery-time* analogue.

## Coverage target (not line count)

Exit bar = "shadow expected-state model; after a kill at a random point,
recovered state is a verified prefix of acknowledged history, across every
durable operation." Measured by: which durable op kinds are swept (commit, flush,
compaction, checkpoint, WAL-truncation, branch ops), how kill points are chosen
(random/seeded, not enumerated), and that the verifier is exact (value + version,
all-or-nothing per batch, no phantoms) — not by harness size.

## Scope and slices (≤1,500 LOC each)

| Slice | Work | Exit gate |
|---|---|---|
| 1a | `ExpectedState` shadow model + acknowledgement recorder | Records every returned `CommitSummary`; reconstructs `(branch, key) → (value, version)` at any watermark W; per-branch ack watermark tracked |
| 1b | Prefix verifier | Given a reopened runtime, finds the recovered watermark W and asserts recovered == model@W under the prefix constraints; fails with a typed mismatch (lost-ack / phantom / torn / gap) |
| 1c | Random kill-point driver | Seeded workload; kills at a uniformly-random op (clean drop *and* mid-publish via the fault seams); reopen; verify; loop over a seed budget |
| 1d | CI wiring + soak | Bounded seed budget in CI (seconds); `#[ignore]` long-seed soak for nightly |

## Implementation detail

### 1a — `ExpectedState` model (`src/testkit/recovery_oracle/model.rs`)
A deterministic, append-only log of acknowledged commits: `Vec<AckedCommit {
branch, version, mutations }>`. `state_at(branch, W)` folds mutations with
version ≤ W into a `BTreeMap<key, (value, version)>`. The recorder wraps the
workload driver so every `commit` that returns `Ok(summary)` appends; a `commit`
that returns `Err` or whose result is unknown (the crash op) is recorded as
*in-doubt*, not acknowledged.

### 1b — Prefix verifier (`src/testkit/recovery_oracle/verify.rs`)
After reopen, enumerate the full recovered state per branch (scan). Find W = the
greatest acknowledged version whose `state_at(branch, W)` matches the recovered
map exactly. Then assert the **prefix invariants**:
- **No lost ack** (durability): W ≥ last *durably-confirmed* version; for
  `Always`, W == last acknowledged (an acknowledged Always-commit may never be
  lost). For `Standard`/default, a contiguous acknowledged *suffix* may be absent
  but nothing below W may be.
- **Contiguity**: no gap — every version ≤ W is reflected.
- **All-or-nothing**: no commit batch is half-applied.
- **No phantom**: no key holds a value from an in-doubt or never-acknowledged
  commit (> W).
Mismatch produces a typed `RecoveryOracleViolation` enum (lost-ack, phantom,
torn-batch, gap) — tests assert the *class*, never a string.

### 1c — Random kill-point driver (`src/testkit/recovery_oracle/driver.rs`)
Seeded RNG (SplitMix64, already in the testkit). Drive a randomized durable
workload (commits + interleaved maintenance). At a seeded op index, crash two
ways: (i) `drop(runtime)` after the op, (ii) mid-publish via the STH-2-shared
fault seams (`inject_*_publish_fault`) so the kill lands inside a non-atomic
durable transition. Reopen with `open_local`, run 1b. The driver is the unit
STH-2/3/4/5 reuse: it takes a "fault schedule" and returns a verified/failed
verdict.

### 1d — CI wiring
A `crash_recovery_oracle.rs` integration target runs a fixed seed budget (e.g.
256 seeds) deterministically in CI seconds; an `#[ignore]` soak runs 100k+ seeds
nightly. Every seed prints on failure and replays exactly.

## Constraints

- Deterministic and seeded; failures print the seed.
- Verifier asserts typed violation classes, never display text.
- Behavioral test names only — no `STH`/class codes in identifiers.
- The oracle lives in `testkit/` so it is reusable across crates and plans; it
  must not depend on internal lifecycle types beyond the public API + the
  existing crash seams.

## Exit gate

- The four prefix invariants are asserted for commit, flush, compaction,
  checkpoint, WAL-truncation, and branch operations, under random+mid-publish
  kills, across all three durability modes.
- The CI seed budget is green and deterministic; the soak target exists.
- The driver + verifier are exported from the testkit and consumed by at least
  one STH-2 sweep, proving reuse.
- Charter class 4 cell flips ❌ → ✅ with this plan named as evidence.
