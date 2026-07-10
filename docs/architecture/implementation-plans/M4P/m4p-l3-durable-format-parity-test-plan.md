# M4P-L3 Test Plan: Durable Format Parity

Status: draft test plan

Implementation plan:
`docs/architecture/implementation-plans/M4P/m4p-l3-durable-format-parity-implementation-plan.md`

Parent test methodology:
`docs/architecture/implementation-plans/m4p-storage-next-parity-restoration-test-plan.md`

## Goal

Prove that every storage-next durable byte format is owned, specified,
strictly decoded, golden-tested, and reachable through the L3 fuzz/testkit
decoder surface.

Tests should fail if M4P-L3:

1. leaves durable payload codecs in lifecycle or service modules;
2. changes checkpoint row-section or retained-history extension bytes while
   claiming a no-format-impact slice;
3. leaves branch catalog or pending releases manifest fixtures unasserted by
   normal tests;
4. leaves implemented L3 decoders unreachable by fuzz/testkit routing;
5. lets lifecycle, service, backend, branch, or API policy leak into L3;
6. weakens strict corruption behavior, trailing-data rejection, checksum
   validation, fixed-length validation, or future/pre-V1 version handling.

## Audit Finding References

Primary audit source:
`docs/architecture/perf-tuning/storage-next-mechanics-parity-audit.md`

Relevant sections:

1. `L3. Durable Format / Codec`
2. `Audit Matrix / Durability, Manifest, WAL, And Recovery Mechanics`
3. `Final Parity Matrix And Architecture-Aligned Gap Plan`

Findings covered by this test plan:

1. checkpoint row-section payload bytes are currently encoded/decoded in
   `lifecycle/recovery.rs`;
2. retained-history extension bytes are currently encoded/decoded in
   `lifecycle/retained_history_extension.rs`;
3. branch catalog and pending releases manifest formats need spec/default
   golden coverage;
4. fuzz/testkit routing is incomplete;
5. V1 identity codec behavior needs an explicit boundary proof.

## Coverage Boundary

In scope for M4P-L3:

1. durable byte codecs;
2. format magic/version/header/length/tag/flag validation;
3. `FormatError` classification;
4. stable fixture bytes and golden assertions;
5. decode corruption tests;
6. fuzz/testkit decoder routing;
7. source guards that keep byte codecs in `format/`;
8. documentation of V1 format fields and codec boundary decisions.

Out of scope for M4P-L3:

1. backend IO and object publish/delete behavior;
2. object-name layout decisions;
3. WAL replay policy;
4. manifest update/publish policy;
5. checkpoint scheduling and recovery install policy;
6. branch LSM, compaction, and retention policy;
7. public API behavior;
8. old-format compatibility;
9. encryption/key-management implementation;
10. benchmark performance claims.

## Old-Code Regression Sources

| Old source | Behavior to preserve | Storage-next assertion |
| --- | --- | --- |
| `crates/storage/src/durability/format/wal_record.rs` | WAL headers/records had explicit versions, framing, and CRC checks. | V1 WAL codec remains strict, self-delimiting, and CRC-checked. |
| `crates/storage/src/durability/format/manifest.rs` | Database manifest bytes were storage-owned physical metadata. | V1 database manifest codec remains in L3 and golden-tested. |
| `crates/storage/src/durability/format/snapshot.rs` | Snapshot containers used explicit section envelopes and footer CRC. | V1 snapshot container and row-section payloads are L3-owned and strictly decoded. |
| `crates/storage/src/durability/format/segment_meta.rs` and `watermark.rs` | Sidecar/watermark bytes were storage-owned. | Segment metadata and watermark codecs stay L3-owned and fuzz-routed. |
| `crates/storage/src/key_encoding.rs` | Internal keys preserve physical-key ascending and commit-version descending order. | `format/key.rs` ordering/golden tests continue to pass. |
| `crates/storage/src/segment_builder.rs` and `segment.rs` | Immutable tables had block framing, compression, and corruption checks. | `format/table/` tests continue to cover table artifact bytes, block CRCs, ordering, compression, and trailing data. |

Tests must not port:

1. old development format versions;
2. old table format bytes;
3. primitive product snapshot DTOs;
4. old MessagePack writesets;
5. AES-GCM implementation;
6. product semantics into L3.

