//! Model-backed API diagnostics contracts.

use strata_core_next::BranchId;

use crate::api::{
    CommitBatch, CommitMutation, CommitOptions, DiagnosticsFactState, DiagnosticsRequest,
    DiagnosticsScope, MaintenanceRequest, MaintenanceScope, MaintenanceTask, RecoveryHealthSummary,
    StorageKey, StorageOpenOptions, StorageRuntime, StorageSpaceId, StorageValue,
};
use crate::testkit::TestkitError;

const DEFAULT_BRANCH_ID: BranchId = BranchId::from_bytes([0x01; BranchId::BYTE_LEN]);
const ENGINE_STORAGE_SPACE: u8 = 0x20;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StorageApiDiagnosticsModelOutcome {
    scripts: usize,
    health_reports: usize,
    resource_reports: usize,
    object_reports: usize,
    branch_reports: usize,
    timeline_reports: usize,
    unsupported_durable_reports: usize,
    closed_reports: usize,
}

pub fn check_storage_api_diagnostics_model_contract(
    script: &[u8],
) -> Result<StorageApiDiagnosticsModelOutcome, TestkitError> {
    let script = if script.is_empty() {
        &[0_u8][..]
    } else {
        script
    };
    let options = if script[0] % 2 == 0 {
        StorageOpenOptions::default()
    } else {
        StorageOpenOptions::default().with_budget_policy(crate::api::StorageBudgetPolicy::LowMemory)
    };
    let mut runtime = StorageRuntime::open(options)
        .map_err(testkit_error)?
        .into_runtime();
    let mut outcome = StorageApiDiagnosticsModelOutcome {
        scripts: 1,
        ..StorageApiDiagnosticsModelOutcome::default()
    };

    runtime
        .commit(&put_batch(b"diag", &[script_byte(script, 1)])?)
        .map_err(testkit_error)?;
    runtime
        .enqueue_maintenance(&MaintenanceRequest::new(
            MaintenanceTask::Flush,
            MaintenanceScope::Branch(DEFAULT_BRANCH_ID),
        ))
        .map_err(testkit_error)?;

    let global = runtime
        .diagnostics(DiagnosticsRequest::new(DiagnosticsScope::Global))
        .map_err(testkit_error)?;
    if global.recovery().health() == Some(RecoveryHealthSummary::Healthy) {
        outcome.health_reports += 1;
    }
    if global.budget().state() == DiagnosticsFactState::Known
        && global.pressure().state() == DiagnosticsFactState::Known
        && global.maintenance().is_some()
    {
        outcome.resource_reports += 1;
    }
    if global.table_manifest().state() == DiagnosticsFactState::Unsupported
        && global.retention().state() == DiagnosticsFactState::Unsupported
        && global.checkpoint().state() == DiagnosticsFactState::Unsupported
    {
        outcome.unsupported_durable_reports += 1;
    }

    let branch = runtime
        .diagnostics(DiagnosticsRequest::new(DiagnosticsScope::Branch(
            DEFAULT_BRANCH_ID,
        )))
        .map_err(testkit_error)?;
    if branch.branch_catalog().active_branches() >= 1 {
        outcome.branch_reports += 1;
    }
    if branch.timeline().max_version().is_some() {
        outcome.timeline_reports += 1;
    }
    if branch.read_activity().state() == DiagnosticsFactState::Unknown {
        outcome.object_reports += 1;
    }

    runtime.close().map_err(testkit_error)?;
    let closed = runtime
        .diagnostics(DiagnosticsRequest::new(DiagnosticsScope::Global))
        .map_err(testkit_error)?;
    if closed.runtime_state() == crate::api::StorageRuntimeState::Closed
        && closed.maintenance_state() == DiagnosticsFactState::Unknown
    {
        outcome.closed_reports += 1;
    }

    Ok(outcome)
}

impl StorageApiDiagnosticsModelOutcome {
    pub const fn scripts(self) -> usize {
        self.scripts
    }

    pub const fn health_reports(self) -> usize {
        self.health_reports
    }

    pub const fn resource_reports(self) -> usize {
        self.resource_reports
    }

    pub const fn object_reports(self) -> usize {
        self.object_reports
    }

    pub const fn branch_reports(self) -> usize {
        self.branch_reports
    }

    pub const fn timeline_reports(self) -> usize {
        self.timeline_reports
    }

    pub const fn unsupported_durable_reports(self) -> usize {
        self.unsupported_durable_reports
    }

    pub const fn closed_reports(self) -> usize {
        self.closed_reports
    }
}

fn put_batch(key: &[u8], value: &[u8]) -> Result<CommitBatch, TestkitError> {
    CommitBatch::new(
        DEFAULT_BRANCH_ID,
        vec![CommitMutation::Put {
            storage_space: StorageSpaceId::new(vec![ENGINE_STORAGE_SPACE])
                .map_err(testkit_error)?,
            key: StorageKey::new(key.to_vec()).map_err(testkit_error)?,
            value: StorageValue::new(value.to_vec()),
            ttl: None,
        }],
        CommitOptions::default(),
    )
    .map_err(testkit_error)
}

fn script_byte(script: &[u8], index: usize) -> u8 {
    script[index % script.len()]
}

fn testkit_error(error: impl std::error::Error) -> TestkitError {
    TestkitError::new(error.to_string())
}
