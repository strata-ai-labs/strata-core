use super::super::{CheckpointManifestOperation, CheckpointService, CheckpointServiceError};
use super::support::{
    assert_manifest_has_no_snapshot_facts, assert_no_snapshot_publish, request, request_with_facts,
    seeded_backend, PublishEvent, PublishPoint, ACTIVE_WAL_SEGMENT, CODEC_ID, DATABASE_ID,
    OTHER_CODEC_ID, OTHER_DATABASE_ID, SNAPSHOT_ID, SNAPSHOT_WATERMARK,
};
use crate::backend::{PublishFailureKind, PublishMode};
use crate::service::{ManifestRole, ManifestServiceError, SnapshotService};
use strata_core::CommitVersion;

#[test]
fn active_wal_manifest_failure_kinds_stop_before_snapshot_publish() {
    // The first manifest mutation happens before snapshot visibility, so every
    // publish outcome must halt without leaving an orphan snapshot.
    let cases = [
        PublishFailureKind::Unsupported,
        PublishFailureKind::PreconditionFailed,
        PublishFailureKind::FailedBeforeVisibility,
        PublishFailureKind::VisibilityUnknown,
        PublishFailureKind::VisibleDurabilityUnconfirmed,
    ];

    for kind in cases {
        let backend = seeded_backend(CODEC_ID);
        backend.fail_next_publish(PublishPoint::ActiveWalManifest, kind);
        let service = CheckpointService::new(&backend);

        let error = service
            .checkpoint(request())
            .expect_err("active WAL manifest failure stops checkpoint");

        assert!(matches!(
            error,
            CheckpointServiceError::Manifest {
                operation: CheckpointManifestOperation::PersistActiveWalSegment,
                ref source,
            } if matches!(
                source.as_ref(),
                ManifestServiceError::Publish {
                    role: ManifestRole::Database,
                    source,
                } if source.kind() == kind
            )
        ));
        assert_eq!(
            backend.events(),
            vec![PublishEvent {
                point: PublishPoint::ActiveWalManifest,
                mode: PublishMode::Replace,
            }]
        );
        assert_no_snapshot_publish(&backend);
        assert_manifest_has_no_snapshot_facts(&backend);
    }
}

#[test]
fn manifest_identity_failures_leave_checkpoint_unstarted() {
    let codec_backend = seeded_backend(OTHER_CODEC_ID);
    let codec_service = CheckpointService::new(&codec_backend);
    let codec_error = codec_service
        .checkpoint(request())
        .expect_err("codec mismatch fails before checkpoint work");

    assert!(matches!(
        codec_error,
        CheckpointServiceError::Manifest {
            operation: CheckpointManifestOperation::LoadCurrent,
            ref source,
        } if matches!(
            source.as_ref(),
            ManifestServiceError::CodecMismatch {
                expected,
                actual,
                ..
            } if expected == CODEC_ID && actual == OTHER_CODEC_ID
        )
    ));
    assert!(codec_backend.events().is_empty());
    assert_no_snapshot_publish(&codec_backend);
    assert_manifest_has_no_snapshot_facts(&codec_backend);

    let database_backend = seeded_backend(CODEC_ID);
    let database_service = CheckpointService::new(&database_backend);
    let database_error = database_service
        .checkpoint(request_with_facts(
            ACTIVE_WAL_SEGMENT,
            SNAPSHOT_ID,
            CommitVersion::new(SNAPSHOT_WATERMARK),
            CODEC_ID,
            OTHER_DATABASE_ID,
        ))
        .expect_err("database mismatch fails before checkpoint work");

    assert!(matches!(
        database_error,
        CheckpointServiceError::DatabaseMismatch {
            expected: OTHER_DATABASE_ID,
            actual: DATABASE_ID,
        }
    ));
    assert!(database_backend.events().is_empty());
    assert_no_snapshot_publish(&database_backend);
    assert_manifest_has_no_snapshot_facts(&database_backend);
}

#[test]
fn final_manifest_uncertainty_keeps_snapshot_directly_loadable() {
    // At this point the snapshot is already visible. Lifecycle recovery owns
    // final resolution, but the checkpoint service must preserve direct access.
    let cases = [
        PublishFailureKind::VisibilityUnknown,
        PublishFailureKind::VisibleDurabilityUnconfirmed,
    ];

    for kind in cases {
        let backend = seeded_backend(CODEC_ID);
        backend.fail_next_publish(PublishPoint::SnapshotFactsManifest, kind);
        let service = CheckpointService::new(&backend);

        let error = service
            .checkpoint(request())
            .expect_err("final manifest uncertainty stops checkpoint");

        let snapshot = match error {
            CheckpointServiceError::FinalManifestUncertain { snapshot, source } => {
                assert!(matches!(
                    source.as_ref(),
                    ManifestServiceError::Publish {
                        role: ManifestRole::Database,
                        source,
                    } if source.kind() == kind
                ));
                snapshot
            }
            other => panic!("expected final manifest uncertainty, got {other:?}"),
        };
        assert_eq!(snapshot.snapshot_id(), SNAPSHOT_ID);
        assert_eq!(
            backend.events(),
            vec![
                PublishEvent {
                    point: PublishPoint::ActiveWalManifest,
                    mode: PublishMode::Replace,
                },
                PublishEvent {
                    point: PublishPoint::Snapshot,
                    mode: PublishMode::Create,
                },
                PublishEvent {
                    point: PublishPoint::SnapshotFactsManifest,
                    mode: PublishMode::Replace,
                },
            ]
        );

        let loaded = SnapshotService::new(&backend)
            .load_required_for_codec(SNAPSHOT_ID, DATABASE_ID, CODEC_ID)
            .expect("published snapshot is directly loadable after final manifest uncertainty");
        assert_eq!(loaded.sections().len(), 2);
    }
}
