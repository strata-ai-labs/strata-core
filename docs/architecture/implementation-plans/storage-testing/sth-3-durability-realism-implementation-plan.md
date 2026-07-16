# STH-3 Implementation Plan: Durability Realism (torn writes, reordering, FS assumptions)

Status: **3a + 3c implemented (2026-06-18); 3b implemented (2026-07-16, TCP1.3)**; 3d reframed as standing regression discipline. See "As-built" below.
Charter classes: 3 — Crash/durability (✅, closed by 3b) and 10 — Unstated FS-assumptions (❌ → ✅)

## As built (3b) — 2026-07-16, slice TCP1.3

`crates/storage/src/testkit/write_ordering_watchdog.rs` +
`tests/write_ordering.rs` + `StorageBackend::write_ordering_local_fs` /
`write_ordering_report`. A pure-observer `Backend` decorator: logical order
for every append/sync/publish/delete, per-WAL-segment unsynced boundary, and
a typed `WriteOrderingViolation` filed at every manifest/snapshot/table
publish that occurs over unsynced WAL-segment bytes. Detection non-vacuity is
proven by recorder-driven unit tests (a staged publish-before-sync produces
the typed violation); the live invariant is proven over real operation
streams: Always, Standard, WAL rotation (small segments), and recovery
reopen — all clean. Two engine facts the watchdog documented on the way:
Standard-mode WAL durability flows through authorized whole-segment
publishes rather than `sync_object` (the buffered WAL's rewrite path — the
watchdog credits a segment publish as its durable boundary), and WAL sidecar
metadata lives under `wal/` but is excluded as an optional fallback artifact.
Nightly runs the integration entry in the durable-invariants job. Known
limitation: steady-state appends through a persistent `BackendAppendHandle`
bypass any decorator (the watchdog, like the fault/reordering backends, does
not override `open_append_handle`, so watched runs use the per-call fallback
path — same fidelity trade the other STH harnesses accept).
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
argued were safe. StrataDB had dormant `corrupt_object_byte` / `truncate_object`
primitives (now activated). This plan enumerates the FS persistence models on the
durable path and verifies every reachable crash state recovers to an oracle-valid
prefix.

> **Correction (2026-06-18):** an earlier draft cited "a real, unexplained
> vanishing-WAL-segment incident." No such incident is on record — the codebase
> has only the generic missing-object fallback path (`MissingTableObject` /
> `NoManifestFallback`), not a specific reproduction. 3d is therefore reframed
> (below) from "reproduce the incident" to standing regression discipline: any bug
> the enumeration surfaces becomes a permanent failing-then-fixed test + corpus seed.

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

## As-built (2026-06-18)

**Delivered: 3a + 3c.** Deferred: 3b. Reframed: 3d.

- **3a — `ReorderingBackend`** (`src/testkit/reordering_backend.rs`). A `Backend`
  decorator over `LocalFsBackend` that records each object's `current_len` (grown by
  `write`/`append`) and `durable_len` (set to `current_len` on `sync_object`; a
  `publish_object` is durable on success) plus the list of published objects.
  `crash(model, seed)` materializes a crash state on the real files via the
  now-`pub(crate)` `crash_sim` primitives (`truncate_object`, `corrupt_object_byte`,
  `drop_object_file`) and returns whether anything was perturbed (non-vacuousness).
  Wired into `StorageBackend` as a feature-gated `Reordering` variant +
  `reordering_local_fs(root)` + `reordering_crash(model, seed)`; like the STH-2
  faulting backend it holds a `Mutex`, so it opens via the borrowed
  (evaluate-and-enqueue) path. `FsModel::{OrderedAtomic, ReorderedAppends,
  GarbageUnsyncedTail, SplitRename}`.
- **3c — FS-model enumeration** (`src/testkit/fs_models/mod.rs` +
  `tests/fs_persistence_models.rs`). Reuses the STH-1 oracle
  (`recovery_oracle::{model, verify, workload}`). Sweeps seed × `FsModel` × crash
  point × {`Standard`, `Always`}: open durable on the reordering backend, drive
  commits recording acks, drop the runtime, `crash(model, seed)`, reopen plain
  (lossy), verify via `classify_recovered`. Family: **`Always` → `ZeroLoss`**
  (nothing acknowledged may be lost under *any* model); **`Standard` →
  `OnDiskDamage`** (clean prefix; the unsynced suffix may be gone). The garbage-tail
  model additionally tolerates a fail-loud reopen (a CRC-rejected torn tail is a safe
  outcome). Seeds scale with the case budget (mirrors STH-2); `#[ignore]` soak honors
  `STRATA_STORAGE_FAULT_CASES`. Non-vacuousness asserted: at least one crash per run
  perturbs the disk; split-rename falls back to the intact log without loss.
- **Deferred — 3b write-ordering watchdog.** The engine already enforces
  WAL-sync-before-publish (checkpoint persists the active WAL segment to the manifest
  before publishing the snapshot), so the watchdog is regression-protection that is
  format-coupled; it closes class 3's remaining bar and is best landed alongside the
  format work. Class 3 advances substantially here but stays 🟡 until 3b.
- **Reframed — 3d.** No specific vanishing-WAL incident exists to reproduce (see the
  correction above). 3d becomes the standing rule for this program: any durability
  bug the enumeration (or any STH harness) surfaces is captured as a permanent
  failing-then-fixed test + corpus seed.

## Implementation detail

> The "As-built" section above is authoritative for **3a** and **3c** (the design
> evolved during implementation — e.g. 3a records each object's durable boundary and
> materializes the crash on the real files, rather than buffering an in-memory write
> log). The detail below is retained for design intent and for the deferred 3b.

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

### 3d — Standing regression discipline (reframed)
There is no specific vanishing-WAL-segment incident on record to reproduce (only
the generic missing-object fallback path). 3d is therefore not a single test but a
standing rule: any durability bug the FS-model enumeration — or any STH harness —
surfaces is locked in as a failing-then-fixed test plus a corpus seed, so it can
never silently return.

## Constraints

- Deterministic, seeded; crash schedule + model printed on failure.
- Watchdog and oracle assert typed violation classes, not text.
- Behavioral names only; the reordering backend and watchdog live in `testkit/`.
- Respect the durability contract precisely: under `Always`, no acknowledged
  commit may be lost under *any* model; the enumeration encodes that as the bar.

## Exit gate

**Met by 3a + 3c (2026-06-18):**
- Reordering/tearing backend (`ReorderingBackend`) materializes all four canonical FS
  models on the durable path; every model × crash point × durability recovers
  oracle-valid. `Always` loses nothing under any model (asserted, non-vacuous);
  `Standard` recovers a clean prefix; a garbage tail is fail-loud or a clean prefix,
  never silently wrong. clippy `--all-features --all-targets -D warnings` + fmt clean;
  STH-1/STH-2 + full `--lib` regression green.
- **Charter class 10 (unstated FS-assumptions) flips to ✅** with this as evidence.

**Outstanding (tracked, not blocking class 10):**
- 3b write-ordering watchdog — closes class 3's remaining bar (class 3 advances here
  but stays 🟡 until 3b lands alongside the format work).
- 3d standing regression discipline — applies continuously; no single artifact.
