# Strata CLI end-to-end suites

Black-box product tests that drive the real `strata` binary the way a user (or
agent) does: every invocation is a separate process against a throwaway
database, so each read after a write also exercises reopen/recovery.

```bash
cargo build -p strata-cli-next
scripts/cli-tests/run_all.sh          # everything
scripts/cli-tests/02_branch.sh        # one suite
STRATA_BIN=/path/to/strata scripts/cli-tests/run_all.sh
```

Requirements: bash, python3 (JSON assertions). No network, no real home dir —
`STRATA_HOME` is sandboxed per suite.

| Suite | Covers |
|---|---|
| `01_kv` | put/get/delete/list/scan/history/count/sample, cursor round-trips, binary + file/stdin input |
| `02_branch` | set → fork → read both sides, divergence, fork-of-fork, empty roots, fork at version/timestamp, lifecycle |
| `03_space` | space lifecycle, per-space key isolation, spaces × branches, deletion guards |
| `04_json` | path writes/reads, typed values, relaxed parsing, history/tombstones, secondary indexes, branch isolation |
| `05_vector` | collections, upsert/query ordering, metadata filters (documented wire shape), patching, bulk deletes, dimension guards |
| `06_event` | dense sequencing, point reads, forward/reverse ranges, type filtering, hash-chain verification, branch isolation |
| `07_graph` | graph/node/edge lifecycle, weights + properties, neighbor traversal by direction, branch isolation |
| `08_time_travel` | `--as-of` across kv/json/graph/event/vector pinned to real commit timestamps; as-of × branches |
| `09_formats` | human vs `--json` vs `--raw`, base64 wire-truth, error envelopes, `command run/print` (the agent path) |
| `10_durability` | all primitives across reopen, write bursts, history survival, cache-mode volatility, targeting guards |
| `11_errors` | stable error codes + classes (never display text), envelope facts, input guards, exit codes |
| `12_admin` | init (first-run placeholder), ping/info/health/metrics/describe, config reads |
| `13_cross` | identical names across primitives, the branch × space × primitive matrix, mixed workloads on forks |
| `14_inference` | feature-gated surface: absence on default builds, smoke on inference builds |
| `15_first_run` | database-target resolution: explicit path/`--db` → `STRATA_DB` → teaching refusal; never an implicit cwd database |
| `16_agents` | the self-describing surface: `agents guide/commands/errors` and repo onboarding via `agents init` |
| `17_mcp` | `strata mcp serve`: a full stdio JSON-RPC client session — tools, meta-tools, teaching errors, durability, target refusal |

## Known-bug pins

`expect_known_bug` documents a confirmed product defect without hiding it: the
suite stays green and prints a `KNOWN-BUG:` line on every run, and **fails the
day the defect is fixed** so the pin gets promoted to a real assertion.

Current pins:

- `02_branch` · **grandchild inherits the fork's state** — regression from the
  read-path perf campaign: forking a fork silently drops the middle branch's
  inherited state (the grandchild reads `(nil)` where the fork's value should
  show). Silent wrong data — the most severe pin in this ledger. Owned by the
  storage workstream (fork-COW plan's deferred "inherited-source re-clamping").
- `08_time_travel` · **fork as-of a pre-fork commit** — regression from the
  read-path perf campaign (W3.1b retained-timeline index): the fork's timeline
  index lacks pre-fork coverage floor seeding, so `--as-of <pre-fork ts>` on a
  fork fails closed with `history_unavailable.engine.persistence_history`
  instead of reading inherited history. Owned by the storage workstream (the
  fork-COW plan's deferred "timestamp_coverage floor seeding" item).

(The event time-travel domain defect this mechanism was built for —
`event len --as-of` filtering on wall-clock payload timestamps instead of the
commit-timestamp domain — was fixed in engine-next and its pin promoted to
real assertions in `08_time_travel`.)

## Conventions

- Assert on error **codes and classes**, never display text (CLAUDE.md #29).
- Human-output assertions pin the renderer contract; wire assertions use
  `--json` and extract fields with python3.
- `seed` is for fixtures (aborts the suite on failure); `expect_*`/`check_*`
  are the assertions.
