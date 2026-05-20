//! Closeout guards for the M4-L6 branch-LSM runtime milestone.

#![deny(unsafe_code)]

mod common;

use std::fs;
use std::path::Path;

#[test]
fn branch_lsm_closeout_generated_harness_exposes_every_counter() {
    let crate_root = common::crate_root();
    let testkit = read_file(&crate_root.join("src/testkit/branch_lsm.rs"));
    let properties = read_file(&crate_root.join("tests/branch_lsm_properties.rs"));

    let counter_methods = branch_lsm_counter_methods(&testkit);
    assert!(
        counter_methods.len() >= 120,
        "branch LSM closeout expected broad generated counter coverage, found {} counters",
        counter_methods.len()
    );

    for counter in counter_methods {
        let qualified_counter = format!("BranchLsmScaffoldOutcome::{counter}");
        assert!(
            properties.contains(&qualified_counter),
            "tests/branch_lsm_properties.rs should require counter {qualified_counter:?}"
        );
    }

    assert_contains_all(
        "src/testkit/branch_lsm.rs",
        &testkit,
        &[
            "check_branch_lsm_reads_contract",
            "check_branch_lsm_inheritance_contract",
            "check_branch_lsm_install_contract",
            "check_branch_lsm_reference_model_contract",
            "check_branch_lsm_fault_window_contract",
            "ModelBranch",
        ],
    );
}

#[test]
fn branch_lsm_closeout_generated_harness_covers_required_category_groups() {
    let crate_root = common::crate_root();
    let testkit = read_file(&crate_root.join("src/testkit/branch_lsm.rs"));
    let properties = read_file(&crate_root.join("tests/branch_lsm_properties.rs"));

    assert_contains_all(
        "src/testkit/branch_lsm.rs",
        &testkit,
        &[
            "check_valid_config",
            "check_invalid_configs",
            "check_read_bounds",
            "check_valid_facts",
            "check_invalid_facts",
            "check_descriptors",
            "check_error_sources",
            "check_stats",
            "check_row_identity_and_rewrites",
            "check_effective_bounds_and_candidates",
            "check_edge_rows_and_encoded_grouping",
            "check_row_chains_and_fork_edges",
            "check_branch_local_state",
            "check_branch_read_view",
            "check_branch_owned_immutable",
            "check_branch_inheritance",
            "check_branch_timestamp_visibility",
            "check_branch_materialization",
            "check_branch_reachability",
            "check_branch_compaction",
            "check_branch_snapshot_install",
        ],
    );

    assert_contains_all(
        "tests/branch_lsm_properties.rs",
        &properties,
        &[
            "valid_config_cases",
            "invalid_config_cases",
            "read_bound_cases",
            "valid_fact_cases",
            "invalid_fact_cases",
            "descriptor_cases",
            "error_source_cases",
            "stats_cases",
            "physical_key_rewrite_cases",
            "candidate_tombstone_cases",
            "row_chain_cases",
            "state_construction_cases",
            "latest_point_read_cases",
            "timestamp_point_read_cases",
            "immutable_l0_install_cases",
            "inherited_key_rewrite_cases",
            "materialization_latest_read_parity_cases",
            "reachability_release_candidate_cases",
            "compaction_output_install_cases",
            "snapshot_single_branch_install_cases",
        ],
    );
}

