//! Lifecycle closeout inventory checks.

#![deny(unsafe_code)]

mod common;

use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn lifecycle_closeout_generated_counters_cover_required_categories() {
    let root = common::crate_root();
    let properties = read(&root.join("tests/lifecycle_properties.rs"));

    assert_contains_all(
        &properties,
        &[
            "lifecycle_generated_script_exercises_input_derived_open_recovery_and_close",
            "lifecycle_generated_script_exercises_input_derived_maintenance_routes",
            "lifecycle_generated_script_exercises_input_derived_reclaim_routes",
            "lifecycle_generated_script_rejects_validation_only_script_without_side_effect_claim",
            "lifecycle_generated_script_model_matches_healthy_recovered_visibility",
            "lifecycle_generated_script_deletion_set_is_subset_of_model_proof",
            "lifecycle_generated_script_watermarks_are_monotonic",
            "lifecycle_generated_script_close_is_idempotent_after_success",
            "lifecycle_generated_script_cache_mode_never_claims_durable_recovery",
            "lifecycle_generated_script_lossy_recovery_records_degraded_health",
            "lifecycle_property_harness_runs_scaffold_contract",
            "lifecycle_property_harness_runs_recovery_contract",
            "lifecycle_property_harness_runs_bootstrap_contract",
            "lifecycle_property_harness_runs_checkpoint_contract",
            "lifecycle_property_harness_runs_maintenance_contract",
            "lifecycle_property_harness_runs_flush_contract",
            "lifecycle_property_harness_runs_generated_script_contract",
            "lifecycle_property_harness_requires_input_derived_recovery_routes",
            "lifecycle_property_harness_requires_input_derived_maintenance_routes",
            "lifecycle_property_harness_requires_input_derived_retention_routes",
            "lifecycle_property_harness_requires_input_derived_quarantine_routes",
            "lifecycle_property_harness_requires_input_derived_close_routes",
            "lifecycle_property_harness_replays_minimized_failure_case",
            "lifecycle_property_harness_records_regression_file",
        ],
    );
    assert_contains_all(
        &properties,
        &[
            "input_open_recovery_close_route_cases",
            "input_maintenance_route_cases",
            "input_reclaim_route_cases",
            "validation_only_rejection_cases",
            "recovered_visibility_match_cases",
            "deletion_subset_check_cases",
            "watermark_monotonic_check_cases",
            "close_idempotence_check_cases",
            "cache_no_durable_claim_check_cases",
            "lossy_degraded_health_check_cases",
            "model_facts()",
        ],
    );
}

#[test]
fn lifecycle_closeout_fault_windows_cover_required_phases() {
    let root = common::crate_root();
    let faults = read(&root.join("tests/lifecycle_faults.rs"));

    assert_contains_all(
        &faults,
        &[
            "fault_capability_mismatch_happens_before_durable_side_effects",
            "fault_writer_guard_acquired_then_manifest_create_fails_releases_or_reports_guard",
            "fault_manifest_create_visible_but_publish_uncertain_records_health_debt",
            "fault_snapshot_published_manifest_update_fails_records_orphan_snapshot",
            "fault_manifest_updated_wal_truncation_fails_keeps_checkpoint_success",
            "fault_partial_wal_tail_strict_fails_before_repair",
            "fault_partial_wal_tail_lossy_repairs_and_degrades_health",
            "fault_corrupt_wal_returns_typed_recovery_error",
            "fault_replay_failure_transitions_bootstrap_to_failed",
            "fault_replay_visible_publication_failure_records_durable_not_visible",
            "fault_flush_table_published_branch_install_fails_reports_orphan_table",
            "fault_table_rewrite_branch_swap_failure_preserves_reads",
            "fault_incomplete_retention_proof_blocks_delete_before_backend_access",
            "fault_quarantine_inventory_publish_failure_blocks_purge",
            "fault_purge_delete_success_inventory_update_failure_preserves_debt",
            "fault_close_quiesce_timeout_is_retryable",
            "fault_close_wal_sync_failure_preserves_source_chain",
            "fault_close_manifest_sync_failure_preserves_final_fact_debt",
            "fault_writer_guard_missing_at_release_is_typed",
        ],
    );
    assert_contains_all(
        &faults,
        &[
            "capability_preflight_cases",
            "writer_guard_manifest_create_cases",
            "snapshot_orphan_cases",
            "checkpoint_truncation_debt_cases",
            "partial_log_strict_cases",
            "partial_log_lossy_cases",
            "corrupt_log_typed_cases",
            "replay_failed_state_cases",
            "replay_visible_debt_cases",
            "flush_orphan_table_cases",
            "rewrite_preserved_read_cases",
            "retention_blocked_delete_cases",
            "quarantine_publish_blocked_purge_cases",
            "purge_delete_debt_cases",
            "close_quiesce_timeout_cases",
            "close_log_sync_source_cases",
            "close_manifest_sync_debt_cases",
            "writer_guard_release_typed_cases",
        ],
    );
}

