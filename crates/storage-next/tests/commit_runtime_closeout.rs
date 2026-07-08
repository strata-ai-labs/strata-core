//! Closeout inventory for the commit-runtime implementation.

#![deny(unsafe_code)]

mod common;

use std::fs;
use std::path::Path;

#[cfg(feature = "testkit")]
use strata_storage_next::testkit::CommitRuntimeAssuranceOutcome;

const COMMIT_RUNTIME_FUZZ_TARGETS: [(&str, &str); 4] = [
    (
        "commit_runtime_batch",
        "check_commit_runtime_batch_contract",
    ),
    (
        "commit_runtime_conflict",
        "check_commit_runtime_conflict_contract",
    ),
    (
        "commit_runtime_durable",
        "check_commit_runtime_durable_contract",
    ),
    (
        "commit_runtime_timeline",
        "check_commit_runtime_timeline_contract",
    ),
];

const ASSURANCE_COUNTERS: [&str; 13] = [
    "decoded_scripts",
    "model_parity_checks",
    "cache_successes",
    "durable_successes",
    "wal_failures",
    "post_wal_failures",
    "conflict_rejections",
    "read_only_diagnostics",
    "guard_or_quiesce_rejections",
    "branch_lifecycle_transitions",
    "branch_lifecycle_rejections",
    "replay_successes",
    "timeline_checks",
];

#[test]
fn commit_runtime_closeout_generated_assurance_exposes_required_counters() {
    let root = common::crate_root();
    let runner = read_file(&root.join("src/testkit/commit_runtime_runner.rs"));
    let properties = read_file(&root.join("tests/commit_runtime_properties.rs"));

    assert!(
        runner.contains("pub struct CommitRuntimeAssuranceOutcome"),
        "generated commit-runtime assurance outcome should be explicit"
    );

    for counter in ASSURANCE_COUNTERS {
        assert!(
            runner.contains(&format!("pub {counter}: usize")),
            "CommitRuntimeAssuranceOutcome should expose counter {counter}"
        );
        assert!(
            properties.contains(&format!("outcome.{counter}")),
            "commit_runtime_properties should assert generated counter {counter}"
        );
    }

    for contract in [
        "check_commit_runtime_batch_contract",
        "check_commit_runtime_conflict_contract",
        "check_commit_runtime_durable_contract",
        "check_commit_runtime_fault_contract",
        "check_commit_runtime_script_contract",
        "check_commit_runtime_timeline_contract",
    ] {
        assert!(
            properties.contains(contract),
            "commit_runtime_properties should exercise {contract}"
        );
    }
}

#[test]
fn commit_runtime_closeout_fault_tests_cover_required_phase_families() {
    let root = common::crate_root();
    let faults = read_file(&root.join("tests/commit_runtime_faults.rs"));
    let cache_tests = read_file(&root.join("src/commit/tests/cache.rs"));
    let durable_tests = read_file(&root.join("src/commit/tests/durable.rs"));
    let replay_tests = read_file(&root.join("src/commit/tests/replay.rs"));

    for required in [
        "cache_branch_admission_rejection_cases",
        "cache_conflict_rejection_cases",
        "cache_applied_above_visible_rejection_cases",
        "durable_clean_wal_failure_cases",
        "durable_uncertain_wal_failure_cases",
        "durable_guard_release_after_failure_cases",
        "durable_not_applied_gate_cases",
        "durable_applied_not_visible_gate_cases",
        "durable_unresolved_gate_block_cases",
        "durable_unresolved_gate_cache_block_cases",
        "durable_clean_wal_no_gate_cases",
        "durable_uncertain_wal_no_gate_cases",
        "generated.wal_failures",
        "generated.post_wal_failures",
        "generated.replay_successes",
        "generated.guard_or_quiesce_rejections",
        "generated.branch_lifecycle_transitions",
        "generated.branch_lifecycle_rejections",
    ] {
        assert!(
            faults.contains(required),
            "commit_runtime_faults should assert required phase family {required}"
        );
    }

    for required in [
        "cache_commit_branch_admission_failures_reject_before_allocation",
        "cache_commit_conflict_rejects_before_allocation_or_mutation",
        "cache_commit_rejects_unpublished_branch_rows_before_allocation",
        "cache_commit_visible_publication_failure_reports_applied_not_visible_and_releases_guard",
        "cache_commit_rejects_any_unresolved_durable_gate_before_allocation",
    ] {
        assert!(
            cache_tests.contains(required),
            "cache direct tests should exercise fault window {required}"
        );
    }
    for required in [
        "durable_clean_wal_failure_leaves_no_visible_rows_but_allocation_may_gap",
        "durable_uncertain_wal_failure_is_distinct_and_leaves_no_visible_rows",
        "durable_always_commit_rejects_unforced_success_before_apply_phase",
        "durable_writer_halted_failure_is_clean_durability_unavailable",
        "durable_apply_failure_after_wal_success_records_durable_not_applied_gate",
        "durable_visibility_failure_after_apply_records_applied_not_visible_gate",
        "durable_commit_proceeds_alongside_open_admission_span_other_branch",
        "unresolved_durable_gate_blocks_durable_commit_before_allocation_and_wal_append",
    ] {
        assert!(
            durable_tests.contains(required),
            "durable direct tests should exercise fault window {required}"
        );
    }
    for required in [
        "replay_apply_failure_records_durable_not_applied_gate_without_catch_up",
        "replay_visible_publish_failure_records_applied_not_visible_gate",
    ] {
        assert!(
            replay_tests.contains(required),
            "replay direct tests should exercise fault window {required}"
        );
    }
}

