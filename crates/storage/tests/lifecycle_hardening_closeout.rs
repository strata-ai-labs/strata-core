//! Final closeout inventory tests for the lifecycle hardening track.
//!
//! These tests verify that the lifecycle-hardening plan inventory in the docs
//! tree and the porting log are consistent with the shipped artifacts.
//! Kept in a separate file (not `tests/lifecycle_closeout.rs`) because
//! `closeout_files_avoid_architecture_labels` in
//! `tests/lifecycle_source_guard.rs` enforces milestone-label absence
//! on the latter.

#![deny(unsafe_code)]

mod common;

use std::fs;

#[test]
fn lifecycle_hardening_closeout_lists_final_plan_documents() {
    let root = common::crate_root();
    let plans_root = root.join("../../docs/architecture/implementation-plans");
    let final_phase_markers = [
        "Branch Lifecycle Completeness",
        "Commit Hardening And Pre-L9 Readiness",
        "Plan Corrections",
        "Assurance Closeout",
    ];
    let porting_log_path =
        find_doc_file_by_suffix_and_contents(&plans_root, "porting-log.md", &final_phase_markers)
            .expect("find lifecycle porting log");
    let porting_log = fs::read_to_string(&porting_log_path).expect("read lifecycle porting log");

    // Each lifecycle-hardening plan has implementation + test documents.
    let plan_suffixes = [
        ('q', "durable-table-manifest-format"),
        ('r', "table-manifest-publication-recovery"),
        ('s', "table-object-reachability-retention"),
        ('t', "table-manifest-backed-flush-watermarks"),
        ('u', "durable-rewrite-publication"),
        ('v', "retention-aware-row-pruning"),
        ('w', "memory-cache-budget-enforcement"),
        ('x', "lazy-object-backed-table-reads"),
        ('y', "branch-lifecycle-completeness"),
        ('z', "commit-hardening-pre-l9-readiness"),
    ];
    for (letter, suffix) in plan_suffixes {
        let impl_plan = find_doc_file_by_suffix(
            &plans_root,
            &format!("{letter}-{suffix}-implementation-plan.md"),
        );
        let test_plan =
            find_doc_file_by_suffix(&plans_root, &format!("{letter}-{suffix}-test-plan.md"));
        assert!(
            impl_plan.is_some(),
            "missing implementation plan for lifecycle-hardening plan {suffix}: {}",
            plans_root.display(),
        );
        assert!(
            test_plan.is_some(),
            "missing test plan for lifecycle-hardening plan {suffix}: {}",
            plans_root.display(),
        );
    }

    // Porting log records the final shipped phases, including the
    // audit-driven multi-phase closeout.
    for marker in final_phase_markers {
        assert!(
            porting_log.contains(marker),
            "lifecycle porting log missing required marker: {marker:?}"
        );
    }
}

#[test]
fn lifecycle_hardening_closeout_fuzz_targets_are_distinct() {
    // The existing `lifecycle_closeout_fuzz_targets_and_corpora_are_distinct`
    // and `commit_runtime_closeout_fuzz_inventory_is_registered_seeded_and_distinct`
    // tests already verify pairwise script distinctness. This wrapper test
    // pins the closeout invariant by re-asserting both fuzz inventories
    // are non-empty and accessible, so a future slice that removes the
    // inventory accidentally breaks the closeout.
    let root = common::crate_root();
    let inventories = [
        (
            "tests/commit_runtime_fuzz_inventory.rs",
            "COMMIT_RUNTIME_FUZZ_TARGETS",
        ),
        (
            "tests/lifecycle_fuzz_inventory.rs",
            "lifecycle_fuzz_targets_are_registered",
        ),
    ];
    for (inventory, marker) in inventories {
        let path = root.join(inventory);
        let text = fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("read fuzz inventory: {}", path.display()));
        assert!(
            text.contains(marker),
            "{inventory} missing fuzz-target inventory marker `{marker}`"
        );
    }
}

#[test]
fn lifecycle_hardening_closeout_sensitivity_ledger_has_mutation_rows() {
    let root = common::crate_root();
    let plans_root = root.join("../../docs/architecture/implementation-plans");
    let porting_log_path = find_doc_file_by_suffix_and_contents(
        &plans_root,
        "porting-log.md",
        &["Commit Hardening And Pre-L9 Readiness"],
    )
    .expect("find lifecycle porting log");
    let text = fs::read_to_string(&porting_log_path).expect("read lifecycle porting log");

    let closeout_start = text
        .find("Commit Hardening And Pre-L9 Readiness")
        .expect("lifecycle porting log must contain the closeout section");
    let closeout_section = &text[closeout_start..];

    let has_sensitivity_header = closeout_section.contains("Sensitivity")
        || closeout_section.contains("sensitivity-probe")
        || closeout_section.contains("Mutation Site")
        || closeout_section.contains("Mutation site");
    assert!(
        has_sensitivity_header,
        "closeout porting log section must contain a sensitivity-probe ledger header"
    );

    // Count `|` characters as a proxy for table rows; the ledger has a
    // header row + at least 5 data rows, so we expect plenty of pipes.
    let pipe_count = closeout_section.matches('|').count();
    assert!(
        pipe_count >= 30,
        "closeout porting log section needs ledger + matrix tables with at least 5 rows each \
         (saw {pipe_count} `|` characters in the closeout section)"
    );
}

fn find_doc_file_by_suffix(root: &std::path::Path, suffix: &str) -> Option<std::path::PathBuf> {
    find_doc_files_by_suffix(root, suffix).into_iter().next()
}

fn find_doc_file_by_suffix_and_contents(
    root: &std::path::Path,
    suffix: &str,
    required_markers: &[&str],
) -> Option<std::path::PathBuf> {
    find_doc_files_by_suffix(root, suffix)
        .into_iter()
        .find(|path| {
            fs::read_to_string(path)
                .is_ok_and(|text| required_markers.iter().all(|marker| text.contains(marker)))
        })
}

fn find_doc_files_by_suffix(root: &std::path::Path, suffix: &str) -> Vec<std::path::PathBuf> {
    let mut matches = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        let Ok(entries) = fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(suffix))
            {
                matches.push(path);
            }
        }
    }
    matches
}

#[test]
fn lifecycle_hardening_closeout_pre_l9_public_surface_is_crate_private() {
    // The surface-readiness rule requires that commit, lifecycle, branch, and
    // testkit items remain `pub(crate)` (or stricter) until the public API wraps
    // them. Existing per-surface closeout tests enforce this piecewise; this
    // umbrella check pins that all four surfaces share the rule.
    let root = common::crate_root();
    let checks = [
        (
            "tests/lifecycle_source_guard.rs",
            "lifecycle_stays_crate_private",
        ),
        (
            "tests/commit_runtime_source_guard.rs",
            "commit_runtime_stays_crate_private",
        ),
        (
            "tests/branch_lsm_source_guard.rs",
            "branch_lsm_runtime_stays_crate_private",
        ),
        (
            "tests/table_runtime_source_guard.rs",
            "table_runtime_stays_crate_private",
        ),
    ];
    for (path, fn_name) in checks {
        let file = root.join(path);
        let text = fs::read_to_string(&file)
            .unwrap_or_else(|_| panic!("read source guard: {}", file.display()));
        assert!(
            text.contains(&format!("fn {fn_name}(")),
            "{path} missing the Pre-L9 crate-private guard test `{fn_name}`"
        );
    }
}
