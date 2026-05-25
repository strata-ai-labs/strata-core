use super::*;
use crate::backend::{
    Backend, BackendAppend, BackendCapabilities, BackendError, BackendErrorKind, BackendMetadata,
    BackendRange, BackendResult, BackendWriterGuard, PublishDurability, PublishError, PublishMode,
    PublishOutcome, PublishResult, DURABLE_LOCAL_MODE_REQUIREMENTS,
};
use crate::branch::{BranchLevel, BranchRuntimeConfig};
use crate::commit::{
    CommitBranchGeneration, CommitManualTimestampSource, CommitRuntimeConfig, CommitStamp,
    CommitTimelineEntry, CommitTimelineRows,
};
use crate::format::{
    encode_manifest, DatabaseManifest, TableManifest, TableManifestInheritedLayer,
    TableManifestInheritedLayerStatus, TableManifestLevel, TableManifestTableBounds,
    TableManifestTableFacts, TableManifestTableProvenance, TableManifestTableRef, WalCommitPayload,
    WalRecord,
};
use crate::layout::ObjectLayout;
use crate::lifecycle::encode_checkpoint_row_section;
use crate::object::{ObjectName, ObjectPrefix};
use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};
use crate::service::{
    SnapshotPublishRequest, SnapshotService, TableManifestService, TableObjectFacts,
    TableObjectService, WalServiceConfig,
};
use crate::table::{
    sort_table_rows_by_key, ImmutableTableBuilder, ImmutableTableReader, TableIdentity,
    TablePhysicalKeyBytes, TableReaderConfig, TableRow,
};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use strata_core_next::{BranchId, CommitVersion, Timestamp};

const DATABASE_ID: [u8; 16] = [0x72; 16];

#[test]
fn recovery_loads_table_manifest_for_branch_and_reports_facts() {
    let backend = ManifestRecoveryBackend::new();
    let branch = branch_id(0x21);
    let row = put_row(branch, 3, b"manifest-load", b"value");
    let table = publish_manifest_table(&backend, branch, BranchLevel::ZERO, "load-table", &[row]);
    publish_table_manifest(
        &backend,
        &TableManifest::new(
            branch,
            None,
            5,
            vec![
                TableManifestLevel::new(BranchLevel::ZERO, vec![table.reference.clone()])
                    .expect("level"),
            ],
            Vec::new(),
            Vec::new(),
        )
        .expect("manifest"),
    );

    let (shell, outcome) = recover_shell(&backend, branch, RecoveryStrictness::Strict);

    let table_manifest = outcome.tables().table_manifest();
    assert_eq!(table_manifest.manifest_sequence(), Some(5));
    assert_eq!(table_manifest.table_count(), 1);
    assert_eq!(
        table_manifest
            .manifest_object()
            .expect("manifest object")
            .as_str(),
        ObjectLayout::branch_table_manifest(&branch.to_string())
            .expect("manifest object")
            .as_str()
    );
    assert_eq!(shell.branch_state().owned_table_count(), 1);
    assert_eq!(
        shell
            .branch_state()
            .capture_read_view()
            .expect("view")
            .latest(&physical_key(branch, b"manifest-load"))
            .expect("read")
            .expect("visible")
            .row()
            .value(),
        b"value"
    );
    assert_eq!(backend.table_list_prefix_calls(), 0);
    let runtime = shell.complete_recovery(&outcome).expect("bootstrap");
    assert!(runtime
        .table_catalog()
        .build_manifest(runtime.branch_state())
        .is_ok());
}

#[test]
fn recovery_installs_manifest_owned_front_and_sorted_tables() {
    let backend = ManifestRecoveryBackend::new();
    let branch = branch_id(0x22);
    let front_row = put_row(branch, 7, b"front-level", b"front");
    let sorted_row = put_row(branch, 4, b"sorted-level", b"sorted");
    let front = publish_manifest_table(
        &backend,
        branch,
        BranchLevel::ZERO,
        "owned-front",
        &[front_row],
    );
    let sorted = publish_manifest_table(
        &backend,
        branch,
        BranchLevel::new(1),
        "owned-sorted",
        &[sorted_row],
    );
    publish_table_manifest(
        &backend,
        &TableManifest::new(
            branch,
            None,
            6,
            vec![
                TableManifestLevel::new(BranchLevel::ZERO, vec![front.reference.clone()])
                    .expect("front level"),
                TableManifestLevel::new(BranchLevel::new(1), vec![sorted.reference.clone()])
                    .expect("sorted level"),
            ],
            Vec::new(),
            Vec::new(),
        )
        .expect("manifest"),
    );

    let (shell, outcome) = recover_shell(&backend, branch, RecoveryStrictness::Strict);

    assert_eq!(
        outcome
            .tables()
            .table_manifest()
            .install_outcome()
            .expect("install")
            .owned_table_count(),
        2
    );
    let view = shell.branch_state().capture_read_view().expect("view");
    assert_eq!(
        view.latest(&physical_key(branch, b"front-level"))
            .expect("read front level")
            .expect("front visible")
            .row()
            .value(),
        b"front"
    );
    assert_eq!(
        view.latest(&physical_key(branch, b"sorted-level"))
            .expect("read sorted level")
            .expect("sorted visible")
            .row()
            .value(),
        b"sorted"
    );
}

#[test]
fn recovery_installs_inherited_layers_from_manifest() {
    let backend = ManifestRecoveryBackend::new();
    let child = branch_id(0x23);
    let source = branch_id(0x24);
    let inherited_row = put_row(source, 9, b"inherited", b"ancestor");
    let inherited = publish_manifest_table(
        &backend,
        source,
        BranchLevel::ZERO,
        "inherited-table",
        &[inherited_row],
    );
    publish_table_manifest(
        &backend,
        &TableManifest::new(
            child,
            None,
            7,
            Vec::new(),
            vec![TableManifestInheritedLayer::new(
                0,
                source,
                None,
                CommitVersion::new(9),
                TableManifestInheritedLayerStatus::Active,
                vec![
                    TableManifestLevel::new(BranchLevel::ZERO, vec![inherited.reference.clone()])
                        .expect("inherited level"),
                ],
            )
            .expect("inherited layer")],
            Vec::new(),
        )
        .expect("manifest"),
    );

    let (shell, outcome) = recover_shell(&backend, child, RecoveryStrictness::Strict);

    let install = outcome
        .tables()
        .table_manifest()
        .install_outcome()
        .expect("install outcome");
    assert_eq!(install.inherited_layer_count(), 1);
    assert_eq!(install.inherited_table_count(), 1);
    assert_eq!(shell.branch_state().inherited_layer_count(), 1);
    assert_eq!(
        shell
            .branch_state()
            .capture_read_view()
            .expect("view")
            .latest(&physical_key(child, b"inherited"))
            .expect("read inherited")
            .expect("inherited visible")
            .row()
            .value(),
        b"ancestor"
    );
}

