//! Closeout guards for storage-next type-count cleanup.

#![deny(unsafe_code)]

mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const INVENTORY_SNAPSHOT: &str =
    "docs/architecture/cleanup/storage-next-type-inventory-after-cln2-t10.md";

#[test]
fn type_inventory_snapshot_is_current() {
    let output = inventory_command()
        .args([
            "--quiet",
            "--require-inventory",
            INVENTORY_SNAPSHOT,
            // +1 over the prior cap for TableSummaryExtras: the per-table summary
            // (timestamp/physical-key bounds, put/tombstone split) cached at seal
            // time so durable publish no longer rescans every row. It is a new
            // cohesive value type rather than extra fields on TableRuntimeFacts,
            // which is compared field-for-field against on-disk-derived facts.
            //
            // +4 more for the off-lock background publish: PreparedPublishStep,
            // PreparedPublish, and PreparedPublishPost carry the three-phase publish
            // across the global-lock release, plus BranchPublishGuard serializes the
            // per-branch off-lock fsync. These are the cohesive state needed to move
            // the manifest fsync off the global runtime lock.
            // +28 more for the Storage Testing Hardening (STH) program's
            // feature-gated test harnesses (recovery_oracle, fault_sweep, fs_models,
            // simulation incl. the 4c fault/crash dimension, reordering_backend).
            // +7 more for source-shape reads and budget accuracy reporting used by
            // the vector index artifact boundary. Five are public API shells and two
            // land in cleanup-target production code; they keep storage generic and
            // do not introduce product/vector vocabulary into storage.
            // +18 all-types / +17 cleanup-target: accumulated on the v1-billion-scale-perf
            // tranche (BS1-BS4 disk-resident/write-path work) — baseline re-snapshotted to
            // current; to be re-attributed per-slice when the tranche closes.
            "--max-all-types",
            "1145",
            "--max-cleanup-target-types",
            "663",
        ])
        .output()
        .expect("run inventory guard");

    assert_success(
        &output,
        "storage-next inventory drifted; regenerate the cleanup inventory and review the count diff",
    );
}

#[test]
fn parent_facade_reexports_do_not_regrow() {
    let output = inventory_command()
        .args([
            "--quiet",
            // +4 for the off-lock background publish surface re-exported from lifecycle:
            // PreparedPublishStep plus the install-without-publish / publish-manifest split
            // (install_prepared_durable_{compaction,materialization}_without_publish and
            // publish_{compaction,materialization}_outcome_manifest) that lets the manifest
            // fsync move off the global runtime lock while synchronous callers stay 2-phase.
            // +4 more: accumulated on the v1-billion-scale-perf tranche; baseline re-snapshotted.
            "--max-reexport-names",
            "lifecycle/mod.rs=247",
            // +13 for the STH program's testkit harness exports (the run_*_harness
            // entry points + *Outcome types for recovery_oracle / fault_sweep /
            // fs_models / simulation + the 4c fault simulation, plus FsModel /
            // ReorderingBackend) re-exported from the testkit facade.
            "--max-reexport-names",
            "testkit/mod.rs=112",
            // +5 for the public source-shape read DTOs and diagnostics budget
            // accuracy enum needed by upper layers without exposing lower storage
            // internals.
            "--max-reexport-names",
            "api/mod.rs=109",
            "--max-reexport-names",
            "commit/mod.rs=80",
            "--max-reexport-names",
            "service/mod.rs=61",
            // +6 more: accumulated on the v1-billion-scale-perf tranche; baseline re-snapshotted.
            "--max-reexport-names",
            "format/mod.rs=92",
            // +1 to re-export TableSummaryExtras so branch state can name the
            // per-table summary type (its `facts` submodule is crate-private).
            // +1 more: accumulated on the v1-billion-scale-perf tranche; baseline re-snapshotted.
            "--max-reexport-names",
            "table/mod.rs=69",
            "--max-reexport-names",
            "testkit/lifecycle/mod.rs=36",
            "--max-reexport-names",
            "testkit/api/mod.rs=14",
        ])
        .output()
        .expect("run re-export guard");

    assert_success(&output, "storage-next parent facade re-export count regrew");
}

