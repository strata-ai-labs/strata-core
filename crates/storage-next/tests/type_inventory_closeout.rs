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
            "--max-all-types",
            "1027",
            "--max-cleanup-target-types",
            "596",
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
            "--max-reexport-names",
            "lifecycle/mod.rs=211",
            "--max-reexport-names",
            "testkit/mod.rs=99",
            "--max-reexport-names",
            "api/mod.rs=102",
            "--max-reexport-names",
            "commit/mod.rs=80",
            "--max-reexport-names",
            "service/mod.rs=61",
            "--max-reexport-names",
            "format/mod.rs=84",
            "--max-reexport-names",
            "table/mod.rs=66",
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
            "--max-suffix-count",
            "Policy=14",
            "--max-suffix-count",
            "Report=12",
            "--max-suffix-count",
            "Reason=9",
            "--max-suffix-count",
            "Proof=7",
            "--max-suffix-count",
            "Stats=4",
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
            "--max-scaffold-markers",
            "lifecycle/durable/maintenance.rs=0:31",
            "--max-scaffold-markers",
            "lifecycle/mod.rs=23:1",
            "--max-scaffold-markers",
            "commit/mod.rs=16:1",
            "--max-scaffold-markers",
            "lifecycle/cache.rs=0:14",
            "--max-scaffold-markers",
            "table/mod.rs=9:1",
            "--max-scaffold-markers",
            "lifecycle/checkpoint.rs=0:10",
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