#[test]
fn recovery_preserves_materializing_layer_status() {
    let backend = ManifestRecoveryBackend::new();
    let child = branch_id(0x25);
    let source = branch_id(0x26);
    let inherited_row = put_row(source, 11, b"materializing", b"value");
    let inherited = publish_manifest_table(
        &backend,
        source,
        BranchLevel::ZERO,
        "materializing-table",
        &[inherited_row],
    );
    publish_table_manifest(
        &backend,
        &TableManifest::new(
            child,
            None,
            8,
            Vec::new(),
            vec![TableManifestInheritedLayer::new(
                0,
                source,
                None,
                CommitVersion::new(11),
                TableManifestInheritedLayerStatus::Materializing,
                vec![
                    TableManifestLevel::new(BranchLevel::ZERO, vec![inherited.reference.clone()])
                        .expect("inherited level"),
                ],
            )
            .expect("inherited layer")],
            Vec::new(),
        )
        .expect("manifest"),
    );

    let (shell, _) = recover_shell(&backend, child, RecoveryStrictness::Strict);

    assert_eq!(
        shell.branch_state().inherited_layers()[0].status(),
        crate::branch::InheritedLayerStatus::Materializing
    );
}

#[test]
fn recovery_ignores_orphan_table_objects_not_in_manifest() {
    let backend = ManifestRecoveryBackend::new();
    let branch = branch_id(0x27);
    let live = publish_manifest_table(
        &backend,
        branch,
        BranchLevel::ZERO,
        "listed-table",
        &[put_row(branch, 12, b"listed", b"live")],
    );
    let orphan = publish_manifest_table(
        &backend,
        branch,
        BranchLevel::ZERO,
        "orphan-table",
        &[put_row(branch, 13, b"orphan", b"ignored")],
    );
    backend.overwrite_object(orphan.reference.object(), b"corrupt orphan bytes".to_vec());
    publish_table_manifest(
        &backend,
        &TableManifest::new(
            branch,
            None,
            9,
            vec![
                TableManifestLevel::new(BranchLevel::ZERO, vec![live.reference.clone()])
                    .expect("level"),
            ],
            Vec::new(),
            Vec::new(),
        )
        .expect("manifest"),
    );

    let (shell, outcome) = recover_shell(&backend, branch, RecoveryStrictness::Strict);

    assert_eq!(outcome.tables().table_manifest().table_count(), 1);
    assert_eq!(backend.table_list_prefix_calls(), 0);
    let view = shell.branch_state().capture_read_view().expect("view");
    assert!(view
        .latest(&physical_key(branch, b"orphan"))
        .expect("read orphan")
        .is_none());
    assert_eq!(
        view.latest(&physical_key(branch, b"listed"))
            .expect("read listed")
            .expect("listed visible")
            .row()
            .value(),
        b"live"
    );
}

#[test]
fn strict_recovery_rejects_corrupt_table_manifest_without_loading_orphans() {
    let backend = ManifestRecoveryBackend::new();
    let branch = branch_id(0x28);
    let orphan = publish_manifest_table(
        &backend,
        branch,
        BranchLevel::ZERO,
        "corrupt-manifest-orphan",
        &[put_row(branch, 14, b"orphan", b"value")],
    );
    let manifest_object =
        ObjectLayout::branch_table_manifest(&branch.to_string()).expect("manifest object");
    backend.write_raw(manifest_object, b"not a manifest".to_vec());

    let mut shell = assemble_shell(&backend, branch, RecoveryStrictness::Strict);
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");
    let error = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect_err("corrupt manifest rejects");

    assert_eq!(error.code(), "corruption.lifecycle.table_manifest");
    assert!(error.source().is_some());
    assert_eq!(backend.table_list_prefix_calls(), 0);
    assert!(backend.object_bytes(orphan.reference.object()).is_some());
    assert!(shell.branch_state().is_empty());
}

#[test]
fn lossy_recovery_reports_corrupt_table_manifest_data_loss() {
    let backend = ManifestRecoveryBackend::new();
    let branch = branch_id(0x31);
    let orphan = publish_manifest_table(
        &backend,
        branch,
        BranchLevel::ZERO,
        "lossy-corrupt-manifest-orphan",
        &[put_row(branch, 24, b"orphan", b"value")],
    );
    let manifest_object =
        ObjectLayout::branch_table_manifest(&branch.to_string()).expect("manifest object");
    backend.write_raw(manifest_object, b"not a manifest".to_vec());

    let (shell, outcome) = recover_shell(
        &backend,
        branch,
        RecoveryStrictness::AllowExplicitLossyFallback,
    );

    assert_table_manifest_data_loss(outcome.health(), RecoveryFaultKind::CorruptManifest);
    assert_eq!(outcome.tables().table_manifest().table_count(), 0);
    assert_eq!(backend.table_list_prefix_calls(), 0);
    assert!(backend.object_bytes(orphan.reference.object()).is_some());
    assert!(shell.branch_state().is_empty());
}

#[test]
fn missing_table_manifest_for_empty_branch_is_healthy() {
    let backend = ManifestRecoveryBackend::new();
    let branch = branch_id(0x29);

    let (shell, outcome) = recover_shell(&backend, branch, RecoveryStrictness::Strict);

    assert_eq!(outcome.health(), &RecoveryHealth::Healthy);
    assert_eq!(outcome.tables().table_manifest().table_count(), 0);
    assert_eq!(backend.table_list_prefix_calls(), 0);
    assert!(shell.branch_state().is_empty());
}

