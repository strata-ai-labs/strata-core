use super::*;

#[test]
fn commit_runtime_default_config_is_valid() {
    let config = CommitRuntimeConfig::default();

    assert!(config.max_mutations_per_batch() > 0);
    assert!(config.max_validation_facts_per_batch() > 0);
    assert!(config.max_commit_rows_per_batch() >= config.max_mutations_per_batch());
    assert_eq!(
        config.read_only_diagnostics(),
        CommitReadOnlyDiagnostics::Enabled
    );
}

#[test]
fn commit_runtime_config_rejects_unusable_limits() {
    assert_eq!(
        CommitRuntimeConfig::new(0, 1, 1, CommitReadOnlyDiagnostics::Enabled),
        Err(CommitRuntimeError::InvalidConfig {
            field: "max_mutations_per_batch",
            reason: "must be nonzero",
        })
    );
    assert_eq!(
        CommitRuntimeConfig::new(1, 0, 1, CommitReadOnlyDiagnostics::Enabled),
        Err(CommitRuntimeError::InvalidConfig {
            field: "max_validation_facts_per_batch",
            reason: "must be nonzero",
        })
    );
    assert_eq!(
        CommitRuntimeConfig::new(1, 1, 0, CommitReadOnlyDiagnostics::Enabled),
        Err(CommitRuntimeError::InvalidConfig {
            field: "max_commit_rows_per_batch",
            reason: "must be nonzero",
        })
    );
    assert_eq!(
        CommitRuntimeConfig::new(2, 1, 1, CommitReadOnlyDiagnostics::Enabled),
        Err(CommitRuntimeError::InvalidConfig {
            field: "max_commit_rows_per_batch",
            reason: "must be greater than or equal to max_mutations_per_batch",
        })
    );
}

#[test]
fn commit_runtime_config_accepts_explicit_diagnostics_mode() {
    let config = CommitRuntimeConfig::new(1, 1, 1, CommitReadOnlyDiagnostics::Disabled)
        .expect("valid commit config");

    assert_eq!(
        config.read_only_diagnostics(),
        CommitReadOnlyDiagnostics::Disabled
    );
}

#[test]
fn commit_runtime_visibility_facts_validate_order() {
    let version = CommitVersion::new(7);
    let facts = CommitVisibilityFacts::new(
        Some(version),
        Some(version),
        Some(version),
        Some(version),
        Some(version),
    )
    .expect("valid visibility facts");

    assert_eq!(facts.allocated_version(), Some(version));
    assert_eq!(facts.durable_version(), Some(version));
    assert_eq!(facts.applied_version(), Some(version));
    assert_eq!(facts.visible_version(), Some(version));
    assert_eq!(facts.timeline_version(), Some(version));

    let durable_not_applied =
        CommitVisibilityFacts::new(Some(version), Some(version), None, None, None)
            .expect("durable not applied facts are valid");
    assert_eq!(durable_not_applied.durable_version(), Some(version));
    assert_eq!(durable_not_applied.applied_version(), None);

    let cache_visible = CommitVisibilityFacts::new(
        Some(version),
        None,
        Some(version),
        Some(version),
        Some(version),
    )
    .expect("visible cache facts can be non-durable");
    assert_eq!(cache_visible.durable_version(), None);
    assert_eq!(cache_visible.visible_version(), Some(version));
}

#[test]
fn commit_runtime_visibility_facts_reject_impossible_order() {
    assert_eq!(
        CommitVisibilityFacts::new(
            Some(CommitVersion::new(1)),
            Some(CommitVersion::new(2)),
            None,
            None,
            None,
        ),
        Err(CommitRuntimeError::InvalidVisibilityFacts {
            reason: "durable version must not exceed allocated version",
        })
    );
    assert_eq!(
        CommitVisibilityFacts::new(
            Some(CommitVersion::new(1)),
            None,
            Some(CommitVersion::new(2)),
            None,
            None,
        ),
        Err(CommitRuntimeError::InvalidVisibilityFacts {
            reason: "applied version must not exceed allocated version",
        })
    );
    assert_eq!(
        CommitVisibilityFacts::new(
            Some(CommitVersion::new(2)),
            None,
            Some(CommitVersion::new(1)),
            Some(CommitVersion::new(2)),
            Some(CommitVersion::new(2)),
        ),
        Err(CommitRuntimeError::InvalidVisibilityFacts {
            reason: "visible version must not exceed applied version",
        })
    );
    assert_eq!(
        CommitVisibilityFacts::new(
            Some(CommitVersion::new(2)),
            None,
            Some(CommitVersion::new(2)),
            None,
            Some(CommitVersion::new(3)),
        ),
        Err(CommitRuntimeError::InvalidVisibilityFacts {
            reason: "timeline version must not exceed applied version",
        })
    );
    assert_eq!(
        CommitVisibilityFacts::new(
            Some(CommitVersion::new(2)),
            None,
            Some(CommitVersion::new(2)),
            Some(CommitVersion::new(2)),
            Some(CommitVersion::new(1)),
        ),
        Err(CommitRuntimeError::InvalidVisibilityFacts {
            reason: "visible version must not exceed timeline version",
        })
    );
}

