# M4P-L8K Implementation Plan: Load-Performance Completion

Status: draft

Closes out the durable-load performance gap identified and scale-validated in the M4P
investigation. Builds on the durability foundation landed in **L8J** (delta checkpoint +
unified recovery).

## Context

Durable `StorageMode::DurableLocal` standard sequential load is ~4–8× slower than the old
engine. The scale-validated root cause is **not** the runtime lock per se — it is the
**admission throttle**, which over-throttles the writer on normal pressure and drives a
flush-fragmentation feedback loop. Same-environment measurements (throttle disabled,
`load-seq`, value=150B, batch=1000):

| Scale | baseline (throttle on) | throttle off | old engine | throttle-off vs baseline | `final_l0` (throttle off) |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1M | 13.0s | **2.86s** | 2.94s | **4.5×** | 4 |
| 5M | ~89s | **26.78s** | 16.84s | **3.3×** | 13 |
| 10M | 295.6s | **40.24s** (load) | 36.78s | **7.3×** | 83 |

Two facts drive this plan:

1. **The throttle is the dominant lever.** Disabling the escalating "urgent" slowdown
   recovers 4.5–7.3× and reaches old-engine parity at 1M. The throttle's per-commit sleeps
   (up to 25ms, escalating) fire at *Urgent* severity — i.e. on normal active-fill /
   transient-frozen pressure — and, by repeatedly yielding the runtime lock mid-load, let
   the background flush rotate a *small* active memtable into many tiny L0 tables
   (fragmentation), which raises pressure and escalates the throttle further.
2. **Backlog bounding is scale-coupled.** With the throttle off, `final_l0` grows with
   scale (4 → 13 → 83) because the lock-serialized maintenance publish makes compaction
   fall behind at 10M. So relaxing the throttle safely (bounded L0) at scale **requires**
   faster draining, i.e. off-lock publish. The two work together: throttle relaxation is
   the headline win; off-lock publish keeps the backlog bounded at scale.

## Prerequisite (landed): L8J — checkpoints handle large state

The throttle cannot be relaxed until checkpoints survive a large backlog. L8J delivered:
J0 encode-time snapshot ceiling, J1 unified recovery (manifest owned levels + checkpoint
delta + WAL), J2 bounded delta checkpoint (active+frozen only) + the empty-delta watermark
advance. Result: a checkpoint is O(in-memory backlog), not O(database size), and a large
backlog no longer crashes the checkpoint. **This slice depends on L8J being in.**

## Slices

| Slice | Work | Headline gate |
| --- | --- | --- |
| **K1 — admission throttle relaxation** (LANDED) | Remove the escalating *Urgent* slowdown, leaving the *Blocking* admission wait-loop (L8H, progress-gated, `admission_wait_timeouts==0`) as the sole backpressure. (The planned background-rotate size-gate, K1a, was dropped — redundant once the throttle is gone, and it broke idle-flush; see K1a below.) | 1M/5M durable standard reach old-engine parity; no commit rejections; `admission_wait_timeouts==0`. (L0 bounding at 5M moved to K2.) |
| **K2 — off-lock publish (= L8I Group C)** | Move the maintenance publish off the global runtime lock: `ArcSwap` version install + off-lock manifest/checkpoint persist under a per-branch publish lock with manifest-sequence reservation; gate WAL-truncation/flush-watermark on durable persist. | Publish holds the runtime lock only for the pointer swap; compaction keeps up at 10M so L0 stays bounded with the throttle relaxed; 10M durable standard ≤ ~2× old at quiescence; crash-consistency suite green. |

Order: **K1 first** (biggest, lowest-risk, no durability/format change), then **K2** (the
durability-critical locking work that bounds the backlog at scale). K2 carries the full
contention map and old-engine blueprint already drafted in
`m4p-l8i-runtime-lock-decoupling-implementation-plan.md` (Group C); this plan re-scopes it
as the backlog-bounding enabler rather than a standalone lever.

## K1 — detail

### K1a. Background-rotate size-gate — DROPPED (not landed)
- **Dropped during implementation.** A pure size-gate on
  `start_next_background_flush_maintenance` (`lifecycle/durable/maintenance.rs:~1237`,
  `active_row_count() > 0` → size threshold) is correct under load but **breaks idle-flush**:
  a sub-threshold active (small workload, or the tail after any load) never rotates, never
  flushes, never advances the flush watermark, and leaves residual `ActiveMutableBytes`
  pressure — the database never reaches a clean fixed point. This regressed
  `lifecycle_background_closed_loop_scaled_durable_bounds_wal_without_public_drain`
  (pressure `Background`/`ActiveMutableBytes` instead of `None` at quiescence).
- It is also **redundant for the perf win**: with the throttle gone (K1b) the fast writer
  fills the active before the background can fragment it, so old-engine parity is reached at
  1M and 5M without the size-gate (see outcome below). The "269 tiny flushes" Exp C/D observed
  were a *throttle-on* artifact (the slowdown's lock-yielding gave the background repeated
  turns to rotate small actives); removing the throttle removes the cause.