#[test]
fn strict_recovery_rejects_missing_manifest_listed_table_object() {
    let backend = ManifestRecoveryBackend::new();
    let branch = branch_id(0x2a);
    let table = publish_manifest_table(
        &backend,
        branch,
        BranchLevel::ZERO,
        "missing-listed-table",
        &[put_row(branch, 15, b"missing", b"value")],
    );
    backend
        .delete_object(table.reference.object())
        .expect("delete listed table object");
    publish_table_manifest(
        &backend,
        &TableManifest::new(
            branch,
            None,
            10,
            vec![TableManifestLevel::new(BranchLevel::ZERO, vec![table.reference]).expect("level")],
            Vec::new(),
            Vec::new(),
        )
        .expect("manifest"),
    );

    let mut shell = assemble_shell(&backend, branch, RecoveryStrictness::Strict);
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");
    let error = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect_err("missing listed table rejects");

    assert_eq!(error.code(), "corruption.lifecycle.table_manifest");
    assert!(error.source().is_some());
    assert!(shell.branch_state().is_empty());
}

#[test]
fn lossy_recovery_reports_missing_manifest_listed_table_object() {
    let backend = ManifestRecoveryBackend::new();
    let branch = branch_id(0x32);
    let table = publish_manifest_table(
        &backend,
        branch,
        BranchLevel::ZERO,
        "lossy-missing-listed-table",
        &[put_row(branch, 25, b"missing", b"value")],
    );
    backend
        .delete_object(table.reference.object())
        .expect("delete listed table object");
    publish_table_manifest(
        &backend,
        &TableManifest::new(
            branch,
            None,
            15,
            vec![TableManifestLevel::new(BranchLevel::ZERO, vec![table.reference]).expect("level")],
            Vec::new(),
            Vec::new(),
        )
        .expect("manifest"),
    );

    let (shell, outcome) = recover_shell(
        &backend,
        branch,
        RecoveryStrictness::AllowExplicitLossyFallback,
    );

    assert_table_manifest_data_loss(outcome.health(), RecoveryFaultKind::MissingTableObject);
    assert_eq!(outcome.tables().table_manifest().table_count(), 0);
    assert!(shell.branch_state().is_empty());
}

#[test]
fn strict_recovery_rejects_corrupt_manifest_listed_table_object() {
    let backend = ManifestRecoveryBackend::new();
    let branch = branch_id(0x2b);
    let table = publish_manifest_table(
        &backend,
        branch,
        BranchLevel::ZERO,
        "corrupt-listed-table",
        &[put_row(branch, 16, b"corrupt", b"value")],
    );
    backend.overwrite_object(table.reference.object(), b"corrupt table bytes".to_vec());
    publish_table_manifest(
        &backend,
        &TableManifest::new(
            branch,
            None,
            11,
            vec![TableManifestLevel::new(BranchLevel::ZERO, vec![table.reference]).expect("level")],
            Vec::new(),
            Vec::new(),
        )
        .expect("manifest"),
    );

    let mut shell = assemble_shell(&backend, branch, RecoveryStrictness::Strict);
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");
    let error = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect_err("corrupt listed table rejects");

    assert_eq!(error.code(), "corruption.lifecycle.table_manifest");
    assert!(error.source().is_some());
    assert!(shell.branch_state().is_empty());
}

#[test]
fn recovery_rejects_table_object_fact_and_bounds_mismatches() {
    let branch = branch_id(0x2c);
    let base_rows = [
        put_row(branch, 17, b"mismatch-a", b"a"),
        put_row(branch, 18, b"mismatch-b", b"b"),
    ];
    let cases = [
        ManifestMismatch::ByteCount,
        ManifestMismatch::RowCount,
        ManifestMismatch::DataBlockCount,
        ManifestMismatch::CommitMin,
        ManifestMismatch::CommitMax,
        ManifestMismatch::Bounds,
    ];

    for mismatch in cases {
        let backend = ManifestRecoveryBackend::new();
        let table = publish_manifest_table(
            &backend,
            branch,
            BranchLevel::ZERO,
            mismatch.identity(),
            &base_rows,
        );
        let bad_ref = table.reference.with_mismatch(mismatch);
        publish_table_manifest(
            &backend,
            &TableManifest::new(
                branch,
                None,
                12,
                vec![TableManifestLevel::new(BranchLevel::ZERO, vec![bad_ref]).expect("level")],
                Vec::new(),
                Vec::new(),
            )
            .expect("manifest"),
        );
        let mut shell = assemble_shell(&backend, branch, RecoveryStrictness::Strict);
        let request =
            LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");

        let error = LifecycleRecoveryRuntime::new(&mut shell)
            .recover(&request)
            .expect_err("mismatched listed table rejects");

        assert_eq!(error.code(), "corruption.lifecycle.table_manifest");
        assert!(shell.branch_state().is_empty());
    }
}

#[test]
fn table_manifest_recovery_does_not_change_wal_replay_start() {
    let backend = ManifestRecoveryBackend::new();
    let branch = branch_id(0x2d);
    let table = publish_manifest_table(
        &backend,
        branch,
        BranchLevel::ZERO,
        "wal-start-table",
        &[put_row(branch, 18, b"table", b"value")],
    );
    publish_table_manifest(
        &backend,
        &TableManifest::new(
            branch,
            None,
            13,
            vec![TableManifestLevel::new(BranchLevel::ZERO, vec![table.reference]).expect("level")],
            Vec::new(),
            Vec::new(),
        )
        .expect("manifest"),
    );
    let mut shell = assemble_shell(&backend, branch, RecoveryStrictness::Strict);
    shell
        .services_mut()
        .wal_mut()
        .append(&wal_record(branch, 19, b"wal", b"value"))
        .expect("append wal record");
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");

    let outcome = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect("recovery");

    assert_eq!(outcome.wal().replay_start(), CommitVersion::ZERO);
    let manifest = shell
        .services()
        .manifest()
        .load_required()
        .expect("database manifest");
    assert_eq!(manifest.flushed_through_commit_id(), None);
}

