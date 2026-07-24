#!/usr/bin/env python3
"""Per-crate PRODUCT-ONLY coverage floor gate (TCP3.0, Phase 3 Tier-2 tracker).

Reads a per-file `cargo llvm-cov report` table and computes per-crate line
coverage over PRODUCT code only: `src/testkit/`, `test_support`, `tests/`
trees and `testkit.rs` files are excluded, because counting test
infrastructure as uncovered product code understates the crates that carry
the heaviest test estates (storage measures 70% raw but 87% product-only).

Ratchet discipline: floors only move up, never down, and a raise is
committed here alongside the coverage that earned it. When a crate clears
its floor by more than RATCHET_HINT points, the gate prints a reminder to
raise the floor (non-fatal).

Usage: cargo llvm-cov report | python3 scripts/coverage_floors.py
   or: python3 scripts/coverage_floors.py <report-file>
"""

import re
import sys

# Floors set 2026-07-17 from the Phase 3 baseline measurement, ~0.5-1.5pt
# below measured to absorb refactor noise. Measured values in comments.
# Floors gate the NIGHTLY RUNNER's measurement — baselines must come from a
# runner (or runner-equivalent) environment, not a dev machine.
FLOORS = {
    "core": 94.0,       # 94.7
    "inference": 91.0,  # 92.0
    "storage": 86.5,    # 87.3
    "executor": 85.0,   # 85.8
    "engine": 82.0,     # 82.9
    "hub": 70.0,        # 70.6
    # 54.8 on the GPU-less nightly runner, stable across every night since
    # the gate first ran. The original 58.0 floor (from a 59.4 measurement)
    # was taken on a GPU-equipped dev box where dlopen("libcuda.so.1")
    # succeeds, so ~140 driver-load/context lines counted as covered that
    # are structurally unreachable on runners — the gate never once passed.
    # Corrected 2026-07-23 (#2764): a baseline fix, not a ratchet-down.
    "gpu-cache": 54.0,  # 54.8 (host-sim + CPU paths; driver/CUDA paths are hardware-gated)
    "cli": 46.5,        # 47.3
}

# wasm is deliberately absent: its tests execute on wasm32-unknown-unknown
# (ci.yml wasm job); host llvm-cov cannot observe them, so a host floor
# would gate on a number that is structurally zero.

EXCLUDE = re.compile(r"/testkit/|/tests/|test_support|testkit\.rs$")
CRATE = re.compile(r"(?:^|/)crates/([A-Za-z0-9_-]+)/src/|^([A-Za-z0-9_-]+)/src/")
RATCHET_HINT = 1.5


def main() -> int:
    source = open(sys.argv[1]) if len(sys.argv) > 1 else sys.stdin
    totals: dict[str, list[int]] = {}
    for line in source:
        parts = line.split()
        if len(parts) < 10 or parts[0] in ("Filename", "TOTAL") or line.startswith("-"):
            continue
        path = parts[0]
        if EXCLUDE.search(path):
            continue
        match = CRATE.search(path)
        if not match:
            continue
        crate = match.group(1) or match.group(2)
        try:
            lines, missed = int(parts[7]), int(parts[8])
        except ValueError:
            continue
        bucket = totals.setdefault(crate, [0, 0])
        bucket[0] += lines
        bucket[1] += missed

    failures = []
    print(f"{'crate':<12}{'product line %':>16}{'floor':>9}")
    for crate, floor in sorted(FLOORS.items(), key=lambda kv: kv[1]):
        if crate not in totals:
            failures.append(f"{crate}: no coverage rows found (report format drift?)")
            continue
        lines, missed = totals[crate]
        pct = 100.0 * (1 - missed / lines) if lines else 0.0
        marker = ""
        if pct < floor:
            marker = "  << BELOW FLOOR"
            failures.append(f"{crate}: {pct:.1f}% < floor {floor:.1f}%")
        elif pct > floor + RATCHET_HINT:
            marker = f"  (ratchet hint: raise floor toward {pct:.1f}%)"
        print(f"{crate:<12}{pct:>15.1f}%{floor:>8.1f}%{marker}")

    if failures:
        for failure in failures:
            print(f"::error::product-only coverage floor violated — {failure}")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
