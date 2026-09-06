# S3 — wall-clock `as_of`: implementation plan

Status: plan (pre-implementation) · Epic: #3112 · Design:
`docs/architecture/engine/wall-clock-commit-time.md` · Predecessors: S1 (#3114),
S2a (#3117), S2b (#3123), S2c (#3125) — all merged.

S3 is the slice where the epic becomes visible: a caller can finally ask a
question in wall-clock terms. S1–S2c only made `committed_at` exist and survive;
nothing reads it yet.

## What changed since the design doc was written

The design doc's implementation map was written before S2a–S2c were built. Three
of its assumptions are now known to be wrong or incomplete. Each changes the
slice.

### F1 — there is no scan fallback for wall-clock lookups

The doc proposed `lookup_committed_at_at_or_before` "mirroring
`lookup_at_or_before` … with scan fallback when non-monotonic". **That fallback
cannot exist.**

`committed_at` is commit-scoped and deliberately absent from timeline rows
(storage-format spec §10 req 13). The scan reads rows. S2b's `seed_from_scan`
states it directly: *"a scan can never supply `committed_at`"*. So:

| | index proven exact | index unproven |
|---|---|---|
| logical `as_of` | index (fast) | scan fallback — correct *wherever timeline rows exist* |
| wall-clock `as_of_time` | index | **no answer exists, ever** |

*Corrected during implementation:* the first draft of this section claimed
logical `as_of` always degrades merely in performance. That is only true where
timeline rows exist — legacy pre-elision databases and testkit views. W3.1c
retired the rows, so on a current database an unproven index leaves *both*
clocks unanswerable, and the surviving asymmetry is narrower but sharper: **a
scan can never supply `committed_at`, even where the scan itself succeeds.**

Either way the requirement is the same, and it is the one that matters: an
unproven index must surface an explicit typed diagnostic, never a silent wrong
answer or a fall-through to logical semantics.

Affected states: a branch recovered but not yet re-completed, a branch whose
index was poisoned by the corruption guard (`complete = false`), and a fork
restored from the catalog manifest before re-completion
(`mark_incomplete_for_fork_recovery`).

### F2 — resolve early; do not thread a new bound down the read path

The doc mapped `ReadSelector::AtWallClock` → `ReadBound::AtWallClock`. Grounding
that against the code: **there is no chokepoint to thread through.** Each of the
31 `as_of: Option<u64>` sites in `crates/executor/src/command.rs` branches to a
*different* engine method — `get_versioned_at`, `list_at`, `list_at_page`,
`count_at`, and their json/vector/event/graph analogues. A new bound variant
forks every one of them, in every capability.

Resolving the instant to a logical timestamp **once, before the read**, reuses
all of that machinery untouched. This is also what D2 already says in words
("the existing deterministic as-of machinery then runs unchanged"); only the map
disagreed.

It yields the slice's defining contract:

> **`as_of_time = T` is exactly equivalent to `as_of = R`, where `R` is the
> logical commit timestamp that `T` resolves to.**

One sentence fully specifies the read behavior, adds zero new read-path
semantics, and is directly testable as a differential property against the
existing `as_of` path. Every temporal rule already locked — at-or-before,
greatest-version-wins on ties, MVCC visibility at the frontier, tombstone/TTL
handling — is inherited rather than restated, so the two forms cannot drift.

Authority stays correct: resolution is engine semantics, exposed as an engine
API. The executor's part is `if as_of_time.is_some() { resolve; }` — transport
glue, not business logic (hard rule 7).

### F3 — undated history is a third boundary case

D3 covers past-the-tip and before-first. There is a third: a target that lands
before the first commit that *has* an instant, on a branch whose history extends
further back. Every database created before this epic has such a prefix, as does
any branch restored from a kind-2 checkpoint section.

Reporting that as "before retained history" would be **false** — the history is
retained and readable, it simply cannot be dated. It needs its own reason string,
distinguishable by the client.

## Resolution semantics

### The rule

```text
resolve(T) = the greatest version V such that runmax(V) <= T
where runmax(V) = max(committed_at[i]) for all dated i <= V
```

The running max exists because raw `committed_at` is non-monotonic by
construction (NTP steps, cross-machine skew) while binary search needs a
monotonic key.

**The running max is not a smoothing trick — it is the only prefix-sound
reading.** Time travel selects a *prefix* of history. If commit V3 carries a
lower instant than V2, V3 cannot be selected without also selecting V2, because
V2 is in every prefix that contains V3.

Worked example, to be pinned as a test:

| version | raw `committed_at` | runmax |
|---|---|---|
| V1 | 100 | 100 |
| V2 | 105 | 105 |
| V3 | 102 | 105 |
| V4 | 110 | 110 |

`resolve(103)` → **V1**, not V3 — even though V3's raw instant (102) is ≤ 103.
Selecting V3 would mean including V2 at instant 105, which is after the target.
This is surprising enough to require an explicit doc example and a named test.

### Undated commits

Commits with `committed_at = None` are **part of history at every target inside
the dated range**, and are never themselves selectable boundaries.

They sit in a prefix by construction (`observe_committed_at` never downgrades a
known instant, S2b's `seed_from_scan` preserves one, and every commit written
since S2a carries one). Version order is the authority for what a prefix
contains, so a resolved version V always implies the whole undated prefix is
included. There is no sound alternative: their instants are unknown, so they
cannot be compared to a target.

The resolver **verifies** the prefix shape rather than assuming it. A known
instant followed by an unknown one is an inconsistency; the resolver refuses
rather than guessing.

### Boundary outcomes

Four distinct results, all on the existing
`history_unavailable.engine.persistence_history` code (the established pattern —
arms are distinguished by the `reason` detail, per `adapter.rs:1328`), except the
input-validation one:

| target | outcome | reason |
|---|---|---|
| inside dated range | resolves | — |
| after runmax(tip) | **raise**, no clamp (D3) | after the latest dated commit |
| before first dated instant, undated prefix exists | **raise** | before the first dated commit; earlier history is undated |
| before first dated instant, no undated prefix | **raise** | before dated history |
| index unproven (F1) | **raise** | wall-clock history is unavailable on this branch |
| both `as_of` and `as_of_time` given | **raise** `invalid_argument.executor.as_of_conflict` | — |

Separating the two "before" cases is the point of F3: one means *you asked before
the database existed*, the other means *the database is older than its clock*.

### A designed safety property: unit mistakes always raise

Instants are UTC epoch **micros**. A client passing the same moment in other
units lands far outside the dated range in every direction — seconds and millis
resolve to the 1970s (before-dated → raise), nanos to the year ~57000
(past-tip → raise). No unit confusion can silently mis-resolve to a plausible
wrong commit. Worth stating in the docs and pinning as a test, because it is a
property clients will rely on.

## Slice split

31 wire sites plus IDL regeneration plus new semantics is well past the ≤1,500
LOC guidance. Split, following the S2a/b/c rhythm that worked:

**S3a — resolver + one pilot path.** The engine resolver, the index lookup, all
diagnostics, and `as_of_time` wired through exactly one command (`kv get`) to
prove the path end to end. All semantic tests live here. This is the PR that
needs real review.

**S3b — rollout.** The remaining 30 sites, IDL regeneration, fixtures, examples.
Mechanical, reviewed for uniformity, with a coverage guard asserting that every
command carrying `as_of` also carries `as_of_time` and enforces reject-if-both —
so the pair can never drift apart as commands are added.

## Implementation map (grounded, current line numbers)

**storage**
- `timeline_index.rs` — `lookup_committed_at_at_or_before(target, version_bound)
  -> Option<WallClockLookup>`. Requires `complete`; returns `None` (unproven) when
  not, since no fallback exists. Computes runmax in the same pass; verifies the
  dated-suffix shape. Pure over the entry slice — factor the search as a free
  function over `&[RetainedTimelineEntry]` so it gets a direct truth-table test
  (mutation gate).
- `api/read.rs` / `api/runtime/data.rs` — a resolution entry point beside
  `timeline_version_at_or_before` (`data.rs:399`). **No new `ReadBound` variant**
  (F2).
- New `StorageApiError` arm or new `reason` strings for the F1/F3 cases.

**engine**
- Resolver API on the persistence adapter + the branch surface. **No new
  `ReadSelector` variant** (F2, `row.rs:74`).
- Error mapping for the new reasons (`adapter.rs:1328`, `:1429` recovery hint).

**executor**
- `as_of_time: Option<u64>` beside `as_of` (pilot: one site in `command.rs`).
- One shared pure helper for reject-if-both + resolve, called per site.
- `error_registry.rs` — `invalid_argument.executor.as_of_conflict`.
- IDL regeneration (`update-surfaces` runbook).

**docs**
- Temporal contract: a new binding decision extending §6 to the wall-clock form.
  Note that §11's own example already writes `as_of = 2026-05-10T12:00:00Z` —
  the contract was drafted assuming semantics that did not exist. S3 makes the
  illustration true.
- The design doc's implementation map gets corrected per F1/F2/F3.

## Test plan

Semantics (S3a):

1. **Differential equivalence** — the centerpiece. For every commit *i* on a
   branch, `as_of_time(committed_at[i])` returns byte-identical results to
   `as_of(commit_timestamp[i])`. Run across kv and json, over a history with
   updates and deletes so MVCC visibility is genuinely exercised.
2. **Between-commit target** resolves to the earlier commit boundary.
3. **Skew / running max** — the V1..V4 table above, asserting `resolve(103) → V1`
   and explicitly that V3 is *not* selected.
4. **Past-tip raises**, does not clamp (D3), with the distinct reason.
5. **Before-dated raises**, distinguishable from past-tip by reason.
6. **Undated prefix** — restore a kind-2 checkpoint section (S2c already builds
   one), commit further, then assert: dated-range targets resolve; a target in
   the undated region raises the *undated* reason, not "before retained history";
   the resolved read still includes the undated prefix's rows.
7. **Index unproven** — poison the index, assert wall-clock raises unavailability
   while logical `as_of` still answers via scan. This pins F1 as a contract.
8. **Inconsistent shape** — a known instant followed by unknown refuses rather
   than guessing.
9. **reject-if-both** → `invalid_argument.executor.as_of_conflict`.
10. **Unit-mistake safety** — the same moment in seconds/millis/nanos all raise.
11. **Pagination stability** — the same `as_of_time` across pages resolves
    identically (the contract at `command.rs:581`).
12. **Cache-mode parity** — identical behavior with no WAL.
13. **Determinism** — the resolver reads no clock at query time; it is pure over
    (entries, target). Golden vectors and DST replay unaffected.

Rollout (S3b):

14. **Coverage guard** — every command with `as_of` has `as_of_time` and
    enforces reject-if-both. Machine-checked, so the pair cannot drift.

Mutation gate: the resolver is pure with a truth table (arms 2–5, 8), and test 1
observes the call site end to end — the S1 lesson was that in-crate tests must
exist for in-crate mutants, so the truth table lives in storage, not executor.

## Decisions needing sign-off before implementation

1. **Undated commits are included in every in-range resolution** (§Undated
   commits). Recommended and, I believe, forced by prefix soundness — but it is a
   real semantic choice and deserves an explicit yes.
2. **Wall-clock `as_of` is unavailable, not degraded, on an unproven index**
   (F1). The alternative — silently answering with logical semantics — is worse,
   but this does mean a recovering branch can serve `as_of` and refuse
   `as_of_time` for a window.
3. **No resolved-version echo in read responses.** Clients cannot see which
   commit their instant resolved to. Cheaper and cleaner to expose that as its
   own command in S4 alongside the batch resolver than to widen 31 response
   envelopes here.
