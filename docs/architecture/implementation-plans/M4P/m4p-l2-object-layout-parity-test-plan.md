# M4P-L2 Test Plan: Object Layout Parity

Status: draft test plan

Implementation plan:
`docs/architecture/implementation-plans/M4P/m4p-l2-object-layout-parity-implementation-plan.md`

Parent test methodology:
`docs/architecture/implementation-plans/m4p-storage-next-parity-restoration-test-plan.md`

## Goal

Prove that L2 is the single source of truth for storage-next object names,
prefixes, object families, object roles, and canonical object-name
classification.

Tests should fail if M4P-L2:

1. leaves `manifest/branch-catalog` or `manifest/pending-releases` undocumented
   or untested;
2. lets lifecycle or service code classify table objects with raw string
   grammar;
3. lets durable services construct canonical `wal/`, `tables/`, `snapshots/`,
   `manifest/`, `quarantine/`, `tmp/`, `locks/`, or `meta/` names outside
   `ObjectLayout`;
4. treats backend-private localfs publish temp files as L2 `tmp/` objects;
5. moves L1 backend IO, L3 bytes, L4 service policy, L8 cleanup policy, or L9
   API behavior into L2;
6. weakens existing object-name validation, prefix, ordering, reserved-family,
   or old-name absence tests.

## Audit Finding References

Primary audit source:
`docs/architecture/perf-tuning/storage-next-mechanics-parity-audit.md`

Relevant sections:

1. `L2. Object Layout`
2. `Audit Matrix / Durability, Manifest, WAL, And Recovery Mechanics`
3. `Final Parity Matrix And Architecture-Aligned Gap Plan`

Findings covered by this test plan:

1. L2 documentation drift for `manifest/branch-catalog` and
   `manifest/pending-releases`;
2. table object shape parsing leaks above L2;
3. no CI guard for the L2 naming boundary;
4. missing explicit V1 `tmp/` namespace decision.

## Coverage Boundary

In scope for M4P-L2:

1. object-name and object-prefix validation already owned by `object`;
2. object-family constructor and prefix tests;
3. exact canonical names for all implemented object families;
4. object-role/classification helpers;
5. malformed reserved-family object handling;
6. table object classifier adoption in lifecycle reachability;
7. service parser adoption when service code currently duplicates canonical
   object layout;
8. source guards for raw object construction and raw role parsing;
9. documentation checks by review and, where practical, source tests.

Out of scope for M4P-L2:

1. backend path escaping and filesystem sync tests;
2. durable delete tests;
3. durable format golden vectors;
4. WAL, manifest, snapshot, table, quarantine, checkpoint, or sidecar service
   policy changes;
5. table retention, cleanup, recovery health, or quarantine policy changes;
6. point-read, load, scan, compaction, or maintenance benchmarks;
7. public API conformance tests.

## Old-Code Regression Sources

| Old source | Behavior to preserve | Storage-next assertion |
| --- | --- | --- |
| `crates/storage/src/durability/layout.rs` | One layout owner for WAL, segment, snapshot, manifest, and follower paths. | Storage-next has one object-layout owner for target object names; old filenames remain absent unless explicitly documented as retired evidence. |
| `crates/storage/src/durability/wal/mod.rs` and `crates/storage/src/durability/format/wal_record.rs` | WAL object names were consistently recognized. | WAL object role parsing is L2-owned; WAL service maps L2 facts into L4 errors/policy. |
| `crates/storage/src/durability/format/snapshot.rs` | Snapshot object names were consistently recognized. | Snapshot object role parsing is L2-owned; snapshot service maps L2 facts into L4 errors/policy. |
| `crates/storage/src/manifest.rs` | Manifest object names were stable and centrally known. | `manifest/current`, `manifest/branch-catalog`, `manifest/pending-releases`, and `tables/<branch>/manifest` are constructor- and classifier-tested. |
| `crates/storage/src/quarantine.rs` and `crates/storage/src/segmented/quarantine_protocol.rs` | Quarantine inventory and quarantined object locations were stable and centrally known. | Quarantine inventory component and quarantine object role parsing are L2-owned; quarantine service keeps reconciliation policy. |

Tests must not port:

1. old filesystem paths into storage-next object names;
2. follower-state or follower-audit names;
3. localfs temp-file names into L2;
4. product open behavior or engine error mappings;
5. service or lifecycle policy into layout tests.