#[test]
fn commit_runtime_closeout_replay_contracts_are_exercised() {
    let root = common::crate_root();
    let replay = read_file(&root.join("src/commit/tests/replay.rs"));

    for required in [
        "replaying_same_record_twice_does_not_duplicate_rows",
        "replay_exact_duplicate_is_idempotent_catches_up_and_clears_matching_gate",
        "replay_rejects_duplicate_mismatch_without_mutating_state",
        "replay_duplicate_mismatch_checks_timestamp_expiry_and_tombstone",
        "replay_rejects_record_without_timeline_pair",
        "replay_rejects_timeline_pair_that_disagrees_with_wal_facts",
        "replay_apply_failure_records_durable_not_applied_gate_without_catch_up",
        "replay_visible_publish_failure_records_applied_not_visible_gate",
    ] {
        assert!(
            replay.contains(&format!("fn {required}(")),
            "commit replay tests should include {required}"
        );
    }
}

#[test]
fn commit_runtime_closeout_cache_and_durable_visibility_safety_is_exercised() {
    let root = common::crate_root();
    let cache = read_file(&root.join("src/commit/tests/cache.rs"));
    let durable = read_file(&root.join("src/commit/tests/durable.rs"));
    let properties = read_file(&root.join("tests/commit_runtime_properties.rs"));

    for required in [
        "cache_commit_apply_failure_releases_guard_without_visible_publication",
        "cache_commit_visible_publication_failure_reports_applied_not_visible_and_releases_guard",
        "cache_commit_rejects_allocator_visible_mismatch_before_apply",
        "cache_commit_rejects_any_unresolved_durable_gate_before_allocation",
    ] {
        assert!(
            cache.contains(&format!("fn {required}(")),
            "cache commit tests should include visibility safety case {required}"
        );
    }

    for required in [
        "durable_standard_commit_appends_wal_record_then_applies_rows_and_publishes_visible",
        "durable_rejects_unpublished_branch_rows_before_allocation_or_wal_append",
        "durable_clean_wal_failure_leaves_no_visible_rows_but_allocation_may_gap",
        "durable_uncertain_wal_failure_is_distinct_and_leaves_no_visible_rows",
        "durable_apply_failure_after_wal_success_records_durable_not_applied_gate",
        "durable_visibility_failure_after_apply_records_applied_not_visible_gate",
    ] {
        assert!(
            durable.contains(&format!("fn {required}(")),
            "durable commit tests should include visibility safety case {required}"
        );
    }

    for counter in [
        "cache_applied_above_visible_rejection_cases",
        "cache_visible_allocator_mismatch_rejection_cases",
        "durable_not_applied_gate_cases",
        "durable_applied_not_visible_gate_cases",
        "durable_clean_wal_no_gate_cases",
        "durable_uncertain_wal_no_gate_cases",
    ] {
        assert!(
            properties.contains(counter),
            "generated properties should assert visibility safety counter {counter}"
        );
    }
}

