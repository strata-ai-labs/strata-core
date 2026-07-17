use super::*;

#[test]
fn api_module_exports_storage_runtime_shell() {
    assert_eq!(
        StorageRuntime::closed().state(),
        StorageRuntimeState::Closed
    );
}

#[test]
fn open_options_do_not_have_implicit_default_mode() {
    let options_source = include_str!("../options.rs");

    assert_storage_open_options_has_no_default(options_source);
}

fn assert_storage_open_options_has_no_default(source: &str) {
    let compact_source = source.split_whitespace().collect::<String>();
    for forbidden_impl in [
        "implDefaultforStorageOpenOptions",
        "implstd::default::DefaultforStorageOpenOptions",
        "implcore::default::DefaultforStorageOpenOptions",
        "impl::std::default::DefaultforStorageOpenOptions",
        "impl::core::default::DefaultforStorageOpenOptions",
    ] {
        assert!(
            !compact_source.contains(forbidden_impl),
            "StorageOpenOptions must not implement Default via {forbidden_impl}"
        );
    }

    assert!(!source.contains("StorageOpenOptions::default"));
    assert_storage_open_options_derive_excludes_default(&compact_source);
}

fn assert_storage_open_options_derive_excludes_default(compact_source: &str) {
    let prefix = compact_source
        .split("pubstructStorageOpenOptions")
        .next()
        .expect("StorageOpenOptions declaration is present");
    let derive_args = prefix
        .rsplit("#[derive(")
        .next()
        .unwrap_or_default()
        .split(")]")
        .next()
        .unwrap_or_default();

    assert!(
        !derive_args
            .split(',')
            .any(|trait_path| trait_path.ends_with("Default")),
        "StorageOpenOptions must not derive Default"
    );
}

#[test]
fn api_module_exports_storage_result() {
    let result = assert_result_type::<()>(Ok(()));
    assert!(result.is_ok());
}

#[test]
fn api_module_exports_storage_error() {
    let error = StorageApiError::InvalidRuntimeState {
        reason: "runtime is closed",
    };
    assert_eq!(error.class(), StorageApiErrorClass::FailedPrecondition);
}

#[test]
fn api_module_exports_open_options_shell() {
    let options = StorageOpenOptions::durable_local(StorageDurabilityPolicy::Always)
        .with_strict_recovery(false);
    assert_eq!(
        options.mode(),
        StorageMode::DurableLocal {
            policy: StorageDurabilityPolicy::Always,
        }
    );
    assert!(!options.strict_recovery());
}

#[test]
fn api_module_exports_read_selector_shell() {
    let request = PointReadRequest::new(
        branch_id(1),
        space(),
        key(b"read-key"),
        ReadBound::AtTimestamp(Timestamp::from_micros(99)),
    );
    assert_eq!(
        request.bound(),
        ReadBound::AtTimestamp(Timestamp::from_micros(99))
    );
    assert_eq!(request.branch_id(), branch_id(1));
    assert_eq!(request.storage_space(), &space());
    assert_eq!(request.key(), &key(b"read-key"));
}

#[test]
fn api_module_exports_commit_batch_shell() {
    let batch = CommitBatch::new(
        branch_id(2),
        vec![CommitMutation::Put {
            storage_space: space(),
            key: key(b"commit-key"),
            value: StorageValue::new(b"value".to_vec()),
            ttl: None,
        }],
        CommitOptions::default().require_conflict_check(true),
    )
    .expect("valid batch");
    assert_eq!(batch.branch_id(), branch_id(2));
    assert!(batch.options().conflict_check_required());
    assert_eq!(batch.mutations().len(), 1);
}

#[test]
fn api_module_exports_branch_request_shell() {
    let request = BranchRequest::new(
        branch_id(3),
        BranchAction::ForkAtVersion {
            source: branch_id(4),
            version: CommitVersion::new(12),
        },
        Some(BranchGeneration::new(2)),
    );
    assert!(matches!(
        request.action(),
        BranchAction::ForkAtVersion { .. }
    ));
    assert_eq!(request.branch_id(), branch_id(3));
    assert_eq!(
        request.expected_generation(),
        Some(BranchGeneration::new(2))
    );
}