- Cost of dropping it: `final_l0` at 5M is **43** rather than the size-gated ~13 — a
  read-amplification residual (compaction falling behind the un-throttled writer), not a
  load-time regression. **Bounding L0 at scale moves to K2** (off-lock publish → compaction
  keeps up). A future idle-flush-aware size-gate (rotate when full OR when the active is the
  last idle work, i.e. frozen empty and L0 below the compaction threshold) could bound L0 in
  the foreground path, but is deferred.

### K1b. Throttle relaxation
- The escalating slowdown lives in `BackgroundAdmissionThrottle::observe_urgent`
  (`api/runtime.rs`), driven by `DEFAULT_BACKGROUND_URGENT_BASE_SLOWDOWN` (100µs) /
  `_MAX_SLOWDOWN` (25ms) and applied in `background_admission_slowdown` when severity is
  `Urgent` and not relieving. **Stop slowing the writer at `Urgent`.** Backpressure is then
  carried solely by the `BlockMutatingAdmission` path — which, post-L8H, *paces* the writer
  on real maintenance progress (the progress-gated wait-loop) rather than rejecting, with
  `admission_wait_timeouts==0` for a serviceable overload and a bounded typed failure only
  for a dead executor.
- **Landed: the `Urgent` slowdown was deleted entirely** — the escalating per-commit sleep,
  the `BackgroundAdmissionThrottle` machinery + `sleep_background_duration`, the four
  `admission_slowdown`/`admission_throttle_*` perf counters, and the benchmark fields that
  read them. The `Blocking` admission wait-loop is now the sole writer-pacing backpressure.
- Preserve: the L8H liveness watchdog semantics (serviceable overload never times out; dead
  executor → one timeout + typed `failed_precondition.storage_api.storage_pressure`); cache
  mode (no admission); deterministic-simulation clock boundary.

### K1 landed — measured outcome
Same-env `storage-next-l9-scale --engines standard --workloads load-seq` (value=150B,
batch=1000) after K1b:

| Scale | K1b | old engine | throttle-on baseline | `final_l0` | `admission_wait_timeouts` | rejections |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 1M | **2.98s** | 2.94s | 13.0s | 4 | 0 | 0 |
| 5M | **17.17s** | 16.84s | ~89s | 43 | 0 | 0 |

Load-time parity at 1M and 5M (4.4×/5.2× over the throttle-on baseline), well inside the
≤1.6× gate, with zero wait-timeouts and zero rejections. Full `--all-features` suite green.

- **L0 not tightly bounded at 5M/10M (K2's gate).** With the slowdown gone and the size-gate
  dropped, the writer outpaces lock-serialized compaction: `final_l0` is 43 at 5M and ~83 at
  10M. Load time is unaffected. **K2 (off-lock publish) owns L0 bounding at 5M and 10M** — a
  widening of K2's original 10M-only L0 charter. Do **not** claim tight L0 bounding on K1.

## K2 — detail (off-lock publish)
Per `m4p-l8i-...-implementation-plan.md` Group C, with this framing: the lock-serialized
publish (manifest persist held under the global mutex) is what makes compaction fall behind
at 10M. Moving the version install to `ArcSwap` and the manifest/checkpoint persist off-lock
(under a per-branch publish lock + manifest-sequence reservation) lets compaction drain L0
fast enough that the `Blocking` admission keeps L0 bounded with the throttle relaxed.
Durability-critical: requires the crash-between-swap-and-persist recovery test and
per-branch serialization to prevent durable manifest sequence regression. (Note: J1 unified
recovery + the flush-watermark proof already cover the widened swap→fsync window on the read
side.)

## Verification
- **K1**: settle-to-quiescence `storage-next-l9-scale --engines standard --workloads load-seq`
  at 1m/5m vs `storage-old-cache-scale --engine standard`. Gates: parity at 1M, ≤~1.6× at 5M;
  `admission_wait_timeouts==0`; zero commit rejections; `final_l0` bounded (≤ blocking
  threshold) at 1M/5M; the full `cargo test -p strata-storage-next --all-features` green
  (especially `commit_hardening`, `compaction`, `recovery`, `flush_watermark`,
  `crash_recovery`, `lifecycle_faults`); fmt + clippy `-D warnings`.
- **K2**: add 10m to the sweep; gate 10m ≤ ~2× old at quiescence with `final_l0` bounded;
  crash-consistency suite (crash between pointer-swap and manifest persist recovers ==
  synchronous baseline); no durable manifest sequence regression under concurrent
  same-branch flush∥compaction; format goldens unchanged.

## Out of scope / sequencing notes
- L8I Group A (admission wait-loop park-until-relief) is abandoned (measured net-negative);
  see the L8I plan. Do not revive it.
- Per-compaction parallelism / per-branch sharding (L8I Groups D/E) remain optional, only if
  K1+K2 do not reach the 2× target at 10M.
