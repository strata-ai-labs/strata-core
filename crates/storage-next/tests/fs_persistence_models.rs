//! Filesystem-persistence-model recovery (STH-3): materialize each way a power-loss
//! crash state can relate to the writes that were issued — ordered+atomic loss of
//! the unsynced tail, partial/reordered append persistence, a garbage (torn)
//! unsynced tail, and a vanished publish (split rename) — across every crash point
//! and both durability policies. The reopened database must be a prefix of
//! acknowledged commit history (the STH-1 recovery oracle): under `Always` nothing
//! acknowledged is lost under any model; under `Standard` only the unsynced suffix
//! may be lost, and recovery is never torn, phantom, or holed.

#![deny(unsafe_code)]

mod common;

#[cfg(all(
    feature = "fault-injection",
    feature = "localfs",
    not(target_arch = "wasm32")
))]
#[test]
fn fs_persistence_models_recover_prefix_of_acknowledged_history() {
    let keep_test_dir = common::keep_test_dir()
        .unwrap_or_else(|error| panic!("invalid fs-model environment: {error}"));
    let test_root_override = common::test_root_override()
        .unwrap_or_else(|error| panic!("invalid fs-model environment: {error}"));
    let case_limit = common::fault_case_limit()
        .unwrap_or_else(|error| panic!("invalid fs-model environment: {error}"));

    let tempdir = test_root_override
        .is_none()
        .then(|| tempfile::tempdir().expect("temp fs-model root"));
    let root = test_root_override.unwrap_or_else(|| {
        tempdir
            .as_ref()
            .expect("tempdir exists without override")
            .path()
            .to_path_buf()
    });

    // Cap the grid for the CI-fast lane; STRATA_STORAGE_FAULT_CASES overrides
    // (set it higher, or to the full grid, for a deeper local/soak run).
    let outcome =
        strata_storage_next::testkit::run_fs_model_harness(&root, case_limit.or(Some(120)))
            .expect("fs-model harness");

    // A budget of 0 is an explicit "run nothing"; otherwise the sweep must run real
    // cases and at least one crash must perturb the disk (never a vacuous green).
    assert!(
        outcome.cases() > 0 || case_limit == Some(0),
        "no fs-model cases swept"
    );
    assert!(
        outcome.perturbed_cases() > 0 || case_limit == Some(0),
        "no crash perturbed the on-disk state"
    );

    if keep_test_dir {
        if let Some(tempdir) = tempdir {
            eprintln!("keeping fs-model root at {}", root.display());
            let _ = tempdir.keep();
        }
    }
}

/// Soak: a deep sweep over many seeds — the genuine bug-hunt run. `#[ignore]` by
/// default; `STRATA_STORAGE_FAULT_CASES` sets the depth.
#[cfg(all(
    feature = "fault-injection",
    feature = "localfs",
    not(target_arch = "wasm32")
))]
#[ignore = "soak: deep multi-seed fs-model sweep; run with --ignored (raise STRATA_STORAGE_FAULT_CASES)"]
#[test]
fn fs_persistence_models_soak_deepens_across_many_seeds() {
    let case_limit = common::fault_case_limit()
        .unwrap_or_else(|error| panic!("invalid fs-model environment: {error}"));
    let tempdir = tempfile::tempdir().expect("temp fs-model soak root");

    let outcome = strata_storage_next::testkit::run_fs_model_harness(
        tempdir.path(),
        case_limit.or(Some(20_000)),
    )
    .expect("fs-model soak");

    assert!(outcome.cases() > 0, "soak swept no cases");
    assert!(
        outcome.perturbed_cases() > 0,
        "soak perturbed nothing: {outcome:?}"
    );
    // The point of seed scaling: the soak must explore beyond the CI seed budget.
    assert!(
        outcome.seeds_executed() > 4,
        "soak did not deepen beyond the default seed budget (got {} seeds)",
        outcome.seeds_executed()
    );
}
