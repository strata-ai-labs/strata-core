# STH-3 Implementation Plan: Durability Realism (torn writes, reordering, FS assumptions)

Status: draft
Charter classes: 3 — Crash/durability (🟡 → ✅) and 10 — Unstated FS-assumptions (❌ → ✅)
Companion: `docs/architecture/v1-storage-testing-taxonomy-and-gaps.md`
Depends on: **STH-1** (oracle is the recovery post-condition).

## Objective

Model the disk that lies. Today's crash windows assume a friendly filesystem:
writes land in order, fsync is honored, rename is atomic. Real storage does none
of that under power loss. This plan adds (a) a backend that *reorders* unsynced
writes, *tears* them, and fills un-fsync'd regions with garbage; (b) a
write-ordering watchdog that asserts no dependent write precedes its WAL sync; and
(c) an ALICE-style enumeration over filesystem persistence models — the technique
that found 60 crash-consistency vulnerabilities across 11 mature systems.

## Why this matters (blog beat)

SQLite's documentation states flatly that any database without crash simulation
"likely contains undetected corruption bugs." The reason is that POSIX does not
promise what every developer assumes: appends can be reordered, a rename can be
observed half-done, an fsync'd file can still have a torn tail. ALICE enumerated
these and broke LMDB, LevelDB, even SQLite, in the exact places their authors had
argued were safe. StrataDB has a dormant `corrupt_object_byte` primitive and a
real, unexplained vanishing-WAL-segment incident. This plan activates the
instrumentation, enumerates the FS models, and turns that incident into a
permanent reproduction.

## Seams to build on (verified 2026-06-17)

- `Backend` trait — the single I/O seam; everything durable goes through it
  (`src/backend/`). The reordering/tearing model is a `Backend` decorator.
- Dormant crash primitives, currently `#[allow(dead_code)]`:
  `corrupt_object_byte`, `truncate_object`, `drop_object_file`
  (`src/testkit/integration_harness.rs:64–140`). This plan wires them in.
- Crash harness + oracle: `run_localfs_crash_recovery_harness` + STH-1.
- Durability contract: WAL fsync ordering is the invariant under test; WAL writer
  halts on fsync failure (CLAUDE.md storage substrate rules 12–13).

## Coverage target (not line count)

Exit bar = "reordering/tearing `Backend` wired into the crash harness; a watchdog
asserts WAL sync precedes dependent writes; the canonical FS persistence models
are enumerated on the durable path; the vanishing-WAL-segment incident has a
permanent reproduction." Measured by which FS models are enumerated and which
durable transitions the watchdog guards — not by harness size.

## Scope and slices (≤1,500 LOC each)

| Slice | Work | Exit gate |
|---|---|---|
| 3a | `ReorderingBackend` decorator | Buffers unsynced writes; on crash applies an arbitrary prefix/subset per a persistence model; tears + garbages the un-fsync'd tail (activates `corrupt_object_byte`/`truncate_object`) |
| 3b | Write-ordering watchdog | Records (write, sync, depends-on) events; asserts no manifest/table object is depended-upon before its WAL sync; runs as an invariant over all durable harnesses |
| 3c | FS persistence-model enumeration | For each durable op sequence, enumerate crash states under {ordered+atomic, reordered appends, split rename, garbage-unsynced}; each recovers oracle-valid |
| 3d | Vanishing-WAL-segment regression | Reproduce the incident under 3a/3b; lock it as a permanent failing-then-fixed test + corpus seed |

## Implementation detail

### 3a — `ReorderingBackend` (`src/testkit/reordering_backend.rs`)
A `Backend` decorator with an in-memory write log of operations not yet fsync'd.
A `crash(model, seed)` call materializes a crash state: select which buffered
writes "made it" (per the model — e.g., reordered appends may land out of order;
a rename may appear as the temp file only, or the target only), then tear the
boundary write (truncate to a random offset, fill the tail with garbage via the
now-activated primitive). The result is handed to `open_local` for recovery, then
the STH-1 oracle.

### 3b — Write-ordering watchdog (`src/testkit/write_ordering_watchdog.rs`)
The decorator timestamps every `write`/`sync`/`publish` with a logical counter and
records declared dependencies (a table/manifest publish depends on its WAL sync).
After a run it asserts: for every dependent object D depending on WAL sync S,
`sync_order(S) < visible_order(D)`. A violation is the SQLite "db write precedes
its journal sync" bug — a typed `WriteOrderingViolation`. Cheap enough to run as a
wrapper over the existing crash + endurance harnesses.

### 3c — FS-model enumeration (`tests/fs_persistence_models.rs`)
Drive a small durable op sequence; for each persistence model, enumerate the
reachable crash states (bounded combinatorial set, like ALICE/CrashMonkey) and
verify each recovers to an oracle-valid prefix. Models: ordered+atomic-rename
(baseline), reordered-appends, split-rename (temp-only / target-only), and
garbage-unsynced-tail.

### 3d — Incident regression (`tests/wal_segment_vanish_regression.rs`)
Use 3a/3b to reproduce the vanishing-WAL-segment trajectory; the test fails
against the buggy behavior and passes once fixed; the triggering schedule is
captured as a corpus seed so it can never silently return.

## Constraints

- Deterministic, seeded; crash schedule + model printed on failure.
- Watchdog and oracle assert typed violation classes, not text.
- Behavioral names only; the reordering backend and watchdog live in `testkit/`.
- Respect the durability contract precisely: under `Always`, no acknowledged
  commit may be lost under *any* model; the enumeration encodes that as the bar.

## Exit gate

- Reordering/tearing backend + watchdog wired into the crash and endurance
  harnesses; all canonical FS models enumerated with oracle-valid recovery.
- The vanishing-WAL-segment incident has a permanent reproduction + seed.
- Charter classes 3 and 10 flip to ✅ with this plan as evidence.
