# #2567 — Bounded recovery memory (design + slice plan)

**Status:** design for sign-off. Drafted 2026-09-04 during the W1 test-audit
remediation. #2567 is the single largest W1 item; it is a mini-epic, not a
one-line audit-fix, so it is scoped here before implementation.

## Problem

`with_memory_budget` is a product contract: it bounds resident memory for an
open database. **Recovery ignores it.** A cleanly-closed 1B-row store
(~266 GiB), SIGKILLed mid-open, then reopened under `--memory-budget 32g`, ran
recovery for 75 minutes with RSS climbing to **55.8 GB** and was OOM-killed
(#2567). The resident set tracked the store's row count, not the (tiny) WAL
tail — recovery materializes O(store) state regardless of the budget.

The TCP4.9b oracle (`crates/engine/tests/recovery_budget.rs`) reproduces the
*budget-is-ignored* property at CI scale and pins it (`pin_2567_*`): a ~64 MB
store recovered under a 16 MB budget peaks ~12× the budget, byte-identical with
no budget at all.

## Root causes (code-cited, enumerated)

Recovery has **several independent O(data) materializations**. The CI proxy and
the 1B OOM hit *different* ones — this is the crux, and why a single narrow fix
would flip the pin green without fixing the field bug.

| # | Structure | Site | Proportional to | Hit by |
|---|---|---|---|---|
| **A** | `decode_checkpoint_rows` accumulates **every** snapshot row into one `Vec<StorageRow>`, then `install_checkpoint_rows` installs them | `recovery.rs:994`, `:1005` | **snapshot rows (whole store)** | **1B OOM** |
| **B** | `load_required_for_codec` → `load_required` reads the **whole snapshot object's bytes** into memory before decode | `service/snapshot.rs` | snapshot object bytes | 1B OOM |
| **C** | `decode_timeline_groups` + `seed_branch_timeline_from_groups` build the retained-timeline index from the snapshot's timeline section | `recovery.rs:901`, `:975` | timeline entries (≈ rows) | 1B OOM (secondary) |
| **D** | `recover_wal`: `read.records().to_vec()` materializes the **entire** WAL tail; `require_contiguous` then builds a `BTreeSet` of every commit version | `recovery.rs:450` | WAL tail | **CI proxy** |
| **E** | `non_seeded_rows: Vec<StorageRow>` held on `LifecycleRecoveredCheckpoint` across catalog build (non-seeded branches installed later) | `recovery.rs` (checkpoint struct) | non-seeded rows | multi-branch stores |
| **F** | Fresh-open / table-inventory fold (`TableObjectService::list_inventory`) materializes per-table metadata | `service/table.rs` | tables | suspect; lower priority |

The dominant 1B cost is **A** (and its input **B**): a clean-closed store
recovers from a full checkpoint whose snapshot holds the whole store; decoding
it into one `Vec` is ~store-sized. The CI test uses a 64 MB **WAL** (no
checkpoint), so it exercises **D**, not A.

## Existing footholds

- `SnapshotService::visit_sections(max_sections, …)` (`service/snapshot.rs`)
  already visits snapshot sections incrementally — the seam for streaming the
  A/C decode+install section-by-section instead of one whole `Vec`.
  **Caveat:** its `read_snapshot_optional` still loads the whole object's bytes,
  so section-streaming bounds the *decoded-row* peak (the larger cost —
  `StorageRow` carries overhead over raw bytes) but not the raw-bytes peak.
  Truly bounding B needs **ranged object reads** (a substrate change).
- The budget ledger (`StorageBudgetLedger`) already exists and is threaded into
  assembly (`recovery.rs:406`); recovery has the budget in hand — it just does
  not gate the fold on it.

## Boundedness strategy per structure

- **A + C (checkpoint install / timeline seed):** replace `decode_checkpoint_rows`
  (whole `Vec`) with a **section-streamed install** over `visit_sections`: decode
  one section, install its rows into branch state, seed its timeline slice, drop,
  repeat. Branch install already goes through `install_snapshot_rows_into_branches`
  batch-by-batch conceptually; the change is to *feed* it per section rather than
  a single giant Vec. Peak becomes O(max section) + the installed table state
  (which is the durable store's own resident footprint, already budget-governed
  by the block cache / table budget).
- **B (object bytes):** ranged/streamed object read so `visit_sections` pulls
  section bytes on demand rather than one `read_snapshot_optional`. Larger; may
  be a follow-on slice (A already removes the *doubling* — the decoded-row Vec).
- **D (WAL replay):** stream `read_after_commit_version` in bounded commit-version
  windows; apply each window and drop it. `require_contiguous` and
  `verify_commit_watermark_recoverable` must become **incremental** (track the
  contiguous upper bound and the attestation max across windows) rather than
  folding over one `Vec`. This is the CI-pin path.
- **E (non_seeded_rows):** stage to a spillable buffer or re-read per non-seeded
  branch at catalog-build time instead of holding all in memory.
- **F (inventory):** bounded/streamed `list_inventory`; likely already
  acceptable at V1 table counts — measure before touching.

## The CI-proxy gap (must close as part of this)

`recovery_budget.rs` only exercises **D**. Before/with the fix, add a
**checkpointed** variant that forces a snapshot (so recovery runs A/B/C) and
asserts the budget envelope there — otherwise the pin flips green on a fix that
never touched the 1B root cause. The read-back completeness assertion
(already permanent) stays on both.

## Target contract (what "fixed" asserts)

Budgeted recovery peak ≤ `budget + fixed_process_overhead_allowance`, and
budgeted peak **materially below** unbudgeted peak, for **both** a WAL-heavy and
a checkpoint-heavy store — while read-back proves recovery lost no data
(budgeted recovery must never trade data for memory).

## Slice plan

1. **S1 — CI coverage first.** Add the checkpoint-heavy recovery-budget variant
   (exercises A/B/C). Expect it to fail the envelope today; pin it alongside
   `pin_2567_*`. Gives A/C a red test before touching the fold.
2. **S2 — Section-streamed checkpoint install (A + C).** Replace
   `decode_checkpoint_rows` with a `visit_sections` streamed decode+install+
   timeline-seed. Flip the S1 checkpoint pin to the envelope contract. **Largest
   slice; the 1B fix.**
3. **S3 — Streamed WAL replay (D).** Bounded commit-version windows; incremental
   contiguity + watermark verification. Flip `pin_2567_*` (the WAL-path pin) to
   the envelope contract.
4. **S4 — Ranged object reads (B)** if S2's measured peak still tracks the object
   bytes. Substrate-level; may be deferred if S2's envelope holds.
5. **S5 — non_seeded_rows (E)** spill/re-read, if multi-branch measurement shows
   it dominates.
6. **F (inventory)** — measure; slice only if it exceeds the envelope.

Each slice is independently testable and ships with its own envelope assertion;
the `pin_2567_*` pin is retired only when S3 lands (its WAL path) and the S1
checkpoint pin is retired when S2 lands.

## Invariants

Recovery correctness is the stake: ARCH-004 (deterministic recovery ordering),
ACID-005 (replay-boundary classification), COW-005/006 and DUR-005 (recovery
completeness). Every slice must preserve "budgeted recovery loses no data" —
the streaming must install the *same* rows in the *same* order, only in bounded
batches. The commit-watermark attestation (#2690/#2769) and tail-repair
forensics (#2784/#2786) must stay intact under incremental replay.