#[cfg(all(
    feature = "testkit",
    feature = "fault-injection",
    not(target_arch = "wasm32")
))]
#[test]
fn lifecycle_closeout_fault_contract_is_input_routed() {
    let capability = strata_storage::testkit::check_lifecycle_fault_contract(b"fault-capability")
        .expect("capability fault route");
    let close = strata_storage::testkit::check_lifecycle_fault_contract(b"fault-close-wal-sync")
        .expect("close fault route");

    assert!(capability.capability_preflight_cases() > 0);
    assert_eq!(capability.close_log_sync_source_cases(), 0);
    assert!(close.close_log_sync_source_cases() > 0);
    assert_eq!(close.capability_preflight_cases(), 0);
}

#[test]
fn lifecycle_closeout_crash_windows_cover_required_phases() {
    let root = common::crate_root();
    let crash = read(&root.join("tests/crash_recovery.rs"));

    assert_contains_all(
        &crash,
        &[
            "crash_after_wal_append_before_visibility_replays_record",
            "crash_after_wal_append_with_unresolved_gate_reconciles_on_reopen",
            "crash_after_snapshot_publish_before_manifest_update_ignores_orphan_snapshot",
            "crash_after_manifest_update_before_wal_truncation_recovers_checkpoint_and_tail",
            "crash_after_table_publish_before_branch_install_reports_orphan_table",
            "crash_after_quarantine_inventory_publish_before_object_move_reports_debt",
            "crash_after_object_quarantine_before_purge_preserves_quarantine_entry",
            "crash_after_close_wal_sync_before_guard_release_reopens_consistently",
            "crash_harness_ignored_cases_have_nonignored_phase_equivalents",
            "crash_harness_respects_case_limit_and_keep_root_environment",
        ],
    );
    assert_contains_all(
        &crash,
        &[
            "feature = \"localfs\"",
            "feature = \"testkit\"",
            "not(target_arch = \"wasm32\")",
            "run_localfs_crash_recovery_harness",
        ],
    );
}

