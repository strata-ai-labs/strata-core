//! Generated branch-LSM scaffold contract helpers.

use super::TestkitError;
use crate::branch::{
    install_snapshot_rows_into_branches, require_row_branch, rewrite_physical_key_branch,
    rewrite_row_branch, row_matches_branch, BranchCompactionKind, BranchCompactionNoopReason,
    BranchCompactionRecovery, BranchCompactionRequest, BranchCompactionRetentionPolicy,
    BranchEffectiveReadBound, BranchForkOutcome, BranchHistoryOptions, BranchHistoryRow,
    BranchImmutableInstallOutcome, BranchInheritedLayer, BranchLevel, BranchLocalState,
    BranchMaterializationOutcome, BranchMaterializationRecovery, BranchMaterializationRequest,
    BranchOwnedTable, BranchProtectionReason, BranchReachabilityAggregate, BranchReachabilityFacts,
    BranchReachabilitySnapshot, BranchReadBound, BranchReadView, BranchReleasePlan,
    BranchRotationOutcome, BranchRotationSkipReason, BranchRowCandidateFacts, BranchRowSource,
    BranchRuntimeConfig, BranchRuntimeError, BranchRuntimeStats, BranchScanBounds,
    BranchSnapshotInstallGroup, BranchSnapshotInstallRecovery, BranchSnapshotInstallRequest,
    BranchSnapshotMissingBranchPolicy, BranchStateDescriptor, BranchStateFacts,
    BranchTableDescriptor, BranchTableRef, BranchTableReferenceKind, BranchTimestampCoverage,
    BranchTimestampHistorySource, BranchUserKeyBound, BranchViewDescriptor, BranchVisibleRow,
    InheritedLayerDescriptor, InheritedLayerStatus, SharedTableRegistry,
};
use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};
use crate::table::{
    sort_table_rows_by_key, ImmutableTableBuilder, ImmutableTableReader, TableBuilderConfig,
    TableCommitRange, TableCompactionConfig, TableIdentity, TableInternalKeyBytes, TableKeyRange,
    TablePhysicalKeyBytes, TableReaderConfig, TableRow, TableRuntimeFacts,
};
use std::error::Error;
use std::fmt;
use strata_core_next::{BranchId, CommitVersion, Timestamp};

include!("outcome.rs");
include!("outcome_accessors.rs");
include!("outcome_absorb.rs");
include!("contracts.rs");
include!("config_identity.rs");
include!("state_read.rs");
include!("immutable_inheritance.rs");
include!("timestamp_own.rs");
include!("timestamp_inherited.rs");
include!("materialization.rs");
include!("reachability.rs");
include!("compaction.rs");
include!("snapshot_install.rs");
include!("model_store.rs");
include!("model_assertions.rs");
include!("fault_helpers.rs");
include!("tests.rs");
