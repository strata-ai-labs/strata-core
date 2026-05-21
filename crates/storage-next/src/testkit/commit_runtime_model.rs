//! Independent in-memory oracle for generated commit-runtime scripts.

use super::commit_runtime_script::{
    CommitRuntimeScript, CommitScriptDurableFault, CommitScriptOperation,
};
use strata_core_next::{BranchId, CommitVersion, Timestamp};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CommitRuntimeModel {
    branches: Vec<ModelBranch>,
    last_allocated: CommitVersion,
    last_timestamp: Option<Timestamp>,
    visible_version: CommitVersion,
    unresolved: Option<ModelUnresolvedDurable>,
    active_guards: Vec<BranchId>,
    quiescing: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ModelBranch {
    branch_id: BranchId,
    generation: u64,
    state: ModelBranchState,
    rows: Vec<ModelRow>,
    timeline: Vec<ModelTimelineEntry>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ModelBranchState {
    Active,
    Deleting,
    Deleted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ModelRow {
    key: u8,
    version: CommitVersion,
    timestamp: Timestamp,
    value: Option<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ModelTimelineEntry {
    branch_id: BranchId,
    version: CommitVersion,
    timestamp: Timestamp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ModelUnresolvedDurable {
    branch_id: BranchId,
    version: CommitVersion,
    timestamp: Timestamp,
    value: ModelMutationValue,
    kind: ModelUnresolvedDurableKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ModelUnresolvedDurableKind {
    DurableNotApplied,
    AppliedNotVisible,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ModelMutationValue {
    Put { key: u8, value: u8 },
    Delete { key: u8 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ModelStepOutcome {
    pub(crate) status: ModelStepStatus,
    pub(crate) branch_id: Option<BranchId>,
    pub(crate) commit_version: Option<CommitVersion>,
    pub(crate) timestamp: Option<Timestamp>,
    pub(crate) category: ModelStepCategory,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ModelStepStatus {
    Succeeded,
    Rejected,
    Noop,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ModelStepCategory {
    CacheCommit,
    DurableCommit,
    DurableWalFailure,
    DurablePostWalFailure,
    ConflictRejection,
    ReadOnly,
    Guard,
    Quiesce,
    BranchLifecycle,
    Replay,
    Timeline,
    AdmissionRejection,
}

impl CommitRuntimeModel {
    pub(crate) fn new(script: &CommitRuntimeScript) -> Self {
        Self {
            branches: script
                .branches()
                .iter()
                .copied()
                .map(|branch_id| ModelBranch {
                    branch_id,
                    generation: 1,
                    state: ModelBranchState::Active,
                    rows: Vec::new(),
                    timeline: Vec::new(),
                })
                .collect(),
            last_allocated: CommitVersion::ZERO,
            last_timestamp: None,
            visible_version: CommitVersion::ZERO,
            unresolved: None,
            active_guards: Vec::new(),
            quiescing: false,
        }
    }

    pub(crate) const fn visible_version(&self) -> CommitVersion {
        self.visible_version
    }

    pub(crate) const fn last_allocated(&self) -> CommitVersion {
        self.last_allocated
    }

    pub(crate) const fn last_timestamp(&self) -> Option<Timestamp> {
        self.last_timestamp
    }

    pub(crate) const fn unresolved(&self) -> Option<ModelUnresolvedDurable> {
        self.unresolved
    }

    pub(crate) fn active_guard_count(&self) -> usize {
        self.active_guards.len()
    }

    pub(crate) const fn is_quiescing(&self) -> bool {
        self.quiescing
    }

    pub(crate) fn branches(&self) -> impl Iterator<Item = BranchId> + '_ {
        self.branches.iter().map(|branch| branch.branch_id)
    }

    pub(crate) fn branch_facts(
        &self,
    ) -> impl Iterator<Item = (BranchId, u64, ModelBranchState)> + '_ {
        self.branches
            .iter()
            .map(|branch| (branch.branch_id, branch.generation, branch.state))
    }

    pub(crate) fn visible_value(&self, branch_id: BranchId, key: u8) -> Option<u8> {
        self.branch(branch_id)
            .and_then(|branch| branch.visible_value(key, self.visible_version))
    }

    pub(crate) fn timeline_version_at_or_before(
        &self,
        branch_id: BranchId,
        query: Timestamp,
    ) -> Option<CommitVersion> {
        self.branch(branch_id).and_then(|branch| {
            branch
                .timeline
                .iter()
                .filter(|entry| entry.timestamp <= query && entry.version <= self.visible_version)
                .max_by_key(|entry| (entry.timestamp, entry.version))
                .map(|entry| entry.version)
        })
    }

    pub(crate) fn visible_timeline_entries(
        &self,
        branch_id: BranchId,
    ) -> impl Iterator<Item = ModelTimelineEntry> + '_ {
        self.branch(branch_id)
            .into_iter()
            .flat_map(|branch| branch.timeline.iter())
            .copied()
            .filter(|entry| entry.version <= self.visible_version)
    }

    pub(crate) fn apply(
        &mut self,
        operation: CommitScriptOperation,
        timestamp: Timestamp,
        branches: &[BranchId],
    ) -> ModelStepOutcome {
        match operation {
            CommitScriptOperation::CachePut { branch, key, value } => self.apply_cache(
                branches[usize::from(branch)],
                ModelMutationValue::Put { key, value },
                timestamp,
            ),
            CommitScriptOperation::CacheDelete { branch, key } => self.apply_cache(
                branches[usize::from(branch)],
                ModelMutationValue::Delete { key },
                timestamp,
            ),
            CommitScriptOperation::DurablePut {
                branch,
                key,
                value,
                fault,
            } => self.apply_durable(
                branches[usize::from(branch)],
                ModelMutationValue::Put { key, value },
                fault,
                timestamp,
            ),
            CommitScriptOperation::DurableDelete { branch, key, fault } => self.apply_durable(
                branches[usize::from(branch)],
                ModelMutationValue::Delete { key },
                fault,
                timestamp,
            ),
            CommitScriptOperation::ConflictPut { branch, key, value } => {
                self.apply_conflict_put(branches[usize::from(branch)], key, value, timestamp)
            }
            CommitScriptOperation::ReadOnlyDiagnostic { branch } => ModelStepOutcome {
                status: ModelStepStatus::Succeeded,
                branch_id: Some(branches[usize::from(branch)]),
                commit_version: None,
                timestamp: None,
                category: ModelStepCategory::ReadOnly,
            },
            CommitScriptOperation::BeginQuiesce => self.begin_quiesce(),
            CommitScriptOperation::ReleaseQuiesce => {
                self.quiescing = false;
                ModelStepOutcome::noop(ModelStepCategory::Quiesce)
            }
            CommitScriptOperation::AcquireBranchGuard { branch } => {
                self.acquire_guard(branches[usize::from(branch)])
            }
            CommitScriptOperation::ReleaseBranchGuard { branch } => {
                self.release_guard(branches[usize::from(branch)]);
                ModelStepOutcome::noop(ModelStepCategory::Guard)
            }
            CommitScriptOperation::ReplayUnresolved => self.replay_unresolved(),
            CommitScriptOperation::TimelineCheck { branch } => ModelStepOutcome {
                status: ModelStepStatus::Succeeded,
                branch_id: Some(branches[usize::from(branch)]),
                commit_version: None,
                timestamp: None,
                category: ModelStepCategory::Timeline,
            },
            CommitScriptOperation::MarkDeleting { branch } => {
                self.mark_deleting(branches[usize::from(branch)])
            }
            CommitScriptOperation::MarkDeleted { branch } => {
                self.mark_deleted(branches[usize::from(branch)])
            }
            CommitScriptOperation::RecreateBranch { branch } => {
                self.recreate_branch(branches[usize::from(branch)])
            }
        }
    }

    fn apply_cache(
        &mut self,
        branch_id: BranchId,
        value: ModelMutationValue,
        timestamp: Timestamp,
    ) -> ModelStepOutcome {
        if !self.branch_is_writable(branch_id)
            || self.unresolved.is_some()
            || self.quiescing
            || self.active_guards.contains(&branch_id)
        {
            return ModelStepOutcome::rejected(branch_id, ModelStepCategory::AdmissionRejection);
        }
        let version = self.allocate(timestamp);
        self.install(branch_id, value, version, timestamp);
        self.visible_version = version;
        ModelStepOutcome::success(
            branch_id,
            version,
            timestamp,
            ModelStepCategory::CacheCommit,
        )
    }

    fn apply_durable(
        &mut self,
        branch_id: BranchId,
        value: ModelMutationValue,
        fault: CommitScriptDurableFault,
        timestamp: Timestamp,
    ) -> ModelStepOutcome {
        if !self.branch_is_writable(branch_id)
            || self.unresolved.is_some()
            || self.quiescing
            || self.active_guards.contains(&branch_id)
        {
            return ModelStepOutcome::rejected(branch_id, ModelStepCategory::AdmissionRejection);
        }
        let version = self.allocate(timestamp);
        match fault {
            CommitScriptDurableFault::None => {
                self.install(branch_id, value, version, timestamp);
                self.visible_version = version;
                ModelStepOutcome::success(
                    branch_id,
                    version,
                    timestamp,
                    ModelStepCategory::DurableCommit,
                )
            }
            CommitScriptDurableFault::CleanWalFailure
            | CommitScriptDurableFault::UncertainWalFailure
            | CommitScriptDurableFault::WriterHalted
            | CommitScriptDurableFault::SegmentIdOverflow => ModelStepOutcome::rejected_at(
                branch_id,
                version,
                timestamp,
                ModelStepCategory::DurableWalFailure,
            ),
            CommitScriptDurableFault::ApplyFailureAfterWal => {
                self.unresolved = Some(ModelUnresolvedDurable {
                    branch_id,
                    version,
                    timestamp,
                    value,
                    kind: ModelUnresolvedDurableKind::DurableNotApplied,
                });
                ModelStepOutcome::rejected_at(
                    branch_id,
                    version,
                    timestamp,
                    ModelStepCategory::DurablePostWalFailure,
                )
            }
            CommitScriptDurableFault::VisibleFailureAfterApply => {
                self.install(branch_id, value, version, timestamp);
                self.unresolved = Some(ModelUnresolvedDurable {
                    branch_id,
                    version,
                    timestamp,
                    value,
                    kind: ModelUnresolvedDurableKind::AppliedNotVisible,
                });
                ModelStepOutcome::rejected_at(
                    branch_id,
                    version,
                    timestamp,
                    ModelStepCategory::DurablePostWalFailure,
                )
            }
        }
    }

    fn apply_conflict_put(
        &mut self,
        branch_id: BranchId,
        key: u8,
        value: u8,
        timestamp: Timestamp,
    ) -> ModelStepOutcome {
        if !self.branch_is_writable(branch_id)
            || self.unresolved.is_some()
            || self.quiescing
            || self.active_guards.contains(&branch_id)
        {
            return ModelStepOutcome::rejected(branch_id, ModelStepCategory::AdmissionRejection);
        }
        if self.visible_value(branch_id, key).is_some() {
            return ModelStepOutcome::rejected(branch_id, ModelStepCategory::ConflictRejection);
        }
        let version = self.allocate(timestamp);
        self.install(
            branch_id,
            ModelMutationValue::Put { key, value },
            version,
            timestamp,
        );
        self.visible_version = version;
        ModelStepOutcome::success(
            branch_id,
            version,
            timestamp,
            ModelStepCategory::CacheCommit,
        )
    }

    fn begin_quiesce(&mut self) -> ModelStepOutcome {
        if self.quiescing || !self.active_guards.is_empty() {
            return ModelStepOutcome {
                status: ModelStepStatus::Rejected,
                branch_id: None,
                commit_version: None,
                timestamp: None,
                category: ModelStepCategory::Quiesce,
            };
        }
        self.quiescing = true;
        ModelStepOutcome::success_no_commit(ModelStepCategory::Quiesce)
    }

    fn acquire_guard(&mut self, branch_id: BranchId) -> ModelStepOutcome {
        if self.quiescing || self.active_guards.contains(&branch_id) {
            return ModelStepOutcome::rejected(branch_id, ModelStepCategory::Guard);
        }
        self.active_guards.push(branch_id);
        ModelStepOutcome::success_no_commit(ModelStepCategory::Guard)
    }

    fn release_guard(&mut self, branch_id: BranchId) {
        if let Some(index) = self
            .active_guards
            .iter()
            .position(|active| *active == branch_id)
        {
            self.active_guards.swap_remove(index);
        }
    }

    fn replay_unresolved(&mut self) -> ModelStepOutcome {
        let Some(unresolved) = self.unresolved.take() else {
            return ModelStepOutcome::noop(ModelStepCategory::Replay);
        };
        if unresolved.kind == ModelUnresolvedDurableKind::DurableNotApplied {
            self.install(
                unresolved.branch_id,
                unresolved.value,
                unresolved.version,
                unresolved.timestamp,
            );
        }
        self.visible_version = unresolved.version;
        ModelStepOutcome::success(
            unresolved.branch_id,
            unresolved.version,
            unresolved.timestamp,
            ModelStepCategory::Replay,
        )
    }

    fn mark_deleting(&mut self, branch_id: BranchId) -> ModelStepOutcome {
        let Some(branch) = self.branch_mut(branch_id) else {
            return ModelStepOutcome::rejected(branch_id, ModelStepCategory::BranchLifecycle);
        };
        branch.state = ModelBranchState::Deleting;
        ModelStepOutcome::success_no_commit_for_branch(
            branch_id,
            ModelStepCategory::BranchLifecycle,
        )
    }

    fn mark_deleted(&mut self, branch_id: BranchId) -> ModelStepOutcome {
        let Some(branch) = self.branch_mut(branch_id) else {
            return ModelStepOutcome::rejected(branch_id, ModelStepCategory::BranchLifecycle);
        };
        branch.state = ModelBranchState::Deleted;
        ModelStepOutcome::success_no_commit_for_branch(
            branch_id,
            ModelStepCategory::BranchLifecycle,
        )
    }

    fn recreate_branch(&mut self, branch_id: BranchId) -> ModelStepOutcome {
        let Some(branch) = self.branch_mut(branch_id) else {
            return ModelStepOutcome::rejected(branch_id, ModelStepCategory::BranchLifecycle);
        };
        if branch.state != ModelBranchState::Deleted {
            return ModelStepOutcome::rejected(branch_id, ModelStepCategory::BranchLifecycle);
        }
        branch.generation = branch
            .generation
            .checked_add(1)
            .expect("bounded generated scripts cannot exhaust branch generations");
        branch.state = ModelBranchState::Active;
        ModelStepOutcome::success_no_commit_for_branch(
            branch_id,
            ModelStepCategory::BranchLifecycle,
        )
    }

    fn allocate(&mut self, timestamp: Timestamp) -> CommitVersion {
        let version = self
            .last_allocated
            .checked_next()
            .expect("bounded generated scripts cannot overflow commit versions");
        self.last_allocated = version;
        self.last_timestamp = Some(timestamp);
        version
    }

    fn install(
        &mut self,
        branch_id: BranchId,
        value: ModelMutationValue,
        version: CommitVersion,
        timestamp: Timestamp,
    ) {
        let row = match value {
            ModelMutationValue::Put { key, value } => ModelRow {
                key,
                version,
                timestamp,
                value: Some(value),
            },
            ModelMutationValue::Delete { key } => ModelRow {
                key,
                version,
                timestamp,
                value: None,
            },
        };
        let branch = self
            .branch_mut(branch_id)
            .expect("script branch ids are registered in the model");
        branch.rows.push(row);
        branch.timeline.push(ModelTimelineEntry {
            branch_id,
            version,
            timestamp,
        });
    }

    fn branch(&self, branch_id: BranchId) -> Option<&ModelBranch> {
        self.branches
            .iter()
            .find(|branch| branch.branch_id == branch_id)
    }

    fn branch_is_writable(&self, branch_id: BranchId) -> bool {
        self.branch(branch_id)
            .is_some_and(|branch| branch.state == ModelBranchState::Active)
    }

    fn branch_mut(&mut self, branch_id: BranchId) -> Option<&mut ModelBranch> {
        self.branches
            .iter_mut()
            .find(|branch| branch.branch_id == branch_id)
    }
}

impl ModelBranch {
    fn visible_value(&self, key: u8, visible_version: CommitVersion) -> Option<u8> {
        self.rows
            .iter()
            .filter(|row| row.key == key && row.version <= visible_version)
            .max_by_key(|row| row.version)
            .and_then(|row| row.value)
    }
}

impl ModelTimelineEntry {
    pub(crate) const fn timestamp(self) -> Timestamp {
        self.timestamp
    }
}

impl ModelUnresolvedDurable {
    pub(crate) const fn branch_id(self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn version(self) -> CommitVersion {
        self.version
    }

    pub(crate) const fn timestamp(self) -> Timestamp {
        self.timestamp
    }

    pub(crate) const fn kind(self) -> ModelUnresolvedDurableKind {
        self.kind
    }
}

impl ModelStepOutcome {
    fn success(
        branch_id: BranchId,
        version: CommitVersion,
        timestamp: Timestamp,
        category: ModelStepCategory,
    ) -> Self {
        Self {
            status: ModelStepStatus::Succeeded,
            branch_id: Some(branch_id),
            commit_version: Some(version),
            timestamp: Some(timestamp),
            category,
        }
    }

    fn success_no_commit(category: ModelStepCategory) -> Self {
        Self {
            status: ModelStepStatus::Succeeded,
            branch_id: None,
            commit_version: None,
            timestamp: None,
            category,
        }
    }

    fn success_no_commit_for_branch(branch_id: BranchId, category: ModelStepCategory) -> Self {
        Self {
            status: ModelStepStatus::Succeeded,
            branch_id: Some(branch_id),
            commit_version: None,
            timestamp: None,
            category,
        }
    }

    fn rejected(branch_id: BranchId, category: ModelStepCategory) -> Self {
        Self {
            status: ModelStepStatus::Rejected,
            branch_id: Some(branch_id),
            commit_version: None,
            timestamp: None,
            category,
        }
    }

    fn rejected_at(
        branch_id: BranchId,
        version: CommitVersion,
        timestamp: Timestamp,
        category: ModelStepCategory,
    ) -> Self {
        Self {
            status: ModelStepStatus::Rejected,
            branch_id: Some(branch_id),
            commit_version: Some(version),
            timestamp: Some(timestamp),
            category,
        }
    }

    fn noop(category: ModelStepCategory) -> Self {
        Self {
            status: ModelStepStatus::Noop,
            branch_id: None,
            commit_version: None,
            timestamp: None,
            category,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CommitRuntimeModel, ModelBranchState, ModelStepCategory, ModelStepStatus,
        ModelUnresolvedDurableKind,
    };
    use crate::testkit::commit_runtime_script::{
        CommitRuntimeScript, CommitScriptDurableFault, CommitScriptOperation,
    };
    use strata_core_next::{CommitVersion, Timestamp};

    #[test]
    fn model_cache_put_and_delete_update_visible_value_and_version() {
        let script = CommitRuntimeScript::decode(&[]);
        let branches = script.branches();
        let branch = branches[0];
        let mut model = CommitRuntimeModel::new(&script);

        let put = model.apply(
            CommitScriptOperation::CachePut {
                branch: 0,
                key: 7,
                value: 0x51,
            },
            Timestamp::from_micros(10),
            branches,
        );
        assert_eq!(put.status, ModelStepStatus::Succeeded);
        assert_eq!(put.category, ModelStepCategory::CacheCommit);
        assert_eq!(model.visible_version(), CommitVersion::new(1));
        assert_eq!(model.visible_value(branch, 7), Some(0x51));

        let delete = model.apply(
            CommitScriptOperation::CacheDelete { branch: 0, key: 7 },
            Timestamp::from_micros(20),
            branches,
        );
        assert_eq!(delete.status, ModelStepStatus::Succeeded);
        assert_eq!(model.visible_version(), CommitVersion::new(2));
        assert_eq!(model.visible_value(branch, 7), None);
    }

    #[test]
    fn model_post_wal_apply_failure_records_unresolved_and_replay_repairs() {
        let script = CommitRuntimeScript::decode(&[]);
        let branches = script.branches();
        let branch = branches[1];
        let mut model = CommitRuntimeModel::new(&script);

        let failed = model.apply(
            CommitScriptOperation::DurablePut {
                branch: 1,
                key: 2,
                value: 0x61,
                fault: CommitScriptDurableFault::ApplyFailureAfterWal,
            },
            Timestamp::from_micros(30),
            branches,
        );
        assert_eq!(failed.status, ModelStepStatus::Rejected);
        assert_eq!(failed.category, ModelStepCategory::DurablePostWalFailure);
        assert_eq!(model.visible_value(branch, 2), None);
        let unresolved = model.unresolved().expect("unresolved durable fact");
        assert_eq!(
            unresolved.kind(),
            ModelUnresolvedDurableKind::DurableNotApplied
        );

        let replayed = model.apply(
            CommitScriptOperation::ReplayUnresolved,
            Timestamp::from_micros(40),
            branches,
        );
        assert_eq!(replayed.status, ModelStepStatus::Succeeded);
        assert_eq!(replayed.category, ModelStepCategory::Replay);
        assert_eq!(model.unresolved(), None);
        assert_eq!(model.visible_value(branch, 2), Some(0x61));
    }

    #[test]
    fn model_branch_lifecycle_rejects_before_allocation_and_recreate_restores_writes() {
        let script = CommitRuntimeScript::decode(&[]);
        let branches = script.branches();
        let branch = branches[4];
        let mut model = CommitRuntimeModel::new(&script);

        let deleting = model.apply(
            CommitScriptOperation::MarkDeleting { branch: 4 },
            Timestamp::from_micros(50),
            branches,
        );
        assert_eq!(deleting.status, ModelStepStatus::Succeeded);
        assert_eq!(deleting.category, ModelStepCategory::BranchLifecycle);

        let rejected = model.apply(
            CommitScriptOperation::CachePut {
                branch: 4,
                key: 3,
                value: 0x71,
            },
            Timestamp::from_micros(60),
            branches,
        );
        assert_eq!(rejected.status, ModelStepStatus::Rejected);
        assert_eq!(rejected.category, ModelStepCategory::AdmissionRejection);
        assert_eq!(model.last_allocated(), CommitVersion::ZERO);
        assert_eq!(model.visible_value(branch, 3), None);

        model.apply(
            CommitScriptOperation::MarkDeleted { branch: 4 },
            Timestamp::from_micros(70),
            branches,
        );
        let recreated = model.apply(
            CommitScriptOperation::RecreateBranch { branch: 4 },
            Timestamp::from_micros(80),
            branches,
        );
        assert_eq!(recreated.status, ModelStepStatus::Succeeded);
        assert_eq!(
            model.branch_facts().find(|facts| facts.0 == branch),
            Some((branch, 2, ModelBranchState::Active))
        );

        let committed = model.apply(
            CommitScriptOperation::CachePut {
                branch: 4,
                key: 3,
                value: 0x72,
            },
            Timestamp::from_micros(90),
            branches,
        );
        assert_eq!(committed.status, ModelStepStatus::Succeeded);
        assert_eq!(model.visible_value(branch, 3), Some(0x72));
    }

    #[test]
    fn model_timeline_lookup_uses_commit_version_tiebreak_for_equal_timestamps() {
        let script = CommitRuntimeScript::decode(&[]);
        let branches = script.branches();
        let branch = branches[0];
        let mut model = CommitRuntimeModel::new(&script);
        let timestamp = Timestamp::from_micros(100);

        model.apply(
            CommitScriptOperation::CachePut {
                branch: 0,
                key: 1,
                value: 0x81,
            },
            timestamp,
            branches,
        );
        model.apply(
            CommitScriptOperation::CachePut {
                branch: 0,
                key: 2,
                value: 0x82,
            },
            timestamp,
            branches,
        );

        assert_eq!(
            model.timeline_version_at_or_before(branch, timestamp),
            Some(CommitVersion::new(2))
        );
    }
}
