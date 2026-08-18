#!/usr/bin/env python3
"""Instruction-count and binary-size ceilings (TCP5.2).

The perf twin of coverage_floors.py, with the ratchet inverted: these are
CEILINGS on cost, so they only move DOWN (a lowering is committed here
alongside the improvement that earned it), and a raise requires the same
justification a coverage-floor lowering would.

Provenance rules for this gate (they differ from the coverage floors):

- Callgrind instruction counts are HARDWARE-INDEPENDENT — committed,
  dev-box-measured baselines are legal here (the one recorded exception to
  the runner-baseline law; cross-checked against the runner on the PR that
  seeds each ceiling).
- Counts are TOOLCHAIN-DEPENDENT: ceilings are pinned to the workspace
  toolchain in rust-toolchain.toml and regenerate on toolchain bumps.
- Measured 5x run-to-run spread is <=0.017% (six of nine benches
  bit-identical; benchmarks/README.md). TOLERANCE below is therefore
  deliberately conservative; ratchet ceilings toward measured+tolerance as
  CI data accumulates.

Usage:
  perf_floors.py --iai-root benchmarks/target/iai      # bench ceilings
  perf_floors.py --binary target/release/strata        # size ceiling
"""

from __future__ import annotations

import json
import pathlib
import sys

# Ceiling = measured max x (1 + TOLERANCE), rounded up. Measured values in
# trailing comments (toolchain 1.94.1, iai-callgrind 0.16.1).
TOLERANCE = 0.10
RATCHET_HINT = 0.05  # nudge when measured drops >=5% below ceiling/(1+tol)

INSTRUCTION_CEILINGS = {
    "commit_small_batch.steady": 75_939,  # 69,035
    "commit_medium_batch.steady": 1_271_647,  # 1,156,042
    "wal_append_burst.steady": 2_431_283,  # 2,210,257
    "recovery_reopen.two_hundred_commits": 12_895_968,  # 11,723,607
    "kv_put_wire.warmed": 69_226,  # 62,932
    "kv_get_wire.warmed": 19_353,  # 17,593
    "kv_scan_wire.warmed": 1_188_428,  # 1,080,389
    "json_set_wire.warmed": 86_907,  # 79,006
    "json_get_wire.warmed": 30_642,  # 27,856
}

# Release `strata` binary, bytes. Unlike instruction counts this IS mildly
# environment-sensitive (linker, debuginfo), hence the wider 15% band.
BINARY_SIZE_CEILING = 36_730_889  # 31,939,904 (toolchain 1.94.1, linux x86_64; re-baselined for the multi-process IPC epic, #2840-#2846 — see #2900)


def iai_instruction_count(summary: dict) -> int | None:
    """The new-measurement Ir count from an iai-callgrind summary.json."""
    for profile in summary.get("profiles", []):
        for part in profile.get("summaries", {}).get("parts", []):
            # metrics_summary is tool-tagged: {"Callgrind": {"Ir": ...}}.
            tagged = part.get("metrics_summary", {})
            per_tool = next(iter(tagged.values()), {}) if tagged else {}
            metric = per_tool.get("Ir")
            if not metric:
                continue
            metrics = metric.get("metrics", {})
            # Baseline-diff runs carry Both([new, old]); fresh runs carry
            # Left(new) with a bare metric object.
            if "Both" in metrics:
                return int(metrics["Both"][0]["Int"])
            if "Left" in metrics:
                return int(metrics["Left"]["Int"])
    return None


def check_benches(iai_root: pathlib.Path) -> int:
    seen: dict[str, int] = {}
    for summary_path in sorted(iai_root.rglob("summary.json")):
        summary = json.loads(summary_path.read_text())
        name = f"{summary.get('function_name')}.{summary.get('id')}"
        count = iai_instruction_count(summary)
        if count is not None:
            seen[name] = count

    failed = False
    for name, ceiling in INSTRUCTION_CEILINGS.items():
        count = seen.pop(name, None)
        if count is None:
            print(f"::error::perf bench '{name}' produced no summary — "
                  "gate cannot pass vacuously")
            failed = True
            continue
        if count > ceiling:
            print(f"::error::perf ceiling exceeded: {name} = {count} "
                  f"instructions (ceiling {ceiling})")
            failed = True
        elif count < ceiling / (1 + TOLERANCE) * (1 - RATCHET_HINT):
            print(f"note: {name} = {count}, well below ceiling {ceiling} — "
                  "consider ratcheting down")
        else:
            print(f"ok: {name} = {count} (ceiling {ceiling})")
    for name in seen:
        print(f"::error::unexpected bench '{name}' has no committed ceiling")
        failed = True
    return 1 if failed else 0


def check_binary(path: pathlib.Path) -> int:
    size = path.stat().st_size
    if BINARY_SIZE_CEILING is None:
        print(f"note: binary size {size} bytes — ceiling not yet seeded, "
              "record this value in BINARY_SIZE_CEILING")
        return 0
    if size > BINARY_SIZE_CEILING:
        print(f"::error::binary size ceiling exceeded: {size} bytes "
              f"(ceiling {BINARY_SIZE_CEILING})")
        return 1
    print(f"ok: binary size {size} bytes (ceiling {BINARY_SIZE_CEILING})")
    return 0


def main() -> int:
    if len(sys.argv) == 3 and sys.argv[1] == "--iai-root":
        return check_benches(pathlib.Path(sys.argv[2]))
    if len(sys.argv) == 3 and sys.argv[1] == "--binary":
        return check_binary(pathlib.Path(sys.argv[2]))
    print("usage: perf_floors.py --iai-root <dir> | --binary <path>",
          file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
