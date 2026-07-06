# CLI Corpus Hardening Issues

This file tracks issues found while building and running
`scripts/cli_next_corpus.sh`.

## Fixed Findings

1. KV timestamp reads used to return `kv_value`, while latest reads returned
   `kv_versioned_value`. Fixed by returning `kv_versioned_value` for both
   latest and timestamp KV gets, preserving the historical writer version and
   timestamp.

## Product Or Contract Findings

1. Branch delete responses report deletion through
   `data.branch.status == "deleted"` rather than a top-level `deleted` boolean.
2. KV history responses are arrays of version entries, not `{ count, items }`.
3. KV batch-get misses are successful items with `status == "ok"` and
   `result.found == false`, not item status `miss`.
4. JSON list prefix matching includes stored-null documents like `doc-null`;
   stored JSON null and missing are correctly distinct.
5. Vector history responses are arrays of version entries, not `{ count, items }`.
6. `event list --limit 2` can return a truncated result with
   `has_more == false` and `cursor == null`; event list does not currently
   expose a usable continuation cursor.
7. Event reverse ranges reverse a bounded forward interval, for example
   `event range 0 --end-seq 3 --direction reverse`; `event range 2 --direction
   reverse` does not mean "walk backward from sequence 2".
8. Event chain verification reports `is_valid`, not `valid`.
9. Graph neighbor page items wrap neighbor data under `node` and edge data
    under `edge`; neighbor ids are `item.node.node_id`.
10. Arrow graph export treats the requested output path as a stem and writes
    separate node and edge files, returning their paths in the response. The
    exact requested path may not exist.
11. CLI JSON error envelopes expose registry-style `retry_policy` rather than a
    boolean `retryable`.

## Corpus Harness Fixes

1. `mapfile` is unavailable in the macOS system bash, so the runner uses a
   portable read loop.