#[test]
fn table_manifest_recovery_then_wal_tail_preserves_latest_reads() {
    let backend = ManifestRecoveryBackend::new();
    let branch = branch_id(0x2e);
    let old_row = put_row(branch, 20, b"same-key", b"old");
    let table = publish_manifest_table(
        &backend,
        branch,
        BranchLevel::ZERO,
        "wal-tail-table",
        &[old_row],
    );
    publish_table_manifest(
        &backend,
        &TableManifest::new(
            branch,
            None,
            14,
            vec![TableManifestLevel::new(BranchLevel::ZERO, vec![table.reference]).expect("level")],
            Vec::new(),
            Vec::new(),
        )
        .expect("manifest"),
    );
    let mut shell = assemble_shell(&backend, branch, RecoveryStrictness::Strict);
    shell
        .services_mut()
        .wal_mut()
        .append(&wal_record(branch, 21, b"same-key", b"new"))
        .expect("append wal record");
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");
    let outcome = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect("recovery");

    let runtime = shell.complete_recovery(&outcome).expect("bootstrap");

    assert_eq!(
        runtime
            .read_view()
            .expect("view")
            .latest(&physical_key(branch, b"same-key"))
            .expect("read")
            .expect("visible")
            .row()
            .value(),
        b"new"
    );
}

#[test]
fn durable_table_catalog_accepts_exact_duplicate_and_rejects_conflicts() {
    let backend = ManifestRecoveryBackend::new();
    let branch = branch_id(0x2f);
    let table = publish_manifest_table(
        &backend,
        branch,
        BranchLevel::ZERO,
        "catalog-conflict-table",
        &[put_row(branch, 22, b"catalog", b"value")],
    );
    let identity = table.reference.table_identity().clone();
    let facts = TableObjectFacts::from_table_manifest_ref(&table.reference);
    let mut catalog = LifecycleDurableTableCatalog::new();

    catalog
        .record_table(identity.clone(), facts.clone())
        .expect("first catalog record");
    catalog
        .record_table(identity.clone(), facts)
        .expect("exact duplicate is idempotent");

    let different_object = table.reference.with_object("catalog-conflict-other");
    let error = catalog
        .record_table(
            identity.clone(),
            TableObjectFacts::from_table_manifest_ref(&different_object),
        )
        .expect_err("identity with different object rejects");
    assert_eq!(
        error.code(),
        "failed_precondition.lifecycle.table_manifest_publication"
    );

    let same_object_different_identity = table.reference.with_identity("catalog-conflict-alias");
    let error = catalog
        .record_table(
            same_object_different_identity.table_identity().clone(),
            TableObjectFacts::from_table_manifest_ref(&same_object_different_identity),
        )
        .expect_err("same object with different identity rejects");
    assert_eq!(
        error.code(),
        "failed_precondition.lifecycle.table_manifest_publication"
    );
}

#[test]
fn durable_table_catalog_rebuilds_from_recovered_manifest_sequence() {
    let backend = ManifestRecoveryBackend::new();
    let branch = branch_id(0x30);
    let table = publish_manifest_table(
        &backend,
        branch,
        BranchLevel::ZERO,
        "catalog-recovered-table",
        &[put_row(branch, 23, b"catalog-recovered", b"value")],
    );
    let manifest = TableManifest::new(
        branch,
        None,
        33,
        vec![TableManifestLevel::new(BranchLevel::ZERO, vec![table.reference]).expect("level")],
        Vec::new(),
        Vec::new(),
    )
    .expect("manifest");
    let mut catalog = LifecycleDurableTableCatalog::new();

    catalog
        .record_manifest(&manifest)
        .expect("record recovered manifest");

    assert_eq!(catalog.next_manifest_sequence(), 34);
}

#[test]
fn durable_table_catalog_rejects_manifest_sequence_regress() {
    let backend = ManifestRecoveryBackend::new();
    let branch = branch_id(0x33);
    let table = publish_manifest_table(
        &backend,
        branch,
        BranchLevel::ZERO,
        "catalog-sequence-table",
        &[put_row(branch, 24, b"catalog-sequence", b"value")],
    );
    let manifest_later = TableManifest::new(
        branch,
        None,
        10,
        vec![
            TableManifestLevel::new(BranchLevel::ZERO, vec![table.reference.clone()])
                .expect("level"),
        ],
        Vec::new(),
        Vec::new(),
    )
    .expect("manifest");
    let mut catalog = LifecycleDurableTableCatalog::new();
    catalog
        .record_manifest(&manifest_later)
        .expect("record later manifest");

    let manifest_older = TableManifest::new(
        branch,
        None,
        5,
        vec![TableManifestLevel::new(BranchLevel::ZERO, vec![table.reference]).expect("level")],
        Vec::new(),
        Vec::new(),
    )
    .expect("manifest");
    let error = catalog
        .record_manifest(&manifest_older)
        .expect_err("older manifest sequence rejects");
    assert_eq!(
        error.code(),
        "failed_precondition.lifecycle.table_manifest_publication"
    );
}

#[test]
fn durable_table_catalog_rejects_identity_with_different_facts() {
    let backend = ManifestRecoveryBackend::new();
    let branch = branch_id(0x34);
    let table = publish_manifest_table(
        &backend,
        branch,
        BranchLevel::ZERO,
        "catalog-facts-table",
        &[put_row(branch, 25, b"catalog-facts", b"value")],
    );
    let identity = table.reference.table_identity().clone();
    let facts = TableObjectFacts::from_table_manifest_ref(&table.reference);
    let mut catalog = LifecycleDurableTableCatalog::new();
    catalog
        .record_table(identity.clone(), facts.clone())
        .expect("first catalog record");

    let mismatched_ref = table.reference.with_mismatch(ManifestMismatch::RowCount);
    let mismatched_facts = TableObjectFacts::from_table_manifest_ref(&mismatched_ref);
    let error = catalog
        .record_table(identity, mismatched_facts)
        .expect_err("identity with different facts rejects");
    assert_eq!(
        error.code(),
        "failed_precondition.lifecycle.table_manifest_publication"
    );
}

