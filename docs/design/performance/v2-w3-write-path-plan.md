# V2-W3: per-commit write overhead — plan

Status: W3.1 design (recon complete, implementation not started).
Owner: billion-scale roadmap v2 § W3 (T3: 28µs/commit vs RocksDB ~3µs).

## W3.1 — derive the commit timeline, stop materializing it

### Recon facts (2026-07-08)

1. **Every commit appends two timeline rows** — a `ts-v1` timestamp→version row
   and a `ver-v1` version→timestamp row in the `timeline` storage space
   (`commit/timeline.rs`, `CommitTimelineRows::into_rows`). A single-put commit
   therefore writes 3 rows: 3× row volume through the memtable, the WAL commit
   payload, flush, and every compaction pass those rows ever ride.
2. **The read side scans the world**: `timeline_view_from_read_view`
   (`api/runtime/data.rs:365`) rebuilds the view by scanning the ENTIRE
   timeline space per `as_of` lookup — the code comments that a retained index
   should exist "before high-cardinality timestamp reads become a hot path".
3. **The WAL stamp is already authoritative**: replay VALIDATES the timeline
   rows against the record's `CommitStamp` (`validate_replay_timeline_rows`) —
   the rows carry no information the stamp does not.
4. **Every data row already embeds its stamp** (`commit_version`,
   `commit_timestamp` on `StorageRow`).

### Design

- Remove `CommitTimelineRows` from commit batches (cache + durable + group
  paths). WAL records keep the stamp — no format change to the record itself
  beyond the absent rows in the payload.
- Add a **per-branch retained timeline index**: append-only in-memory
  `Vec<(Timestamp, CommitVersion)>` (16B/commit), appended at apply and at
  replay, answering `version_at_or_before` by binary search. Replaces the
  full-space scan on every `as_of`.
