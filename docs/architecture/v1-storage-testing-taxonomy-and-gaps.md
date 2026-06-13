# How Embedded Databases Get Tested, and Where Storage-Next Stands

Status: living document
Date: 2026-06-12
Companion to: `docs/architecture/v1-existing-test-inventory-and-porting-plan.md`

This document is the conceptual frame behind the M\*T test tracks. The porting
plan inventories *what tests exist*; this document classifies *what bug classes
exist*, how the gold-standard engines cover each, and where storage-next is
strong or exposed. Cite it when scoping a test slice so coverage is reasoned
about by bug class, not by file count.

Source basis: a survey of SQLite, RocksDB/LevelDB, FoundationDB, TigerBeetle,
DuckDB, and the research literature (ALICE/OSDI'14, CrashMonkey/OSDI'18),
mapped against a full inventory of the storage-next test surface (40 integration
targets, 28 fuzz targets, the testkit, and the fault seams).

## The bug-class taxonomy

Testing across these systems converges on ~12 distinct bug classes, each needing
a different technique. What is striking is how little overlap the techniques
have — coverage in one class says nothing about another.

| # | Bug class | Technique that catches it | Exemplar |
|---|---|---|---|
| 1 | Contract violations (component does the wrong thing per spec) | Unit/integration tests, property tests vs. a model | Everyone |
| 2 | Silent wrong results | Differential testing (run two ways, diff outputs) | SQLite SLT (7.2M queries vs. 4 other DBs); DuckDB optimized-vs-unoptimized |
| 3 | Crash/durability bugs (torn writes, fsync ordering, OS write reordering) | Crash-simulation VFS: snapshot FS at op N, inject power-loss damage, verify atomicity; plus a write-ordering watchdog asserting no db write precedes its journal sync | SQLite (states any DB without this "likely contains undetected corruption bugs") |
| 4 | Silent data loss / recovery holes | Expected-state oracle: shadow model of what every key should contain; after crash, verify recovered state is a *prefix of acknowledged history* — not just "it reopened" | RocksDB db_stress (caught 3 real bugs incl. an undetected recovery hole) |
| 5 | Error-path bugs (I/O error, OOM, disk-full mid-operation) | Systematic injection sweeps: fail the Nth operation, verify integrity, increment N until clean — not hand-picked windows | SQLite (fail-once and fail-continuously modes, then `integrity_check`) |
| 6 | Failure-during-failure | Compound anomaly tests (I/O error *during* crash recovery) | SQLite |
| 7 | Hostile-input bugs (corrupt files, malformed data) | Fuzzing (incl. structure-aware DB-file fuzzing) + deliberate byte-flip corruption tests | SQLite dbsqlfuzz (~500M cases/day); LevelDB corruption_test |
| 8 | Trajectory/liveness bugs (backlog outgrows drain cadence, starvation, unbounded resources, deadlock-by-contract) | Closed-loop sustained workloads with liveness assertions — a dedicated liveness mode ("cluster eventually makes progress") separate from safety mode | TigerBeetle VOPR; RocksDB db_stress |
| 9 | Rare-interleaving and fault-combination bugs | Deterministic simulation: all nondeterminism behind seeded abstractions; any failure replays exactly | FoundationDB; TigerBeetle |
| 10 | Unstated filesystem-assumption bugs (atomic rename, ordered appends that POSIX doesn't promise) | ALICE-style crash-state enumeration under different FS persistence models — found 60 vulnerabilities across 11 mature systems incl. SQLite, LevelDB, LMDB | ALICE; adopted by hashicorp/raft-wal |
| 11 | Weak-test bugs (code executed but effects unchecked) | Coverage gates (SQLite: 100% MC/DC) + mutation testing (SQLite mutates ~20k branches and verifies the suite kills each mutant) | SQLite |
| 12 | Memory-safety / UB / races | Sanitizers, Miri, leak checks after every test | RocksDB (ASAN/TSAN/UBSAN continuously); SQLite (valgrind + leak checks per test) |

Two cultural facts worth internalizing: SQLite's test-to-source ratio is
**~590:1**, and RocksDB treats "extend the stress test" as a **required part of
shipping any feature**. One cautionary tale: LMDB ships with a toy test suite
and argues safety from design — and ALICE found a real crash-consistency
vulnerability in exactly the place the argument hand-waved.

## The map: storage-next today

The honest picture is **strong in classes 1, 2, 7, and format stability —
concentrated gaps in 3–6, 8–10, 11.**

| Class | storage-next status | Evidence |
|---|---|---|
| 1. Contract | ✅ Strong | 8 property suites with model-parity contracts, 14+ harnesses, closeout/source guards |
| 2. Differential | 🟡 Partial | Model-parity *is* differential testing vs. a reference model — good. No cross-engine or config-sweep differential |
| 3. Crash/durability | 🟡 Partial | 8 crash windows + 19 fault-window routes is real coverage of *chosen* transitions. Missing: torn-write/reordering simulation, write-ordering watchdog; crash points are enumerated by hand, not swept |
| 4. Recovery oracle | 🟡 Partial | Crash windows verify specific transitions recover. Missing: the prefix-of-acknowledged-history invariant under *random* kill points — the thing that catches silent holes |
| 5. Error-path sweeps | 🟡 Partial | The seams exist (`fault-injection` feature, `LocalFsPublishStep`/`LocalFsDeleteStep`) but drive 19 hand-picked windows, not "fail op N, sweep N." No disk-full, no OOM/budget-exhaustion injection |
| 6. Failure-during-failure | ❌ Missing | No compound anomaly tests |
| 7. Hostile input | ✅ Strong | 28 fuzz targets with corpora across every decoder and state machine; 36+ golden vectors |
| 8. Trajectory/liveness | ❌ Missing | `stress.rs` runs 3 service-level scripts — never opens a `StorageRuntime`. All three of the June 2026 scale findings live here: the perf collapse, the Block-admission deadlock, WAL retention never running |
| 9. Deterministic simulation | ❌ Missing — window closing (see below) | — |
| 10. FS-assumption enumeration | ❌ Missing | Relevant suspect for the vanishing-WAL-segment incident |
| 11. Coverage/mutation | ❌ Missing | No coverage gates, no mutation testing |
| 12. Memory safety | 🟡 Partial | `deny(unsafe_code)` + Rust covers most; no Miri/sanitizer CI, no leak assertions |

The pattern: storage-next has world-class coverage of **state-space**
correctness (what states are reachable and are they right) and near-zero
coverage of **time** (what happens when the system runs), **scale** (what
happens when structures deepen), and **oracle-verified recovery** (did we get
the *right data* back, not just *a* database back).

## Strategic note: the deterministic-simulation window (class 9)

Deterministic simulation testing (DST) is the single most powerful technique in
the taxonomy — it catches the rare interleavings and fault combinations nothing
else reaches, and makes every failure replay exactly. It is normally a
near-impossible retrofit, because it requires *all* nondeterminism (threads,
time, I/O, randomness) to sit behind swappable abstractions.

Storage-next was, until recently, unusually close to satisfying the
preconditions by construction: a single-threaded core, all I/O behind the
`Backend` trait, and an injectable data-plane clock (`CommitTimestampSource`).

**Current state (verified against the L8E implementation, commits `957c235b`
and `fa46abec`):** L8E introduced the first threads into the crate, and as
implemented it spent part of that precondition. Specifically:

- `BackgroundScheduler` is a concrete struct with `thread::spawn`,
  `JoinHandle`, and `parking_lot::Condvar` baked in. It is **not** behind a
  `MaintenanceExecutor` trait, and `BackgroundRuntimeController` holds it as a
  concrete `Arc<BackgroundScheduler>`. The production drain logic
  (`drain_cache_background_round`) can only run on a spawned worker thread —
  there is no seam to run it inline under simulation control.
- Maintenance scheduling timing uses raw `std::time::Instant::now()` throughout
  (drain limits, block-wait deadline, pressure-rejection slowdown). Maintenance
  time is not injectable.
- `DeterministicInline` remains a *separate* drive path, so "deterministic"
  tests do not exercise the production `Background` path.

What survived: `Backend`-trait I/O isolation and the `CommitTimestampSource`
data-plane clock are intact, and the leaf execution (`MaintenanceTaskRunner`)
plus the authoritative lifecycle queue are unchanged.

The door is **closing, not closed.** Because the drive logic is localized and
the leaf is already trait-bound, the retrofit is bounded today: (1) a
`MaintenanceExecutor` trait over submit/drain/shutdown/wait; (2)
`BackgroundRuntimeController` holding `Arc<dyn MaintenanceExecutor>`; (3) an
`InlineExecutor` running closures synchronously under step control; (4) a
`Clock` handle replacing `Instant::now()` in the drive logic; (5) re-expressing
`Background` + inline-executor as the path deterministic tests use. Each
subsequent slice that builds drive logic against the concrete scheduler and
`Instant::now()` (L8C recovery, L8D retention, M5 engine adapter) enlarges the
retrofit. The test-architecture decision and the executor design are the same
decision, and the time to make it is before those slices land.

## Priorities (cheapest leverage first)

1. **Closed-loop endurance suite, scaled constants (class 8).** Public-API-only
   sustained load with lifecycle thresholds shrunk ~1000× so trajectories play
   out at ~50k rows in CI seconds. Liveness assertions: commits never
   permanently fail, WAL bounded, queue drains, shape converges. Would have
   caught the perf collapse, the Block-admission deadlock, and WAL retention
   never running.
2. **Error-injection sweeps (class 5).** Generalize the existing 19 windows to
   SQLite-style "fail backend op N, sweep N, verify integrity each time." The
   seams already exist; this is mostly a loop. Add disk-full and
   budget-exhaustion modes.
3. **Recovery oracle (class 4).** RocksDB-style expected-state tracking on top of
   the existing crash harness, asserting prefix-recovery under random kill
   points.
4. **Torn-write/reordering backend (classes 3, 10).** A `Backend` wrapper that
   models an OS write cache: reorders unsynced writes, tears them, fills unsynced
   regions with garbage. Plus the SQLite-style watchdog: assert WAL sync precedes
   dependent writes. The instrumentation most likely to explain the
   vanishing-WAL-segment incident.
5. **Executor-behind-trait + DST-lite (class 9).** Decided jointly with the L8E
   executor refactor, per the strategic note above.
6. **Coverage gates and mutation testing (class 11).** Real, but M10-shaped;
   defer.
