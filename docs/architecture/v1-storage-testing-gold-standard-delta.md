# Gold-Standard Storage Testing: SQLite (and Peers) vs StrataDB — the Delta

Status: living document
Date: 2026-06-17
Companion to: `v1-storage-testing-taxonomy-and-gaps.md` (the charter) and
`implementation-plans/storage-testing/` (the STH program)

Purpose: learn everything actionable from SQLite — the most thoroughly documented
testing program in the field — and from the durability-critical peers, then state
the *precise delta* against our charter + STH so we adopt deliberately, not all at
once. This doc also seeds the "How we test StrataDB" blog.

Provenance: SQLite primary sources (`sqlite.org/testing.html`, `th3.html`,
`qmplan.html`) plus an adversarially-verified multi-source research pass (22
confirmed claims across FoundationDB, TigerBeetle, RocksDB, etcd/Antithesis, and
the OSDI/OOPSLA/VLDB/ICSE literature). Refuted material was excluded; see Caveats.

## The one-paragraph delta

We are not behind on *technique selection* — the STH program already plans the
gold-standard moves (fault sweeps, write-reordering crash simulation, a recovery
oracle, deterministic simulation, FS-assumption enumeration, mutation testing,
sanitizers). Where SQLite is genuinely ahead is **rigor and discipline**: a
*standard* (100% MC/DC), *code-level machinery* that makes the standard
achievable (`testcase()`/`ALWAYS()`/`NEVER()` + running the suite three ways), and
*process* (requirements-to-test traceability, a human release checklist). And the
research surfaces five techniques our charter does not yet name: **bounded-
exhaustive crash enumeration (the small-scope hypothesis), metamorphic oracles, a
self-contained structural integrity check, a final-state concurrency oracle (WSS),
and stochastic fault-rate injection.** The single most important finding is a
*positioning* one, not a gap: the modern DBMS-fuzzing literature optimizes for
logic/crash/perf bugs and contains **zero** treatment of durability or crash
recovery — that space belongs to the crash-consistency lineage (ALICE → CrashMonkey
→ Pathfinder), and it is exactly StrataDB's hardest problem.

## SQLite's methodology, distilled (the gold standard)

The numbers are the easy part to admire and the wrong part to copy:

- **590:1 test:code ratio** — 155.8 KSLOC core vs 92,053 KSLOC of test code/data.
- **100% branch AND 100% MC/DC** on the core, maintained every release since 2009,
  measured by the proprietary **TH3** harness (~1M LOC of generated C, run to
  **DO-178B** avionics standards on multiple platforms — "test what you fly").
- **dbsqlfuzz**: structure-aware fuzzer mutating **SQL and the database file
  simultaneously**, ~500M–1B cases/day on ~16 cores; it "all but stopped" external
  fuzzer bug reports.
- **SLT (SQL Logic Test)**: differential testing of 7.2M queries against
  PostgreSQL/MySQL/SQL-Server/Oracle.
- **Fault-injection sweeps**: OOM, I/O error, and disk-full are tested by
  *failing the Nth operation, verifying, then incrementing N* — in both
  *fail-once* and *fail-continuously* modes — with **`PRAGMA integrity_check`** as
  the post-fault oracle.
- **Crash testing**: a VFS that reorders and corrupts un-`fsync`'d writes (models
  power loss); a separate **journal-ordering VFS** asserts the journal is synced
  *before* the database file is written.
- **Mutation testing**: flip ~20k branch instructions; the suite must catch each.
- **Defensive-coding discipline**: `testcase()` (1,184 uses) marks branches that
  need both-way coverage; `ALWAYS()`/`NEVER()` mark unreachable defensive
  branches; the suite is run **three ways** (release / test-assert / coverage) and
  must produce identical results. 6,754 `assert()`s, compiled out in release.