#[test]
fn recovery_preflights_checkpoint_and_table_manifest_disjoint_succeeds() {
    let backend = ManifestRecoveryBackend::new();
    let branch = branch_id(0x35);
    let manifest_row = put_row(branch, 5, b"manifest-only", b"manifest");
    let manifest_table = publish_manifest_table(
        &backend,
        branch,
        BranchLevel::ZERO,
        "preflight-disjoint-manifest",
        &[manifest_row],
    );
    publish_table_manifest(
        &backend,
        &TableManifest::new(
            branch,
            None,
            40,
            vec![
                TableManifestLevel::new(BranchLevel::ZERO, vec![manifest_table.reference])
                    .expect("level"),
            ],
            Vec::new(),
            Vec::new(),
        )
        .expect("manifest"),
    );
    let checkpoint_row = put_row(branch, 7, b"checkpoint-only", b"checkpoint");
    publish_snapshot_for_test(
        &backend,
        11,
        CommitVersion::new(7),
        std::slice::from_ref(&checkpoint_row),
    );
    seed_database_manifest_with_snapshot(&backend, 11, CommitVersion::new(7));

    let (shell, _outcome) = recover_shell(&backend, branch, RecoveryStrictness::Strict);

    let view = shell.branch_state().capture_read_view().expect("view");
    assert_eq!(
        view.latest(checkpoint_row.physical_key())
            .expect("read checkpoint row")
            .expect("checkpoint visible")
            .row()
            .value(),
        b"checkpoint"
    );
}

#[test]
fn recovery_rejects_checkpoint_table_manifest_duplicate_internal_key_conflict() {
    let backend = ManifestRecoveryBackend::new();
    let branch = branch_id(0x36);
    let manifest_row = put_row(branch, 5, b"shared-key", b"manifest-bytes");
    let manifest_table = publish_manifest_table(
        &backend,
        branch,
        BranchLevel::ZERO,
        "preflight-conflict-manifest",
        &[manifest_row],
    );
    publish_table_manifest(
        &backend,
        &TableManifest::new(
            branch,
            None,
            41,
            vec![
                TableManifestLevel::new(BranchLevel::ZERO, vec![manifest_table.reference])
                    .expect("level"),
            ],
            Vec::new(),
            Vec::new(),
        )
        .expect("manifest"),
    );
    let checkpoint_row = put_row(branch, 5, b"shared-key", b"checkpoint-bytes");
    publish_snapshot_for_test(&backend, 12, CommitVersion::new(5), &[checkpoint_row]);
    seed_database_manifest_with_snapshot(&backend, 12, CommitVersion::new(5));

    let mut shell = assemble_shell(&backend, branch, RecoveryStrictness::Strict);
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");
    let error = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect_err("conflicting bytes reject");

    assert_eq!(
        error.code(),
        "failed_precondition.lifecycle.table_manifest_checkpoint_conflict"
    );
}

#[test]
fn recovery_accepts_exact_duplicate_checkpoint_table_manifest_rows() {
    let backend = ManifestRecoveryBackend::new();
    let branch = branch_id(0x37);
    let shared_row = put_row(branch, 5, b"shared-key", b"shared-value");
    let manifest_table = publish_manifest_table(
        &backend,
        branch,
        BranchLevel::ZERO,
        "preflight-exact-dupe-manifest",
        std::slice::from_ref(&shared_row),
    );
    publish_table_manifest(
        &backend,
        &TableManifest::new(
            branch,
            None,
            42,
            vec![
                TableManifestLevel::new(BranchLevel::ZERO, vec![manifest_table.reference])
                    .expect("level"),
            ],
            Vec::new(),
            Vec::new(),
        )
        .expect("manifest"),
    );
    publish_snapshot_for_test(
        &backend,
        13,
        CommitVersion::new(5),
        std::slice::from_ref(&shared_row),
    );
    seed_database_manifest_with_snapshot(&backend, 13, CommitVersion::new(5));

    let (shell, _outcome) = recover_shell(&backend, branch, RecoveryStrictness::Strict);

    let view = shell.branch_state().capture_read_view().expect("view");
    assert_eq!(
        view.latest(shared_row.physical_key())
            .expect("read shared row")
            .expect("shared visible")
            .row()
            .value(),
        b"shared-value"
    );
}

#[test]
fn build_manifest_reports_volatile_rewrite_output_with_clear_error() {
    // Simulate the durable-compaction case where branch state contains a
    // table whose identity has no durable catalog entry yet (deferred to a
    // later slice that publishes rewrite outputs). The manifest builder must
    // report this with a clear, actionable reason.
    let backend = ManifestRecoveryBackend::new();
    let branch = branch_id(0x3a);
    let durable = publish_manifest_table(
        &backend,
        branch,
        BranchLevel::ZERO,
        "volatile-builder-durable",
        &[put_row(branch, 30, b"durable", b"value")],
    );
    publish_table_manifest(
        &backend,
        &TableManifest::new(
            branch,
            None,
            55,
            vec![
                TableManifestLevel::new(BranchLevel::ZERO, vec![durable.reference]).expect("level"),
            ],
            Vec::new(),
            Vec::new(),
        )
        .expect("manifest"),
    );

    let (shell, _outcome) = recover_shell(&backend, branch, RecoveryStrictness::Strict);
    let branch_state_with_table = shell.branch_state().clone();
    // Use a fresh, empty catalog. This simulates the case where a volatile
    // rewrite output exists in branch state without a corresponding catalog
    // entry — which is the situation after durable compaction in this slice.
    let empty_catalog = LifecycleDurableTableCatalog::new();
    let error = empty_catalog
        .build_manifest(&branch_state_with_table)
        .expect_err("volatile rewrite output blocks manifest publish");
    assert_eq!(
        error.code(),
        "failed_precondition.lifecycle.table_manifest_publication"
    );
    assert!(matches!(
        error,
        LifecycleError::TableManifestPublicationFailed { reason, .. }
            if reason.contains("volatile rewrite output")
    ));
}

#[test]
fn recovery_rejects_table_object_from_wrong_branch_namespace() {
    let backend = ManifestRecoveryBackend::new();
    let branch = branch_id(0x38);
    let other_branch = branch_id(0x39);
    let row = put_row(other_branch, 6, b"wrong-namespace", b"value");
    // Publish table object under other_branch but reference it from branch's manifest.
    let other_table = publish_manifest_table(
        &backend,
        other_branch,
        BranchLevel::ZERO,
        "wrong-namespace-table",
        &[row],
    );
    // The TableManifest::new validates table_object branch must match manifest branch.
    let error = TableManifest::new(
        branch,
        None,
        50,
        vec![
            TableManifestLevel::new(BranchLevel::ZERO, vec![other_table.reference]).expect("level"),
        ],
        Vec::new(),
        Vec::new(),
    )
    .expect_err("wrong-branch object rejected by format validation");
    // Format module rejects with InvalidValue on table_object_branch.
    assert!(matches!(
        error,
        crate::format::FormatError::InvalidValue {
            field: "table_object_branch"
        }
    ));
}