#[test]
fn api_module_exports_maintenance_request_shell() {
    let request = MaintenanceRequest::new(
        MaintenanceTask::Repair,
        MaintenanceScope::Branch(branch_id(5)),
    );
    assert_eq!(request.task(), MaintenanceTask::Repair);
    assert_eq!(request.scope(), MaintenanceScope::Branch(branch_id(5)));
}

#[test]
fn api_module_exports_diagnostics_shell() {
    let request = DiagnosticsRequest::new(DiagnosticsScope::Global);
    assert_eq!(request.scope(), DiagnosticsScope::Global);
}

#[test]
fn storage_api_error_codes_are_stable() {
    let branch = branch_id(7);
    let cases = [
        (
            StorageApiError::InvalidArgument {
                field: "key",
                reason: "bad key",
            },
            "invalid_argument.storage_api.argument",
        ),
        (
            StorageApiError::UnsupportedCapability {
                capability: "object_durable",
                reason: "unsupported",
            },
            "unsupported.storage_api.capability",
        ),
        (
            StorageApiError::BranchNotFound { branch_id: branch },
            "not_found.storage_api.branch",
        ),
        (
            StorageApiError::RetainedHistoryUnavailable {
                branch_id: branch,
                reason: "pruned",
            },
            "history_unavailable.storage_api.retained",
        ),
        (
            StorageApiError::TimestampHistoryUnavailable {
                branch_id: branch,
                reason: "pruned",
            },
            "history_unavailable.storage_api.timestamp",
        ),
        (
            StorageApiError::durable_uncertain("sync uncertain"),
            "ambiguous_commit.storage_api.durable_uncertain",
        ),
        (
            StorageApiError::RecoveryDegraded {
                reason: "recovery has storage health debt",
            },
            "failed_precondition.storage_api.recovery_degraded",
        ),
    ];

    for (error, code) in cases {
        assert_eq!(error.code(), code);
    }
}

#[test]
fn storage_api_error_display_is_not_empty() {
    let error = StorageApiError::InvalidRuntimeState {
        reason: "runtime is closed",
    };
    assert!(!error.to_string().is_empty());
}

#[test]
fn storage_api_error_source_chain_is_preserved() {
    let error = StorageApiError::lower_layer_with(
        StorageApiLowerLayer::Service,
        "service failed",
        SourceError,
    );

    assert_eq!(error.code(), "internal.storage_api.service");
    assert!(error.source().is_some());
}

#[test]
fn storage_api_error_invalid_argument_has_structured_field() {
    let error = StorageApiError::InvalidArgument {
        field: "key",
        reason: "empty",
    };
    match error {
        StorageApiError::InvalidArgument { field, reason } => {
            assert_eq!(field, "key");
            assert_eq!(reason, "empty");
        }
        _ => panic!("expected invalid argument"),
    }
}

#[test]
fn storage_api_error_unsupported_capability_has_structured_field() {
    let error = StorageApiError::UnsupportedCapability {
        capability: "object_durable",
        reason: "unsupported",
    };
    assert_eq!(error.class(), StorageApiErrorClass::Unsupported);
    match error {
        StorageApiError::UnsupportedCapability { capability, reason } => {
            assert_eq!(capability, "object_durable");
            assert_eq!(reason, "unsupported");
        }
        _ => panic!("expected unsupported capability"),
    }
}

#[test]
fn storage_api_error_history_unavailable_is_distinct_from_not_found() {
    let branch = branch_id(8);
    let history = StorageApiError::RetainedHistoryUnavailable {
        branch_id: branch,
        reason: "pruned",
    };
    let not_found = StorageApiError::BranchNotFound { branch_id: branch };

    assert_ne!(history.code(), not_found.code());
    assert_eq!(history.class(), StorageApiErrorClass::HistoryUnavailable);
    assert_eq!(not_found.class(), StorageApiErrorClass::NotFound);
}