#[cfg(all(feature = "testkit", feature = "localfs", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_closeout_crash_harness_runs_distinct_phase_cases() {
    let tempdir = tempfile::tempdir().expect("temp crash harness root");
    let outcome = strata_storage::testkit::run_localfs_crash_recovery_harness(tempdir.path(), None)
        .expect("crash harness");

    assert!(outcome.cases_executed() >= 8);
    // The ignored-pairing counter is derived from a real scan of
    // tests/crash_recovery.rs (see `verify_ignored_crash_test_pairing`). It
    // is the number of validated `#[ignore]` -> non-ignored phase pairs, or
    // 1 if there are no ignored tests (the invariant is trivially upheld).
    // Either way it must be at least 1; equality with `cases_executed` would
    // re-encode the old tautology and silently mask scan regressions.
    assert!(outcome.ignored_case_equivalent_cases() >= 1);
    assert!(outcome.log_append_replay_cases() > 0);
    assert!(outcome.unresolved_gate_reconcile_cases() > 0);
    assert!(outcome.orphan_snapshot_ignored_cases() > 0);
    assert!(outcome.checkpoint_tail_recovered_cases() > 0);
    assert!(outcome.orphan_table_reported_cases() > 0);
    assert!(outcome.quarantine_inventory_debt_cases() > 0);
    assert!(outcome.object_quarantine_preserved_cases() > 0);
    assert!(outcome.close_reopen_consistent_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_closeout_crash_contract_is_input_routed() {
    let replay = strata_storage::testkit::check_lifecycle_crash_contract(b"crash-wal-append")
        .expect("replay crash route");
    let close = strata_storage::testkit::check_lifecycle_crash_contract(b"crash-close-reopen")
        .expect("close crash route");

    assert!(replay.log_append_replay_cases() > 0);
    assert_eq!(replay.close_reopen_consistent_cases(), 0);
    assert!(close.close_reopen_consistent_cases() > 0);
    assert_eq!(close.log_append_replay_cases(), 0);
}

#[test]
fn lifecycle_closeout_fuzz_targets_and_corpora_are_distinct() {
    let root = common::crate_root();
    let manifest = read(&root.join("fuzz/Cargo.toml"));
    let inventory = read(&root.join("tests/lifecycle_fuzz_inventory.rs"));
    let targets = [
        (
            "lifecycle_recovery",
            "check_lifecycle_recovery_fuzz_contract",
            ["valid_seed", "corrupt_seed", "mixed_seed"],
        ),
        (
            "lifecycle_maintenance",
            "check_lifecycle_maintenance_fuzz_contract",
            ["valid_seed", "fault_seed", "close_seed"],
        ),
        (
            "lifecycle_retention",
            "check_lifecycle_retention_fuzz_contract",
            ["valid_seed", "blocked_seed", "purge_seed"],
        ),
    ];
    let contracts = [
        "check_lifecycle_recovery_fuzz_contract",
        "check_lifecycle_maintenance_fuzz_contract",
        "check_lifecycle_retention_fuzz_contract",
    ];

    for (target, contract, seeds) in targets {
        assert!(manifest.contains(&format!("name = \"{target}\"")));
        assert!(manifest.contains(&format!("fuzz_targets/{target}.rs")));
        let target_text = read(&root.join(format!("fuzz/fuzz_targets/{target}.rs")));
        assert!(target_text.contains(contract));
        for other in contracts {
            if other != contract {
                assert!(
                    !target_text.contains(other),
                    "{target} should not reference unrelated lifecycle fuzz contract {other}"
                );
            }
        }
        assert!(!target_text.contains("check_lifecycle_scaffold_contract"));
        assert!(!target_text.contains("check_lifecycle_generated_script_contract(data)"));

        for seed in seeds {
            let path = root.join(format!("fuzz/corpus/{target}/{seed}"));
            assert!(path.is_file(), "{} is missing", path.display());
            assert!(fs::metadata(&path).expect("seed metadata").len() > 0);
        }
    }
    assert_contains_all(
        &inventory,
        &[
            "lifecycle_fuzz_targets_are_registered",
            "lifecycle_fuzz_targets_call_distinct_contracts",
            "lifecycle_fuzz_corpora_have_non_empty_seed_files",
            "lifecycle_fuzz_named_seeds_are_structured_route_scripts",
            "lifecycle_recovery_fuzz_seed_hits_valid_and_corrupt_routes",
            "lifecycle_maintenance_fuzz_seed_hits_task_and_close_routes",
            "lifecycle_retention_fuzz_seed_hits_delete_and_defer_routes",
            "recovered_visibility_match_cases() > 0",
            "lossy_degraded_health_check_cases() > 0",
            "input_maintenance_route_cases() > 0",
            "close_idempotence_check_cases() > 0",
            "input_reclaim_route_cases() > 0",
            "deletion_subset_check_cases() > 0",
        ],
    );
}

#[test]
fn lifecycle_closeout_source_guards_cover_required_boundaries() {
    let root = common::crate_root();
    let guard = read(&root.join("tests/lifecycle_source_guard.rs"));

    let required_guards = [
        "lifecycle_source_does_not_import_engine_product_or_raw_io",
        "lifecycle_implementation_avoids_architecture_labels",
        "lifecycle_stays_crate_private",
        "lower_layers_do_not_import_lifecycle_upward",
        "lifecycle_capability_validator_stays_preflight_only",
        "lifecycle_production_does_not_import_testkit_or_fuzz",
        "lifecycle_fuzz_targets_use_distinct_contracts",
        "lifecycle_fuzz_corpora_are_seeded",
        "lifecycle_crash_tests_are_feature_gated",
        "ignored_crash_tests_have_nonignored_phase_equivalents",
        "lifecycle_generated_properties_assert_input_derived_counters",
        "lifecycle_assurance_tests_avoid_sleeps_and_thread_spawns",
        "lifecycle_cache_runtime_stays_cache_only",
        "lifecycle_durable_runtime_stays_bootstrap_only",
        "lifecycle_bootstrap_runtime_does_not_perform_durable_assembly",
        "lifecycle_recovery_runtime_does_not_call_commit_replay_or_product_hooks",
        "lifecycle_checkpoint_runtime_avoids_segment_parsing_and_direct_delete",
        "lifecycle_maintenance_executor_stays_scheduler_only",
        "lifecycle_durable_maintenance_stays_out_of_assembly_and_bootstrap",
        "lifecycle_durable_close_stays_out_of_assembly_bootstrap_and_cache",
        "lifecycle_table_rewrite_source_uses_branch_runtime_boundaries",
        "lifecycle_retention_source_delegates_durable_mutation",
        "lifecycle_quarantine_source_uses_quarantine_service_boundary",
        "cache_compaction_does_not_call_table_object_service",
    ];
    assert_contains_all(&guard, &required_guards);
    // Behavioral non-vacuousness check: for each required guard, extract
    // its function body and assert it carries at least one non-trivial
    // assertion. A guard reduced to `assert!(true)` or an empty body would
    // pass the substring-presence check above but provide no real
    // enforcement; the body scan below catches that failure mode.
    for guard_name in required_guards {
        let body = extract_fn_body(&guard, guard_name).unwrap_or_else(|| {
            panic!("could not extract body of guard `{guard_name}` for non-vacuousness scan",)
        });
        let assertion_count = count_assertions(&body);
        assert!(
            assertion_count >= 1,
            "guard `{guard_name}` has only {assertion_count} non-trivial assertion(s); body looks vacuous",
        );
        assert!(
            !body_is_trivially_true(&body),
            "guard `{guard_name}` body reduces to `assert!(true)` or similar tautology",
        );
    }
    assert_contains_all(
        &guard,
        &[
            "strata_engine",
            "stratahub",
            "crate::testkit",
            "std::fs",
            "std::env",
            "contains_sleep_or_thread_spawn",
        ],
    );
}

#[test]
fn lifecycle_closeout_integration_surfaces_cover_required_categories() {
    let root = common::crate_root();
    let maintenance = read(&root.join("tests/lifecycle_maintenance.rs"));
    let recovery = read(&root.join("tests/lifecycle_recovery.rs"));

    assert_contains_all(
        &maintenance,
        &[
            "lifecycle_generated_integration_runs_default_mode_script",
            "lifecycle_generated_integration_runs_durable_mode_script",
            "lifecycle_generated_integration_runs_reclaim_close_script",
            "lifecycle_fault_integration_covers_all_phase_families",
            "lifecycle_crash_integration_reports_case_counts",
            "generated_maintenance_model_matches_enqueue_coalesce_run_cancel_drain",
            "generated_flush_preserves_read_parity_and_candidate_watermark",
            "generated_checkpoint_truncation_never_removes_uncovered_wal_records",
            "generated_table_rewrite_preserves_reads_after_compaction",
            "generated_retention_never_deletes_reachable_tables_or_live_snapshots",
            "lifecycle_quarantine_integration",
            "lifecycle_purge_integration",
            "lifecycle_repair_reconciliation_integration",
            "lifecycle_quarantine_then_purge_round_trip",
            "lifecycle_quarantine_publish_failure_surfaces_health_debt",
            "generated_quarantine_happens_before_purge",
            "generated_purge_requires_fresh_inventory_proof",
            "generated_repair_reports_inconclusive_without_mutating_state",
            "generated_close_blocks_new_commits_and_ordinary_maintenance",
        ],
    );
    assert_contains_all(
        &recovery,
        &[
            "lifecycle_recovery_contract_exercises_storage_recovery_paths",
            "lifecycle_bootstrap_contract_exercises_commit_bootstrap_paths",
            "generated_recovery_empty_checkpoint_tail_and_lossy_routes_are_input_driven",
            "generated_recovery_corrupt_manifest_snapshot_wal_and_table_are_typed",
            "generated_bootstrap_catches_allocator_timestamp_and_visible_facts",
            "generated_bootstrap_rejects_timeline_mismatch",
            "generated_bootstrap_reconciles_unresolved_durable_gate",
            "generated_recovery_health_matches_fault_family_model",
        ],
    );
}

#[test]
fn closeout_files_avoid_architecture_labels() {
    let root = common::crate_root();
    for closeout_file in [
        root.join("tests/commit_runtime_closeout.rs"),
        root.join("tests/table_runtime_closeout.rs"),
        root.join("tests/lifecycle_closeout.rs"),
        root.join("tests/branch_lsm_closeout.rs"),
    ] {
        let text = fs::read_to_string(&closeout_file).expect("read closeout file");
        for (line_number, line) in text.lines().enumerate() {
            assert!(
                !common::source_guard_helpers::contains_milestone_label(line),
                "{}:{} contains architecture label: {line}",
                closeout_file
                    .strip_prefix(&root)
                    .unwrap_or(&closeout_file)
                    .display(),
                line_number + 1
            );
        }
    }
}

#[test]
fn lifecycle_closeout_has_no_mutation_probe_artifacts() {
    let root = workspace_root();
    let mut files = Vec::new();
    collect_files(&root, &mut files);

    for path in files {
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        assert!(
            !is_mutation_probe_artifact(name),
            "{} looks like a mutation-probe artifact",
            path.strip_prefix(&root).unwrap_or(&path).display()
        );
    }
}

fn workspace_root() -> PathBuf {
    let crate_root = common::crate_root();
    crate_root
        .parent()
        .and_then(|crates_dir| crates_dir.parent())
        .expect("crate root is under workspace crates directory")
        .to_path_buf()
}

fn assert_contains_all(text: &str, needles: &[&str]) {
    for needle in needles {
        assert!(text.contains(needle), "missing closeout evidence: {needle}");
    }
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

fn collect_files(dir: &Path, files: &mut Vec<PathBuf>) {
    if should_skip_dir(dir) {
        return;
    }
    for entry in fs::read_dir(dir).unwrap_or_else(|error| panic!("read {}: {error}", dir.display()))
    {
        let path = entry.expect("read directory entry").path();
        if path.is_dir() {
            collect_files(&path, files);
        } else {
            files.push(path);
        }
    }
}

fn should_skip_dir(dir: &Path) -> bool {
    dir.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            matches!(
                name,
                ".git" | "target" | ".nextest" | "node_modules" | ".idea" | ".vscode"
            )
        })
}

