mod sequencing;
mod support;

use super::{CheckpointManifestOperation, CheckpointService, CheckpointServiceError};
use crate::backend::{Backend, PublishDurability, PublishFailureKind, PublishMode};
use crate::layout::ObjectLayout;
use crate::service::{
    DatabaseManifestService, ManifestRole, ManifestServiceError, SnapshotService,
    SnapshotServiceError,
};
use strata_core_next::{CommitVersion, Timestamp};
use support::{
    assert_manifest_has_no_snapshot_facts, assert_no_snapshot_publish, request, request_with_facts,
    seeded_backend, PublishEvent, PublishPoint, RecordingBackend, ACTIVE_WAL_SEGMENT, CODEC_ID,
    DATABASE_ID, OTHER_CODEC_ID, OTHER_DATABASE_ID, SNAPSHOT_ID, SNAPSHOT_WATERMARK,
};

#[test]
fn checkpoint_success_persists_wal_snapshot_and_manifest_facts_in_order() {
    let backend = seeded_backend(CODEC_ID);
    let service = CheckpointService::new(&backend);
    let write = service.checkpoint(request()).expect("checkpoint succeeds");

    assert_eq!(backend.read_count(), 2);
    assert_eq!(
        backend.events(),
        vec![
            PublishEvent {
                point: PublishPoint::ActiveWalManifest,
                mode: PublishMode::Replace
            },
            PublishEvent {
                point: PublishPoint::Snapshot,
                mode: PublishMode::Create
            },
            PublishEvent {
                point: PublishPoint::SnapshotFactsManifest,
                mode: PublishMode::Replace
            },
        ]
    );

    let manifest = backend.read_database_manifest();
    assert_eq!(manifest.active_wal_segment(), ACTIVE_WAL_SEGMENT);
    assert_eq!(manifest.snapshot_id(), Some(SNAPSHOT_ID));
    assert_eq!(manifest.snapshot_watermark(), Some(SNAPSHOT_WATERMARK));

    let snapshot = write.snapshot();
    assert_eq!(write.active_wal_segment(), ACTIVE_WAL_SEGMENT);
    assert_eq!(snapshot.snapshot_id(), SNAPSHOT_ID);
    assert_eq!(
        snapshot.snapshot_watermark(),
        CommitVersion::new(SNAPSHOT_WATERMARK)
    );
    assert_eq!(snapshot.created_at(), Timestamp::from_micros(1_700));
    assert_eq!(snapshot.section_count(), 2);
    assert_eq!(snapshot.publish_outcome().object(), snapshot.object());
    assert_eq!(
        snapshot.publish_outcome().durability(),
        PublishDurability::Durable
    );
    assert_eq!(
        backend
            .snapshot_bytes(SNAPSHOT_ID)
            .expect("snapshot bytes")
            .len() as u64,
        snapshot.byte_count()
    );

    let loaded = SnapshotService::new(&backend)
        .load_required_for_codec(SNAPSHOT_ID, DATABASE_ID, CODEC_ID)
        .expect("load published snapshot");
    assert_eq!(loaded.sections().len(), 2);
}

#[test]
fn checkpoint_missing_manifest_fails_before_snapshot_publish() {
    let backend = RecordingBackend::new();
    let service = CheckpointService::new(&backend);

    let error = service
        .checkpoint(request())
        .expect_err("checkpoint requires manifest");

    assert!(matches!(
        error,
        CheckpointServiceError::Manifest {
            operation: CheckpointManifestOperation::LoadCurrent,
            ref source,
        } if matches!(
            source.as_ref(),
            ManifestServiceError::Missing {
                role: ManifestRole::Database,
                ..
            }
        )
    ));
    assert_no_snapshot_publish(&backend);
}

#[test]
fn checkpoint_corrupt_manifest_fails_before_snapshot_publish() {
    let backend = RecordingBackend::new();
    let object = ObjectLayout::database_manifest().expect("database manifest object");
    backend
        .write_object(&object, b"not a database manifest")
        .expect("write corrupt manifest");
    let service = CheckpointService::new(&backend);

    let error = service
        .checkpoint(request())
        .expect_err("corrupt manifest fails closed");

    assert!(matches!(
        error,
        CheckpointServiceError::Manifest {
            operation: CheckpointManifestOperation::LoadCurrent,
            ref source,
        } if matches!(source.as_ref(), ManifestServiceError::Decode { .. })
    ));
    assert_no_snapshot_publish(&backend);
}