#[test]
fn operation_family_suffix_counts_do_not_regrow() {
    let output = inventory_command()
        .args([
            "--quiet",
            "--max-suffix-count",
            "Outcome=47",
            "--max-suffix-count",
            "Request=24",
            // +2: accumulated on the v1-billion-scale-perf tranche; baseline re-snapshotted.
            "--max-suffix-count",
            "Policy=18",
            "--max-suffix-count",
            "Report=12",
            "--max-suffix-count",
            "Reason=9",
            "--max-suffix-count",
            "Proof=7",
            "--max-suffix-count",
            "Stats=5",
            "--max-suffix-count",
            "Plan=3",
            "--max-suffix-count",
            "Recovery=1",
            "--max-suffix-count",
            "Invalidity=1",
            "--max-suffix-count",
            "Attestation=1",
            "--max-suffix-count",
            "Candidate=2",
            "--max-suffix-count",
            "PreparedOutput=1",
        ])
        .output()
        .expect("run suffix guard");

    assert_success(&output, "storage-next operation-family suffix count regrew");
}

#[test]
fn scaffold_allowance_markers_do_not_regrow() {
    let output = inventory_command()
        .args([
            "--quiet",
            // +1 in each background maintenance dispatcher for feature-gated
            // perf-trace hooks that are dead in the default testkit feature set
            // but intentionally compiled for strict all-target clippy.
            "--max-scaffold-markers",
            "lifecycle/durable/maintenance.rs=0:32",
            "--max-scaffold-markers",
            "lifecycle/mod.rs=24:1",
            "--max-scaffold-markers",
            "commit/mod.rs=16:1",
            "--max-scaffold-markers",
            "lifecycle/cache.rs=0:15",
            "--max-scaffold-markers",
            "table/mod.rs=9:1",
            "--max-scaffold-markers",
            "lifecycle/checkpoint.rs=0:11",
            "--max-scaffold-markers",
            "lifecycle/durable/bootstrap.rs=0:7",
            "--max-scaffold-markers",
            "service/table.rs=0:7",
            "--max-scaffold-markers",
            "service/mod.rs=3:0",
            "--max-scaffold-markers",
            "service/manifest.rs=0:6",
            "--max-scaffold-markers",
            "lifecycle/table_manifest.rs=0:6",
            "--max-scaffold-markers",
            "lifecycle/recovery.rs=0:2",
            "--max-scaffold-markers",
            "branch/state/manifest_recovery.rs=0:6",
            "--max-scaffold-markers",
            "lifecycle/compaction.rs=0:5",
            "--max-scaffold-markers",
            "format/mod.rs=3:1",
            "--max-scaffold-markers",
            "format/branch_catalog_manifest.rs=0:4",
            "--max-scaffold-markers",
            "backend/mod.rs=0:4",
            "--max-scaffold-markers",
            "branch/error.rs=0:3",
            "--max-scaffold-markers",
            "testkit/integration_harness.rs=0:2",
            "--max-scaffold-markers",
            "testkit/lifecycle/fault.rs=0:1",
            "--max-scaffold-markers",
            "service/wal.rs=0:1",
        ])
        .output()
        .expect("run scaffold marker guard");

    assert_success(
        &output,
        "storage-next scaffold allowance marker count regrew",
    );
}

fn inventory_command() -> Command {
    let repo = repo_root();
    let mut command = Command::new(python());
    command
        .current_dir(&repo)
        .arg("scripts/architecture/storage_next_type_inventory.py");
    command
}

fn repo_root() -> PathBuf {
    let crate_root = common::crate_root();
    crate_root
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .expect("storage-next crate lives under crates/storage-next")
}

fn python() -> &'static str {
    if cfg!(windows) {
        "python"
    } else {
        "python3"
    }
}

fn assert_success(output: &Output, message: &str) {
    assert!(
        output.status.success(),
        "{message}\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
