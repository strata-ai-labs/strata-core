//! Generated branch-LSM scaffold contract helpers.

use super::TestkitError;
use crate::branch::config::BranchRuntimeConfig;
use crate::branch::error::{BranchRuntimeError, BranchTimestampHistorySource};
use crate::branch::facts::{
    BranchLevel, BranchProtectionReason, BranchReachabilityAggregate, BranchReachabilityFacts,
    BranchReachabilitySnapshot, BranchReleasePlan, BranchStateFacts, BranchTableDescriptor,
    BranchTableRef, BranchTableReferenceKind, InheritedLayerDescriptor, InheritedLayerStatus,
    SharedTableRegistry,
};
use crate::branch::identity::{
    require_row_branch, rewrite_physical_key_branch, rewrite_row_branch, row_matches_branch,
};
use crate::branch::read::{
    BranchEffectiveReadBound, BranchHistoryOptions, BranchHistoryRow, BranchInheritedLayer,
    BranchOwnedTable, BranchReadBound, BranchReadView, BranchRowSource, BranchScanBounds,
    BranchTimestampCoverage, BranchUserKeyBound, BranchVisibleRow,
};
use crate::branch::state::compaction::{
    BranchCompactionKind, BranchCompactionNoopReason, BranchCompactionRequest,
    BranchCompactionRetentionPolicy,
};
use crate::branch::state::fork::BranchForkOutcome;
use crate::branch::state::materialization::{
    BranchMaterializationOutcome, BranchMaterializationRecovery, BranchMaterializationRequest,
};
use crate::branch::state::read_hooks::{BranchStateDescriptor, BranchViewDescriptor};
use crate::branch::state::rotation::BranchRotationSkipReason;
use crate::branch::state::snapshot::{
    install_snapshot_rows_into_branches, BranchSnapshotInstallGroup, BranchSnapshotInstallRequest,
    BranchSnapshotMissingBranchPolicy,
};
use crate::branch::state::{
    BranchImmutableInstallOutcome, BranchLocalState, BranchRotationOutcome,
};
use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};
use crate::table::{
    sort_table_rows_by_key, ImmutableTableBuilder, ImmutableTableReader, TableBuilderConfig,
    TableCommitRange, TableCompactionConfig, TableIdentity, TableInternalKeyBytes, TableKeyRange,
    TablePhysicalKeyBytes, TableReaderConfig, TableRow, TableRuntimeFacts,
};
use std::error::Error;
use std::fmt;
use strata_core::{BranchId, CommitVersion, Timestamp};

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