## Test Locations

Use:

1. `crates/storage-next/src/format/tests.rs` for shared/default golden tests and
   broad format harness assertions.
2. `crates/storage-next/src/format/snapshot/tests.rs` or a sibling module for
   checkpoint row-section payload tests.
3. `crates/storage-next/src/format/table_manifest/tests/` or a dedicated
   retained-history format test module for retained-history extension payload
   tests.
4. `crates/storage-next/src/format/branch_catalog_manifest.rs` tests for branch
   catalog strict decode cases.
5. `crates/storage-next/src/format/pending_releases_manifest.rs` tests for
   pending releases strict decode cases.
6. `crates/storage-next/src/format/fuzzing.rs` plus its route tests, if present,
   for decoder-routing coverage.
7. `crates/storage-next/src/lifecycle/tests/recovery.rs` for unchanged recovery
   behavior after checkpoint row-section codec migration.
8. `crates/storage-next/src/lifecycle/tests/compaction/row_pruning.rs` and
   related retained-history tests for unchanged lifecycle behavior after
   extension codec migration.
9. `crates/storage-next/tests/` for source guards.
10. `docs/spec/strata-storage-format-v1.md` for spec closeout.

Keep Rust test names behavior-focused. Do not use `M4P`, `L3`, or roadmap slice
labels inside Rust identifiers, comments, fixture bytes, panic messages, or
user-facing text.

## Direct Unit Tests

### 1. Checkpoint Row-Section Payload

Required behavior:

1. valid empty row section encodes and decodes;
2. valid multi-row section encodes storage rows with u32 row lengths;
3. encoded section kind remains `1`;
4. payload magic remains `STRR`;
5. payload version remains `1`;
6. decoder rejects insufficient header bytes;
7. decoder rejects invalid magic;
8. decoder rejects future version;
9. decoder rejects row count larger than possible length-prefixed rows;
10. decoder rejects truncated row length;
11. decoder rejects truncated row bytes;
12. decoder rejects invalid nested storage-row bytes;
13. decoder rejects trailing bytes after declared rows;
14. row ordering is preserved exactly.

Assertions:

1. old lifecycle test fixtures continue to decode to the same `StorageRow`
   values;
2. `FormatError` is returned by the L3 decoder before lifecycle maps errors;
3. L3 tests do not assert recovery install behavior.

### 2. Retained-History Extension Payload

Required behavior:

1. valid payload with timestamp floor round-trips;
2. valid payload without timestamp floor round-trips;
3. encoded payload length remains 24 bytes;
4. extension kind remains `storage.retained_history`;
5. `preserve_on_rewrite` remains true when building the extension section;
6. decoder rejects short payloads;
7. decoder rejects long payloads;
8. decoder rejects unknown timestamp flag values;
9. decoder rejects nonzero reserved bytes;
10. timestamp flag `0` handling is explicitly documented and tested: either it
    rejects nonzero timestamp bytes as strict V1, or it accepts and ignores them
    as a recorded development-format compatibility behavior;
11. timestamp flag `1` preserves timestamp micros exactly;
12. retained version floor preserves its `u64` value exactly.

Assertions:

1. lifecycle tests still prove when the extension is emitted;
2. L3 tests only prove bytes and decoded facts;
3. L3 does not import `BranchTimestampCoverage`.

### 3. Manifest-Family Metadata Formats

Required behavior:

1. branch catalog empty, single-active, active/deleted, and parented fixtures
   decode and re-encode to identical bytes;
2. pending releases empty, single, and multi-entry fixtures decode and re-encode
   to identical bytes;
3. branch catalog rejects bad magic;
4. branch catalog rejects pre-V1 or future versions according to the codec's
   existing policy;
5. branch catalog rejects zero generation;
6. branch catalog rejects unsorted entries;
7. branch catalog rejects invalid status values;
8. branch catalog rejects invalid parent relationships if the codec owns that
   byte-level rule;
9. pending releases rejects bad magic;
10. pending releases rejects pre-V1 or future versions according to the codec's
    existing policy;