- **Reopen**: rebuild the index from (a) WAL replay stamps for the
  un-checkpointed tail and (b) the embedded stamps of retained data rows for
  flushed history — either one pass at open or lazily per branch on first
  `as_of` (the lazy cost equals today's per-lookup scan, paid once).

### Correctness argument (exactness for retained data)

`as_of(t)` resolves to the largest version with timestamp ≤ t, then reads rows
with version ≤ bound. If NO row with version v survives retention, then bounds
v and predecessor(v) select identical surviving-row sets (any surviving row
with version ≤ v but not ≤ pred(v) would have version exactly v — none exist).
So a timeline derived from retained rows + WAL tail yields exactly the answers
the materialized rows yield, for all queries over retained data. Timeline rows
for fully-pruned versions add nothing observable.

### Open questions (settle during implementation)

- **O1 — index persistence**: full derive-at-open costs a table scan per
  branch; persisting the index as an engine-owned derived-state artifact at
  checkpoint (contract §22/§25 pattern) bounds reopen cost. Decide by
  measuring derive-at-open at 10M first.
- **O2 — `timeline_bounds` / diagnostics semantics** report retained bounds
  once rows are gone; empty branches and fully-pruned prefixes need explicit
  contract wording.
- **O3 — timestamp ties**: index is (ts, ver) sorted; equal timestamps across
  versions resolve by version order (matches current row-key ordering).
- **O4 — guards**: commit-runtime source guards and closeout inventories
  reference timeline row machinery by name; update them WITH the change
  (BS5.2 lesson — never leave a guard pinning a removed name).
- **O5 — pre-V1 databases** containing timeline rows: no migration (rule 41);
  the timeline space simply stops being written and stops being read.

### Expected effect

Single-put commits write 1 row instead of 3; WAL payloads shrink; flush and
compaction input volume on write-heavy cells drops toward ⅓ of today's row
count (compounds with every W1 win); `as_of` goes from O(timeline-space scan)
to O(log commits). Combined with W3.2 (solo-writer fast path), targets the
28µs → ≤8µs commit.

### Slicing (agreed 2026-07-08)

Three slices, riskiest-part-first-with-oracle ordering — the old rows stay alive
as a differential oracle until the recovery story is proven:

- **W3.1a — index as cache** (tasks #77): the retained index + binary-search
  lookup + api switch; rows still written; rebuild = one timeline-space scan per
  branch on first use. Oracle: index ≡ scan on randomized histories.
- **W3.1b — persistence at checkpoint** (#78): reopen cannot afford a full data
  scan post-elision, so the checkpoint artifact lands BEFORE elision; reopen =
  artifact + WAL-tail stamps. Oracle: artifact-loaded ≡ row-derived across every
  recovery path (crash sweeps). Settles O1; new artifact kind gets golden
  coverage per the frozen-codec rule.
- **W3.1c — elision** (#79): drop the rows from batches/replay, retire the
  scans, update guards WITH the change (O4), write the retained-bounds contract
  wording (O2). The 3× row-volume win and the measurement land here.

### W3.1c LANDED (2026-07-08)

Commits stage user rows only (`prepare_commit_rows`); the apply funnel observes
every row's stamp (idempotent dedup); replay is stamp-only (a partial legacy
pair still fails closed). Completeness became INVARIANT: recovery's
`ensure_branch_timeline_complete` bridge covers fresh opens, pre-elision
databases (their rows still decode — corruption fails the open closed, coverage
moved from read time to the bridge), and section-less recoveries; fork children
seed from the parent's index at fork_version (era-independent). All four public
timeline surfaces materialize from the index (`timeline_view_or_index`).
**O2 resolved**: `timeline_bounds` reports the retained index's bounds —
identical to what the rows would have said. The after-allocation row-limit
failure is retired (unconstructible: rows == mutations) — its tests pin the
no-padding invariant. ~28 test expectations updated; replay/bootstrap contracts
flipped with closeout inventories in the same change.

Measured (A durable 10M): run **3,000 → 4,884 ops/s (+63%)**, update p99
**4.2ms → 1.13ms**, p99.9 99ms → 5.1ms, **max 879ms — sub-second for the first
time**; pacing sleeps 18.5s → 8.7s. Load unchanged (~81K, in-band): batched
load carried only 0.2% timeline overhead (2 rows per 1,000-row commit) — the
3× win is single-put commits, where A lives. `CommitTimelineRows` is now
replay-validation + test-only; delete at W3.2 or cutover.

## W3.2 — solo-writer fast path: RESOLVED BY MEASUREMENT (2026-07-08)

Attribution first (the W6 rule), and the attribution closed the slice: the
roadmap's three levers are already spent, and the remaining path cost sits in
W3.3's territory. No fast-path surgery shipped — the slice's deliverables are
the attribution itself, permanent probes, a timer-semantics fix, and the
`CommitTimelineRows` retirement promised at W3.1c.

### Measured commit-path anatomy (A durable, 100k records, 249K commits)

In-runtime mean 25.3µs/commit decomposes as:

| Component | µs/commit | Verdict |
|---|---|---|
| Grouped dispatch envelope | 12.0 | = stages 8.5 + fg lock wait 3.5 (exact) |
| — wal_append | 3.9 | one `write()` syscall per commit → **W3.3** |
| — apply (memtable insert) | 1.4 | fundamental |
| — admit | 1.3 | guards/registry/projection fragments; pressure walk only 0.27 |
| — post_maint | 1.2 | second pressure walk + compaction scoring + coverage |
| — stage / post_growth | 0.7 | fine |
| Pacing sleeps (graded throttle) | 7.0 | policy, not path — timer wrapped them (fixed below) |
| Drain notify | 0.8 | *wanted* work: kicks drains; wake-bit coalesced at cap |
| Batch clone + map | 0.24 | retry copy — negligible, theory dead |
| Residual (loop, summary, growth checks, block-waits) | ~2.3 | fragments + 12 rare 74ms pressure block-waits |

### Roadmap levers, audited

- **"Skip group formation bookkeeping"** — measured ≈ 0: the dispatch envelope
  minus stages minus lock wait is noise. The uncontended join is a mutex + bool.
- **"Reuse encode buffers"** — already implemented: 0 allocations, 997,672
  reuses over the run.
- **"Cache admission verdicts"** — the only live lever, worth ~1µs of
  sub-µs fragments at 100k shape. Deferred: pressure collection runs 3×/commit
  (admit, post_maint, compaction scoring) at 0.27µs/call on a 100k shape, but
  the walk is O(levels×tables) — re-audit at 10M via the new
  `[probe] pressure collection` line before building epoch-cache machinery.

### Why no surgery

True uncontended path ≈ 11.5µs. The ≤8µs target is reachable only by removing
the per-commit `write()` syscall — that is W3.3 (Standard coalescing), not
micro-shaving. And A@10M is not commit-path-bound at all: pacing is 87µs/op
there (42% of wall, debt-driven) — the M-A/M-B lanes carry A, commit-path µs
do not. The notify path was audited and left alone: below the drain cap it
performs wanted drain-kicking; at cap it is two atomics (BS5.5 liveness
lessons apply).

### Shipped in this slice

- **Timer semantics fix**: `api_commit_runtime_ns` no longer wraps the
  post-completion policy sleeps (WAL-growth backpressure, graded throttle) —
  they misattributed pacing as path cost twice during W3 attribution.
- **Permanent probes**: `commit_group_dispatch_ns`, `commit_drain_notify_ns`
  counters; engine-ycsb `[probe] commit path` and `[probe] pressure collection`
  lines.
- **`CommitTimelineRows` retired from production**: the type and its key
  construction are now `#[cfg(any(test, feature = "testkit"))]` — production
  structurally cannot stage timeline rows; tests/testkit keep constructing
  legacy rows to exercise the compat paths that remain product behavior until
  cutover (pre-elision WAL replay, recovery bridge).

## W3.3 — Standard WAL write coalescing (unchanged from roadmap)

Now the sole carrier of the ≤8µs solo-commit target (−3.9µs syscall).