/// Extract the body of `fn <fn_name>(` from `source`, returning the
/// substring between the opening `{` (after the parameter list) and the
/// matching closing `}`. Returns `None` if the function definition is
/// not found or the braces do not balance. Used by the closeout
/// behavioral-check to scan guard bodies for non-vacuous assertions.
fn extract_fn_body(source: &str, fn_name: &str) -> Option<String> {
    let needle = format!("fn {fn_name}(");
    let start = source.find(&needle)?;
    let open = source[start..].find('{')? + start;
    let mut depth = 0u32;
    let bytes = source.as_bytes();
    for (offset, byte) in bytes.iter().enumerate().skip(open) {
        match byte {
            b'{' => depth = depth.saturating_add(1),
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(source[open + 1..offset].to_owned());
                }
            }
            _ => {}
        }
    }
    None
}

/// Count occurrences of non-trivial assertion-like patterns in a
/// function body: `assert!`, `assert_eq!`, `assert_ne!`, `assert_matches!`,
/// `.expect(`, `.unwrap()`, panic-by-message (`panic!`), or `Err(`.
/// Catches whether a guard does *something* beyond a single
/// tautological `assert!(true)`.
fn count_assertions(body: &str) -> usize {
    let patterns = [
        "assert!",
        "assert_eq!",
        "assert_ne!",
        "assert_matches!",
        ".expect(",
        ".unwrap()",
        "panic!",
        "Err(",
    ];
    patterns
        .iter()
        .map(|pattern| body.matches(pattern).count())
        .sum()
}