- **Process**: cryptographic-hash **requirements-to-test traceability** (a
  requirement's text can't change without changing its ID, and the doc build emits
  a requirement→test matrix); a **~200-item human release checklist** ("is this
  really right?"); Fossil VCS mirrored on 3 machines; "all problems fixed
  expeditiously — no lingering problems"; code written to be maintained "through
  2050."

The lesson is not the ratio. It is that SQLite has a **standard**, **machinery to
hit it**, and **process to keep it** — three layers above raw test volume.

## The comparative landscape (what's NOT on the SQLite pages)

The field splits into two camps, and a third "middle":

- **Coverage-maximizing camp (SQLite):** exhaustive coverage + deterministic fault
  sweeps + structure-aware fuzzing. Oracle = **self-contained internal check**
  (`integrity_check`).
- **Deterministic-simulation camp (FoundationDB, TigerBeetle, etcd):** run the
  whole system single-threaded with clock/network/disk/scheduling behind seams, so
  any bug **replays from a seed**. FoundationDB pioneered it (Flow actor language;
  "~1 trillion CPU-hours" of sim, a hedged estimate). TigerBeetle's **VOPR** tunes
  fault parameters from a seed (drop/reorder packets, partition, corrupt disk
  reads/writes) and replays from *seed + git commit*. etcd runs inside the
  **Antithesis** deterministic hypervisor. Oracle = **declarative invariants**
  ("data consistency is never violated") searched against fault injection.
- **The middle (RocksDB):** `db_stress` with **blackbox** (signal-kill at
  intervals) and **whitebox** (`kill_random_test` at code points) crash modes, and
  **stochastic "one-in-N" fault rates** (write {0,128,1000}, read {0,32,1000},
  metadata {0,128,1000}) — *probabilistic*, not SQLite's deterministic sweep.
  Oracle = **external model** ("expected values" file db_stress verifies against).

Three oracle styles, then: **self-contained** (SQLite), **external-model**
(RocksDB), **declarative-invariant** (FDB/TigerBeetle). StrataDB's STH-1 oracle is
external-model; we should add the other two (below).

The crash-consistency research lineage is the durability-specific backbone:

- **ALICE (OSDI'14):** found 60 static / 156 dynamic crash-consistency
  vulnerabilities across 11 systems (incl. SQLite, Postgres, LMDB, LevelDB);
  ~half manifest on ext3/ext4/btrfs. **SQLite-in-WAL-mode was the only
  configuration of all 11 with ZERO vulnerabilities** — direct external validation
  of WAL-based durability.
- **CrashMonkey + ACE (OSDI'18):** "bounded black-box crash testing" (B3) —
  *automates both workload generation and the consistency checker* and
  **exhaustively** enumerates a bounded workload space (vs hand-crafted ALICE
  workloads). Justified by the **small-scope hypothesis**: most crash bugs
  reproduce with **≤3 filesystem operations after an `fsync`**.
- **Pathfinder (OOPSLA'25):** "representative testing" — reduces the crash-state
  space by exploiting correlation between crash states (scalability + coverage).
- **WriteCheck (PVLDB'25):** "write-specific serializability" (WSS) — a
  final-state oracle: a concurrent schedule must yield the **same final state** as
  some serial schedule; needs **no dependency-graph cycle analysis**. 91.4% of 35
  real transaction bugs violate WSS; 90.6% are triggerable by deterministic
  statement ordering.
- **Modern DBMS fuzzing** (NoREC/TLP metamorphic — single-engine, no reference
  DB; PQS constraint-solving; QPG plan-diversity guidance) targets logic bugs. QPG
  (ICSE'23) found **17× more unique bugs than the coverage-guided SOTA**, evidence
  that **once branch coverage saturates (SQLite at 100%), coverage is a weak
  guidance signal** and diversity matters more.

**The positioning fact:** the 2026 DBMS-fuzzing survey scopes bugs to exactly
*crashes / logic / performance* — the words "recovery" and "durability" (and
fsync/WAL/checkpoint/power-loss) **appear nowhere**. Durability/crash-consistency
is owned by the ALICE→CrashMonkey→Pathfinder lineage, and it is precisely
StrataDB's storage concern.

## The delta table

Verdicts: ✅ matched in the STH plan · 🟢 our design choice validated by the
research · ➕ net-new, not yet in charter/STH · ⚪ not applicable to a storage
substrate (SQL-engine-specific).

| Gold-standard technique (who) | StrataDB status | Verdict |
|---|---|---|
| Fail-Nth-op fault sweep, fail-once/continuously (SQLite) | STH-2 (class 5) | ✅ |
| Write-reordering / tearing crash VFS (SQLite) | STH-3 ReorderingBackend (class 3) | ✅ |
| Journal-synced-before-db write-ordering watchdog (SQLite) | STH-3 watchdog | ✅ exact |
| External-model post-fault oracle (RocksDB) | STH-1 prefix-of-history oracle | ✅ |
| Disk-full / ENOSPC injection (SQLite/FDB) | STH-2c | ✅ |
| Deterministic simulation, seed-replayable (FDB/TigerBeetle/etcd) | STH-4 (class 9) | ✅ + 🟢 |
| FS-persistence-model enumeration (ALICE/CrashMonkey) | STH-3 (class 10) | ✅ |
| Mutation testing (SQLite ~20k branches) | STH-7b `cargo-mutants` | ✅ |
| Structure-aware DB-file fuzzing (dbsqlfuzz) | STH-7c + class 7 | ✅ |
| Curated-corpus replay every run (fuzzcheck) | STH-7 corpus | ✅ |
| Sanitizers / leak-tracking every run (RocksDB/SQLite) | STH-7a (class 12) | ✅ |
| Config/optimization-toggle differential (SQLite) | STH-6 (class 2) | ✅ |
| Compound failure-during-failure (SQLite) | STH-5 (class 6) | ✅ |
| Regression test + corpus seed per bug (all) | charter discipline | ✅ |
| WAL/MVCC durability design | the substrate | 🟢 ALICE: WAL = 0 vulns |
| Bug-class-not-coverage framing | the charter | 🟢 QPG: coverage saturates |
| **100% MC/DC as the standard** (SQLite/TH3) | STH-7 sets "a threshold" | ➕ |
| **`testcase()`/`ALWAYS()`/`NEVER()` + run suite 3 ways** (SQLite) | none | ➕ |
| **Requirements-to-test traceability matrix** (SQLite) | source-guards only | ➕ |
| **Self-contained structural integrity check** (SQLite `integrity_check`) | none (oracle is model-based) | ➕ |
| **Bounded-exhaustive crash enumeration / small-scope** (CrashMonkey B3) | STH-3 is random kills | ➕ |
| **Metamorphic oracles, single-engine** (NoREC/TLP) | STH-6 is config-diff | ➕ |
| **Final-state concurrency oracle (WSS)** (WriteCheck) | none | ➕ |
| **Stochastic one-in-N fault rates** (RocksDB) | STH-2 is deterministic sweep | ➕ |
| **Human release checklist** (SQLite ~200 items) | STH-7 is automated gates | ➕ |
| **Test object code / multi-arch+endian** (SQLite/TH3) | golden vectors (format only) | ➕ |
| Cross-engine differential vs other DBs (SQLite SLT) | — | ⚪ no peer engine |
| Query-plan-diversity guidance (QPG) | — | ⚪ no query planner |

## Net-new adoptions, prioritized

Highest leverage first; none required all at once.

1. **Adopt the small-scope hypothesis → bounded-exhaustive crash enumeration
   (amend STH-3, STH-1).** The strongest research result for us: most crash bugs
   need ≤3 durable ops after an `fsync`. For short durable sequences, *exhaustively*
   enumerate crash states (CrashMonkey B3) instead of only random kills — provably
   higher yield, and cheap because the space is bounded. Keep random kills for the
   long tail. This is a methodology upgrade to plans we already have.

2. **`testcase!()` / `always!()` / `never!()` macros + run the suite three ways
   (new; enables STH-7's MC/DC).** A Rust analog of SQLite's defensive-coding
   machinery: `testcase!(cond)` marks a branch needing both-way coverage;
   `always!`/`never!` assert in test builds and compile to the bare expression in
   release; run the suite under release / debug-assert / coverage builds and
   require identical results. This is the *mechanism* that makes a coverage
   standard achievable and separates defensive code from logic errors.

3. **Set the coverage standard to MC/DC on the durable core (amend STH-7).** Not
   "a threshold" — hold **100% MC/DC** on format, WAL, recovery, and commit (the
   code where a missed branch loses data), matching SQLite's DO-178B bar for the
   part of StrataDB that has the same consequence-of-failure. Leave outer layers on
   line/branch coverage.

4. **A self-contained structural integrity check (new; the `integrity_check`
   analog).** STH-1's oracle is model-based (needs the workload). Build a
   *self-contained* verifier that, given only a database on disk, checks the
   storage invariants (manifest ↔ table ↔ WAL consistency, no orphan/dangling
   references, checksum integrity). Usable as a post-fault check *without* the
   model, in fuzzing, and potentially in production. This gives us two of the three
   oracle styles (model-based + self-contained); STH-4's sim invariants give the
   third (declarative).

5. **Requirements-to-test traceability for the format spec + error contract
   (new).** SQLite ties every requirement to its test via hashed IDs and an
   auto-built matrix. We have architecture source-guards but no requirement→test
   map. Apply it where it matters most: each clause of `strata-storage-format-v1`
   and each code in the error contract gets a stable ID and a guard asserting a
   test exercises it. Natural extension of STH-7's anti-drift guard.

6. **Metamorphic oracles (new; deepen STH-6).** We have no peer engine to diff
   against, but NoREC/TLP show you can test a single engine via semantics-
   preserving relations. Storage-native examples: a prefix scan must equal the
   union of its point reads; a value read at version V must equal replay-to-V; a
   value across a branch fork must equal the parent at the fork point. These catch
   silent-wrong-result with no reference engine.

7. **WSS final-state oracle for branch-aware MVCC concurrency (new).** For
   concurrent commits / background maintenance interleavings, assert the final
   state equals *some* serial schedule's — no Elle/Cobra dependency-graph checker
   needed. Composes naturally into the STH-4 simulator.

8. **Process: a human storage-release checklist (new).** Automated gates (STH-7)
   catch what we thought to encode; a short human checklist catches "is this really
   right?" Start small, grow it per the Checklist Manifesto model SQLite cites.

9. **Stochastic fault rates as a long-run complement (minor; STH-4).** Deterministic
   sweeps (STH-2) are exhaustive but bounded; add RocksDB-style one-in-N rates in
   the long-running simulator for combinations the bounded sweep can't reach.

10. **Multi-arch / endianness golden verification (minor; class 7/format).** SQLite
    runs the object code on 32/64-bit and big/little-endian. Our golden vectors
    freeze the format; verify them byte-identical across architectures in CI.

## What the research validates (blog material)

- **WAL/MVCC was the right substrate.** ALICE tested 11 mature systems; only
  SQLite-in-WAL-mode had **zero** crash-consistency vulnerabilities. StrataDB's
  branch-aware MVCC WAL substrate is on the proven side of that line.
- **Our DST bet is sound and we are unusually well-positioned.** DST is the
  dominant correctness paradigm for the durability-critical camp, normally an
  impossible retrofit — and StrataDB's single-threaded core plus the already-landed
  `MaintenanceExecutor`/`MaintenanceClock` seams (charter class 9) put us where
  TigerBeetle is, not where most databases are.
- **Bug-class-not-file-count is the right frame.** QPG shows code coverage
  saturates as a signal; the charter's insistence on reasoning by bug class (and
  the STH focus on oracles + diversity, not LOC) matches where the field is.

## The blog thesis ("How we test StrataDB")

The testing world has split: query-fuzzing (logic/crash/perf) and
crash-consistency/durability — and the two literatures barely cite each other. For
a database whose job is to be the durable substrate under AI agents, **silent data
loss is the unforgivable bug**, so StrataDB lives in the durability camp the
query-fuzzers ignore. We test the storage substrate the way avionics software is
tested (SQLite/DO-178B coverage discipline) *and* the way distributed systems are
tested (FoundationDB/TigerBeetle deterministic simulation), unified by a single
oracle — *recovered state is a prefix of acknowledged history* — because that one
invariant is the difference between "it reopened" and "we kept your data."

## Caveats (verification hygiene)

From the adversarial pass — do **not** repeat these in the blog without
re-checking: ALICE's internal mechanism (the APM / micro-operation machinery) did
*not* survive verification — cite ALICE's *findings*, not its internals;
FoundationDB's "1 trillion CPU-hours" is a hedged self-estimate; RocksDB whitebox
crash points are predefined (around FS ops), not arbitrary; QPG's plan-diversity
multiple is *not* a bug-finding metric (use "17× more bugs than SQLRight"); BOB is
a filesystem-level, not application-level, tool. Vendor parameter sets (RocksDB
fault tuples, VOPR/FDB pages) reflect mid-2026 main branches and drift — re-verify
before quoting.

## Sources

Primary: sqlite.org/testing.html · sqlite.org/th3.html · sqlite.org/qmplan.html ·
apple.github.io/foundationdb/testing.html ·
github.com/tigerbeetle/tigerbeetle/blob/main/docs/internals/vopr.md ·
github.com/facebook/rocksdb/blob/main/tools/db_crashtest.py ·
etcd.io/blog/2025/autonomus_testing_with_antithesis ·
ALICE (OSDI'14, usenix.org) · CrashMonkey+ACE (OSDI'18, cs.utexas.edu) ·
Pathfinder (OOPSLA'25, arxiv 2503.01390) · WriteCheck (PVLDB Vol.18) ·
QPG (ICSE'23, arxiv 2312.17510) · DBMS-fuzzing survey (ACM CSUR 10.1145/3799227).
