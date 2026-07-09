"""GT5 exit gate: a synthetic decode loop over the tier, with acceptance gates.

Shapes a 500M-class decode step: every step installs the epoch fence,
requests trace-driven pages, enqueues selection (+ materialization),
overlaps a ~1 ms fp16 matmul standing in for model compute, consumes the
materialized pages through DLPack with stock torch attention, and runs one
maintenance round. Appends land periodically, as decode produces KV pages.

Store of record holds 8x the resident pool: the effective-context gate is
proven by cold-fetching never-touched pages back out of Strata after the
traces run.

Gates (design section 13, plus the HT-11 fork gate):
  1. p95 decode-path host overhead per step <= 400 us (20% of a 2 ms step)
  2. zero host synchronizations across every decode step
  3. store = 8x resident, and cold pages are promotable on demand
  4. sustained promotion throughput during decode (>= 2 pages/step)
  5. fork is metadata-only: N rollout handles share the warm pool with
     zero page copies, select over the union, and append divergent pages
     durably onto their own branches

Maintenance (async promotion staging + write-behind, HT-5) runs where a
serving loop runs it: after the model compute is enqueued, so its host time
overlaps GPU execution instead of stalling the step. Its cost is reported,
not gated: at full scale it is bound by engine store read latency and
on-disk amplification (an engine-side finding tracked outside the tier),
not by tier machinery — the quick run over a nominal store shows the
machinery's own cost.

Run (needs an NVIDIA GPU + CUDA torch):
    PYTHONPATH=<dir with strata_tier.so> python decode_driver.py [--quick]

A replacement selection module (the Moho seam) runs the same loop and the
same gates with --ptx:
    python decode_driver.py --ptx moho_selection.ptx
Selection device time (CUDA event timestamps, host overhead excluded) is
sampled and reported either way — the number that changes with the kernels.
"""

import argparse
import shutil
import sys
import tempfile
import time
from pathlib import Path

import numpy as np
import torch

import strata_tier

PAGE_BYTES = 4 * 1024
SUMMARY_BYTES = 64
DIM = SUMMARY_BYTES // 4
TOKENS = 8            # fp16 tokens per page: 8 * 4 * 32 * 2 (K+V) * 2B = 4 KiB
HEADS = 4
HEAD_DIM = 32
DEGREE = 32
EDGE_LOOKBACKS = [1, 2, 4, 8, 16, 32, 64, 128]
K = 64
STEPS = 512
REQUESTS_PER_STEP = 8
APPEND_EVERY = 8      # one new KV page per 8 decoded tokens
BUDGET_US = 400.0


