# Arrow conformance — scenario matrix and coverage ledger

Bug #3063 (`arrow import --target json` silently storing an Arrow `Display`
string instead of a JSON document) slipped through because the Arrow tests were
one-test-per-target happy paths, not a matrix — whole dimensions (formats,
value types, edge cases) were untested and nobody could see the holes. A
follow-up review (2026-09-04) then found ~9 more issues of the same class
(#3077–#3083, #3075, #3073).

This document + the machine-checked ledger make Arrow coverage a **tracked
map**: what is tested, what is a known gap (with its issue), and what is
intentionally out of scope — enforced so it can't silently rot again.

## The scenario space

Arrow import/export is the cross product of several dimensions. A "cell" is one
scenario in that space.

- **Direction:** import, export, and **round-trip** (export → file → import,
  asserting identity — the strongest fidelity check).
- **Target / primitive:** `kv`, `json`, `vector`, `event`, `graph` (5).
- **Format:** `csv`, `jsonl`, `parquet` (3). **IPC (`.arrow`) is not supported** —
  `ArrowFileFormat = {Csv, Jsonl, Parquet}`; `.arrow` is cleanly rejected.
- **Value fidelity:** every Arrow type → the correct stored value, round-trip
  identical: scalars (int/uint/float/bool/utf8/binary/null), nested (struct,
  list, large-list, fixed-size-list, map), and temporal/decimal/dictionary.
- **Edge cases:** null cells, empty batch / empty database, unicode, delimiters
  embedded in values (CSV quoting), numeric extremes (NaN/Inf, u64 > f64),
  multi-batch, duplicate keys, encoding hints (`*_encoding` columns).
- **Error paths:** missing column, bad/unknown format, unreadable file,
  feature-disabled, type/schema mismatch — each a *typed* error, not a panic or
  a silent success.
- **Silent-success:** the worst class — an import that reports `rows_imported`
  success while dropping, mangling, or not storing data (#3063's shape).

Not every point in the raw cross product is meaningful; the ledger enumerates
the ones that are.

## The ledger

`crates/executor/tests/arrow_conformance_ledger.yaml` lists one entry per cell:

```yaml
- id: roundtrip.kv.jsonl
  scenario: KV export -> JSONL -> import preserves bytes
  status: covered           # covered | gap | accepted
  test: csv_kv_import_and_jsonl_export_round_trip_bytes
```

- `covered` → `test:` names the `#[test] fn` that exercises it.
- `gap` → `issue:` names the tracking issue (a known, filed hole).
- `accepted` → `note:` states why it is intentionally not covered (e.g. IPC
  unsupported, or a by-design non-round-trippable field).

## The lint

`crates/executor/tests/arrow_conformance.rs::arrow_conformance_ledger_is_consistent`
runs on every PR (plain integration test, no feature gate) and enforces:

- every `covered` cell names a `test:` that exists as a `#[test] fn` in an arrow
  test source (`arrow_behavior.rs`, `arrow_disabled_behavior.rs`,
  `arrow/schema.rs`, `arrow_conformance.rs`);
- every `gap` cell names an `issue:`;
- every `accepted` cell carries a `note:`;
- no duplicate cell ids.

It prints the coverage summary (`N covered / M gap / K accepted`) so the state
is visible in CI output.

## Working with it

- **Fixing a gap:** land the fix + a test, then flip its cell `gap → covered`
  and point `test:` at the new fn — in the same PR. (This is the pin-promotion
  discipline the storage program uses, applied to Arrow.)
- **New scenario:** add a cell. If untested, `gap` + a filed issue.
- **New Arrow test:** add or update its cell so the ledger stays complete.

## Current state (2026-09-04, at framework standup)

**17 covered / 15 gap** — the covered set is the pre-existing per-target
round-trips + error paths + the #3063 struct/list conversion fix. The 15 gaps
are the review findings, each carrying its issue:

| Area | Gap cells → issue |
|---|---|
| CSV round-trip type corruption | #3077 |
| Float NaN/Inf → null | #3078 |
| `*_encoding` beyond base64 | #3079 |
| 100-row schema inference | #3080 |
| event/graph silent over-report | #3081 |
| vector → CSV broken | #3082 |
| temporal/decimal + Map/LargeList/FixedSizeList conversion | #3075 |
| parquet round-trips, edge cases, error-list accuracy, null flattening | #3083 |

As each fix lands its cell flips to `covered`; the lint guarantees the map stays
truthful.