## Test Locations

Use:

1. `crates/storage-next/src/layout/tests.rs` for constructor, prefix, ordering,
   role-classification, malformed-shape, and reserved-family tests.
2. `crates/storage-next/tests/object_layout_properties.rs` for L2 source guards.
3. Existing service tests under `crates/storage-next/src/service/` for WAL,
   manifest, snapshot, and quarantine behavior after parser adoption.
4. Existing lifecycle tests under `crates/storage-next/src/lifecycle/tests/` for
   table reachability and table-object retention behavior after parser
   adoption.
5. `docs/architecture/storage/l2-object-layout.md` for documentation
   closeout.

Keep Rust test names behavior-focused. Do not use `M4P`, `L2`, or roadmap slice
labels inside Rust identifiers, comments, fixture bytes, panic messages, or
user-facing text.

## Direct Unit Tests

The lists below are behavior requirements. Rust test names may group closely
related assertions where that keeps the suite maintainable.

### 1. Canonical Constructors And Prefixes

Required behavior:

1. every reserved `ObjectFamily` has a stable string and prefix;
2. `manifest/current` is constructed by `database_manifest`;
3. `manifest/branch-catalog` is constructed by `branch_catalog_manifest`;
4. `manifest/pending-releases` is constructed by `pending_releases_manifest`;
5. `wal/<segment-id>` uses 16-character lowercase fixed-width hex;
6. `meta/wal/<segment-id>` uses the same segment-id encoding;
7. `tables/<branch>/manifest` uses the branch table manifest constructor;
8. `tables/<branch>/l0000/<table-id>` uses the table object constructor;
9. `snapshots/<snapshot-id>` uses 16-character lowercase fixed-width hex;
10. `tmp/<operation-id>/<object-id>` stays under its operation prefix;
11. `quarantine/<branch>/manifest` uses the reserved inventory component;
12. `quarantine/<branch>/<object-id>` rejects the reserved inventory component
    as a quarantine object id if the helper exposes that validation;
13. `locks/writer` and `meta/database` remain reserved names.

Assertions:

1. constructor output strings match the documented canonical layout;
2. each constructor result is a valid `ObjectName`;
3. each prefix result is a valid `ObjectPrefix`;
4. object names generated by one family do not appear under another family's
   prefix;
5. old-storage filenames remain absent from constructor outputs.

### 2. Classification Roundtrip

Required behavior:

1. classifying an object produced by each constructor returns the expected
   object role;
2. classifying a role and reconstructing through `ObjectLayout` returns the
   original object name;
3. family-specific classification returns a nonmatching-family fact for objects
   outside that family;
4. malformed reserved-family objects fail closed.

Table object cases:

1. `tables/<branch>/manifest` is a branch table manifest;
2. `tables/<branch>/l0000/<table-id>` is a table data object;
3. `tables/<branch>` is malformed, not non-table;
4. `tables/<branch>/manifest/extra` is malformed;
5. `tables/<branch>/L0/<table-id>` is malformed;
6. `tables/<branch>/l0000/<table-id>/extra` is malformed;
7. `tables/<branch>/l10000/<table-id>` is malformed because levels must fit the
   documented range and width;
8. `manifest/current` is non-table for table-specific classification.

Manifest cases:

1. `manifest/current` is the database manifest;
2. `manifest/branch-catalog` is the branch catalog manifest;
3. `manifest/pending-releases` is the pending releases manifest;
4. `manifest/current/extra` is malformed;
5. `manifest/unknown` is malformed or unknown according to the chosen helper,
   but it must not silently become a valid manifest role.

WAL and snapshot cases:

1. valid fixed-width lowercase hex IDs parse to numeric IDs;
2. uppercase, short, long, empty, non-hex, and extra-component IDs are
   malformed;
3. object id zero behavior remains caller-policy if existing L4 services reject
   zero. If L2 rejects zero, the plan must record that as an object-name rule.

Quarantine cases:

1. `quarantine/<branch>/manifest` is quarantine inventory;
2. `quarantine/<branch>/<object-id>` is a quarantined object;
3. `quarantine/<branch>` is malformed;
4. `quarantine/<branch>/<object-id>/extra` is malformed;
5. reserved inventory object id is exposed by L2 without service-side `rsplit`.