#[test]
fn commit_runtime_empty_visibility_and_stats_are_explicit() {
    assert_eq!(
        CommitVisibilityFacts::empty(),
        CommitVisibilityFacts::default()
    );
    let stats = CommitRuntimeStats::default();

    assert_eq!(stats.committed_batches(), 0);
    assert_eq!(stats.read_only_batches(), 0);
    assert_eq!(stats.rejected_batches(), 0);
    assert_eq!(stats.replayed_batches(), 0);
    assert_eq!(stats.durable_but_not_visible(), 0);
}

#[test]
fn commit_runtime_phase_and_durability_vocabulary_is_closed() {
    let phases = [
        CommitPhase::RejectedBeforeAllocation,
        CommitPhase::AllocatedNotDurable,
        CommitPhase::DurableNotApplied,
        CommitPhase::AppliedNotVisible,
        CommitPhase::Visible,
        CommitPhase::Replay,
    ];
    let durability = [
        CommitDurabilityClass::NotDurable,
        CommitDurabilityClass::Standard,
        CommitDurabilityClass::Always,
        CommitDurabilityClass::Uncertain,
    ];

    assert_eq!(phases.len(), 6);
    assert_eq!(durability.len(), 4);
}

#[test]
fn commit_runtime_error_variants_are_constructible() {
    let branch = branch_id(200);
    let other = branch_id(201);
    let errors = [
        CommitRuntimeError::InvalidConfig {
            field: "field",
            reason: "bad limit",
        },
        CommitRuntimeError::InvalidCommitState {
            reason: "bad state",
        },
        CommitRuntimeError::InvalidCommitPhase {
            reason: "bad phase",
        },
        CommitRuntimeError::InvalidVisibilityFacts {
            reason: "bad facts",
        },
        CommitRuntimeError::InvalidBatch {
            reason: "bad batch",
        },
        CommitRuntimeError::InvalidMutation {
            reason: "bad mutation",
        },
        CommitRuntimeError::InvalidValidationFacts {
            reason: "bad validation",
        },
        CommitRuntimeError::DuplicateMutationKey {
            space_id: StorageSpaceId::engine(0x20).expect("engine id"),
        },
        CommitRuntimeError::BranchMismatch {
            expected: branch,
            actual: other,
        },
        CommitRuntimeError::StorageOwnedMutationSpace {
            space_id: StorageSpaceId::COMMIT_TIMELINE,
        },
        CommitRuntimeError::BranchUnavailable {
            reason: "branch closed",
        },
        CommitRuntimeError::AppliedButNotVisible {
            branch_id: branch,
            commit_version: CommitVersion::new(3),
            reason: "visible publication failed",
        },
        CommitRuntimeError::DurabilityUnavailable {
            reason: "wal writer halted",
        },
        CommitRuntimeError::VersionAllocatorOverflow {
            last_allocated: CommitVersion::MAX,
        },
        CommitRuntimeError::timestamp_unavailable("clock unavailable"),
        CommitRuntimeError::InvalidTimestampPolicy {
            reason: "timestamp moved backward",
        },
        CommitRuntimeError::lower_layer(CommitLowerLayer::WalService, "append failed"),
    ];

    for err in errors {
        let display = err.to_string();
        assert!(display.contains("commit"));
        assert!(!display.contains("TransactionContext"));
        assert!(!display.contains("TransactionId"));
        assert!(err.source().is_none());
    }
}

#[test]
fn commit_runtime_error_display_is_bounded_storage_vocabulary() {
    let err = CommitRuntimeError::InvalidCommitPhase {
        reason: "visible before applied",
    };
    let display = err.to_string();
    let durability_display = CommitRuntimeError::DurabilityUnavailable {
        reason: "wal writer halted",
    }
    .to_string();

    assert!(display.contains("commit phase"));
    assert!(durability_display.contains("durability"));
    assert!(!display.contains("rollback"));
    assert!(!display.contains("VersionedValue"));
}

#[test]
fn commit_runtime_error_source_chain_is_preserved() {
    let err = CommitRuntimeError::lower_layer_with(
        CommitLowerLayer::BranchRuntime,
        "branch state rejected commit rows",
        WrappedSource,
    );
    let format_err = CommitRuntimeError::lower_layer(CommitLowerLayer::WalFormat, "decode failed");

    assert_eq!(
        err.source().map(ToString::to_string),
        Some("wrapped source".to_owned())
    );
    assert!(format_err.to_string().contains("wal format"));
    assert_eq!(
        err,
        CommitRuntimeError::lower_layer(
            CommitLowerLayer::BranchRuntime,
            "branch state rejected commit rows",
        )
    );
}

#[test]
fn commit_runtime_result_alias_uses_commit_error() {
    fn returns_alias() -> CommitRuntimeResult<()> {
        Err(CommitRuntimeError::BranchUnavailable {
            reason: "branch is closing",
        })
    }

    assert!(matches!(
        returns_alias(),
        Err(CommitRuntimeError::BranchUnavailable { .. })
    ));
}

#[test]
fn commit_runtime_stats_can_be_constructed_for_scaffold_contracts() {
    let stats = CommitRuntimeStats::new(1, 2, 3, 4, 5);

    assert_eq!(stats.committed_batches(), 1);
    assert_eq!(stats.read_only_batches(), 2);
    assert_eq!(stats.rejected_batches(), 3);
    assert_eq!(stats.replayed_batches(), 4);
    assert_eq!(stats.durable_but_not_visible(), 5);
}
#[derive(Debug)]
struct WrappedSource;

impl fmt::Display for WrappedSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("wrapped source")
    }
}

impl Error for WrappedSource {}