#[derive(Clone)]
struct PublishedManifestTable {
    reference: TableManifestTableRef,
}

#[derive(Clone, Copy)]
enum ManifestMismatch {
    ByteCount,
    RowCount,
    DataBlockCount,
    CommitMin,
    CommitMax,
    Bounds,
}

impl ManifestMismatch {
    const fn identity(self) -> &'static str {
        match self {
            Self::ByteCount => "byte-count-mismatch",
            Self::RowCount => "row-count-mismatch",
            Self::DataBlockCount => "data-block-count-mismatch",
            Self::CommitMin => "commit-min-mismatch",
            Self::CommitMax => "commit-max-mismatch",
            Self::Bounds => "bounds-mismatch",
        }
    }
}

trait TableRefMismatch {
    fn with_mismatch(&self, mismatch: ManifestMismatch) -> TableManifestTableRef;
    fn with_object(&self, identity_suffix: &str) -> TableManifestTableRef;
    fn with_identity(&self, identity: &str) -> TableManifestTableRef;
}

impl TableRefMismatch for TableManifestTableRef {
    fn with_mismatch(&self, mismatch: ManifestMismatch) -> TableManifestTableRef {
        let facts = self.facts();
        let bad_facts = match mismatch {
            ManifestMismatch::ByteCount => TableManifestTableFacts::new(
                facts.byte_count() + 1,
                facts.row_count(),
                facts.data_block_count(),
                facts.commit_min(),
                facts.commit_max(),
                facts.timestamp_min(),
                facts.timestamp_max(),
            ),
            ManifestMismatch::RowCount => TableManifestTableFacts::new(
                facts.byte_count(),
                facts.row_count() + 1,
                facts.data_block_count(),
                facts.commit_min(),
                facts.commit_max(),
                facts.timestamp_min(),
                facts.timestamp_max(),
            ),
            ManifestMismatch::DataBlockCount => TableManifestTableFacts::new(
                facts.byte_count(),
                facts.row_count() + 1,
                facts.data_block_count() + 1,
                facts.commit_min(),
                facts.commit_max(),
                facts.timestamp_min(),
                facts.timestamp_max(),
            ),
            ManifestMismatch::CommitMin => TableManifestTableFacts::new(
                facts.byte_count(),
                facts.row_count(),
                facts.data_block_count(),
                CommitVersion::new(facts.commit_min().as_u64() + 1),
                facts.commit_max(),
                facts.timestamp_min(),
                facts.timestamp_max(),
            ),
            ManifestMismatch::CommitMax => TableManifestTableFacts::new(
                facts.byte_count(),
                facts.row_count(),
                facts.data_block_count(),
                facts.commit_min(),
                CommitVersion::new(facts.commit_max().as_u64() + 1),
                facts.timestamp_min(),
                facts.timestamp_max(),
            ),
            ManifestMismatch::Bounds => Ok(self.facts().clone()),
        }
        .expect("mismatched facts");
        let bad_bounds = if matches!(mismatch, ManifestMismatch::Bounds) {
            TableManifestTableBounds::new(
                b"not-the-first-key".to_vec(),
                b"zzzz".to_vec(),
                self.bounds().internal_first().to_vec(),
                self.bounds().internal_last().to_vec(),
            )
            .expect("mismatched bounds")
        } else {
            self.bounds().clone()
        };
        TableManifestTableRef::new(
            self.table_identity().clone(),
            self.object().clone(),
            self.order(),
            bad_facts,
            bad_bounds,
            self.provenance().clone(),
        )
        .expect("mismatched table ref")
    }

    fn with_object(&self, identity_suffix: &str) -> TableManifestTableRef {
        let (prefix, _) = self
            .object()
            .as_str()
            .rsplit_once('/')
            .expect("table object has table id component");
        let object = format!("{prefix}/{identity_suffix}");
        TableManifestTableRef::new(
            self.table_identity().clone(),
            ObjectName::new(object).expect("table object"),
            self.order(),
            self.facts().clone(),
            self.bounds().clone(),
            self.provenance().clone(),
        )
        .expect("table ref with different object")
    }

    fn with_identity(&self, identity: &str) -> TableManifestTableRef {
        TableManifestTableRef::new(
            TableIdentity::new(identity).expect("table identity"),
            self.object().clone(),
            self.order(),
            self.facts().clone(),
            self.bounds().clone(),
            self.provenance().clone(),
        )
        .expect("table ref with different identity")
    }
}

fn recover_shell(
    backend: &ManifestRecoveryBackend,
    branch: BranchId,
    strictness: RecoveryStrictness,
) -> (
    LifecycleDurableLocalShell<'_, CommitManualTimestampSource>,
    LifecycleRecoveryOutcome,
) {
    let mut shell = assemble_shell(backend, branch, strictness);
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");
    let outcome = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect("recovery outcome");
    (shell, outcome)
}

fn assemble_shell(
    backend: &ManifestRecoveryBackend,
    branch: BranchId,
    strictness: RecoveryStrictness,
) -> LifecycleDurableLocalShell<'_, CommitManualTimestampSource> {
    let lifecycle_config = if strictness == RecoveryStrictness::AllowExplicitLossyFallback {
        LifecycleConfig::new(
            1024,
            16,
            LifecycleCloseTimeoutPolicy::ReturnTypedTimeout,
            LifecycleLossyRecoveryPolicy::ExplicitlyAllowed,
        )
        .expect("lossy lifecycle config")
    } else {
        LifecycleConfig::default()
    };
    LifecycleDurableLocalShell::assemble(
        LifecycleDurableLocalOpenRequest::new(
            StorageOpenPlan::new(
                StorageMode::DurableLocalStandard,
                LifecycleCodecId::identity(),
                strictness,
                lifecycle_config,
            )
            .expect("open plan"),
            DATABASE_ID,
            branch,
            CommitBranchGeneration::new(1).expect("generation"),
            BranchRuntimeConfig::default(),
            CommitRuntimeConfig::default(),
            WalServiceConfig::default(),
        )
        .expect("open request"),
        backend,
        CommitManualTimestampSource::new(Timestamp::from_micros(8_000)),
    )
    .expect("durable shell")
}