### 3. Invalid Component And Namespace Tests

Required behavior:

1. constructors reject empty components;
2. constructors reject components containing `/`;
3. constructors reject traversal components `.` and `..`;
4. constructors reject absolute paths, platform path separators, backend URL
   syntax, whitespace, non-ASCII, and oversized names through `ObjectName`
   validation;
5. malformed classifier inputs are represented by layout-owned facts/errors.

Assertions:

1. invalid branch, table, operation, temporary, and quarantine components fail
   before object-name construction;
2. classifier errors do not expose service, backend, lifecycle, or product
   error types;
3. classifier tests cover both invalid components that cannot be constructed by
   `ObjectLayout` and malformed but still syntactically valid `ObjectName`
   values such as `tables/branch/l0000/table/extra`.

### 4. Consumer Behavior Regression

Required behavior:

1. lifecycle table reachability produces the same retention decisions after it
   consumes L2 table classification;
2. table manifests are still excluded from table data retention decisions;
3. malformed table-family objects still fail or defer with the same lifecycle
   outcome as before;
4. WAL listing still sorts by segment id and preserves service-specific invalid
   listed object errors;
5. snapshot listing still sorts by snapshot id and preserves weak-prefix ignore
   behavior;
6. manifest service still maps branch table manifest object names to branch ids
   with the same service error classification;
7. quarantine reconciliation still distinguishes clean inventory, corrupt
   inventory, malformed listed objects, unlisted objects, missing objects, and
   backend-unavailable outcomes.

Assertions:

1. existing lifecycle/service tests remain behaviorally stable;
2. new tests are added only where the old raw parsing had no direct coverage;
3. service and lifecycle tests do not assert L2 internals except through public
   crate-private helper behavior available to that layer.

## Source Guard Tests

The source guard must assert:

1. production code outside `src/layout/` does not contain raw canonical
   reserved-family object names;
2. production service/lifecycle code outside L2 does not classify object roles
   using reserved-family string grammar;
3. production code outside L2 does not call `ObjectName::new` with a raw
   reserved-family literal;
4. production code outside L2 does not use `format!` to assemble canonical
   reserved-family object names;
5. production L3 format code may call `ObjectName::new(decoded_string)` for
   persisted object-name validation;
6. `format/table_manifest.rs` consumes L2 table-object classification for
   canonical table-object shape and keeps only branch/provenance checks in L3;
7. production L1 localfs code may map validated object components to private
   backend paths;
8. `#[cfg(test)]`, `src/*/tests/`, and `src/testkit/` fixtures are skipped.

Seeded guard probes should prove the guard fails on:

1. `ObjectName::new("tables/branch/l0000/table")`;
2. `format!("wal/{segment_id:016x}")`;
3. `object.as_str().starts_with("tables/")`;
4. `object.as_str().ends_with("/manifest")` when used as a table-role rule;
5. `object.as_str().split('/')` followed by matching a reserved family outside
   allowed L2/backend/format contexts;
6. `object.as_str().rsplit('/')` followed by matching a reserved component
   outside allowed L2/backend/format contexts.

Seeded guard probes should prove the guard allows:

1. the same forbidden strings inside a `#[cfg(test)]` module;
2. layout constructors and classifiers;
3. low-level `ObjectName` validation tests;
4. localfs path mapping over already-validated object names;
5. L3 decoding of persisted object-name strings.

## Documentation Proof

Documentation closeout requires review of:

1. `docs/architecture/storage/l2-object-layout.md`;
2. `docs/architecture/storage/l3-durable-format-codec.md` if it enumerates
   manifest-family object names;
3. `docs/architecture/storage/l4-log-manifest-snapshot-services.md` if it
   enumerates manifest-family object names.

Required documentation assertions:

1. implemented canonical layout lists `manifest/current`,
   `manifest/branch-catalog`, and `manifest/pending-releases`;
2. the V1 `tmp/` decision states that current localfs publish temps are
   backend-private and not L2 `tmp/` objects;
3. old-storage filenames are clearly evidence or retired names;
4. no doc implies L2 owns durable bytes, backend IO, cleanup policy, or service
   recovery policy.

## Mode Testing

L2 object layout is backend-independent. The mode proof is narrow:

1. cache and localfs backends consume identical `ObjectName` values from L2;
2. localfs path mapping remains tested in L1/backend tests, not L2 tests;
3. no-default-features and wasm-none-supported builds compile without requiring
   localfs-specific layout behavior;
