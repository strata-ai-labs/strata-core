# CLI Corpus Hardening Issues

This file tracks issues found while building and running
`scripts/cli_next_corpus.sh`.

## Fixed Findings

1. KV timestamp reads used to return `kv_value`, while latest reads returned
   `kv_versioned_value`. Fixed by returning `kv_versioned_value` for both
   latest and timestamp KV gets, preserving the historical writer version and
   timestamp.
2. Branch delete responses only reported deletion through
   `data.branch.status == "deleted"`. Fixed by adding top-level
   `deleted == true` and a delete `effect` while preserving branch and cleanup
   details.
3. JSON list prefix matching includes stored-null documents like `doc-null`.
   Fixed as an explicit contract: stored JSON `null` is a live document value,
   so list/count/timestamp reads include it while missing documents remain
   distinct.

## Product Or Contract Findings

1. KV history responses are arrays of version entries, not `{ count, items }`.
2. KV batch-get misses are successful items with `status == "ok"` and
   `result.found == false`, not item status `miss`.
3. Vector history responses are arrays of version entries, not `{ count, items }`.
4. `event list --limit 2` can return a truncated result with
   `has_more == false` and `cursor == null`; event list does not currently
   expose a usable continuation cursor.
5. Event reverse ranges reverse a bounded forward interval, for example
   `event range 0 --end-seq 3 --direction reverse`; `event range 2 --direction
   reverse` does not mean "walk backward from sequence 2".
6. Event chain verification reports `is_valid`, not `valid`.
7. Graph neighbor page items wrap neighbor data under `node` and edge data
    under `edge`; neighbor ids are `item.node.node_id`.
8. Arrow graph export treats the requested output path as a stem and writes
    separate node and edge files, returning their paths in the response. The
    exact requested path may not exist.
9. CLI JSON error envelopes expose registry-style `retry_policy` rather than a
    boolean `retryable`.

## Corpus Harness Fixes

1. `mapfile` is unavailable in the macOS system bash, so the runner uses a
   portable read loop.