#[test]
fn branch_lsm_closeout_direct_branch_tests_cover_required_surfaces() {
    let crate_root = common::crate_root();
    let branch_tests = read_branch_test_sources(&crate_root);

    assert_contains_all(
        "src/branch/tests.rs and src/branch/tests/*.rs",
        &branch_tests,
        &[
            "branch_runtime_config_rejects_unusable_zero_limits",
            "branch_state_facts_accept_empty_shape_and_reject_impossible_shapes",
            "branch_descriptors_preserve_storage_owned_facts",
            "branch_runtime_errors_are_typed_and_preserve_sources",
            "branch_row_identity_accepts_matching_rows_and_rejects_mismatches",
            "branch_rewrite_groups_inherited_rows_with_child_local_encoded_keys",
            "branch_local_state_appends_puts_tombstones_and_preserves_row_facts",
            "branch_local_state_rejects_active_and_frozen_duplicates_without_mutation",
            "branch_local_state_rotation_preserves_rows_and_newest_first_order",
            "branch_read_view_is_pinned_across_append_and_rotation",
            "branch_read_view_latest_and_version_reads_follow_row_chain_not_source_order",
            "branch_read_view_history_preserves_tombstones_limits_and_before_version",
            "branch_read_view_prefix_and_range_scans_group_by_physical_key",
            "branch_read_view_timestamp_reads_filter_by_timestamp_then_commit_version",
            "branch_owned_l0_tables_accept_overlaps_and_select_by_version_not_index",
            "branch_owned_nonzero_levels_are_sorted_and_reject_overlaps_without_mutation",
            "branch_fork_into_empty_child_captures_inherited_layers_without_copying_rows",
            "branch_inherited_reads_apply_fork_gate_and_child_tombstone_shadowing",
            "branch_inherited_timestamp_reads_apply_timestamp_and_fork_gates",
            "branch_chained_fork_prefers_nearest_inherited_layer_for_exact_ties",
            "branch_materialization_preserves_scans_tombstones_ttl_and_pinned_views",
            "branch_materialization_handles_empty_and_already_materialized_layers",
            "branch_reachability_aggregate_registry_and_release_plans_are_safe",
            "branch_compaction_rejects_pruning_policies_without_mutation",
            "branch_compaction_l0_keep_all_installs_replacement_and_preserves_pinned_view",
            "branch_compaction_release_facts_cover_shared_refs_disagreement_and_clear_outputs",
            "branch_snapshot_install_builds_l0_tables_and_preserves_reads",
            "branch_snapshot_install_rejects_invalid_rows_before_any_branch_mutates",
            "branch_snapshot_install_table_build_failure_preserves_every_target",
        ],
    );
}

#[test]
fn branch_lsm_closeout_exports_dedicated_fuzz_contracts() {
    let crate_root = common::crate_root();
    let testkit_mod = read_file(&crate_root.join("src/testkit/mod.rs"));

    assert_contains_all(
        "src/testkit/mod.rs",
        &testkit_mod,
        &[
            "check_branch_lsm_reads_contract",
            "check_branch_lsm_inheritance_contract",
            "check_branch_lsm_install_contract",
            "check_branch_lsm_scaffold_contract",
            "check_branch_lsm_reference_model_contract",
            "check_branch_lsm_fault_window_contract",
            "BranchLsmScaffoldOutcome",
        ],
    );
}

#[test]
fn branch_lsm_closeout_source_guard_suite_covers_required_boundary_categories() {
    let crate_root = common::crate_root();
    let source_guard = read_file(&crate_root.join("tests/branch_lsm_source_guard.rs"));

    assert_contains_all(
        "tests/branch_lsm_source_guard.rs",
        &source_guard,
        &[
            "branch_lsm_source_does_not_import_upper_layers_or_engines",
            "branch_lsm_source_does_not_use_product_dtos_or_payload_vocabulary",
            "branch_lsm_source_does_not_use_io_backend_or_lifecycle_apis",
            "branch_lsm_runtime_stays_crate_private",
            "branch_lsm_source_has_no_premature_behavior_entrypoints",
            "branch_lsm_source_guard_catches_required_forbidden_terms",
            "branch_lsm_source_guard_catches_io_and_orchestration_terms",
            "branch_lsm_source_guard_catches_backend_operation_call_forms",
        ],
    );

    assert_contains_all(
        "tests/branch_lsm_source_guard.rs",
        &source_guard,
        &[
            "crate::commit",
            "crate::lifecycle",
            "crate::api",
            "strata_engine",
            "crate::backend",
            "crate::object",
            "crate::layout",
            "crate::service",
            "crate::testkit",
            "std::fs",
            "std::path",
            "std::env",
            "rename(",
            "remove_file",
            "SystemTime",
            "Instant::now",
            "Timestamp::now",
            "VersionedValue",
            "Versioned<",
            "Value",
            "Key",
            "Namespace",
            "TypeTag",
            "EntityRef",
            "TransactionContext",
            "StrataHub",
            "RemoteTrackingRef",
            "ProviderCapability",
            "Dataset",
            "BranchName",
            "read_object",
            "read_range",
            "write_object",
            "append_object",
            "delete_object",
            "list_prefix",
            "object_metadata",
            "publish_object",
            "crate::service::wal",
            "crate::service::checkpoint",
            "crate::service::quarantine",
            "schedule_retention",
            "quarantine_object",
            "pub mod branch;",
        ],
    );
}

