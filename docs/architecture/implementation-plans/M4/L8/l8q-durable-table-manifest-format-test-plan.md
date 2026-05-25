# L8Q Test Plan: Durable Table Manifest Format

Status: draft test plan

Implementation plan:
`docs/architecture/implementation-plans/M4/L8/l8q-durable-table-manifest-format-implementation-plan.md`

Parent test plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`

## Goal

Prove that the durable table-manifest format is deterministic,
self-validating, primitive-neutral, and rich enough for later recovery and
retention slices to trust it as table reachability evidence.

The suite must fail if L8Q:

1. accepts corrupt or truncated bytes;
2. accepts a future table-manifest version;
3. produces different bytes for equivalent manifests;
4. loses L0 ordering, L1+ ordering, or inherited-layer ordering;
5. accepts duplicate table identities or object names;
6. accepts invalid object names, path-like names, or raw filesystem paths;
7. omits materialization or inherited-layer status facts;
8. silently accepts unknown required sections;
9. imports product, primitive, StrataHub, raw IO, or lifecycle execution code;
10. requires a table object, backend, manifest service, or L6 runtime to decode
    bytes.

Do not add tests whose only assertion is that planning documents exist or link
to each other.

## Coverage Boundary

L8Q tests cover the format and validation surface only.

Covered:

1. in-memory manifest constructors;
2. encode/decode round trips;
3. canonical ordering;
4. golden vectors;
5. corruption/future/truncation rejection;
6. section framing;
7. source guards;
8. fuzz/decode robustness.

Not covered in this slice:

1. manifest object publication;
2. recovery into L6;
3. retention proof;
4. table-object quarantine or purge;
5. flush watermark proof;
6. durable compaction/materialization publication;
7. lazy table reads;
8. public L9 API behavior.

## Old-Code Regression Sources

| Old source | Behavior to preserve | Storage-next assertion |
|---|---|---|
| `crates/storage/src/manifest.rs` | Manifest records owned entries, inherited layers, level, status, fork version, and CRC. | Round-trip owned levels and inherited layers with checksum validation. |
| `crates/storage/src/segmented/tests/leveled.rs` | Recovery restores levels and rejects corrupt manifests. | Format tests pin level order and corrupt-byte rejection. |
| `crates/storage/src/segmented/tests/concurrency.rs` | Corrupt manifest must not cause orphan table loading. | Decoder rejects corrupt bytes before yielding partial table refs. |
| `crates/storage/src/segmented/tests/gc_under_degradation.rs` | Corrupt or missing durable reachability blocks unsafe reclaim. | L8Q produces typed decode failures; retention policy remains later. |
| `crates/storage/src/segmented/quarantine_protocol.rs` | Manifest reachability is durable reclaim evidence. | Manifest entries enumerate object names and table identities deterministically. |
| `crates/storage/src/segmented/ref_registry.rs` | Runtime ref registry is an accelerator, not durable truth. | Decode requires no runtime ref registry fields. |

Tests must not port:

1. raw path fixtures;
2. direct filesystem mutation;
3. old public command names;
4. logs-only assertions;
5. product branch names;
6. primitive DTOs;
7. recovery fallback policy.

## Test Locations

Use:

1. `crates/storage-next/src/format/table_manifest.rs` for focused unit tests
   that need private helpers.
2. `crates/storage-next/src/format/tests.rs` for golden vectors if that remains
   the format golden test home.
3. `crates/storage-next/src/testdata/goldens/storage-format-v1/` for checked-in
   golden bytes.
4. `crates/storage-next/src/format/fuzzing.rs` for the decode contract hook.
5. `crates/storage-next/fuzz/fuzz_targets/format_table_manifest.rs` for fuzzing.
6. `crates/storage-next/fuzz/corpus/format_table_manifest/` for seed corpora.
7. `crates/storage-next/tests/lifecycle_source_guard.rs` or a format-specific
   source guard for boundary checks.

If direct format tests approach 1,000 lines, split them under
`crates/storage-next/src/format/table_manifest/tests/`.

## Test Data Principles

1. Use validated `ObjectName` values, not raw paths.
2. Include at least two branches in fixtures across the suite.
3. Include L0 and L1+ tables.
4. Include L0 precedence ordering that is not identity-sorted.
5. Include L1+ physical ranges that are identity-sorted differently from key
   order.
6. Include inherited layers with at least two ancestors.
7. Include materializing and active inherited-layer statuses.
8. Include materialization replacement provenance.
9. Include timestamp and commit ranges.
10. Include optional extension sections if the initial format carries section
    framing.

## Direct Unit Tests

### 1. Constructor Validation

Required tests:

1. `table_manifest_accepts_empty_branch_graph`
2. `table_manifest_accepts_zero_branch_id_as_opaque_atom`
3. `table_manifest_rejects_zero_manifest_sequence_if_reserved`
4. `table_manifest_rejects_invalid_branch_generation`
5. `table_manifest_rejects_duplicate_level`
6. `table_manifest_rejects_invalid_level`
7. `table_manifest_rejects_duplicate_table_identity`
8. `table_manifest_rejects_duplicate_object_name`
9. `table_manifest_rejects_empty_table_entry`
10. `table_manifest_rejects_zero_row_count`
11. `table_manifest_rejects_commit_min_greater_than_max`
12. `table_manifest_rejects_timestamp_min_greater_than_max`
13. `table_manifest_rejects_invalid_physical_bounds`
14. `table_manifest_rejects_invalid_internal_bounds`

Assertions:

1. validation happens before encode;
2. errors are typed by field;
3. no invalid manifest can be encoded through public constructors;
4. branch ids are treated as opaque `BranchId` atoms; all-zero is not a
   format-level empty sentinel.

### 2. Object Name Validation

Required tests:

1. `table_manifest_accepts_layout_valid_table_object_name`
2. `table_manifest_rejects_absolute_path_object_name`
3. `table_manifest_rejects_parent_component_object_name`
4. `table_manifest_rejects_empty_object_component`
5. `table_manifest_rejects_manifest_object_used_as_table_object`
6. `table_manifest_rejects_snapshot_object_used_as_table_object`
7. `table_manifest_rejects_quarantine_object_used_as_table_object`

Assertions:

1. entries store object names, not filesystem paths;
2. object-family mismatches fail before bytes are trusted.

### 3. Canonical Encoding

Required tests:

1. `table_manifest_round_trips_owned_tables`
2. `table_manifest_round_trips_inherited_layers`
3. `table_manifest_round_trips_materialization_provenance`
4. `table_manifest_canonicalizes_level_order`
5. `table_manifest_preserves_l0_precedence_order`
6. `table_manifest_sorts_l1_plus_by_physical_range`
7. `table_manifest_preserves_inherited_layer_order`
8. `table_manifest_preserves_tables_inside_inherited_layer`
9. `equivalent_table_manifests_encode_identically`
10. `different_l0_order_encodes_differently`

Assertions:

1. equivalent manifests have identical bytes;
2. semantically different precedence order is not erased;
3. decoded manifests compare equal to validated in-memory values.

### 4. Golden Vectors

Required tests:

1. `table_manifest_empty_matches_golden_vector`
2. `table_manifest_owned_levels_matches_golden_vector`
3. `table_manifest_inherited_layers_matches_golden_vector`
4. `table_manifest_materialization_provenance_matches_golden_vector`
5. `table_manifest_extension_section_matches_golden_vector`

Assertions:

1. golden bytes are checked in;
2. decode of every golden vector succeeds;
3. re-encode of every golden vector matches the checked-in bytes exactly.

### 5. Corruption And Version Rejection

Required tests:

1. `table_manifest_rejects_bad_magic`
2. `table_manifest_rejects_future_version`
3. `table_manifest_rejects_pre_v1_version_if_reserved`
4. `table_manifest_rejects_truncated_header`
5. `table_manifest_rejects_truncated_table_entry`
6. `table_manifest_rejects_truncated_inherited_layer`
7. `table_manifest_rejects_trailing_bytes`
8. `table_manifest_rejects_checksum_mismatch`
9. `table_manifest_rejects_count_overflow`
10. `table_manifest_rejects_length_overflow`
11. `table_manifest_rejects_invalid_utf8`
12. `table_manifest_rejects_reserved_flag_bits`

Assertions:

1. decoder never returns a partial manifest on corrupt bytes;
2. allocation counts are bounded before allocation;
3. checksum failure is distinct from structural failure.

### 6. Level And Range Invariants

Required tests:

1. `table_manifest_rejects_non_contiguous_l0_order`
2. `table_manifest_rejects_duplicate_l0_order`
3. `table_manifest_rejects_l1_plus_overlap`
4. `table_manifest_rejects_l1_plus_out_of_order_ranges`
5. `table_manifest_allows_l0_overlapping_ranges`
6. `table_manifest_allows_distinct_versions_of_same_logical_key_when_ranges_are_valid`
7. `table_manifest_allows_sparse_owned_levels_until_branch_policy`

Assertions:

1. L0 preserves precedence semantics;
2. L1+ uses physical range ordering and non-overlap;
3. format rules match L6 table-level invariants.

### 7. Inherited Layers

Required tests:

1. `table_manifest_inherited_layer_records_source_and_fork`
2. `table_manifest_rejects_duplicate_inherited_layer_source_fork`
3. `table_manifest_rejects_non_contiguous_inherited_layer_order`
4. `table_manifest_preserves_active_status`
5. `table_manifest_preserves_materializing_status`
6. `table_manifest_preserves_materialized_status_if_supported`
7. `table_manifest_rejects_inherited_layer_with_invalid_fork_version`
8. `table_manifest_rejects_inherited_layer_with_duplicate_table_identity`
9. `table_manifest_rejects_inherited_layer_with_duplicate_object_name`
10. `table_manifest_does_not_require_runtime_materialization_handle`

Assertions:

1. durable status is separate from runtime handles;
2. nearest-ancestor order survives round trip;
3. malformed inherited-layer graphs fail closed.

### 8. Provenance

Required tests:

1. `table_manifest_preserves_flush_provenance`
2. `table_manifest_preserves_snapshot_install_provenance`
3. `table_manifest_preserves_compaction_provenance`
4. `table_manifest_preserves_materialization_replacement_provenance`
5. `table_manifest_preserves_recovered_provenance`
6. `table_manifest_rejects_materialization_provenance_without_source`
7. `table_manifest_rejects_unknown_required_provenance`

Assertions:

1. provenance is storage diagnostic/recovery vocabulary;
2. provenance does not contain product workflow names;
3. replacement provenance has enough facts for L8R/L8S to reason later.

### 9. Extension Sections

Required tests:

1. `table_manifest_rejects_unknown_required_section`
2. `table_manifest_accepts_unknown_optional_section_without_core_fact_loss`
3. `table_manifest_preserves_known_extension_section`
4. `table_manifest_rejects_duplicate_required_section`
5. `table_manifest_rejects_invalid_section_identifier`
6. `table_manifest_rejects_product_named_section`
7. `table_manifest_rejects_primitive_named_section`

Assertions:

1. section behavior is explicit;
2. optional sections cannot affect core ordering;
3. product/primitive vocabulary does not enter durable table reachability.

### 10. Decode Robustness

Required tests:

1. `table_manifest_decode_empty_bytes_returns_typed_error`
2. `table_manifest_decode_random_bytes_returns_typed_error_or_valid_manifest`
3. `table_manifest_decode_large_counts_does_not_allocate_unbounded_memory`
4. `table_manifest_decode_rejects_deeply_nested_sections`
5. `table_manifest_decode_rejects_noncanonical_reencoded_bytes`

Assertions:

1. arbitrary bytes do not panic;
2. decoder is allocation-bounded;
3. noncanonical but structurally valid bytes are either normalized by
   re-encoding or rejected, as the implementation plan chooses.

## Source Guards

Required source guard tests:

1. `table_manifest_format_does_not_import_raw_io`
2. `table_manifest_format_does_not_import_backend_services`
3. `table_manifest_format_does_not_import_lifecycle_execution`
4. `table_manifest_format_does_not_import_engine_or_product_crates`
5. `table_manifest_format_does_not_import_stratahub`
6. `table_manifest_format_does_not_import_primitive_modules`
7. `table_manifest_format_does_not_use_product_workflow_words`
8. `lower_layers_do_not_import_lifecycle_table_manifest_policy`

Forbidden production tokens include:

1. `std::fs`
2. `std::path::Path`
3. `std::env`
4. `OpenOptions`
5. `File::`
6. `crate::lifecycle`
7. `strata_engine`
8. `stratahub`
9. `graph`
10. `vector`
11. `json`
12. `merge`
13. `cherry`
14. `revert`

Use token checks carefully enough to avoid false positives from this planning
document or test names.

## Generated And Fuzz Tests

Add a format fuzz contract:

```text
format_table_manifest(bytes):
  attempt decode
  if decode succeeds:
    validate manifest
    encode manifest
    decode encoded bytes
    assert equality
  if decode fails:
    assert typed FormatError
