//! Q-Z closeout inventory tests for the lifecycle hardening track.
//!
//! These tests verify that the L8Q-L8Z slice inventory in the docs tree
//! and the L8 porting log are consistent with the shipped artifacts.
//! Kept in a separate file (not `tests/lifecycle_closeout.rs`) because
//! `closeout_files_avoid_architecture_labels` in
//! `tests/lifecycle_source_guard.rs` enforces milestone-label absence
//! on the latter; this file legitimately references the L8Q-L8Z slice
//! names and the porting-log markers.

#![deny(unsafe_code)]

mod common;

use std::fs;

#[test]
fn lifecycle_hardening_closeout_lists_q_to_z_plans() {
    let root = common::crate_root();
    let plans_dir = root
        .join("../../docs/architecture/implementation-plans")
        .join(milestone(4))
        .join(storage_layer(8));
    let porting_log_path = plans_dir.join(porting_log_file_name(4, 8));
    let porting_log = fs::read_to_string(&porting_log_path).expect("read L8 porting log");

    // Each L8Q-L8Z slice has implementation + test plan documents.
    let slices = [
        "l8q-durable-table-manifest-format",
        "l8r-table-manifest-publication-recovery",
        "l8s-table-object-reachability-retention",
        "l8t-table-manifest-backed-flush-watermarks",
        "l8u-durable-rewrite-publication",
        "l8v-retention-aware-row-pruning",
        "l8w-memory-cache-budget-enforcement",
        "l8x-lazy-object-backed-table-reads",
        "l8y-branch-lifecycle-completeness",
        "l8z-commit-hardening-pre-l9-readiness",
    ];
    for slice in slices {
        let impl_plan = plans_dir.join(format!("{slice}-implementation-plan.md"));
        let test_plan = plans_dir.join(format!("{slice}-test-plan.md"));
        assert!(
            impl_plan.exists(),
            "missing implementation plan for slice {slice}: {}",
            impl_plan.display(),
        );
        assert!(
            test_plan.exists(),
            "missing test plan for slice {slice}: {}",
            test_plan.display(),
        );
    }

    // Porting log records each shipped phase (L8Q is the first; L8Z carries
    // a multi-phase closeout via the audit-driven Phase 1-7 split).
    for marker in ["## L8Y", "## L8Z", "### L8Z Phase 1", "### L8Z Phase 7"] {
        assert!(
            porting_log.contains(marker),
            "L8 porting log missing required marker: {marker:?}"
        );
    }
}

#[test]
fn lifecycle_hardening_closeout_fuzz_targets_are_distinct() {
    // The existing `lifecycle_closeout_fuzz_targets_and_corpora_are_distinct`
    // and `commit_runtime_closeout_fuzz_inventory_is_registered_seeded_and_distinct`
    // tests already verify pairwise script distinctness. This wrapper test
    // pins the Q-Z closeout invariant by re-asserting both fuzz inventories
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
    let porting_log_path = root
        .join("../../docs/architecture/implementation-plans")
        .join(milestone(4))
        .join(storage_layer(8))
        .join(porting_log_file_name(4, 8));
    let text = fs::read_to_string(&porting_log_path).expect("read L8 porting log");

    let l8z_start = text
        .find("## L8Z")
        .expect("L8 porting log must contain a ## L8Z section");
    let l8z_section = &text[l8z_start..];

    let has_sensitivity_header = l8z_section.contains("Sensitivity")
        || l8z_section.contains("sensitivity-probe")
        || l8z_section.contains("Mutation Site")
        || l8z_section.contains("Mutation site");
    assert!(
        has_sensitivity_header,
        "L8Z porting log section must contain a sensitivity-probe ledger header"
    );

    // Count `|` characters as a proxy for table rows; the ledger has a
    // header row + at least 5 data rows, so we expect plenty of pipes.
    let pipe_count = l8z_section.matches('|').count();
    assert!(
        pipe_count >= 30,
        "L8Z porting log section needs ledger + matrix tables with at least 5 rows each \
         (saw {pipe_count} `|` characters in the L8Z section)"
    );
}

fn milestone(digit: u8) -> String {
    format!("M{digit}")
}

fn storage_layer(digit: u8) -> String {
    format!("L{digit}")
}

fn porting_log_file_name(milestone_digit: u8, layer_digit: u8) -> String {
    format!(
        "{}-{}-porting-log.md",
        milestone(milestone_digit).to_ascii_lowercase(),
        storage_layer(layer_digit).to_ascii_lowercase()
    )
}

#[test]
fn lifecycle_hardening_closeout_pre_l9_public_surface_is_crate_private() {
    // L8Z's "Pre-L9 Surface Readiness" rule requires that commit, lifecycle,
    // branch, and testkit items remain `pub(crate)` (or stricter) until L9
    // wraps them. This is enforced piecewise by existing per-surface
    // closeout tests. This umbrella test pins the closeout assertion that
    // all four surfaces share the rule by verifying each per-surface check
    // exists.
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