fn publish_manifest_table(
    backend: &ManifestRecoveryBackend,
    branch: BranchId,
    level: BranchLevel,
    identity: &str,
    rows: &[StorageRow],
) -> PublishedManifestTable {
    let identity = TableIdentity::new(identity).expect("table identity");
    let bytes = table_bytes(identity.clone(), rows);
    let write = TableObjectService::new(backend)
        .publish_create(
            &branch.to_string(),
            u32::from(level.raw()),
            identity.as_str(),
            &bytes,
        )
        .expect("publish table object");
    let reader =
        ImmutableTableReader::open_bytes(identity.clone(), bytes, TableReaderConfig::default())
            .expect("table reader");
    let reference = TableManifestTableRef::new(
        identity,
        write.facts().object().clone(),
        0,
        table_manifest_facts(&reader),
        table_manifest_bounds(reader.rows()),
        TableManifestTableProvenance::Flush,
    )
    .expect("table manifest ref");
    PublishedManifestTable { reference }
}

fn publish_table_manifest(backend: &ManifestRecoveryBackend, manifest: &TableManifest) {
    TableManifestService::new(backend)
        .publish_replace_manifest(manifest.branch_id(), manifest)
        .expect("publish table manifest");
}

fn publish_snapshot_for_test(
    backend: &ManifestRecoveryBackend,
    snapshot_id: u64,
    watermark: CommitVersion,
    rows: &[StorageRow],
) {
    SnapshotService::new(backend)
        .publish_create(SnapshotPublishRequest::new(
            snapshot_id,
            watermark,
            Timestamp::from_micros(7_000),
            DATABASE_ID,
            "identity",
            vec![encode_checkpoint_row_section(rows).expect("row section")],
        ))
        .expect("publish snapshot");
}

fn seed_database_manifest_with_snapshot(
    backend: &ManifestRecoveryBackend,
    snapshot_id: u64,
    watermark: CommitVersion,
) {
    let manifest = DatabaseManifest::new(DATABASE_ID, "identity")
        .expect("database manifest")
        .with_recovery_facts(1, Some(watermark.as_u64()), Some(snapshot_id), None)
        .expect("database manifest facts");
    let bytes = encode_manifest(&manifest).expect("database manifest bytes");
    backend.write_raw(
        ObjectLayout::database_manifest().expect("database manifest object"),
        bytes,
    );
}

fn table_manifest_facts(reader: &ImmutableTableReader) -> TableManifestTableFacts {
    let (timestamp_min, timestamp_max) = timestamp_bounds(reader.rows());
    TableManifestTableFacts::new(
        reader.facts().byte_count(),
        reader.facts().row_count(),
        reader.facts().data_block_count(),
        reader.facts().commit_range().min(),
        reader.facts().commit_range().max(),
        timestamp_min,
        timestamp_max,
    )
    .expect("table manifest facts")
}

fn table_manifest_bounds(rows: &[TableRow]) -> TableManifestTableBounds {
    let first = rows.first().expect("non-empty table rows");
    let mut physical_first = TablePhysicalKeyBytes::from_row(first.row());
    let mut physical_last = physical_first.clone();
    let mut internal_first = first.key().clone();
    let mut internal_last = internal_first.clone();
    for row in rows.iter().skip(1) {
        let physical = TablePhysicalKeyBytes::from_row(row.row());
        if physical < physical_first {
            physical_first = physical.clone();
        }
        if physical > physical_last {
            physical_last = physical;
        }
        if row.key() < &internal_first {
            internal_first = row.key().clone();
        }
        if row.key() > &internal_last {
            internal_last = row.key().clone();
        }
    }
    TableManifestTableBounds::new(
        physical_first.as_slice().to_vec(),
        physical_last.as_slice().to_vec(),
        internal_first.as_slice().to_vec(),
        internal_last.as_slice().to_vec(),
    )
    .expect("table manifest bounds")
}

fn timestamp_bounds(rows: &[TableRow]) -> (Option<Timestamp>, Option<Timestamp>) {
    let mut timestamps = rows.iter().map(TableRow::commit_timestamp);
    let Some(first) = timestamps.next() else {
        return (None, None);
    };
    let (min, max) = timestamps.fold((first, first), |(min, max), timestamp| {
        (min.min(timestamp), max.max(timestamp))
    });
    (Some(min), Some(max))
}

fn table_bytes(identity: TableIdentity, rows: &[StorageRow]) -> Vec<u8> {
    let mut table_rows = rows.iter().cloned().map(TableRow::new).collect::<Vec<_>>();
    sort_table_rows_by_key(&mut table_rows);
    ImmutableTableBuilder::new(crate::table::TableBuilderConfig::default())
        .expect("table builder")
        .build_from_rows(identity, &table_rows)
        .expect("build table")
        .into_bytes()
}

fn wal_record(
    branch: BranchId,
    version: u64,
    user_key: &'static [u8],
    value: &'static [u8],
) -> WalRecord {
    let commit_version = CommitVersion::new(version);
    let timestamp = Timestamp::from_micros(version * 100);
    let stamp = CommitStamp::new(branch, commit_version, timestamp).expect("stamp");
    let row = StorageRow::put(
        physical_key(branch, user_key),
        commit_version,
        timestamp,
        Timestamp::EPOCH,
        value.to_vec(),
    );
    let timeline_rows =
        CommitTimelineRows::from_entry(CommitTimelineEntry::from_stamp(stamp).expect("entry"))
            .expect("timeline rows")
            .into_rows();
    let payload = WalCommitPayload::new(vec![
        row,
        timeline_rows[0].clone(),
        timeline_rows[1].clone(),
    ])
    .expect("payload");
    WalRecord::new(commit_version, branch, timestamp, payload).expect("record")
}

fn put_row(
    branch: BranchId,
    version: u64,
    user_key: &'static [u8],
    value: &'static [u8],
) -> StorageRow {
    StorageRow::put(
        physical_key(branch, user_key),
        CommitVersion::new(version),
        Timestamp::from_micros(version * 100),
        Timestamp::EPOCH,
        value.to_vec(),
    )
}

fn physical_key(branch: BranchId, user_key: &'static [u8]) -> PhysicalKey {
    PhysicalKey::new(
        branch,
        "table-manifest",
        StorageSpaceId::engine(0x72).expect("storage space"),
        user_key.to_vec(),
    )
    .expect("physical key")
}

