# Deterministic simulation (DST) — seed corpus and replay contract

The `simulation` module is the whole-runtime deterministic simulation harness
(TCP4.11): one seed derives the workload, the branch/fork/delete grammar, the
maintenance cadence, clock advancement, the fault schedule, and the crash
points. Every trajectory is a pure function of its seed, so **any failure
reproduces from seed + commit** — a nightly finding is a local repro, never a
flake.

## Lanes

| Lane | Entry | Integration target | Grammar |
|---|---|---|---|
| Clean interleaving | `run_simulation_harness` | `tests/simulation_smoke.rs` | KV commits × maintenance cadence × clock advancement, single branch |
| Fault/crash | `run_fault_simulation_harness` | `tests/simulation_faults.rs` | One backend-op fault case + one power-loss crash case per seed over a seeded interleaving |
| Whole-DB | `run_whole_db_harness` | `tests/simulation_whole_db.rs` | Multi-branch (fork current/at-version, delete, delete-recreate), multi-epoch crash → recover → continue, temporal probes; canonical shape 3 epochs × 24 steps |

All three run on the same substrate: `DeterministicInline` maintenance (inline
executor on the real `Background` logic), the manual maintenance clock, one
`SplitMix64` entropy source with XOR-salted streams, and the write-ordering
watchdog stacked over the fault/reordering backends as a continuous oracle.

## The replay contract

A sweep failure names its seed and prints its one-line repro. For the whole-DB
lane:

```text
STRATA_SIM_SEED=<n> cargo test -p strata-storage --features fault-injection \
    --test simulation_whole_db -- replay_single_seed --ignored --nocapture
```

Replay is bit-exact at the canonical trajectory shape: the same seed produces
the same action stream, the same commit versions AND timestamps
(`ApiTimestampSource` is a +1µs counter — no wall clock anywhere on the
committed path), the same content-derived identities, and the same crash
materializations. `same_seed_replays_bit_exact` (clean lane),
`fault_case_replays_bit_exact` / `crash_case_replays_bit_exact` (fault lane),
and the whole-DB twin pin this guarantee in CI.

### What is determinized

Workload, branch grammar, fault schedule, crash points, maintenance
scheduling and its clock, logical commit versions, commit timestamps, object
identities, directory listings (sorted).

### What is not (by design)

- OS thread scheduling under the default `Background` (threaded) executor —
  loom owns small interleavings (`crates/storage/src/sync.rs` seam); the DST
  runs maintenance inline instead.
- The WAL coalescing staleness window (`pending_since: Instant`) — affects
  flush *grouping*, never content; the bit-exact twins pass despite it. If a
  twin ever trips on it, de-wall-clock it then.

The environment guard (`assert_deterministic_environment`) refuses to run
under behavior-changing env knobs (`STRATA_SUBCOMPACTIONS`,
`STRATA_COMPACTION_LANES`, `STRATA_ADMISSION` at non-defaults) instead of
diverging silently.

## CI tiers

- **Per-PR**: the harness unit tests (exact-constant sweep pins, bit-exact
  twins, sabotage twins, gate-7 pins) ride the workspace suite; the
  integration smoke tests run wherever `--features fault-injection` lanes
  run.
- **Nightly** (`storage-soak-lanes`): deep multi-seed soaks via the
  `#[ignore]` tests; `STRATA_STORAGE_FAULT_CASES` scales depth. The
  whole-DB soak line is DEFERRED until the #2828 tracker closes (the first
  deepened sweep found 66/200 seeds failing — #2826, #2827, #2828; no
  born-red lanes). The whole-DB smoke (seeds 0–5, green) runs nightly in
  the deterministic-simulation step.

## Whole-DB seed corpus (canonical shape: 3 epochs × 24 steps)

Recorded so grammar changes are caught deliberately (a grammar change shifts
every constant below — re-pin consciously, and re-validate that the named
seeds still exercise what they are named for).

| Seed | Why it is named | Recorded trajectory |
|---|---|---|
| 0 | Action-histogram pin (kills grammar/label mutants replay twins cannot see) | advance_clock 14, commit 34, delete_branch 4, drain 3, enqueue_checkpoint 4, enqueue_flush 3, fork_at_version 4, fork_current 1, recreate_branch 5; facts: deletes 1, forks 3, recreates 0, deletes_refused 0, forks_unavailable 1, temporal_probes_unavailable 5 |
| 2 | The #2820 → #2823 discovery trajectory (fork + delete-parent + crash; then replay-redundant fork sources). Both fixed; the seed now completes end-to-end and is pinned as `replay_redundant_fork_sources_recover_cleanly` | 3 epochs × 24 steps, completes clean |
| 4, 5 | Distinct-constant facts pins (seed 4: deletes 2, recreates 1; seed 5: deletes 0) — distinct values across configs keep constant-mutants from coinciding with any single pin | see `facts_accessors_report_distinct_pinned_values` |
| 6 | DUR-008 refusal seed: fork-source durable live delete refused (deletes_refused 1, deletes 3), recovery survives — promoted pin `fork_source_deletion_is_refused_and_recovery_survives` | 3 epochs, refusal is a seeded no-op |
| 10 | #2827 gate-7 pin: fork-materialized table object missing while the child manifest durably lists it — reopen bricks with `corruption.lifecycle.table_manifest` | fails at epoch 2 reopen until fixed (`pin_2827_*`) |
| 11 | Bit-exact replay twin | identical facts across two runs |
| 28 | #2826 gate-7 pin: cross-generation resurrection — `Always` durability, two clean drops, deleted gen-1 `pool-b0`'s commit (o1 @ v10) resurrects into the gen-2 re-fork seeded from `default` ≤ v9; live oracle green, recovery diverges | fails at epoch 2 reopen until fixed (`pin_2826_*`) |

The first deepened sweep (seeds 0–199, 2026-07-27) failed 66 seeds across
seven signature families — #2826 (41 seeds, dominant), #2827 (4), and five
families tracked on #2828. Every failing seed reproduces via the replay
contract above. The Phase 4 exit gate ("DST soaks clean across seed
corpora") is open until #2828 closes.

Fault-lane named seeds: 3 (crash perturbation pin), 155 (perturbs
SplitRename/Standard). Grammar changes can silently VACUATE a pinned seed —
every pin asserts its own perturbation/non-vacuity fact, never just "passes".

## Adding to the corpus

1. New grammar arm → re-pin the exact-constant sweeps (`mod.rs` unit tests)
   and the seed-0 histogram, and re-verify each named seed still exercises
   its named behavior (assert the fact, not the pass).
2. New oracle → add its sabotage twin (a planted violation must redden it).
3. New finding → pin `pin_<issue>_*` asserting the CURRENT broken behavior +
   a loud shrink-only sweep allowance; the fix PR promotes the pin to a
   permanent contract and removes the allowance (precedent: #2820 → #2824,
   #2823 → #2825).

## 4.12 hook

`ExpectedState` (per-branch model with `candidate_watermarks`/`mutations_at`)
and the per-run facts are the history substrate the elle-style history
checker (TCP4.12) will consume: every trajectory already yields a totally
ordered op log with acknowledged versions per branch.
