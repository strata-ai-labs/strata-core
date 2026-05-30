//! Generated branch-LSM scaffold contract helpers.

use super::TestkitError;
use crate::branch::config::BranchRuntimeConfig;
use crate::branch::error::{BranchRuntimeError, BranchTimestampHistorySource};
use crate::branch::facts::{
    BranchLevel, BranchProtectionReason, BranchReachabilityAggregate, BranchReachabilityFacts,
    BranchReachabilitySnapshot, BranchReleasePlan, BranchRuntimeStats, BranchStateFacts,
    BranchTableDescriptor, BranchTableRef, BranchTableReferenceKind, InheritedLayerDescriptor,
    InheritedLayerStatus, SharedTableRegistry,
};
use crate::branch::identity::{
    require_row_branch, rewrite_physical_key_branch, rewrite_row_branch, row_matches_branch,
};
use crate::branch::read::{
    BranchEffectiveReadBound, BranchHistoryOptions, BranchHistoryRow, BranchInheritedLayer,
    BranchOwnedTable, BranchReadBound, BranchReadView, BranchRowCandidateFacts, BranchRowSource,
    BranchScanBounds, BranchTimestampCoverage, BranchUserKeyBound, BranchVisibleRow,
};
use crate::branch::state::{
    install_snapshot_rows_into_branches, BranchCompactionKind, BranchCompactionNoopReason,
    BranchCompactionRecovery, BranchCompactionRequest, BranchCompactionRetentionPolicy,
    BranchForkOutcome, BranchImmutableInstallOutcome, BranchLocalState,
    BranchMaterializationOutcome, BranchMaterializationRecovery, BranchMaterializationRequest,
    BranchRotationOutcome, BranchRotationSkipReason, BranchSnapshotInstallGroup,
    BranchSnapshotInstallRecovery, BranchSnapshotInstallRequest, BranchSnapshotMissingBranchPolicy,
    BranchStateDescriptor, BranchViewDescriptor,
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
