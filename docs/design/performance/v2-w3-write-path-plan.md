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

## W3.2 — solo-writer fast path (unchanged from roadmap)

## W3.3 — Standard WAL write coalescing (unchanged from roadmap)