#[test]
fn commit_runtime_closeout_fuzz_inventory_is_registered_seeded_and_distinct() {
    let root = common::crate_root();
    let manifest = read_file(&root.join("fuzz/Cargo.toml"));
    assert!(
        manifest.contains(
            "strata-storage-next = { path = \"..\", default-features = false, features = [\"testkit\"] }"
        ),
        "commit runtime fuzz crate must depend on storage-next with only the testkit feature"
    );

    for (target, contract) in COMMIT_RUNTIME_FUZZ_TARGETS {
        let target_path = root.join(format!("fuzz/fuzz_targets/{target}.rs"));
        let target_text = read_file(&target_path);
        assert!(
            manifest.contains(&format!("name = \"{target}\"")),
            "fuzz/Cargo.toml should register {target}"
        );
        assert!(
            manifest.contains(&format!("path = \"fuzz_targets/{target}.rs\"")),
            "fuzz/Cargo.toml should point {target} at its target file"
        );
        assert!(
            target_text.contains(contract),
            "{target} should call its focused contract {contract}"
        );
        assert!(
            target_text.contains("use strata_storage_next::testkit::"),
            "{target} should route through the testkit boundary"
        );
        assert!(
            !target_text.contains("strata_storage_next::commit")
                && !target_text.contains("strata_storage_next::{commit")
                && !target_text.contains("crate::commit")
                && !target_text.contains("super::commit"),
            "{target} must not import commit runtime internals directly"
        );
        assert!(
            !target_text.contains("check_commit_runtime_scaffold_contract"),
            "{target} must not route fuzz bytes through the broad scaffold contract"
        );
        for (_, other_contract) in COMMIT_RUNTIME_FUZZ_TARGETS {
            if other_contract != contract {
                assert!(
                    !target_text.contains(other_contract),
                    "{target} should not call unrelated contract {other_contract}"
                );
            }
        }

        let corpus = root.join(format!("fuzz/corpus/{target}"));
        assert!(
            corpus.join("generated-script").is_file(),
            "{target} corpus should include the generated-script seed"
        );
        assert!(
            corpus.join("fault-script").is_file(),
            "{target} corpus should include the fault-script seed"
        );
        for seed in ["generated-script", "fault-script"] {
            let seed_path = corpus.join(seed);
            let metadata = fs::metadata(&seed_path)
                .unwrap_or_else(|error| panic!("read {}: {error}", seed_path.display()));
            assert!(
                metadata.len() > 0,
                "{} should be non-empty",
                seed_path.display()
            );
        }
    }
}

#[test]
fn commit_runtime_closeout_fuzz_inventory_executes_seed_success_and_failure_routes() {
    let root = common::crate_root();
    let fuzz_inventory = read_file(&root.join("tests/commit_runtime_fuzz_inventory.rs"));

    for required in [
        "commit_runtime_fuzz_corpus_seeds_exercise_success_and_failure_routes",
        "run_target_contract",
        "seed_reaches_success_route",
        "seed_reaches_failure_route",
        "saw_success",
        "saw_failure",
    ] {
        assert!(
            fuzz_inventory.contains(required),
            "commit runtime fuzz inventory should execute corpus seeds through {required}"
        );
    }
}

#[test]
fn commit_runtime_closeout_source_tests_cover_boundary_inventory() {
    let root = common::crate_root();
    let source_guard = read_file(&root.join("tests/commit_runtime_source_guard.rs"));

    for required in [
        "commit_runtime_source_does_not_import_upper_layers_engines_or_product_api",
        "commit_runtime_source_does_not_use_product_session_or_payload_vocabulary",
        "commit_runtime_source_does_not_use_table_backend_layout_or_io_apis",
        "commit_runtime_stays_crate_private",
        "commit_runtime_source_guard_allows_durable_wal_and_replay_boundaries",
        "commit_runtime_source_guard_pins_timeline_module_boundary",
        "lower_storage_layers_do_not_import_commit_runtime_upward",
    ] {
        assert!(
            source_guard.contains(required),
            "commit_runtime_source_guard should include boundary check {required}"
        );
    }
}