fn branch_id(byte: u8) -> BranchId {
    BranchId::from_bytes([byte; BranchId::BYTE_LEN])
}

fn assert_table_manifest_data_loss(health: &RecoveryHealth, expected_kind: RecoveryFaultKind) {
    assert!(matches!(
        health,
        RecoveryHealth::Degraded {
            class: RecoveryDegradationClass::DataLoss,
            faults,
        } if faults.iter().any(|fault| fault.kind() == expected_kind)
    ));
}

#[derive(Debug)]
struct ManifestRecoveryBackend {
    objects: Mutex<BTreeMap<ObjectName, Vec<u8>>>,
    fail_reads: Mutex<BTreeSet<ObjectName>>,
    list_prefix_calls: AtomicUsize,
    table_list_prefix_calls: AtomicUsize,
    lock_held: Arc<AtomicBool>,
}

impl ManifestRecoveryBackend {
    fn new() -> Self {
        Self {
            objects: Mutex::new(BTreeMap::new()),
            fail_reads: Mutex::new(BTreeSet::new()),
            list_prefix_calls: AtomicUsize::new(0),
            table_list_prefix_calls: AtomicUsize::new(0),
            lock_held: Arc::new(AtomicBool::new(false)),
        }
    }

    fn write_raw(&self, object: ObjectName, bytes: Vec<u8>) {
        self.objects.lock().expect("objects").insert(object, bytes);
    }

    fn overwrite_object(&self, object: &ObjectName, bytes: Vec<u8>) {
        self.objects
            .lock()
            .expect("objects")
            .insert(object.clone(), bytes);
    }

    fn object_bytes(&self, object: &ObjectName) -> Option<Vec<u8>> {
        self.objects.lock().expect("objects").get(object).cloned()
    }

    fn table_list_prefix_calls(&self) -> usize {
        self.table_list_prefix_calls.load(Ordering::SeqCst)
    }
}

impl Backend for ManifestRecoveryBackend {
    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::from_slice(DURABLE_LOCAL_MODE_REQUIREMENTS)
    }

    fn read_object(&self, name: &ObjectName) -> BackendResult<Vec<u8>> {
        if self
            .fail_reads
            .lock()
            .expect("read failures")
            .contains(name)
        {
            return Err(BackendError::new(
                BackendErrorKind::Unavailable,
                "injected read failure",
            ));
        }
        self.objects
            .lock()
            .expect("objects")
            .get(name)
            .cloned()
            .ok_or_else(|| BackendError::new(BackendErrorKind::NotFound, "object not found"))
    }

    fn read_range(&self, name: &ObjectName, range: BackendRange) -> BackendResult<Vec<u8>> {
        let bytes = self.read_object(name)?;
        let start = usize::try_from(range.offset()).unwrap_or(usize::MAX);
        let end = usize::try_from(range.end_offset().unwrap_or(u64::MAX)).unwrap_or(usize::MAX);
        Ok(bytes[start.min(bytes.len())..end.min(bytes.len())].to_vec())
    }

    fn write_object(&self, name: &ObjectName, bytes: &[u8]) -> BackendResult<BackendMetadata> {
        self.objects
            .lock()
            .expect("objects")
            .insert(name.clone(), bytes.to_vec());
        Ok(BackendMetadata::new(bytes.len() as u64, None))
    }

    fn delete_object(&self, name: &ObjectName) -> BackendResult<()> {
        self.objects.lock().expect("objects").remove(name);
        Ok(())
    }

    fn list_prefix(&self, prefix: &ObjectPrefix) -> BackendResult<Vec<ObjectName>> {
        self.list_prefix_calls.fetch_add(1, Ordering::SeqCst);
        if prefix.as_str().starts_with("tables/") {
            self.table_list_prefix_calls.fetch_add(1, Ordering::SeqCst);
        }
        let mut names = self
            .objects
            .lock()
            .expect("objects")
            .keys()
            .filter(|name| name.as_str().starts_with(prefix.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        names.sort();
        Ok(names)
    }

    fn object_metadata(&self, name: &ObjectName) -> BackendResult<BackendMetadata> {
        self.objects
            .lock()
            .expect("objects")
            .get(name)
            .map(|bytes| BackendMetadata::new(bytes.len() as u64, None))
            .ok_or_else(|| BackendError::new(BackendErrorKind::NotFound, "object not found"))
    }

    fn acquire_writer_lock(&self, name: &ObjectName) -> BackendResult<BackendWriterGuard> {
        if self.lock_held.swap(true, Ordering::SeqCst) {
            return Err(BackendError::new(
                BackendErrorKind::Unavailable,
                "writer lock already held",
            ));
        }
        Ok(BackendWriterGuard::new(
            name.clone(),
            HeldWriterLock {
                locked: Arc::clone(&self.lock_held),
            },
        ))
    }

    fn append_object(&self, name: &ObjectName, bytes: &[u8]) -> BackendResult<BackendAppend> {
        let mut objects = self.objects.lock().expect("objects");
        let object = objects.entry(name.clone()).or_default();
        let start_offset = object.len() as u64;
        object.extend_from_slice(bytes);
        Ok(BackendAppend::new(
            start_offset,
            bytes.len() as u64,
            BackendMetadata::new(object.len() as u64, None),
        ))
    }

    fn sync_object(&self, _name: &ObjectName) -> BackendResult<()> {
        Ok(())
    }

    fn publish_object(
        &self,
        name: &ObjectName,
        bytes: &[u8],
        mode: PublishMode,
    ) -> PublishResult<PublishOutcome> {
        let mut objects = self.objects.lock().expect("objects");
        if mode == PublishMode::Create && objects.contains_key(name) {
            return Err(PublishError::precondition_failed(
                name,
                "object already exists",
            ));
        }
        objects.insert(name.clone(), bytes.to_vec());
        Ok(PublishOutcome::new(
            name.clone(),
            BackendMetadata::new(bytes.len() as u64, None),
            PublishDurability::Durable,
        ))
    }
}

struct HeldWriterLock {
    locked: Arc<AtomicBool>,
}

impl Drop for HeldWriterLock {
    fn drop(&mut self) {
        self.locked.store(false, Ordering::SeqCst);
    }
}