#[test]
fn storage_api_error_durable_uncertain_is_distinct_from_lower_layer_failure() {
    let uncertain = StorageApiError::durable_uncertain("sync uncertain");
    let lower = StorageApiError::lower_layer_with(
        StorageApiLowerLayer::Service,
        "service failed",
        SourceError,
    );

    assert_ne!(uncertain.code(), lower.code());
    assert_eq!(uncertain.class(), StorageApiErrorClass::AmbiguousCommit);
    assert_eq!(lower.class(), StorageApiErrorClass::Internal);
}

#[test]
fn storage_api_error_display_does_not_include_payload_bytes() {
    let error = StorageApiError::lower_layer_with(
        StorageApiLowerLayer::Service,
        "service failed",
        PayloadSourceError,
    );
    let display = error.to_string();

    assert!(!display.contains("secret-payload"));
    assert!(!display.contains("[1, 2, 3]"));
}

#[test]
fn storage_api_error_classes_do_not_overclaim_corruption() {
    assert_eq!(
        StorageApiError::RecoveryDegraded {
            reason: "policy downgrade",
        }
        .class(),
        StorageApiErrorClass::FailedPrecondition
    );
    assert_eq!(
        StorageApiError::durable_uncertain("sync uncertain").class(),
        StorageApiErrorClass::AmbiguousCommit
    );
}

#[test]
fn storage_key_rejects_empty_when_required() {
    assert!(StorageKey::new(Vec::<u8>::new()).is_err());
}

#[test]
fn storage_value_accepts_opaque_bytes() {
    let bytes = vec![0, 159, 255, b'\n'];
    let value = StorageValue::new(bytes.clone());
    assert_eq!(value.as_bytes(), bytes.as_slice());
}

#[test]
fn read_limit_rejects_zero_when_zero_is_invalid() {
    assert!(ReadLimit::new(0).is_err());
    assert_eq!(ReadLimit::new(3).expect("valid limit").get(), 3);
}

#[test]
fn scan_bound_order_is_validated() {
    let start = key(b"a");
    let end = key(b"z");
    let valid = ScanRange::new(Some(start.clone()), Some(end)).expect("valid range");
    assert_eq!(valid.start(), Some(&start));

    let invalid = ScanRange::new(Some(key(b"z")), Some(key(b"a"))).expect_err("invalid range");
    assert_eq!(invalid.code(), "invalid_argument.storage_api.argument");
}

#[test]
fn branch_generation_zero_policy_is_explicit() {
    assert_eq!(BranchGeneration::ZERO.as_u64(), 0);
    assert_eq!(BranchGeneration::new(0), BranchGeneration::ZERO);
}

#[test]
fn maintenance_request_kind_is_constructible() {
    let kinds = [
        MaintenanceTask::Checkpoint,
        MaintenanceTask::Flush,
        MaintenanceTask::Compact,
        MaintenanceTask::Materialize,
        MaintenanceTask::Retain,
        MaintenanceTask::SnapshotPruning,
        MaintenanceTask::Reclaim,
        MaintenanceTask::Quarantine,
        MaintenanceTask::Purge,
        MaintenanceTask::Repair,
        MaintenanceTask::WalGrowth,
    ];

    for kind in kinds {
        assert_eq!(
            MaintenanceRequest::new(kind, MaintenanceScope::Global).task(),
            kind
        );
    }
}

#[test]
fn diagnostics_request_kind_is_constructible() {
    let global = DiagnosticsRequest::new(DiagnosticsScope::Global);
    let branch = DiagnosticsRequest::new(DiagnosticsScope::Branch(branch_id(9)));

    assert_eq!(global.scope(), DiagnosticsScope::Global);
    assert_eq!(branch.scope(), DiagnosticsScope::Branch(branch_id(9)));
}