#[test]
fn commit_runtime_closeout_has_no_mutation_probe_scratch_artifacts() {
    let root = common::crate_root();
    for relative_dir in [
        "src/commit",
        "src/testkit",
        "tests",
        "fuzz/fuzz_targets",
        "fuzz/corpus",
    ] {
        let mut files = Vec::new();
        collect_files(&root.join(relative_dir), &mut files);
        for file in files {
            let relative = file.strip_prefix(&root).unwrap_or(&file);
            let name = file
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            let extension = file
                .extension()
                .and_then(|extension| extension.to_str())
                .unwrap_or_default();
            assert!(
                !matches!(extension, "orig" | "rej" | "tmp" | "bak")
                    && !name.contains("mutation-probe")
                    && !name.contains("probe-scratch")
                    && !name.contains("mutant"),
                "mutation-probe scratch artifact should not remain: {}",
                relative.display()
            );
        }
    }
}

#[cfg(feature = "testkit")]
#[test]
fn commit_runtime_closeout_generated_contracts_exercise_required_categories() {
    use strata_storage_next::testkit::{
        check_commit_runtime_batch_contract, check_commit_runtime_conflict_contract,
        check_commit_runtime_durable_contract, check_commit_runtime_fault_contract,
        check_commit_runtime_script_contract, check_commit_runtime_timeline_contract,
    };

    let seed = b"commit-runtime-closeout-counters-v1";
    let generated = check_commit_runtime_script_contract(seed).expect("generated script contract");
    assert_required_generated_categories(&generated);

    let batch = check_commit_runtime_batch_contract(seed).expect("batch contract");
    assert!(batch.cache_successes > 0);
    assert!(batch.read_only_diagnostics > 0);
    assert!(batch.branch_lifecycle_transitions > 0);
    assert!(batch.branch_lifecycle_rejections > 0);

    let conflict = check_commit_runtime_conflict_contract(seed).expect("conflict contract");
    assert!(conflict.conflict_rejections > 0);

    let durable = check_commit_runtime_durable_contract(seed).expect("durable contract");
    assert!(durable.durable_successes > 0);
    assert!(durable.wal_failures > 0);
    assert!(durable.post_wal_failures > 0);
    assert!(durable.replay_successes > 0);

    let timeline = check_commit_runtime_timeline_contract(seed).expect("timeline contract");
    assert!(timeline.timeline_checks > 0);

    let fault = check_commit_runtime_fault_contract(seed).expect("fault contract");
    assert!(fault.wal_failures > 0);
    assert!(fault.post_wal_failures > 0);
}

#[cfg(feature = "testkit")]
fn assert_required_generated_categories(outcome: &CommitRuntimeAssuranceOutcome) {
    assert!(outcome.decoded_scripts > 0);
    assert!(outcome.model_parity_checks > 0);
    assert!(outcome.cache_successes > 0);
    assert!(outcome.durable_successes > 0);
    assert!(outcome.wal_failures >= 4);
    assert!(outcome.post_wal_failures >= 2);
    assert!(outcome.conflict_rejections > 0);
    assert!(outcome.read_only_diagnostics > 0);
    assert!(outcome.guard_or_quiesce_rejections > 0);
    assert!(outcome.branch_lifecycle_transitions > 0);
    assert!(outcome.branch_lifecycle_rejections > 0);
    assert!(outcome.replay_successes >= 2);
    assert!(outcome.timeline_checks > 0);
}

fn read_file(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

fn collect_files(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
    for entry in fs::read_dir(dir).unwrap_or_else(|error| panic!("read {}: {error}", dir.display()))
    {
        let entry = entry.unwrap_or_else(|error| panic!("read entry {}: {error}", dir.display()));
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, files);
        } else {
            files.push(path);
        }
    }
}
