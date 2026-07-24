# Strata differential corpus format (TCP4.2b)

Committed, oracle-validated regression corpora — the SQLite SLT
"completion mode" adapted to Strata's wire surface. One file per
(capability, seed): `<capability>-<seed>.jsonl`.

## Format (version 1)

JSONL. The first line is the header:

```json
{"format":1,"corpus":"json-0001","capability":"json","seed":1,"ops":250,
 "generator":"differential_json v1","validated_against":"mongodb (record time)",
 "modes":["cache"]}
```

Every following line is one replayable case:

```json
{"op":{"type":"json_set","key":"d0001","path":"$","value":{...}},
 "expect":{"type":"json_write","data":{...}}}
{"op":{"type":"json_get","key":"absent"},"expect_err":"<stable error code>"}
```

- `op` is the exact wire command (the executor `Command` JSON).
- `expect` is the canonicalized output envelope; `expect_err` is the stable
  error code (rule 29: codes, never prose).
- Ops replay IN ORDER against one fresh cache executor per file; outputs
  must match byte-for-byte after canonicalization.

## Canonicalization

Volatile fields are scrubbed recursively before comparison (both at record
and replay): any object field named `timestamp`, `timestamps`,
`recorded_at`, `elapsed_ms`, or `duration_ms` has its value replaced with
`0`. Commit versions are KEPT — the logical commit clock is deterministic
for a fixed op sequence on a fresh database.

## Recording discipline

Corpora are recorded locally (`STRATA_CORPUS_RECORD=1`, differential
feature), never in CI:

1. The generator replays each seed TWICE and requires canonically identical
   outputs — a missed volatile field fails recording, not replay.
2. Shared-contract ops are validated live against the reference engine
   (MongoDB for JSON) at record time; recording refuses to run without it.
3. Files only change when deliberately re-recorded; the replay lane
   (`corpus_replay.rs`, no features, every PR) is the drift net.

## Reuse contract (4.2c/d/e)

New capabilities reuse this format verbatim: same header, same
canonicalizer, same replay runner (it discovers `*.jsonl` and dispatches on
the header's `capability` only for provenance — replay is capability-blind
wire replay). Divergences found at record time are bugs: file first, then
either fix or record the corpus after the fix.
