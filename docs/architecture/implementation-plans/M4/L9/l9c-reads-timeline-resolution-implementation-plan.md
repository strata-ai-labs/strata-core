# L9C Implementation Plan: Reads And Timeline Resolution

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l9-storage-api-boundary-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L9/l9c-reads-timeline-resolution-test-plan.md`

## Objective

Expose storage reads and timeline resolution through L9.

L9C wraps L6 read views and L7 timeline facts behind stable point, history,
prefix, range, and timeline APIs. It must preserve retained-history and
timestamp-history errors, tombstone facts, TTL behavior, and deterministic scan
ordering without exposing branch-LSM internals.

## Inputs

1. L9A and L9B.
2. `crates/storage-next/src/branch/read.rs`
3. `crates/storage-next/src/branch/state.rs`
4. `crates/storage-next/src/commit/timeline.rs`
5. `crates/storage-next/src/lifecycle/`
6. `docs/architecture/engine/temporal-context-and-timeline-resolver-contract.md`
7. `crates/storage/src/traits.rs`

## Scope

L9C implements:

1. latest point read;
2. point read at commit version;
3. point read at timestamp;
4. retained history read;
5. prefix scan;
6. range scan;
7. scan limits and ordering;
8. timestamp-to-version lookup;
9. version-to-timestamp lookup;
10. retained timeline bounds;
11. read-view pin or equivalent retention-safe read selector when needed;
12. boundary read errors.

L9C does not implement:

1. product time-travel UX;
2. primitive decoding;
3. query planning;
4. index/search/vector reads;
5. commit mutation.

## Read Selectors

Selectors:

1. branch;
2. storage space;
3. key or key range;
4. bound: latest, at version, at timestamp;
5. limit;
6. include tombstone facts option if needed;
7. consistency/pin token if needed.

All selectors validate before lower-layer reads.

## Outcome Shape

Read outcomes should expose:

1. key;
2. value bytes or none;
3. visible commit version;
4. commit timestamp if available;
5. tombstone fact;
6. TTL/expiration fact if relevant;
7. retention-bound facts when a miss is due to history unavailability;
8. branch generation observed if needed for engine adapters.

Do not expose `StorageRow`, table IDs, read candidate source ordering, or L6
row-source internals.

## Timeline Semantics

Timestamp lookup rule:

1. find newest commit at or before the requested timestamp;
2. if multiple commits share the timestamp, choose greatest commit version;
3. if retained history does not cover the requested timestamp, return a
   timestamp-history error, not not-found;
4. cache mode uses in-memory timeline facts and reports cache durability facts.

Version lookup returns the recorded commit timestamp or a retained-history error
if the version is not retained.

## Scan Semantics

Prefix/range scans must:

1. be deterministic;
2. return key order defined by storage key bytes;
3. apply the same bound semantics as point reads;
4. enforce limits without changing ordering;
5. preserve tombstone visibility rules;
6. preserve inherited-layer and materialized-row visibility through L6.

## Verification

```bash
cargo fmt --package strata-storage-next --check
cargo test -p strata-storage-next --locked --lib api
cargo test -p strata-storage-next --locked --test api_conformance
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
```
