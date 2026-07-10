# L9A Test Plan: API Vocabulary And Visibility Boundary

Status: draft test plan

Parent plan:
`docs/architecture/implementation-plans/M4/L9/l9a-api-vocabulary-visibility-boundary-implementation-plan.md`

## Goal

Prove that the storage API scaffold exists, is synchronous, is product-neutral,
and is the only public storage-next production surface.

The suite must fail if L9A:

1. exposes lower-layer implementation types;
2. makes lower modules public by accident;
3. imports engine or product crates;
4. exposes async/runtime-specific types;
5. uses display text as the error oracle;
6. lacks stable error codes;
7. leaves public enums closed to future growth where growth is expected.

## Test Locations

1. `crates/storage-next/src/api/tests/` for direct construction tests.
2. `crates/storage-next/tests/api_source_guard.rs` for source-boundary scans.
3. `crates/storage-next/tests/api_snapshot.rs` for public signature checks if a
   snapshot tool is available.
4. `docs/architecture/implementation-plans/M4/L9/m4-l9-porting-log.md` for
   sensitivity probe evidence.

## Required Direct Tests

### Module And Export Shape

1. `api_module_exports_storage_runtime_shell`
2. `api_module_exports_storage_result`
3. `api_module_exports_storage_error`
4. `api_module_exports_open_options_shell`
5. `api_module_exports_read_selector_shell`
6. `api_module_exports_commit_batch_shell`
7. `api_module_exports_branch_request_shell`
8. `api_module_exports_maintenance_request_shell`
9. `api_module_exports_diagnostics_shell`
10. `lower_modules_are_not_public_api`

### Error Vocabulary

1. `storage_api_error_codes_are_stable`
2. `storage_api_error_display_is_not_empty`
3. `storage_api_error_source_chain_is_preserved`
4. `storage_api_error_invalid_argument_has_structured_field`
5. `storage_api_error_unsupported_capability_has_structured_field`
6. `storage_api_error_history_unavailable_is_distinct_from_not_found`
7. `storage_api_error_durable_uncertain_is_distinct_from_lower_layer_failure`
8. `storage_api_error_display_does_not_include_payload_bytes`

### Boundary Type Validation

1. `storage_key_rejects_empty_when_required`
2. `storage_value_accepts_opaque_bytes`
3. `read_limit_rejects_zero_when_zero_is_invalid`
4. `scan_bound_order_is_validated`
5. `branch_generation_zero_policy_is_explicit`
6. `maintenance_request_kind_is_constructible`
7. `diagnostics_request_kind_is_constructible`

## Source Guard Requirements

`api_source_guard.rs` must fail on:

1. public `mod branch`, `mod commit`, `mod lifecycle`, `mod service`,
   `mod format`, `mod table`, `mod backend`, or `mod layout` exports from
   `lib.rs`;
2. `crate::api` imports from lower production modules;
3. `strata_engine`, `engine`, `intelligence`, `inference`, `executor`, `cli`,
   or SDK imports from `src/api/**`;
4. `async`, `Future`, `tokio`, `async_std`, or runtime-specific public types in
   `src/api/**`;
5. product vocabulary in `src/api/**`;
6. public signatures containing lower-layer concrete type names.

The guard should avoid naive substring checks where common English words create
false positives. Prefer token or path-based matching.

## Sensitivity Probes

Record probes for:

1. adding a public lower-layer module export;
2. returning a lifecycle concrete type from an API signature;
3. adding an async marker type;
4. removing an error code;
5. adding product vocabulary to production API code.

## Verification

```bash
cargo fmt --package strata-storage-next --check
cargo test -p strata-storage-next --locked --lib api
cargo test -p strata-storage-next --locked --test api_source_guard
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
```