4. object names do not depend on cache, durable-local standard, durable-local
   always, or future object-durable modes.

Required commands:

1. `cargo check -p strata-storage-next --no-default-features --target wasm32-unknown-unknown --all-targets --locked`
   if implementation touches `object`, `layout`, backend mappings, or source
   guards.

## Generated And Fuzz Testing

M4P-L2 does not require a new fuzz target unless implementation adds a complex
stateful parser. Deterministic property-style tests are sufficient if they cover:

1. sampled valid branch, table, operation, and quarantine components;
2. sampled invalid components;
3. sampled ordered `u64` values for WAL and snapshot IDs;
4. malformed but syntactically valid object names under reserved families;
5. constructor-to-classifier roundtrips.

If a general `classify_object` parser becomes large or stateful, add generated
samples that compare classifier output against an independent simple model in
tests. Do not use production helper output as the only oracle.

## Verification Commands

Narrow verification:

1. `cargo test -p strata-storage-next --locked layout::`
2. `cargo test -p strata-storage-next --locked --test object_layout_properties`
3. `cargo test -p strata-storage-next --locked service::manifest`
4. `cargo test -p strata-storage-next --locked service::wal`
5. `cargo test -p strata-storage-next --locked service::snapshot`
6. `cargo test -p strata-storage-next --locked service::quarantine`
7. `cargo test -p strata-storage-next --locked lifecycle::tests::table_object_retention`

Broad verification:

1. `cargo test -p strata-storage-next --locked`
2. `cargo clippy -p strata-storage-next --lib --features perf-trace -- -D warnings`
3. `cargo check -p strata-storage-next --no-default-features --target wasm32-unknown-unknown --all-targets --locked`
4. `cargo fmt --package strata-storage-next --check`
5. `git diff --check`

Benchmark verification is not required for M4P-L2. If benchmarks are run, they
should be treated as regression smoke tests only; no performance claim should be
made from this slice.

## Sensitivity Probes

Before closeout, run or document manual mutation probes where practical:

1. Change `manifest/branch-catalog` constructor output and confirm layout tests
   fail.
2. Let lifecycle table reachability use `starts_with("tables/")` and confirm
   the source guard fails.
3. Add `ObjectName::new("wal/0000000000000001")` in a production service and
   confirm the source guard fails.
4. Add the same raw name in a `#[cfg(test)]` module and confirm the source guard
   skips it.
5. Change a WAL or snapshot parser to accept uppercase or short IDs and confirm
   role/classifier tests fail.

## Deferral Rules

A finding may be deferred only if the implementation records:

1. exact file and function where raw parsing remains;
2. owner layer;
3. reason it cannot be converted without semantic changes;
4. replacement proof that prevents incorrect cleanup/recovery behavior;
5. later slice that will close it.

Allowed likely deferrals:

1. L3 format modules may keep `ObjectName::new(decoded_string)` validation of
   persisted bytes because L3 owns durable byte decoding.
2. `format/table_manifest.rs` must call L2 table-object classification for
   canonical shape and keep only branch/provenance checks in L3.
3. L1 localfs may keep object-name component splitting for private path mapping
   because backend path translation belongs to L1.
4. Test fixtures and testkit may keep raw canonical names where they are
   intentionally exercising validation or malformed data.

Disallowed deferrals:

1. lifecycle table reachability continuing to classify table roles with raw
   string checks;
2. durable services constructing target canonical object names with `format!`;
3. source guards that only pass because they skip all service/lifecycle code.

## Closeout Requirements

M4P-L2 test closeout requires:

1. constructor and prefix tests cover all implemented object families;
2. explicit tests cover `manifest/branch-catalog` and
   `manifest/pending-releases`;
3. classifier tests cover valid, nonmatching, and malformed objects for every
   family touched by implementation;
4. lifecycle table reachability tests pass after consuming L2 table
   classification;
5. affected service tests pass after consuming L2 role helpers;
6. source guard tests pass and include failing-fixture probes;
7. docs record the manifest object update and V1 `tmp/` decision;
8. no roadmap labels are added to production Rust identifiers, comments,
   fixture bytes, panic messages, or user-visible text;
9. narrow and broad verification commands either pass or are recorded with
   precise skip reasons.