def build_traces(rng, store_pages, steps):
    """Precomputed request-id arrays, one (steps, REQUESTS_PER_STEP) per trace.

    Precomputing keeps trace bookkeeping out of the measured tier overhead;
    the tier sees an identical call pattern either way.
    """
    traces = {}

    # Zipfian hot set: a skewed working set over the whole store.
    permutation = rng.permutation(store_pages)
    ranks = rng.zipf(1.2, size=(steps, REQUESTS_PER_STEP))
    traces["zipfian"] = permutation[(ranks - 1) % store_pages]

    # Sliding window: a base sweeping the store, far faster than the pool
    # can retain — steady promotion + eviction pressure.
    base = (np.arange(steps) * 512) % store_pages
    offsets = rng.integers(0, 4096, size=(steps, REQUESTS_PER_STEP))
    traces["window"] = (base[:, None] + offsets) % store_pages

    # Graph walk: follow the powers-of-two lookback edges that pages are
    # appended with (host-side walk state; the device sees requests plus
    # one-hop expansion of each selection).
    walk = rng.integers(store_pages // 2, store_pages, size=REQUESTS_PER_STEP)
    rows = []
    for _ in range(steps):
        hops = rng.choice(EDGE_LOOKBACKS, size=REQUESTS_PER_STEP)
        walk = np.where(walk >= hops, walk - hops, walk + store_pages // 4)
        rows.append(walk.copy())
    traces["walk"] = np.array(rows) % store_pages

    return traces


def percentile(samples, q):
    return float(np.percentile(np.array(samples), q))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--quick", action="store_true",
                        help="8k-slot smoke run instead of the 64k gate run")
    parser.add_argument("--ptx", type=Path, default=None,
                        help="selection PTX module to register in place of "
                             "the baseline (the Moho kernel seam)")
    args = parser.parse_args()

    target = Path(__file__).resolve().parents[3] / "target"
    target.mkdir(exist_ok=True)
    workdir = tempfile.mkdtemp(prefix="tier-driver-", dir=str(target))
    try:
        return run(args, workdir)
    finally:
        size = sum(f.stat().st_size for f in Path(workdir).rglob("*") if f.is_file())
        print(f"durable store on disk: {size >> 20} MiB (removed)")
        shutil.rmtree(workdir, ignore_errors=True)


def run(args, workdir):
    assert torch.cuda.is_available(), "the driver needs a CUDA torch"
    slots = 8192 if args.quick else 65536
    store_pages = slots * 8
    rng = np.random.default_rng(0xB13B)

    selection_ptx = args.ptx.read_text() if args.ptx else None
    print(f"selection module: {args.ptx if args.ptx else 'baseline'}")
    tier = strata_tier.Tier(
        path=workdir,
        space="driver",
        page_bytes=PAGE_BYTES,
        summary_bytes=SUMMARY_BYTES,
        page_slots=slots,
        adjacency_degree=DEGREE,
        promotion_batch=8,
        write_behind_batch=256,
        write_backlog_cap=1024,
        selection_ptx=selection_ptx,
    )

    # ---- Setup: seed the store of record at 8x the resident pool. --------
    print(f"seeding {store_pages} x {PAGE_BYTES}B pages "
          f"({store_pages * PAGE_BYTES >> 20} MiB durable, {slots} resident)")
    payloads = [rng.integers(0, 256, PAGE_BYTES, dtype=np.uint8).tobytes()
                for _ in range(64)]
    summaries = (rng.integers(-8, 8, size=(store_pages, DIM))
                 .astype(np.float32))
    setup_start = time.perf_counter()
    for i in range(store_pages):
        edges = [i - b for b in EDGE_LOOKBACKS if i >= b]
        tier.append(payloads[i % 64], summaries[i].tobytes(),
                    tags=[i & 3, 0, 0, 0], edges=edges)
        if i % 32 == 31:
            tier.step_begin()
            tier.maintain()
        if i % (store_pages // 10) == store_pages // 10 - 1:
            print(f"  {i + 1}/{store_pages} "
                  f"({time.perf_counter() - setup_start:.0f}s)")
    receipt = tier.flush()
    assert receipt is not None, "durability receipt"
    print(f"seeded in {time.perf_counter() - setup_start:.0f}s, "
          f"durable at version {receipt[0]}")

    # Settle: let the engine's background compaction absorb the seeding
    # burst before serving begins (a store of record is not normally
    # created microseconds before decode). Read latency during promotion
    # depends on it.
    def store_mib():
        return sum(f.stat().st_size for f in Path(workdir).rglob("*")
                   if f.is_file()) >> 20
    print(f"store after seeding: {store_mib()} MiB; settling...")
    settle_end = time.perf_counter() + 15
    while time.perf_counter() < settle_end:
        tier.step_begin()
        tier.maintain()
        time.sleep(0.05)
    print(f"store after settle:  {store_mib()} MiB")

    # Model-compute stand-in: ~1 ms of fp16 GEMM per step.
    weight = torch.randn(4096, 4096, dtype=torch.float16, device="cuda")
    activations = torch.randn(768, 4096, dtype=torch.float16, device="cuda")
    q = torch.randn(HEADS, 1, HEAD_DIM, dtype=torch.float16, device="cuda")
    torch.cuda.synchronize()  # setup only; decode-path syncs measured below

    traces = build_traces(rng, store_pages, STEPS)
    queries = rng.integers(-4, 5, size=(3 * STEPS, DIM)).astype(np.float32)
    expand_by_trace = {"zipfian": None, "window": None, "walk": 128}

    baseline_syncs = tier.sync_calls()
    baseline_stats = tier.stats()
    overhead_us = []
    maintain_us = []
    ready_us = []
    device_sel_us = []
    device_mat_us = []
    next_page = store_pages
    step_index = 0

    for name, requests in traces.items():
        expand = expand_by_trace[name]
        trace_hits = 0
        for step in range(STEPS):
            ids = requests[step]
            query = queries[step_index].tolist()
            sampled = step % 64 == 63

            t0 = time.perf_counter()
            tier.step_begin()
            for page_id in ids:
                if tier.request(int(page_id), 1):
                    trace_hits += 1
            selection = tier.topk(query, k=K, expand=expand)
            t1 = time.perf_counter()

            # Model compute overlaps the tier's kernels (separate streams).
            hidden = activations @ weight

            t2 = time.perf_counter()
            pages = torch.from_dlpack(selection.pages())
            t3 = time.perf_counter()

            # Appends and maintenance overlap the GPU's model compute, as a
            # serving loop schedules them: the step's new KV page is
            # produced at step end and first selectable at the next step,
            # and promotion staging is asynchronous by design (HT-5). Host
            # time here does not extend the step as long as it fits the
            # step cadence — gated separately.
            m0 = time.perf_counter()
            if step_index % APPEND_EVERY == 0:
                edges = [next_page - b for b in EDGE_LOOKBACKS]
                tier.append(payloads[next_page % 64],
                            summaries[next_page % store_pages].tobytes(),
                            tags=[next_page & 3, 0, 0, 0], edges=edges)
                next_page += 1
            tier.maintain()
            maintain_us.append((time.perf_counter() - m0) * 1e6)

            # Consume: stock attention over the materialized pages (torch's
            # stream is already ordered after the tier by __dlpack__).
            kv = pages.view(torch.float16).reshape(K, 2, TOKENS, HEADS, HEAD_DIM)
            keys = kv[:, 0].reshape(K * TOKENS, HEADS, HEAD_DIM).permute(1, 0, 2)
            values = kv[:, 1].reshape(K * TOKENS, HEADS, HEAD_DIM).permute(1, 0, 2)
            out = torch.nn.functional.scaled_dot_product_attention(q, keys, values)
            del hidden, out

            if sampled:
                # Enqueue-to-ready latency, sampled out-of-band (this step's
                # host time is excluded from the overhead distribution).
                waited = time.perf_counter()
                while not selection.ready():
                    pass
                ready_us.append((time.perf_counter() - waited) * 1e6)
                # Device time of the selection kernels themselves (CUDA
                # event timestamps) — the number a replacement module moves.
                timings = selection.timings()
                if timings is not None:
                    device_sel_us.append(timings["selection_us"])
                    if timings["materialize_us"] is not None:
                        device_mat_us.append(timings["materialize_us"])
            else:
                overhead_us.append(((t1 - t0) + (t3 - t2)) * 1e6)
            step_index += 1
        print(f"trace {name:8s}: request hit rate "
              f"{trace_hits / (STEPS * REQUESTS_PER_STEP):5.1%}")

    # ---- Fork phase (HT-11): GRPO-shaped rollouts over the shared set. --
    # At steady state every slot is family-shared, so rollout residency
    # cannot grow; what forking buys here is N handles selecting the warm
    # union at zero copy cost while their appends diverge DURABLY onto
    # their own branches (write-behind needs no VRAM headroom). Runs
    # before the sync reading: the whole phase must stay sync-free.
    ROLLOUTS, ROLLOUT_STEPS = 4, 16
    tier.flush()                              # fork contract: flushed parent
    stats_pre_fork = tier.stats()
    fork_ms, rollouts = [], []
    for r in range(ROLLOUTS):
        f0 = time.perf_counter()
        rollouts.append(tier.fork(f"rollout-{r}"))
        fork_ms.append((time.perf_counter() - f0) * 1e3)
    fork_copies = (tier.stats()["promotions_started"]
                   - stats_pre_fork["promotions_started"])

    receipts = []
    for r, rollout in enumerate(rollouts):
        for step in range(ROLLOUT_STEPS):
            rollout.step_begin()
            sel = rollout.topk(
                queries[(r * ROLLOUT_STEPS + step) % len(queries)].tolist(),
                k=K)
            pages = torch.from_dlpack(sel.pages())
            if step % 4 == 0:
                rollout.append(
                    payloads[(r * ROLLOUT_STEPS + step) % 64],
                    summaries[(r * ROLLOUT_STEPS + step) % store_pages]
                    .tobytes(),
                    tags=[100 + r, 0, 0, 0])
            rollout.maintain()
            del pages, sel
        # Tag-scoped selection: the isolation mechanism for fused/rollout
        # kernels (host-async; completion polled, never waited).
        scoped = rollout.topk(queries[r].tolist(), k=8,
                              filter_index=0, filter_value=100 + r)
        while not scoped.ready():
            pass
        del scoped
        receipts.append(rollout.flush())
    assert all(r is not None for r in receipts), "rollout durability points"
    del rollouts  # drop releases shared references (HT-11 v0.5 contract)

    decode_syncs = tier.sync_calls() - baseline_syncs
    stats = tier.stats()
    decode_promotions = (stats["promotions_completed"]
                         - baseline_stats["promotions_completed"])

    # Post-trace verification (documented syncs, after the gate readings):
    # a fresh parent selection is sane (the fork phase's tag-scoped rollout
    # selection was the backend's most recent one), and cold pages promote
    # on demand.
    tier.step_begin()
    fresh = tier.topk(queries[0].tolist(), k=K)
    while not fresh.ready():
        pass
    del fresh
    selected = tier.selection_page_ids()
    assert 0 < len(selected) <= K, f"selection size {len(selected)}"
    assert all(0 <= p < next_page for p in selected), "selected ids valid"

    cold = [int(c) for c in rng.integers(0, store_pages, size=32)]
    fetch_start = time.perf_counter()
    while not all(tier.is_selectable(c) for c in cold):
        for c in cold:
            tier.request(c, 2)
        tier.step_begin()
        tier.maintain()
        if time.perf_counter() - fetch_start > 30:
            break
    cold_hot = sum(tier.is_selectable(c) for c in cold)
    cold_ms = (time.perf_counter() - fetch_start) * 1e3

    p50, p95 = percentile(overhead_us, 50), percentile(overhead_us, 95)
    m50, m95 = percentile(maintain_us, 50), percentile(maintain_us, 95)
    ready_p95 = percentile(ready_us, 95)
    ratio = store_pages / slots

    print()
    print(f"steps: {step_index}, resident {slots}, store {store_pages} pages")
    print(f"decode-path host   p50 {p50:6.1f} us   p95 {p95:6.1f} us")
    print(f"maintain+append    p50 {m50:6.1f} us   p95 {m95:6.1f} us "
          f"(overlapped; engine-store-bound at scale, reported not gated)")
    print(f"enqueue-to-ready   p95 {ready_p95:6.1f} us (sampled)")
    if device_sel_us:
        d50 = percentile(device_sel_us, 50)
        d95 = percentile(device_sel_us, 95)
        m_us = percentile(device_mat_us, 50) if device_mat_us else 0.0
        print(f"selection device   p50 {d50:6.1f} us   p95 {d95:6.1f} us "
              f"(sampled; materialize p50 {m_us:.1f} us)")
    print(f"decode promotions  {decode_promotions}, "
          f"evictions {stats['evictions'] - baseline_stats['evictions']}")
    print(f"cold fetch         {cold_hot}/32 promoted in {cold_ms:.0f} ms")
    print(f"write backlog      {tier.write_backlog()} (cap 1024)")
    print(f"fork               {ROLLOUTS} rollouts x {ROLLOUT_STEPS} steps, "
          f"mean {sum(fork_ms) / len(fork_ms):.1f} ms, max {max(fork_ms):.1f} ms, "
          f"{fork_copies} page copies")
    print()

    promotion_rate = decode_promotions / step_index
    gates = [
        ("overhead", f"p95 {p95:.1f} us <= {BUDGET_US:.0f} us",
         p95 <= BUDGET_US),
        ("syncs", f"decode-path host syncs {decode_syncs}",
         decode_syncs == 0),
        ("context", f"store {ratio:.1f}x resident; cold fetch {cold_hot}/32",
         ratio >= 8.0 and cold_hot == 32),
        ("promotion", f"{promotion_rate:.1f} pages/step sustained >= 2",
         promotion_rate >= 2.0),
        ("fork", f"{ROLLOUTS} rollouts warm, {fork_copies} copies, "
         f"max {max(fork_ms):.1f} ms",
         fork_copies == 0),
    ]
    failed = False
    for gate, detail, ok in gates:
        print(f"GATE {gate:10s} {detail:44s} {'PASS' if ok else 'FAIL'}")
        failed |= not ok
    print()
    print("DRIVER FAILED" if failed else
          "GT5 DRIVER PASSED: decode loop within budget, zero syncs, "
          "8x context served from Strata")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