```

Required seed corpus:

1. empty branch manifest;
2. owned L0 + L1 manifest;
3. inherited-layer manifest;
4. materialization provenance manifest;
5. unknown optional extension section;
6. bad checksum;
7. future version;
8. truncated table entry.

Generated/property tests should build valid manifests from structured inputs,
then mutate:

1. magic;
2. version;
3. checksum;
4. counts;
5. object names;
6. order fields;
7. ranges;
8. statuses;
9. section flags.

## Sensitivity Probes

Record probes in
`docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md` after
implementation.

| Probe | Mutation | Expected failure |
|---|---|---|
| Q1 | Ignore checksum mismatch. | Corruption test and fuzz contract fail. |
| Q2 | Sort L0 by table identity instead of explicit order. | `different_l0_order_encodes_differently` fails. |
| Q3 | Allow duplicate object names. | Duplicate object-name test fails. |
| Q4 | Accept invalid path-like object names. | Object-name validation test fails. |
| Q5 | Ignore inherited-layer status. | Status round-trip test fails. |
| Q6 | Drop materialization provenance on encode. | Provenance round-trip test fails. |
| Q7 | Accept unknown required section. | Required-section test fails. |
| Q8 | Use internal-key non-overlap for L1+ instead of physical ranges. | Range invariant test fails. |
| Q9 | Remove count bounds before allocation. | Large-count robustness test fails. |
| Q10 | Import raw filesystem API in format module. | Source guard fails. |

## Command Matrix

Mandatory commands before L8Q closeout:

```bash
cargo test -p strata-storage-next --locked --lib table_manifest
cargo test -p strata-storage-next --locked --lib format::
cargo test -p strata-storage-next --locked --test format_golden
cargo test -p strata-storage-next --locked --test table_format_source_guard
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --features testkit --locked --lib format_fuzz
cargo test -p strata-storage-next --locked --test testkit_boundary
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo check --manifest-path crates/storage-next/fuzz/Cargo.toml --locked --bin format_table_manifest
cargo +nightly fuzz run format_table_manifest -- -runs=1
rustfmt --check crates/storage-next/src/format/mod.rs crates/storage-next/src/format/fuzzing.rs crates/storage-next/src/format/tests.rs crates/storage-next/src/format/table_manifest.rs crates/storage-next/src/format/table_manifest/tests.rs crates/storage-next/src/format/table_manifest/tests/*.rs crates/storage-next/src/testkit/format_fuzz.rs crates/storage-next/tests/format_golden.rs crates/storage-next/tests/table_format_source_guard.rs crates/storage-next/fuzz/fuzz_targets/format_table_manifest.rs
git diff --check
```

If fuzzing cannot run in the local environment, record the reason and run the
format unit/property tests plus corpus inventory checks.

## Exit Gate

L8Q test coverage is complete when:

1. every required direct test above exists or is explicitly superseded by a
   stricter named test;
2. golden vectors are checked in and stable;
3. arbitrary corrupt bytes cannot panic the decoder;
4. source guards cover raw IO, product, primitive, lifecycle, and StrataHub
   boundaries;
5. fuzz target and seed corpus exist;
6. sensitivity probes are recorded with mutation and failing test;
7. the command matrix is recorded in the porting log.