/// Returns true if the body is a vacuous hollow-out: empty, or composed
/// solely of repeated `assert!(true)` invocations (with arbitrary
/// whitespace/semicolons between them). Catches both the single-
/// `assert!(true)` and the `assert!(true); assert!(true);` patterns —
/// any number of literally-true asserts collapses to the same tripwire.
/// A real guard touches at least one external symbol or makes a
/// comparative assertion.
fn body_is_trivially_true(body: &str) -> bool {
    let stripped: String = body
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ';')
        .collect();
    if stripped.is_empty() {
        return true;
    }
    // Any positive number of `assert!(true)` runs concatenated together.
    let mut remaining = stripped.as_str();
    while let Some(rest) = remaining.strip_prefix("assert!(true)") {
        remaining = rest;
    }
    remaining.is_empty()
}

#[allow(
    clippy::case_sensitive_file_extension_comparisons,
    reason = "`name` is already lowercased into `lower`, so case-sensitive extension comparison against a literal-lowercase pattern is the intended behavior"
)]
fn is_mutation_probe_artifact(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    // In-house probe naming convention.
    if lower.contains("mutation-probe")
        || lower.contains("mutation_probe")
        || lower.ends_with(".mutant")
        || lower.ends_with(".mutation")
    {
        return true;
    }
    // cargo-mutants writes its working tree under `mutants.out/` and its
    // result summary as `outcomes.json`; mutmut prefixes its outputs with
    // `mutmut`. Patch/sed/`git` leave `*.orig`, `*.rej`, `*.bak` files
    // when a mutation diff is applied or partially reverted.
    if lower == "mutants.out" || lower.starts_with("mutants.out.") {
        return true;
    }
    if lower == "outcomes.json" {
        return true;
    }
    if lower.starts_with("mutmut") {
        return true;
    }
    if lower.ends_with(".orig") || lower.ends_with(".rej") || lower.ends_with(".bak") {
        return true;
    }
    false
}

#[test]
fn lifecycle_closeout_mutation_probe_detector_matches_known_patterns() {
    // Fixture self-test for `is_mutation_probe_artifact`. Any future
    // editor that narrows the detector must update this list.
    for positive in [
        "snapshot.mutation-probe.rs",
        "snapshot_mutation_probe",
        "scratch.mutant",
        "diff.mutation",
        "mutants.out",
        "mutants.out.dir",
        "outcomes.json",
        "mutmut_results",
        "patch.orig",
        "patch.rej",
        "file.bak",
    ] {
        assert!(
            is_mutation_probe_artifact(positive),
            "expected {positive} to be flagged as a mutation-probe artifact",
        );
    }
    for negative in [
        "lifecycle_closeout.rs",
        "snapshot_service.rs",
        "outcomes_test.rs",
        "mutex.rs",
    ] {
        assert!(
            !is_mutation_probe_artifact(negative),
            "expected {negative} NOT to be flagged",
        );
    }
}