#[test]
fn checkpoint_codec_mismatch_fails_before_snapshot_publish() {
    let backend = seeded_backend(OTHER_CODEC_ID);
    let service = CheckpointService::new(&backend);

    let error = service
        .checkpoint(request())
        .expect_err("codec mismatch fails closed");

    assert!(matches!(
        error,
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
    assert_no_snapshot_publish(&backend);
}

#[test]
fn checkpoint_database_mismatch_fails_before_snapshot_publish() {
    let backend = seeded_backend(CODEC_ID);
    let service = CheckpointService::new(&backend);

    let error = service
        .checkpoint(request_with_facts(
            ACTIVE_WAL_SEGMENT,
            SNAPSHOT_ID,
            CommitVersion::new(SNAPSHOT_WATERMARK),
            CODEC_ID,
            OTHER_DATABASE_ID,
        ))
        .expect_err("database mismatch fails closed");

    assert!(matches!(
        error,
        CheckpointServiceError::DatabaseMismatch {
            expected: OTHER_DATABASE_ID,
            actual: DATABASE_ID,
        }
    ));
    assert_no_snapshot_publish(&backend);
}

#[test]
fn checkpoint_rejects_invalid_input_facts_before_manifest_mutation() {
    let cases = [
        (
            request_with_facts(
                0,
                SNAPSHOT_ID,
                CommitVersion::new(SNAPSHOT_WATERMARK),
                CODEC_ID,
                DATABASE_ID,
            ),
            "active_wal_segment",
        ),
        (
            request_with_facts(
                ACTIVE_WAL_SEGMENT,
                0,
                CommitVersion::new(SNAPSHOT_WATERMARK),
                CODEC_ID,
                DATABASE_ID,
            ),
            "snapshot_id",
        ),
        (
            request_with_facts(
                ACTIVE_WAL_SEGMENT,
                SNAPSHOT_ID,
                CommitVersion::ZERO,
                CODEC_ID,
                DATABASE_ID,
            ),
            "snapshot_watermark",
        ),
    ];

    for (request, field) in cases {
        let backend = seeded_backend(CODEC_ID);
        let service = CheckpointService::new(&backend);

        let error = service
            .checkpoint(request)
            .expect_err("invalid fact is rejected");

        assert!(matches!(
            error,
            CheckpointServiceError::InvalidCheckpointFact {
                field: actual,
                value: 0,
            } if actual == field
        ));
        assert!(backend.events().is_empty());
        assert_manifest_has_no_snapshot_facts(&backend);
        assert!(backend.snapshot_bytes(SNAPSHOT_ID).is_none());
    }
}

#[test]
fn active_wal_manifest_failure_stops_before_snapshot_publish() {
    let backend = seeded_backend(CODEC_ID);
    backend.fail_next_publish(
        PublishPoint::ActiveWalManifest,
        PublishFailureKind::FailedBeforeVisibility,
    );
    let service = CheckpointService::new(&backend);

    let error = service
        .checkpoint(request())
        .expect_err("active WAL manifest update fails");

    assert!(matches!(
        error,
        CheckpointServiceError::Manifest {
            operation: CheckpointManifestOperation::PersistActiveWalSegment,
            ref source,
        } if matches!(
            source.as_ref(),
            ManifestServiceError::Publish {
                role: ManifestRole::Database,
                ..
            }
        )
    ));
    assert_eq!(
        backend.events(),
        vec![PublishEvent {
            point: PublishPoint::ActiveWalManifest,
            mode: PublishMode::Replace
        }]
    );
    let manifest = backend.read_database_manifest();
    assert_eq!(manifest.active_wal_segment(), 1);
    assert_eq!(manifest.snapshot_id(), None);
    assert!(backend.snapshot_bytes(SNAPSHOT_ID).is_none());
}

#[test]
fn snapshot_publish_failures_do_not_persist_snapshot_manifest_facts() {
    let cases = [
        PublishFailureKind::Unsupported,
        PublishFailureKind::PreconditionFailed,
        PublishFailureKind::FailedBeforeVisibility,
        PublishFailureKind::VisibilityUnknown,
        PublishFailureKind::VisibleDurabilityUnconfirmed,
    ];

    for kind in cases {
        let backend = seeded_backend(CODEC_ID);
        backend.fail_next_publish(PublishPoint::Snapshot, kind);
        let service = CheckpointService::new(&backend);

        let error = service
            .checkpoint(request())
            .expect_err("snapshot publish failure stops checkpoint");

        assert!(matches!(
            error,
            CheckpointServiceError::Snapshot {
                ref source,
            } if matches!(
                source.as_ref(),
                SnapshotServiceError::Publish {
                    snapshot_id: SNAPSHOT_ID,
                    source,
                } if source.kind() == kind
            )
        ));
        assert_eq!(
            backend.events(),
            vec![
                PublishEvent {
                    point: PublishPoint::ActiveWalManifest,
                    mode: PublishMode::Replace
                },
                PublishEvent {
                    point: PublishPoint::Snapshot,
                    mode: PublishMode::Create
                },
            ]
        );
        let manifest = backend.read_database_manifest();
        assert_eq!(manifest.active_wal_segment(), ACTIVE_WAL_SEGMENT);
        assert_eq!(manifest.snapshot_id(), None);
        assert_eq!(manifest.snapshot_watermark(), None);

        if kind == PublishFailureKind::VisibleDurabilityUnconfirmed {
            assert!(backend.snapshot_bytes(SNAPSHOT_ID).is_some());
        } else {
            assert!(backend.snapshot_bytes(SNAPSHOT_ID).is_none());
        }
    }
}

#[test]
fn final_manifest_no_visible_failures_return_orphan_snapshot_facts() {
    let cases = [
        PublishFailureKind::Unsupported,
        PublishFailureKind::PreconditionFailed,
        PublishFailureKind::FailedBeforeVisibility,
    ];

    for kind in cases {
        let backend = seeded_backend(CODEC_ID);
        backend.fail_next_publish(PublishPoint::SnapshotFactsManifest, kind);
        let service = CheckpointService::new(&backend);

        let error = service
            .checkpoint(request())
            .expect_err("final manifest update fails");

        let snapshot = match error {
            CheckpointServiceError::OrphanSnapshot { snapshot, source } => {
                assert!(matches!(
                    source.as_ref(),
                    ManifestServiceError::Publish {
                        role: ManifestRole::Database,
                        source,
                    } if source.kind() == kind
                ));
                snapshot
            }
            other => panic!("expected orphan snapshot error, got {other:?}"),
        };

        assert_eq!(snapshot.snapshot_id(), SNAPSHOT_ID);
        assert_eq!(
            snapshot.snapshot_watermark(),
            CommitVersion::new(SNAPSHOT_WATERMARK)
        );
        assert_eq!(snapshot.created_at(), Timestamp::from_micros(1_700));
        assert_eq!(snapshot.section_count(), 2);
        assert_eq!(
            snapshot.publish_outcome().durability(),
            PublishDurability::Durable
        );
        assert_eq!(
            backend
                .snapshot_bytes(SNAPSHOT_ID)
                .expect("orphan snapshot bytes are visible")
                .len() as u64,
            snapshot.byte_count()
        );
        assert_eq!(
            backend.events(),
            vec![
                PublishEvent {
                    point: PublishPoint::ActiveWalManifest,
                    mode: PublishMode::Replace
                },
                PublishEvent {
                    point: PublishPoint::Snapshot,
                    mode: PublishMode::Create
                },
                PublishEvent {
                    point: PublishPoint::SnapshotFactsManifest,
                    mode: PublishMode::Replace
                },
            ]
        );

        let manifest = backend.read_database_manifest();
        assert_eq!(manifest.active_wal_segment(), ACTIVE_WAL_SEGMENT);
        assert_eq!(manifest.snapshot_id(), None);
        assert_eq!(manifest.snapshot_watermark(), None);
        SnapshotService::new(&backend)
            .load_required_for_codec(SNAPSHOT_ID, DATABASE_ID, CODEC_ID)
            .expect("orphan snapshot can be directly loaded");
    }
}

#[test]
fn final_manifest_uncertain_failures_are_not_reported_as_orphans() {
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
            .expect_err("final manifest update is uncertain");

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
            snapshot.snapshot_watermark(),
            CommitVersion::new(SNAPSHOT_WATERMARK)
        );
        assert!(backend.snapshot_bytes(SNAPSHOT_ID).is_some());

        let manifest = backend.read_database_manifest();
        if kind == PublishFailureKind::VisibleDurabilityUnconfirmed {
            assert_eq!(manifest.snapshot_id(), Some(SNAPSHOT_ID));
            assert_eq!(manifest.snapshot_watermark(), Some(SNAPSHOT_WATERMARK));
        } else {
            assert_eq!(manifest.snapshot_id(), None);
            assert_eq!(manifest.snapshot_watermark(), None);
        }
    }
}

#[test]
fn final_manifest_failure_preserves_previous_snapshot_facts() {
    let backend = seeded_backend(CODEC_ID);
    DatabaseManifestService::new(&backend)
        .persist_snapshot_facts(3, CommitVersion::new(11))
        .expect("persist previous snapshot facts");
    backend.clear_events();
    backend.fail_next_publish(
        PublishPoint::SnapshotFactsManifest,
        PublishFailureKind::FailedBeforeVisibility,
    );
    let service = CheckpointService::new(&backend);

    let error = service
        .checkpoint(request())
        .expect_err("final manifest update fails");

    assert!(matches!(
        error,
        CheckpointServiceError::OrphanSnapshot { .. }
    ));
    let manifest = backend.read_database_manifest();
    assert_eq!(manifest.active_wal_segment(), ACTIVE_WAL_SEGMENT);
    assert_eq!(manifest.snapshot_id(), Some(3));
    assert_eq!(manifest.snapshot_watermark(), Some(11));
    assert!(backend.snapshot_bytes(SNAPSHOT_ID).is_some());
}
