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
8. KV history responses were bare arrays of version entries. Fixed by returning
   present history as `{ count, items }`, while preserving `data: null` for
   missing keys.
9. KV batch-get misses were successful wrapper items with `status == "ok"` and
   only `result.found == false`. Fixed by adding shared item status `miss` and
   using it for valid KV batch-get misses while preserving the primitive
   `found: false` payload.
10. Vector history responses were bare arrays of version entries. Fixed by
    returning present history as `{ count, items }`, while preserving
    `data: null` for missing vector keys.
11. Event reverse ranges reversed a bounded forward interval. Fixed by making
    reverse sequence ranges walk backward from `start_seq`, with `end_seq` as
    an exclusive lower bound when provided.
12. Graph neighbor page items exposed neighbor ids only as
    `item.node.node_id`. Fixed by adding top-level neighbor identity fields to
    each hit while keeping nested node and edge details.
13. The corpus runner originally relied on `mapfile`, which is unavailable in
    the macOS system Bash. Fixed by collecting discovered scenario paths with
    a portable read loop.

## Product Or Contract Findings

No open product or contract findings.

## Corpus Harness Fixes

No open corpus harness findings.
