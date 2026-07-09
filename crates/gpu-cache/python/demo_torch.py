"""GT4 exit gate: stock PyTorch consumes the tier with no custom kernels.

Appends pages of fp16 KV-shaped data through the tier (durable in Strata,
hot in VRAM), selects with `topk`, and consumes the materialized pages as a
CUDA tensor via DLPack — then runs stock scaled_dot_product_attention over
them. Asserts byte identity, device residency, ordering via the DLPack
stream contract, and that the tier issued zero host synchronizations on the
decode path.

Run (needs an NVIDIA GPU + a CUDA-enabled torch):
    PYTHONPATH=<dir with strata_tier.so> python demo_torch.py
"""

import tempfile

import torch

import strata_tier

PAGE_BYTES = 64 * 1024
SUMMARY_BYTES = 64
DIM = SUMMARY_BYTES // 4
TOKENS = 128           # fp16 tokens per page: PAGE_BYTES / (heads*dim*2*2)
HEADS = 4
HEAD_DIM = 32          # 128 tokens * 4 heads * 32 dim * 2 (K+V) * 2B = 64 KiB


def page_payload(seed: int) -> tuple[bytes, bytes]:
    """A page of deterministic fp16 K/V data plus an axis-aligned summary."""
    torch.manual_seed(seed)
    kv = torch.randn(2, TOKENS, HEADS, HEAD_DIM, dtype=torch.float16)
    summary = torch.zeros(DIM, dtype=torch.float32)
    summary[seed % DIM] = 8.0
    return kv.numpy().tobytes(), summary.numpy().tobytes()


def main() -> None:
    assert torch.cuda.is_available(), "the demo needs a CUDA torch"
    workdir = tempfile.mkdtemp(prefix="strata-tier-demo-")

    tier = strata_tier.Tier(
        path=workdir,
        space="tier",
        page_bytes=PAGE_BYTES,
        summary_bytes=SUMMARY_BYTES,
        page_slots=8,
        adjacency_degree=8,
    )

    payloads = {}
    for seed in range(4):
        page_bytes, summary = page_payload(seed)
        page_id = tier.append(page_bytes, summary, tags=[seed, 0, 0, 0])
        payloads[page_id] = page_bytes
    tier.maintain()
    receipt = tier.flush()
    assert receipt is not None, "durability point"
    for page_id in payloads:
        assert tier.is_selectable(page_id), f"page {page_id} hot"

    baseline_syncs = tier.sync_calls()

    # Select the two pages whose summaries align with axis 1, best first.
    query = [0.0] * DIM
    query[1] = 4.0
    query[0] = 2.0
    tier.step_begin()
    selection = tier.topk(query, k=2)

    # Stock-torch consumption: from_dlpack passes torch's current stream to
    # __dlpack__, which orders it after the tier's kernels — no host sync.
    pages = torch.from_dlpack(selection.pages())
    assert pages.is_cuda and pages.dtype == torch.uint8
    assert pages.shape == (2, PAGE_BYTES)
    assert tier.sync_calls() == baseline_syncs, "zero syncs on the decode path"

    block_table = torch.from_dlpack(selection.block_table())
    scores = torch.from_dlpack(selection.scores())
    assert block_table.is_cuda and block_table.dtype == torch.int32
    assert scores.is_cuda and scores.dtype == torch.float32

    # Reshape the raw pages into K/V and run *stock* attention.
    kv = pages.view(torch.float16).reshape(2, 2, TOKENS, HEADS, HEAD_DIM)
    keys = kv[:, 0].reshape(2 * TOKENS, HEADS, HEAD_DIM).permute(1, 0, 2)
    values = kv[:, 1].reshape(2 * TOKENS, HEADS, HEAD_DIM).permute(1, 0, 2)
    q = torch.randn(HEADS, 1, HEAD_DIM, dtype=torch.float16, device="cuda")
    out = torch.nn.functional.scaled_dot_product_attention(q, keys, values)
    assert out.shape == (HEADS, 1, HEAD_DIM) and out.is_cuda
    assert not torch.isnan(out).any(), "attention over tier pages is finite"

    # Byte identity: the materialized rows equal the appended payloads, in
    # selection order (page 1 aligned with axis 1 outranks page 0).
    ids = tier.selection_page_ids()  # documented sync: verification only
    assert ids[0] == 1 and ids[1] == 0, f"selection order {ids}"
    host_pages = pages.cpu().numpy().tobytes()
    expected = payloads[ids[0]] + payloads[ids[1]]
    assert host_pages == expected, "VRAM pages == appended bytes, in rank order"

    print("selected page ids (best first):", ids)
    print("scores:", scores.cpu().tolist())
    print("attention output norm:", float(out.float().norm()))
    print("tier stats:", tier.stats())
    print("host syncs (verification readback only; decode path was zero):", tier.sync_calls() - baseline_syncs)
    print("GT4 DEMO PASSED: stock PyTorch consumed the tier with zero custom kernels")


if __name__ == "__main__":
    main()