#[test]
fn branch_lsm_closeout_fuzz_inventory_matches_branch_targets() {
    let crate_root = common::crate_root();
    let fuzz_manifest = read_file(&crate_root.join("fuzz/Cargo.toml"));

    for (target, contract) in [
        ("branch_lsm_reads", "check_branch_lsm_reads_contract"),
        (
            "branch_lsm_inheritance",
            "check_branch_lsm_inheritance_contract",
        ),
        ("branch_lsm_install", "check_branch_lsm_install_contract"),
    ] {
        let target_path = crate_root.join(format!("fuzz/fuzz_targets/{target}.rs"));
        assert!(
            target_path.is_file(),
            "missing documented branch fuzz target {}",
            target_path.display()
        );
        let target_text = read_file(&target_path);
        let manifest_name = format!("name = \"{target}\"");
        let manifest_path = format!("path = \"fuzz_targets/{target}.rs\"");
        assert_contains_all(
            "fuzz/Cargo.toml",
            &fuzz_manifest,
            &[manifest_name.as_str(), manifest_path.as_str()],
        );
        assert_exclusive_fuzz_contract(target, &target_text, contract);
        assert!(
            !target_text.contains("check_branch_lsm_scaffold_contract"),
            "{target} must exercise its dedicated branch-LSM contract"
        );
        assert_nonempty_corpus(&crate_root, target);
    }
}

#[test]
fn branch_lsm_closeout_docs_record_required_matrix_commands_and_deferrals() {
    let crate_root = common::crate_root();
    let workspace_root = workspace_root(&crate_root);
    let implementation_plan = read_file(&workspace_root.join(
        "docs/architecture/implementation-plans/M4/L6/l6l-l6-conformance-closeout-implementation-plan.md",
    ));
    let test_plan = read_file(&workspace_root.join(
        "docs/architecture/implementation-plans/M4/L6/l6l-l6-conformance-closeout-test-plan.md",
    ));
    let parent_test_plan = read_file(
        &workspace_root
            .join("docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-test-plan.md"),
    );

    for slice in [
        "L6A", "L6B", "L6C", "L6D", "L6E", "L6F", "L6G", "L6H", "L6I", "L6J", "L6K",
    ] {
        assert!(
            implementation_plan.contains(slice) && test_plan.contains(slice),
            "L6L plans should include coverage matrix row for {slice}"
        );
    }

    assert_contains_all(
        "l6l-l6-conformance-closeout-test-plan.md",
        &test_plan,
        &[
            "direct unit tests",
            "generated/property tests",
            "source guards",
            "fuzz or fuzz-adjacent coverage",
            "cross-feature coverage",
            "old-code behavior mapped",
            "deferred behavior and owner",
            "mandatory commands",
            "cargo test -p strata-storage-next --locked --lib branch",
            "cargo test -p strata-storage-next --locked --test branch_lsm_source_guard",
            "cargo test -p strata-storage-next --locked --test branch_lsm_closeout",
            "cargo test -p strata-storage-next --features testkit --locked --test branch_lsm_properties",
            "cargo test -p strata-storage-next --no-default-features --features testkit --locked --test branch_lsm_properties",
            "cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked",
            "cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings",
            "cargo test -p strata-storage-next --locked",
            "cargo fmt --package strata-storage-next --check",
            "git diff --check",
            "cargo +nightly fuzz run branch_lsm_reads",
            "cargo +nightly fuzz run branch_lsm_inheritance",
            "cargo +nightly fuzz run branch_lsm_install",
            "commit-version allocation belongs to L7",
            "branch manifest publication belongs to L8",
            "public branch naming and product branch workflows belong to L9/engine",
            "StrataHub push/pull/clone/sync integration belongs above storage-next",
            "post-V1",
        ],
    );

    assert_contains_all(
        "m4-l6-branch-lsm-runtime-test-plan.md",
        &parent_test_plan,
        &[
            "branch_lsm_reads",
            "branch_lsm_inheritance",
            "branch_lsm_install",
            "cargo test -p strata-storage-next --locked --test branch_lsm_closeout",
        ],
    );
}

#[test]
fn branch_lsm_closeout_porting_log_records_full_l6_ledger() {
    let crate_root = common::crate_root();
    let workspace_root = workspace_root(&crate_root);
    let porting_log = read_file(
        &workspace_root.join("docs/architecture/implementation-plans/M4/L6/m4-l6-porting-log.md"),
    );

    for slice in [
        "L6A", "L6B", "L6C", "L6D", "L6E", "L6F", "L6G", "L6H", "L6I", "L6J", "L6K", "L6L",
    ] {
        assert!(
            porting_log.contains(&format!("## {slice}:")),
            "m4-l6-porting-log.md should contain a {slice} section"
        );
    }

    assert_contains_all(
        "m4-l6-porting-log.md",
        &porting_log,
        &[
            "branch_lsm_reads",
            "branch_lsm_inheritance",
            "branch_lsm_install",
            "check_branch_lsm_reads_contract",
            "check_branch_lsm_inheritance_contract",
            "check_branch_lsm_install_contract",
            "check_branch_lsm_reference_model_contract",
            "check_branch_lsm_fault_window_contract",
            "cargo test -p strata-storage-next --locked --test branch_lsm_closeout",
            "cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings",
            "commit-version allocation",
            "WAL-before-visible",
            "durable branch manifest publication",
            "duplicate branch create",
            "fork on a missing source branch",
            "fork-at-history requests",
            "StrataHub",
            "post-V1",
        ],
    );
}

