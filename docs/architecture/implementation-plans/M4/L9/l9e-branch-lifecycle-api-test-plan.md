# L9E Test Plan: Branch Lifecycle API

Status: draft test plan

Parent plan:
`docs/architecture/implementation-plans/M4/L9/l9e-branch-lifecycle-api-implementation-plan.md`

## Goal

Prove that L9 branch APIs expose storage branch mechanics safely without product
branch workflows.

## Test Locations

1. `crates/storage-next/src/api/tests/branch.rs`
2. `crates/storage-next/tests/api_conformance.rs`
3. `crates/storage-next/tests/api_properties.rs`
4. `crates/storage-next/tests/api_source_guard.rs`

## Required Tests

### Create/List/Describe

1. `branch_create_returns_generation`
2. `branch_create_duplicate_rejects`
3. `branch_create_invalid_identifier_rejects`
4. `branch_list_is_deterministic`
5. `branch_describe_reports_generation`
6. `branch_describe_unknown_rejects`

### Fork

1. `branch_fork_current_copies_visible_frontier`
2. `branch_fork_current_preserves_inherited_visibility`
3. `branch_fork_at_retained_version_succeeds`
4. `branch_fork_at_retained_watermark_between_commits_succeeds`
5. `branch_fork_at_unretained_version_rejects`
6. `branch_fork_invalid_source_identifier_rejects`
7. `branch_fork_at_timestamp_resolves_timeline`
8. `branch_fork_at_unretained_timestamp_rejects`
9. `branch_fork_generation_mismatch_rejects`
10. `branch_fork_after_close_rejects`

### Clear/Delete

1. `branch_clear_removes_visible_rows`
2. `branch_clear_preserves_branch_identity`
3. `branch_clear_generation_mismatch_rejects`
4. `branch_clear_with_pinned_view_reports_protected_release`
5. `branch_delete_removes_from_list`
6. `branch_delete_generation_mismatch_rejects`
7. `branch_delete_with_pinned_view_reports_protected_release`
8. `branch_delete_unknown_rejects`
9. `branch_delete_reports_cleanup_facts`
10. `branch_recreate_deleted_reports_generation_transition`

### Durable Round Trip

1. `durable_branch_catalog_round_trips_after_reopen`

### Deferred Product Workflows

1. `branch_api_has_no_merge_method`
2. `branch_api_has_no_cherry_pick_method`
3. `branch_api_has_no_revert_method`
4. `branch_api_has_no_restore_method`
5. `branch_api_has_no_publish_review_method`

## Generated Branch Contract

Generate scripts with:

1. create branch;
2. commit to branch;
3. fork current;
4. fork at version;
5. clear;
6. delete;
7. list;
8. read after branch operation;
9. recreate deleted branch;
10. invalid source branch rejection.

The model should track branch generation and retained-history bounds
independently.

Required property:

1. `api_property_harness_matches_generated_branch_model`

## Sensitivity Probes

1. Ignore generation mismatch.
2. Drop protected cleanup facts for pinned reachability.
3. Let fork-at-history use latest instead of requested version.
4. Leak merge vocabulary into API.
5. Require an exact timeline row instead of a retained fork watermark.
6. Skip source-branch identifier validation.
7. Drop deleted-generation facts during branch recreate.

## Verification

```bash
cargo test -p strata-storage-next --locked --lib api
cargo test -p strata-storage-next --features testkit --locked --test api_properties
cargo test -p strata-storage-next --locked --test api_source_guard
```