11. pending releases rejects zero sequence;
12. pending releases rejects unsorted entries;
13. pending releases rejects empty released table identities;
14. pending releases rejects trailing data;
15. both codecs reject oversized counts before unbounded allocation.

Assertions:

1. default tests read fixture files from
   `crates/storage-next/testdata/goldens/storage-format-v1/`;
2. ignored golden emitters are not the only proof;
3. spec text names every field asserted by tests.

### 4. V1 Identity Codec Boundary

Required behavior:

1. V1 accepts only the identity codec id currently documented by each format;
2. unsupported codec ids produce `FormatError` or service errors derived from a
   `FormatError`, depending on where the existing boundary sits;
3. service-local no-op application is documented or replaced by an L3 helper;
4. WAL, snapshot, and table code do not diverge into separate codec switches.

Assertions:

1. codec-boundary tests do not add encryption support;
2. tests prove unsupported codec behavior without requiring keys;
3. docs make identity-only behavior explicit.

## Golden Tests

Golden assertions must cover:

1. existing core fixtures already tested by `format/tests.rs`;
2. branch catalog manifest fixtures:
   - `branch-catalog-manifest-empty.hex`;
   - `branch-catalog-manifest-single-active.hex`;
   - `branch-catalog-manifest-active-and-deleted.hex`;
   - `branch-catalog-manifest-with-parent.hex`.
3. pending releases manifest fixtures:
   - `pending-releases-manifest-empty.hex`;
   - `pending-releases-manifest-single.hex`;
   - `pending-releases-manifest-multi.hex`.
4. retained-history extension payload fixture or table-manifest extension
   fixture that proves the exact payload bytes.
5. checkpoint row-section payload fixture if the row-section codec is stable
   enough to warrant a stored fixture; otherwise use deterministic encoded-byte
   assertions with a documented reason.

Golden tests must compare exact bytes. Roundtrip-only assertions are necessary
but not sufficient.

## Consumer Regression Tests

After moving codecs, run existing consumers:

1. lifecycle recovery tests that build and install checkpoint row sections;
2. checkpoint recovery tests that reject malformed row-section payloads;
3. compaction row-pruning tests that emit and recover retained-history
   extension facts;
4. table manifest extension tests;
5. manifest service tests for branch catalog and pending releases manifests;
6. WAL/snapshot/table tests if codec-boundary helpers are introduced.

Expected result:

1. behavior remains unchanged;
2. only import paths and error-mapping wrappers change in lifecycle/service code;
3. lifecycle/service tests assert policy, not byte layout internals.

## Source Guard Tests

The source guard must assert:

1. production durable payload magic/version/header constants live under
   `src/format/`;
2. production lifecycle/service code does not define new `*_MAGIC`,
   `*_FORMAT_VERSION`, or `*_HEADER_SIZE` durable byte markers;
3. production lifecycle/service code does not hand-roll snapshot row-section or
   table-manifest extension payload encode/decode loops;
4. production lifecycle/service code may map `FormatError` into higher-layer
   error types;
5. tests, testkit fixtures, and local helper functions inside `src/format/**`
   are allowed.

Seeded guard probes should prove the guard fails on:

1. a lifecycle module declaring `const ROWS_MAGIC: [u8; 4] = *b"STRR";`;
2. a service module declaring `const PAYLOAD_LEN: usize = 24;` and manually
   reading retained-history fields;
3. a lifecycle module reading row counts and lengths from a snapshot payload
   with `from_le_bytes`;
4. a service module writing a retained-history extension payload with
   `to_le_bytes`.

Seeded guard probes should prove the guard allows:

1. the same bytes in `src/format/**`;
2. test-only fixtures;
3. lifecycle/service code calling L3 encode/decode helpers;
4. lifecycle/service code mapping `FormatError`.

## Fuzz And Generated Tests

Fuzz/testkit routing must include:

1. `decode_branch_catalog_manifest`;
2. `decode_pending_releases_manifest`;
3. checkpoint row-section payload decoder;
4. retained-history extension payload decoder;
5. existing routes for key, manifest, quarantine, segment metadata, snapshot
   envelope, storage row, table artifact, table block, table manifest, WAL
   commit payload, WAL record, WAL segment header, and watermark.

Generated or deterministic property-style tests should cover:

1. multiple row counts and row byte sizes for checkpoint row sections;
2. row-count declarations near payload length boundaries;
3. retained-history timestamp flag combinations;
4. reserved byte mutation;
5. manifest entry ordering permutations;
6. oversized count fields that must fail before large allocations.

Fuzz routes should return only success/failure. They must not call lifecycle
install, service publish, backend IO, or public APIs.

## Documentation Proof

Documentation closeout requires review of:

1. `docs/architecture/storage/l3-durable-format-codec.md`;
2. `docs/spec/strata-storage-format-v1.md`;
3. `docs/architecture/storage/target-crate-shape-and-test-harness.md`;
4. `crates/storage-next/testdata/goldens/storage-format-v1/README.md`.

Required documentation assertions:

1. checkpoint row-section payload fields are specified if the payload remains a
   stable V1 snapshot section;
2. retained-history extension payload fields are specified if the extension
   remains a stable V1 table-manifest extension;
3. branch catalog manifest fields are specified;
4. pending releases manifest fields are specified;
5. identity-only codec behavior is explicit;
6. old format versions are documented as rejected pre-launch evidence, not
   compatibility inputs.

## Mode Testing

L3 durable bytes are backend-independent and should compile across modes.

Required commands:

1. `cargo test -p strata-storage-next --locked --lib format::`
2. `cargo test -p strata-storage-next --locked --test format_golden` if a
   standalone golden integration test exists after implementation.
3. `cargo test -p strata-storage-next --locked --lib lifecycle::tests::recovery`
4. `cargo test -p strata-storage-next --locked --lib lifecycle::tests::compaction::row_pruning`
5. `cargo clippy -p strata-storage-next --lib --features perf-trace -- -D warnings`
6. `cargo check -p strata-storage-next --no-default-features --target wasm32-unknown-unknown --all-targets --locked`
7. `cargo fmt --package strata-storage-next --check`
8. `git diff --check`

If the standalone `format_golden` integration test does not exist, record the
exact default golden test command used instead.

## Sensitivity Probes

Before closeout, run or document manual mutation probes where practical:

1. Change checkpoint row-section magic and confirm L3 tests fail.
2. Change retained-history payload length and confirm L3 tests fail.
3. Remove a branch catalog fixture assertion and confirm the golden test fails
   or the source list test catches the omission.
4. Remove a fuzz route and confirm route-coverage tests fail.
5. Add a durable byte magic constant in lifecycle code and confirm the source
   guard fails.
6. Add an unsupported codec id and confirm decode rejects it without needing
   service policy.

## Deferral Rules

A finding may be deferred only if the implementation records:

1. exact file and function where the gap remains;
2. owner layer;
3. reason it cannot be closed without semantic changes;
4. replacement proof that prevents corrupt or silently changed durable bytes;
5. later slice that will close it.

Allowed likely deferrals:

1. no reusable codec API if V1 identity-only behavior is explicitly documented
   and current services do not duplicate real codec switches;
2. no stored checkpoint row-section golden if deterministic exact-byte unit tests
   provide equivalent proof and the format spec records the byte contract;
3. no encryption support.

Disallowed deferrals:

1. checkpoint row-section payload codec remaining in L8 without an L3 wrapper;
2. retained-history payload codec remaining in L8 without an L3 wrapper;
3. branch catalog and pending releases manifest fixtures remaining absent from
   default tests;
4. fuzz routing missing implemented public L3 decoders;
5. source guards that pass only by skipping lifecycle/service production code.

## Closeout Requirements

M4P-L3 test closeout requires:

1. moved codecs have strict L3 unit tests;
2. moved codecs preserve exact current bytes or record a format-impact decision;
3. default golden tests assert branch catalog and pending releases fixtures;
4. retained-history and checkpoint payloads have exact-byte proof;
5. consumer lifecycle/service tests pass unchanged in behavior;
6. fuzz/testkit routes cover every implemented L3 decoder;
7. source guard tests pass and include failing-fixture probes;
8. docs/spec record each format and the identity codec decision;
9. no roadmap labels are added to production Rust identifiers, comments,
   fixture bytes, panic messages, or user-visible text;
10. verification commands either pass or are recorded with precise skip reasons.