#[test]
fn branch_lsm_closeout_sensitivity_probe_ledger_is_recorded() {
    let crate_root = common::crate_root();
    let workspace_root = workspace_root(&crate_root);
    let porting_log = read_file(
        &workspace_root.join("docs/architecture/implementation-plans/M4/L6/m4-l6-porting-log.md"),
    );
    let l6l_log = section_after_heading(&porting_log, "## L6L:");

    assert_contains_all(
        "m4-l6-porting-log.md L6L section",
        l6l_log,
        &[
            "tombstone",
            "TTL",
            "fork-version",
            "child-local",
            "materialization",
            "shared",
            "pruning",
            "branch-id mismatch",
            "duplicate",
            "VersionedValue",
            "crate::commit",
            "crate::lifecycle",
            "crate::backend",
            "crate::service",
        ],
    );
}

fn read_file(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

fn read_branch_test_sources(crate_root: &Path) -> String {
    let mut text = read_file(&crate_root.join("src/branch/tests.rs"));
    let test_dir = crate_root.join("src/branch/tests");
    let mut entries = fs::read_dir(&test_dir)
        .unwrap_or_else(|error| panic!("read {}: {error}", test_dir.display()))
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_else(|error| panic!("read entries in {}: {error}", test_dir.display()));
    entries.sort_by_key(std::fs::DirEntry::path);
    for entry in entries {
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
            text.push('\n');
            text.push_str(&read_file(&path));
        }
    }
    text
}

fn assert_contains_all(label: &str, text: &str, needles: &[&str]) {
    for needle in needles {
        assert!(text.contains(needle), "{label} should contain {needle:?}");
    }
}

fn assert_exclusive_fuzz_contract(target: &str, target_text: &str, expected_contract: &str) {
    let contracts = [
        "check_branch_lsm_reads_contract",
        "check_branch_lsm_inheritance_contract",
        "check_branch_lsm_install_contract",
    ];
    for contract in contracts {
        let count = target_text.matches(contract).count();
        if contract == expected_contract {
            assert_eq!(
                count, 2,
                "{target} should import and call only its dedicated contract {contract}"
            );
            assert!(
                target_text.contains(&format!("use strata_storage_next::testkit::{contract};")),
                "{target} should import {contract} directly"
            );
            assert!(
                target_text.contains(&format!("{contract}(data)")),
                "{target} should pass fuzzer data directly to {contract}"
            );
        } else {
            assert_eq!(
                count, 0,
                "{target} should not reference unrelated branch fuzz contract {contract}"
            );
        }
    }
}

fn branch_lsm_counter_methods(testkit: &str) -> Vec<String> {
    let mut counters = testkit
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            let rest = trimmed.strip_prefix("pub const fn ")?;
            let name = rest.split_once('(')?.0;
            name.ends_with("_cases").then(|| name.to_owned())
        })
        .collect::<Vec<_>>();
    counters.sort();
    counters.dedup();
    counters
}

fn assert_nonempty_corpus(crate_root: &Path, target: &str) {
    let corpus = crate_root.join(format!("fuzz/corpus/{target}"));
    assert!(
        corpus.is_dir(),
        "missing checked-in fuzz corpus directory {}",
        corpus.display()
    );
    let has_seed = fs::read_dir(&corpus)
        .unwrap_or_else(|error| panic!("read {}: {error}", corpus.display()))
        .any(|entry| {
            let Ok(entry) = entry else {
                return false;
            };
            let path = entry.path();
            path.is_file()
                && path
                    .metadata()
                    .map(|metadata| metadata.len() != 0)
                    .unwrap_or(false)
        });
    assert!(
        has_seed,
        "fuzz corpus directory {} should contain at least one non-empty seed",
        corpus.display()
    );
}

fn workspace_root(crate_root: &Path) -> &Path {
    crate_root
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
}

fn section_after_heading<'a>(text: &'a str, heading: &str) -> &'a str {
    let Some((_, section)) = text.split_once(heading) else {
        panic!("missing section heading {heading:?}");
    };
    section
        .split_once("\n## ")
        .map_or(section, |(body, _)| body)
}
