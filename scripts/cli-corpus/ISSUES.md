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
4. `event list --limit 2` used to return a truncated result with
   `has_more == false` and `cursor == null`. Fixed by routing event list
   through sequence-cursor pagination and accepting `--cursor`/`--after-sequence`
   for continuation.
5. Event chain verification reported `is_valid`, not `valid`. Fixed by
   serializing the public JSON field as `valid` while still accepting legacy
   `is_valid` during deserialization.
6. Arrow graph export treats the requested output path as a stem and writes
   separate node and edge files. Fixed by documenting that split-stem contract
   in the command/CLI surface and asserting that callers consume the concrete
   `paths` returned in the export response.
7. CLI JSON error envelopes exposed registry-style `retry_policy` without the
   simpler boolean `retryable`. Fixed by serializing `retryable` as a derived
   public field while preserving `retry_policy` for precise client behavior.

## Product Or Contract Findings

1. KV history responses are arrays of version entries, not `{ count, items }`.
2. KV batch-get misses are successful items with `status == "ok"` and
   `result.found == false`, not item status `miss`.
3. Vector history responses are arrays of version entries, not `{ count, items }`.
4. Event reverse ranges reverse a bounded forward interval, for example
   `event range 0 --end-seq 3 --direction reverse`; `event range 2 --direction
   reverse` does not mean "walk backward from sequence 2".
5. Graph neighbor page items wrap neighbor data under `node` and edge data
   under `edge`; neighbor ids are `item.node.node_id`.

## Corpus Harness Fixes

1. `mapfile` is unavailable in the macOS system bash, so the runner uses a
   portable read loop.
