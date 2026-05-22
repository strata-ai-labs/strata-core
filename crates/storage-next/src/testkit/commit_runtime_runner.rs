//! Model-vs-runtime execution for generated L7M commit scripts.

use crate::branch::{
    BranchLocalState, BranchReadBound, BranchReadView, BranchRuntimeConfig, BranchScanBounds,
    BranchVisibleRow,
};
use crate::commit::{
    execute_read_only_diagnostic, CommitBatch, CommitBatchOptions, CommitBranchApplyTarget,
    CommitBranchGeneration, CommitBranchGenerationGuard, CommitBranchGuard, CommitBranchGuardSet,
    CommitBranchRegistry, CommitBranchState, CommitCacheRuntime, CommitConflictValidationMode,
    CommitDuplicateKeyPolicy, CommitDurabilityClass, CommitDurabilityMode, CommitDurableRuntime,
    CommitExpiry, CommitFactAllocator, CommitLowerLayer, CommitManualTimestampSource,
    CommitMutation, CommitObservedVersion, CommitOrigin, CommitOutcome, CommitQuiesceGuard,
    CommitReadFact, CommitReplayRequest, CommitReplayRuntime, CommitRetentionHint,
    CommitRuntimeConfig, CommitRuntimeError, CommitRuntimeResult, CommitTimelineView,
    CommitTimestampGuard, CommitTimestampPolicy, CommitUnresolvedDurableGate,
    CommitUnresolvedDurableKind, CommitValidationFacts, CommitVersionAllocator,
    CommitVisibilityFacts, CommitVisiblePublisher, CommitWalAppendError, CommitWalAppendFacts,
    CommitWalAppender, VisibleVersionPublish, VisibleVersionTracker, COMMIT_TIMELINE_SPACE,
};
use crate::config::mode::DurabilityPolicy;
use crate::format::WalRecord;
use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};
use strata_core_next::{BranchId, CommitVersion, Timestamp};

use super::commit_runtime_model::{
    CommitRuntimeModel, ModelBranchState, ModelStepCategory, ModelStepOutcome, ModelStepStatus,
    ModelUnresolvedDurableKind,
};
use super::commit_runtime_script::{
    CommitRuntimeScript, CommitScriptDurableFault, CommitScriptOperation,
};
use super::TestkitError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitRuntimeAssuranceOutcome {
    pub decoded_scripts: usize,
    pub model_parity_checks: usize,
    pub input_derived_model_parity_checks: usize,
    pub cache_successes: usize,
    pub durable_successes: usize,
    pub wal_failures: usize,
    pub post_wal_failures: usize,
    pub clean_wal_failures: usize,
    pub uncertain_wal_failures: usize,
    pub writer_halted_failures: usize,
    pub segment_id_overflow_failures: usize,
    pub apply_after_wal_failures: usize,
    pub visible_after_apply_failures: usize,
    pub conflict_rejections: usize,
    pub read_only_diagnostics: usize,
    pub guard_or_quiesce_rejections: usize,
    pub branch_lifecycle_transitions: usize,
    pub branch_lifecycle_rejections: usize,
    pub replay_successes: usize,
    pub timeline_checks: usize,
}

pub fn check_commit_runtime_script_contract(
    data: &[u8],
) -> Result<CommitRuntimeAssuranceOutcome, TestkitError> {
    let script = CommitRuntimeScript::decode(data);
    run_script_with_focus(&script, ContractFocus::General)
}

pub fn check_commit_runtime_generated_input_contract(
    data: &[u8],
) -> Result<CommitRuntimeAssuranceOutcome, TestkitError> {
    let script = CommitRuntimeScript::decode_generated_only(data);
    run_script_with_focus(&script, ContractFocus::General)
}

pub fn check_commit_runtime_batch_contract(
    data: &[u8],
) -> Result<CommitRuntimeAssuranceOutcome, TestkitError> {
    let script = CommitRuntimeScript::decode(data);
    run_script_with_focus(&script, ContractFocus::Batch)
}

pub fn check_commit_runtime_conflict_script_contract(
    data: &[u8],
) -> Result<CommitRuntimeAssuranceOutcome, TestkitError> {
    check_commit_runtime_conflict_contract(data)
}

pub fn check_commit_runtime_conflict_contract(
    data: &[u8],
) -> Result<CommitRuntimeAssuranceOutcome, TestkitError> {
    let script = CommitRuntimeScript::decode(data);
    run_script_with_focus(&script, ContractFocus::Conflict)
}

pub fn check_commit_runtime_durable_script_contract(
    data: &[u8],
) -> Result<CommitRuntimeAssuranceOutcome, TestkitError> {
    check_commit_runtime_durable_contract(data)
}

pub fn check_commit_runtime_durable_contract(
    data: &[u8],
) -> Result<CommitRuntimeAssuranceOutcome, TestkitError> {
    let script = CommitRuntimeScript::decode(data);
    run_script_with_focus(&script, ContractFocus::Durable)
}

pub fn check_commit_runtime_timeline_script_contract(
    data: &[u8],
) -> Result<CommitRuntimeAssuranceOutcome, TestkitError> {
    check_commit_runtime_timeline_contract(data)
}

pub fn check_commit_runtime_timeline_contract(
    data: &[u8],
) -> Result<CommitRuntimeAssuranceOutcome, TestkitError> {
    let script = CommitRuntimeScript::decode(data);
    run_script_with_focus(&script, ContractFocus::Timeline)
}

pub fn check_commit_runtime_fault_contract(
    data: &[u8],
) -> Result<CommitRuntimeAssuranceOutcome, TestkitError> {
    let script = CommitRuntimeScript::decode(data);
    run_script_with_focus(&script, ContractFocus::Fault)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ContractFocus {
    General,
    Batch,
    Conflict,
    Durable,
    Timeline,
    Fault,
}

fn run_script_with_focus(
    script: &CommitRuntimeScript,
    focus: ContractFocus,
) -> Result<CommitRuntimeAssuranceOutcome, TestkitError> {
    let outcome = run_script(script)?;
    validate_contract_focus(focus, &outcome)?;
    Ok(outcome)
}

fn run_script(script: &CommitRuntimeScript) -> Result<CommitRuntimeAssuranceOutcome, TestkitError> {
    let mut model = CommitRuntimeModel::new(script);
    let mut production = ProductionCommitRunner::new(script.branches())?;
    let mut outcome = CommitRuntimeAssuranceOutcome {
        decoded_scripts: 1,
        model_parity_checks: 0,
        input_derived_model_parity_checks: 0,
        cache_successes: 0,
        durable_successes: 0,
        wal_failures: 0,
        post_wal_failures: 0,
        clean_wal_failures: 0,
        uncertain_wal_failures: 0,
        writer_halted_failures: 0,
        segment_id_overflow_failures: 0,
        apply_after_wal_failures: 0,
        visible_after_apply_failures: 0,
        conflict_rejections: 0,
        read_only_diagnostics: 0,
        guard_or_quiesce_rejections: 0,
        branch_lifecycle_transitions: 0,
        branch_lifecycle_rejections: 0,
        replay_successes: 0,
        timeline_checks: 0,
    };

    for (index, operation) in script.operations().iter().copied().enumerate() {
        let timestamp = generated_timestamp(index);
        production.set_next_timestamp(timestamp);
        let expected = model.apply(operation, timestamp, script.branches());
        let actual = production.apply(operation)?;
        compare_step(index, operation, expected, actual)?;
        production.compare_to_model(&model)?;
        let input_derived = index >= script.canonical_operation_count();
        outcome.record(operation, expected, input_derived);
        outcome.model_parity_checks += 1;
        if input_derived {
            outcome.input_derived_model_parity_checks += 1;
        }
    }

    Ok(outcome)
}

fn validate_contract_focus(
    focus: ContractFocus,
    outcome: &CommitRuntimeAssuranceOutcome,
) -> Result<(), TestkitError> {
    let exercised = match focus {
        ContractFocus::General => outcome.decoded_scripts > 0 && outcome.model_parity_checks > 0,
        ContractFocus::Batch => {
            outcome.cache_successes > 0
                && outcome.read_only_diagnostics > 0
                && outcome.branch_lifecycle_transitions > 0
                && outcome.branch_lifecycle_rejections > 0
        }
        ContractFocus::Conflict => outcome.conflict_rejections > 0,
        ContractFocus::Durable => {
            outcome.durable_successes > 0
                && outcome.wal_failures > 0
                && outcome.post_wal_failures > 0
                && outcome.replay_successes > 0
        }
        ContractFocus::Timeline => outcome.timeline_checks > 0,
        ContractFocus::Fault => outcome.wal_failures > 0 && outcome.post_wal_failures > 0,
    };
    if exercised {
        Ok(())
    } else {
        Err(TestkitError::new(format!(
            "commit runtime generated contract did not exercise {focus:?}"
        )))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ProductionStepOutcome {
    status: ModelStepStatus,
    branch_id: Option<BranchId>,
    commit_version: Option<CommitVersion>,
    timestamp: Option<Timestamp>,
    category: ModelStepCategory,
}

struct ProductionCommitRunner {
    config: CommitRuntimeConfig,
    registry: CommitBranchRegistry,
    guard_set: CommitBranchGuardSet,
    allocator: CommitFactAllocator<CommitManualTimestampSource>,
    states: Vec<BranchStateSlot>,
    visible: VisibleVersionTracker,
    durable_gate: CommitUnresolvedDurableGate,
    held_guards: Vec<CommitBranchGuard>,
    quiesce_guard: Option<CommitQuiesceGuard>,
    replay_records: Vec<StoredWalRecord>,
    touched_keys: Vec<(BranchId, u8)>,
}

struct BranchStateSlot {
    branch_id: BranchId,
    state: BranchLocalState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct StoredWalRecord {
    branch_id: BranchId,
    version: CommitVersion,
    record: WalRecord,
    durability: CommitDurabilityClass,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FakeWalMode {
    Succeed,
    CleanFailure,
    UncertainFailure,
    WriterHalted,
    SegmentIdOverflow,
}

struct FakeWalAppender {
    policy: DurabilityPolicy,
    mode: FakeWalMode,
    records: Vec<WalRecord>,
}

struct FaultingBranchTarget<'a> {
    branch: &'a mut BranchLocalState,
    fail_apply: bool,
}

struct FaultingVisiblePublisher<'a> {
    visible: &'a mut VisibleVersionTracker,
    fail_publish: bool,
}

impl CommitRuntimeAssuranceOutcome {
    fn record(
        &mut self,
        operation: CommitScriptOperation,
        step: ModelStepOutcome,
        input_derived: bool,
    ) {
        match (step.category, step.status) {
            (ModelStepCategory::CacheCommit, ModelStepStatus::Succeeded) => {
                self.cache_successes += 1;
            }
            (ModelStepCategory::DurableCommit, ModelStepStatus::Succeeded) => {
                self.durable_successes += 1;
            }
            (ModelStepCategory::DurableWalFailure, ModelStepStatus::Rejected) => {
                self.wal_failures += 1;
            }
            (ModelStepCategory::DurablePostWalFailure, ModelStepStatus::Rejected) => {
                self.post_wal_failures += 1;
            }
            (ModelStepCategory::ConflictRejection, ModelStepStatus::Rejected) => {
                self.conflict_rejections += 1;
            }
            (ModelStepCategory::ReadOnly, ModelStepStatus::Succeeded) => {
                self.read_only_diagnostics += 1;
            }
            (
                ModelStepCategory::Guard
                | ModelStepCategory::Quiesce
                | ModelStepCategory::AdmissionRejection,
                ModelStepStatus::Rejected,
            ) => {
                self.guard_or_quiesce_rejections += 1;
            }
            (ModelStepCategory::BranchLifecycle, ModelStepStatus::Succeeded) => {
                self.branch_lifecycle_transitions += 1;
            }
            (ModelStepCategory::BranchLifecycle, ModelStepStatus::Rejected) => {
                self.branch_lifecycle_rejections += 1;
            }
            (ModelStepCategory::Replay, ModelStepStatus::Succeeded) => {
                self.replay_successes += 1;
            }
            (ModelStepCategory::Timeline, ModelStepStatus::Succeeded) => {
                self.timeline_checks += 1;
            }
            _ => {}
        }
        if input_derived && step.status == ModelStepStatus::Rejected {
            match operation {
                CommitScriptOperation::DurablePut {
                    fault: CommitScriptDurableFault::CleanWalFailure,
                    ..
                }
                | CommitScriptOperation::DurableDelete {
                    fault: CommitScriptDurableFault::CleanWalFailure,
                    ..
                } => self.clean_wal_failures += 1,
                CommitScriptOperation::DurablePut {
                    fault: CommitScriptDurableFault::UncertainWalFailure,
                    ..
                }
                | CommitScriptOperation::DurableDelete {
                    fault: CommitScriptDurableFault::UncertainWalFailure,
                    ..
                } => self.uncertain_wal_failures += 1,
                CommitScriptOperation::DurablePut {
                    fault: CommitScriptDurableFault::WriterHalted,
                    ..
                }
                | CommitScriptOperation::DurableDelete {
                    fault: CommitScriptDurableFault::WriterHalted,
                    ..
                } => self.writer_halted_failures += 1,
                CommitScriptOperation::DurablePut {
                    fault: CommitScriptDurableFault::SegmentIdOverflow,
                    ..
                }
                | CommitScriptOperation::DurableDelete {
                    fault: CommitScriptDurableFault::SegmentIdOverflow,
                    ..
                } => self.segment_id_overflow_failures += 1,
                CommitScriptOperation::DurablePut {
                    fault: CommitScriptDurableFault::ApplyFailureAfterWal,
                    ..
                }
                | CommitScriptOperation::DurableDelete {
                    fault: CommitScriptDurableFault::ApplyFailureAfterWal,
                    ..
                } => self.apply_after_wal_failures += 1,
                CommitScriptOperation::DurablePut {
                    fault: CommitScriptDurableFault::VisibleFailureAfterApply,
                    ..
                }
                | CommitScriptOperation::DurableDelete {
                    fault: CommitScriptDurableFault::VisibleFailureAfterApply,
                    ..
                } => self.visible_after_apply_failures += 1,
                _ => {}
            }
        }
    }
}

impl ProductionCommitRunner {
    fn new(branches: &[BranchId]) -> Result<Self, TestkitError> {
        let mut registry = CommitBranchRegistry::new();
        let generation = CommitBranchGeneration::new(1).map_err(testkit_error)?;
        let mut states = Vec::with_capacity(branches.len());
        for branch_id in branches {
            registry
                .register_active(*branch_id, generation)
                .map_err(testkit_error)?;
            states.push(BranchStateSlot {
                branch_id: *branch_id,
                state: BranchLocalState::new(*branch_id, BranchRuntimeConfig::default())
                    .map_err(testkit_error)?,
            });
        }
        Ok(Self {
            config: CommitRuntimeConfig::default(),
            registry,
            guard_set: CommitBranchGuardSet::new(),
            allocator: CommitFactAllocator::new(
                CommitVersionAllocator::default(),
                CommitTimestampGuard::default(),
                CommitManualTimestampSource::new(Timestamp::from_micros(1)),
            ),
            states,
            visible: VisibleVersionTracker::default(),
            durable_gate: CommitUnresolvedDurableGate::new(),
            held_guards: Vec::new(),
            quiesce_guard: None,
            replay_records: Vec::new(),
            touched_keys: Vec::new(),
        })
    }

    fn set_next_timestamp(&mut self, timestamp: Timestamp) {
        self.allocator.source_mut().set_next_timestamp(timestamp);
    }

    fn apply(
        &mut self,
        operation: CommitScriptOperation,
    ) -> Result<ProductionStepOutcome, TestkitError> {
        match operation {
            CommitScriptOperation::CachePut { branch, key, value } => {
                let branch_id = self.branch_by_index(branch);
                self.note_key(branch_id, key);
                self.execute_cache(branch_id, mutation_put(branch_id, key, value), None)
            }
            CommitScriptOperation::CacheDelete { branch, key } => {
                let branch_id = self.branch_by_index(branch);
                self.note_key(branch_id, key);
                self.execute_cache(branch_id, mutation_delete(branch_id, key), None)
            }
            CommitScriptOperation::DurablePut {
                branch,
                key,
                value,
                fault,
            } => {
                let branch_id = self.branch_by_index(branch);
                self.note_key(branch_id, key);
                self.execute_durable(branch_id, mutation_put(branch_id, key, value), fault)
            }
            CommitScriptOperation::DurableDelete { branch, key, fault } => {
                let branch_id = self.branch_by_index(branch);
                self.note_key(branch_id, key);
                self.execute_durable(branch_id, mutation_delete(branch_id, key), fault)
            }
            CommitScriptOperation::ConflictPut { branch, key, value } => {
                let branch_id = self.branch_by_index(branch);
                self.note_key(branch_id, key);
                let validation = CommitValidationFacts::new(
                    vec![CommitReadFact::new(
                        physical_key(branch_id, key),
                        CommitObservedVersion::Missing,
                    )],
                    Vec::new(),
                );
                self.execute_cache(
                    branch_id,
                    mutation_put(branch_id, key, value),
                    Some(validation),
                )
            }
            CommitScriptOperation::ReadOnlyDiagnostic { branch } => {
                let branch_id = self.branch_by_index(branch);
                self.execute_read_only(branch_id)
            }
            CommitScriptOperation::BeginQuiesce => Ok(self.begin_quiesce()),
            CommitScriptOperation::ReleaseQuiesce => Ok(self.release_quiesce()),
            CommitScriptOperation::AcquireBranchGuard { branch } => {
                Ok(self.acquire_branch_guard(branch))
            }
            CommitScriptOperation::ReleaseBranchGuard { branch } => {
                Ok(self.release_branch_guard(branch))
            }
            CommitScriptOperation::ReplayUnresolved => self.execute_replay(),
            CommitScriptOperation::TimelineCheck { branch } => self.check_timeline(branch),
            CommitScriptOperation::MarkDeleting { branch } => Ok(self.mark_deleting(branch)),
            CommitScriptOperation::MarkDeleted { branch } => Ok(self.mark_deleted(branch)),
            CommitScriptOperation::RecreateBranch { branch } => self.recreate_branch(branch),
        }
    }

    fn begin_quiesce(&mut self) -> ProductionStepOutcome {
        match self.guard_set.try_begin_quiesce() {
            Ok(guard) => {
                self.quiesce_guard = Some(guard);
                ProductionStepOutcome::success_no_commit(ModelStepCategory::Quiesce)
            }
            Err(_) => ProductionStepOutcome::rejected(ModelStepCategory::Quiesce),
        }
    }

    fn release_quiesce(&mut self) -> ProductionStepOutcome {
        self.quiesce_guard = None;
        ProductionStepOutcome::noop(ModelStepCategory::Quiesce)
    }

    fn acquire_branch_guard(&mut self, branch: u8) -> ProductionStepOutcome {
        let branch_id = self.branch_by_index(branch);
        match self.guard_set.try_acquire_branch_guard(branch_id) {
            Ok(guard) => {
                self.held_guards.push(guard);
                ProductionStepOutcome::success_no_commit(ModelStepCategory::Guard)
            }
            Err(_) => {
                ProductionStepOutcome::rejected_for_branch(branch_id, ModelStepCategory::Guard)
            }
        }
    }

    fn release_branch_guard(&mut self, branch: u8) -> ProductionStepOutcome {
        let branch_id = self.branch_by_index(branch);
        if let Some(index) = self
            .held_guards
            .iter()
            .position(|guard| guard.branch_id() == branch_id)
        {
            self.held_guards.swap_remove(index);
        }
        ProductionStepOutcome::noop(ModelStepCategory::Guard)
    }

    fn check_timeline(&self, branch: u8) -> Result<ProductionStepOutcome, TestkitError> {
        let branch_id = self.branch_by_index(branch);
        self.timeline_view(branch_id)?;
        Ok(ProductionStepOutcome {
            status: ModelStepStatus::Succeeded,
            branch_id: Some(branch_id),
            commit_version: None,
            timestamp: None,
            category: ModelStepCategory::Timeline,
        })
    }

    fn mark_deleting(&mut self, branch: u8) -> ProductionStepOutcome {
        let branch_id = self.branch_by_index(branch);
        let result = self.registry.mark_deleting(branch_id);
        lifecycle_step(branch_id, result.is_ok())
    }

    fn mark_deleted(&mut self, branch: u8) -> ProductionStepOutcome {
        let branch_id = self.branch_by_index(branch);
        let result = self.registry.mark_deleted(branch_id);
        lifecycle_step(branch_id, result.is_ok())
    }

    fn recreate_branch(&mut self, branch: u8) -> Result<ProductionStepOutcome, TestkitError> {
        let branch_id = self.branch_by_index(branch);
        let current = self.registry.lookup(branch_id).map_err(testkit_error)?;
        let generation = current
            .generation()
            .get()
            .checked_add(1)
            .and_then(|value| CommitBranchGeneration::new(value).ok())
            .ok_or_else(|| TestkitError::new("generated branch generation overflow"))?;
        let result = self.registry.recreate_active(branch_id, generation);
        Ok(lifecycle_step(branch_id, result.is_ok()))
    }

    fn execute_cache(
        &mut self,
        branch_id: BranchId,
        mutation: CommitMutation,
        validation: Option<CommitValidationFacts>,
    ) -> Result<ProductionStepOutcome, TestkitError> {
        let batch = CommitBatch::mutating(
            branch_id,
            vec![mutation],
            validation.unwrap_or_else(CommitValidationFacts::empty),
            CommitBatchOptions::default(),
        );
        let state_index = self.state_index(branch_id)?;
        let generation_guard = self.generation_guard_for(branch_id)?;
        let result = {
            let state = &mut self.states[state_index].state;
            CommitCacheRuntime::new(
                &self.config,
                &self.registry,
                &self.guard_set,
                &mut self.allocator,
                state,
                &mut self.visible,
                &self.durable_gate,
            )
            .execute(batch, generation_guard)
        };
        Ok(step_from_commit_result(
            result,
            branch_id,
            ModelStepCategory::CacheCommit,
        ))
    }

    fn execute_durable(
        &mut self,
        branch_id: BranchId,
        mutation: CommitMutation,
        fault: CommitScriptDurableFault,
    ) -> Result<ProductionStepOutcome, TestkitError> {
        let batch = CommitBatch::mutating(
            branch_id,
            vec![mutation],
            CommitValidationFacts::empty(),
            durable_options(),
        );
        let state_index = self.state_index(branch_id)?;
        let generation_guard = self.generation_guard_for(branch_id)?;
        let mut wal = FakeWalAppender::new(fault);
        let result = {
            let state = &mut self.states[state_index].state;
            let mut branch = FaultingBranchTarget {
                branch: state,
                fail_apply: fault == CommitScriptDurableFault::ApplyFailureAfterWal,
            };
            let mut visible = FaultingVisiblePublisher {
                visible: &mut self.visible,
                fail_publish: fault == CommitScriptDurableFault::VisibleFailureAfterApply,
            };
            CommitDurableRuntime::new(
                &self.config,
                &self.registry,
                &self.guard_set,
                &mut self.allocator,
                &mut branch,
                &mut visible,
                &mut wal,
                &self.durable_gate,
            )
            .execute(batch, generation_guard)
        };
        for record in wal.records {
            self.replay_records.push(StoredWalRecord {
                branch_id,
                version: record.commit_version(),
                record,
                durability: CommitDurabilityClass::Standard,
            });
        }
        let category = match fault {
            CommitScriptDurableFault::None => ModelStepCategory::DurableCommit,
            CommitScriptDurableFault::CleanWalFailure
            | CommitScriptDurableFault::UncertainWalFailure
            | CommitScriptDurableFault::WriterHalted
            | CommitScriptDurableFault::SegmentIdOverflow => ModelStepCategory::DurableWalFailure,
            CommitScriptDurableFault::ApplyFailureAfterWal
            | CommitScriptDurableFault::VisibleFailureAfterApply => {
                ModelStepCategory::DurablePostWalFailure
            }
        };
        Ok(step_from_commit_result(result, branch_id, category))
    }

    fn execute_read_only(
        &self,
        branch_id: BranchId,
    ) -> Result<ProductionStepOutcome, TestkitError> {
        let batch = CommitBatch::read_only_diagnostic(
            branch_id,
            CommitValidationFacts::empty(),
            CommitBatchOptions::default(),
        )
        .validate(&self.config)
        .map_err(testkit_error)?;
        execute_read_only_diagnostic(&batch, &self.config, self.visible).map_err(testkit_error)?;
        Ok(ProductionStepOutcome {
            status: ModelStepStatus::Succeeded,
            branch_id: Some(branch_id),
            commit_version: None,
            timestamp: None,
            category: ModelStepCategory::ReadOnly,
        })
    }

    fn execute_replay(&mut self) -> Result<ProductionStepOutcome, TestkitError> {
        let Some(unresolved) = self.durable_gate.unresolved().map_err(testkit_error)? else {
            return Ok(ProductionStepOutcome::noop(ModelStepCategory::Replay));
        };
        let Some(record_index) = self.replay_records.iter().position(|record| {
            record.branch_id == unresolved.branch_id()
                && record.version == unresolved.commit_version()
        }) else {
            return Err(TestkitError::new(
                "unresolved durable fact missed WAL record",
            ));
        };
        let record = self.replay_records[record_index].clone();
        let state_index = self.state_index(record.branch_id)?;
        let request = CommitReplayRequest::new(record.record, record.durability);
        let report = {
            let state = &mut self.states[state_index].state;
            CommitReplayRuntime::new(
                &self.config,
                &mut self.allocator,
                state,
                &mut self.visible,
                &self.durable_gate,
            )
            .replay(&request)
        }
        .map_err(testkit_error)?;
        let version = report
            .outcome()
            .commit_version()
            .ok_or_else(|| TestkitError::new("replay outcome missed commit version"))?;
        let timestamp = report
            .outcome()
            .commit_timestamp()
            .ok_or_else(|| TestkitError::new("replay outcome missed commit timestamp"))?;
        Ok(ProductionStepOutcome {
            status: ModelStepStatus::Succeeded,
            branch_id: Some(record.branch_id),
            commit_version: Some(version),
            timestamp: Some(timestamp),
            category: ModelStepCategory::Replay,
        })
    }

    fn compare_to_model(&self, model: &CommitRuntimeModel) -> Result<(), TestkitError> {
        if self.visible.visible_version() != model.visible_version() {
            return Err(TestkitError::new("visible version diverged from model"));
        }
        if self.allocator.version_allocator().last_allocated() != model.last_allocated() {
            return Err(TestkitError::new(
                "commit version allocator diverged from model",
            ));
        }
        if self.allocator.timestamp_guard().last_allocated() != model.last_timestamp() {
            return Err(TestkitError::new(
                "commit timestamp guard diverged from model",
            ));
        }
        if self
            .durable_gate
            .unresolved()
            .map_err(testkit_error)?
            .map(|unresolved| {
                (
                    unresolved.branch_id(),
                    unresolved.commit_version(),
                    unresolved.commit_timestamp(),
                    unresolved.durability(),
                    unresolved_kind_tag(unresolved.kind()),
                )
            })
            != model.unresolved().map(|unresolved| {
                (
                    unresolved.branch_id(),
                    unresolved.version(),
                    unresolved.timestamp(),
                    CommitDurabilityClass::Standard,
                    model_unresolved_kind_tag(unresolved.kind()),
                )
            })
        {
            return Err(TestkitError::new(
                "unresolved durable gate diverged from model",
            ));
        }
        if self.guard_set.active_guard_count().map_err(testkit_error)? != model.active_guard_count()
            || self.guard_set.is_quiescing().map_err(testkit_error)? != model.is_quiescing()
        {
            return Err(TestkitError::new("commit guard state diverged from model"));
        }
        for (branch_id, expected_generation, expected_state) in model.branch_facts() {
            let actual = self.registry.lookup(branch_id).map_err(testkit_error)?;
            if actual.generation().get() != expected_generation
                || branch_state_tag(actual.state()) != model_branch_state_tag(expected_state)
            {
                return Err(TestkitError::new(format!(
                    "branch lifecycle facts diverged for branch {branch_id}"
                )));
            }
        }
        for (branch_id, key) in &self.touched_keys {
            let expected = model.visible_value(*branch_id, *key);
            let actual = self.visible_value(*branch_id, *key)?;
            if actual != expected {
                return Err(TestkitError::new(format!(
                    "visible value diverged for branch {branch_id} key {key}"
                )));
            }
        }
        for branch_id in model.branches() {
            self.compare_timeline(branch_id, model)?;
        }
        Ok(())
    }

    fn visible_value(&self, branch_id: BranchId, key: u8) -> Result<Option<u8>, TestkitError> {
        let state = &self.states[self.state_index(branch_id)?].state;
        let view = state.capture_read_view().map_err(testkit_error)?;
        let key = physical_key(branch_id, key);
        let row = view
            .at_version(&key, self.visible.visible_version())
            .map_err(testkit_error)?;
        Ok(row.and_then(|row| {
            if row.row().is_tombstone() {
                None
            } else {
                row.row().value().first().copied()
            }
        }))
    }

    fn compare_timeline(
        &self,
        branch_id: BranchId,
        model: &CommitRuntimeModel,
    ) -> Result<(), TestkitError> {
        let timeline = self.timeline_view(branch_id)?;
        let expected_count = model.visible_timeline_entries(branch_id).count();
        if timeline.entries().len() != expected_count {
            return Err(TestkitError::new(format!(
                "timeline entry count diverged for branch {branch_id}"
            )));
        }
        for entry in model.visible_timeline_entries(branch_id) {
            let actual = timeline
                .version_at_or_before(entry.timestamp())
                .matched_version();
            let expected = model.timeline_version_at_or_before(branch_id, entry.timestamp());
            if actual != expected {
                return Err(TestkitError::new(format!(
                    "timeline lookup diverged for branch {branch_id}"
                )));
            }
        }
        Ok(())
    }

    fn timeline_view(&self, branch_id: BranchId) -> Result<CommitTimelineView, TestkitError> {
        let state = &self.states[self.state_index(branch_id)?].state;
        let view = state.capture_read_view().map_err(testkit_error)?;
        let bounds = BranchScanBounds::unbounded(
            branch_id,
            COMMIT_TIMELINE_SPACE,
            StorageSpaceId::COMMIT_TIMELINE,
        )
        .map_err(testkit_error)?;
        let rows = view
            .scan_range(
                &bounds,
                BranchReadBound::at_version(self.visible.visible_version()),
            )
            .map_err(testkit_error)?;
        CommitTimelineView::from_rows(branch_id, rows.iter().map(BranchVisibleRow::row))
            .map_err(testkit_error)
    }

    fn branch_by_index(&self, branch: u8) -> BranchId {
        self.states[usize::from(branch)].branch_id
    }

    fn state_index(&self, branch_id: BranchId) -> Result<usize, TestkitError> {
        self.states
            .iter()
            .position(|slot| slot.branch_id == branch_id)
            .ok_or_else(|| TestkitError::new("script referenced an unknown branch"))
    }

    fn note_key(&mut self, branch_id: BranchId, key: u8) {
        if !self.touched_keys.contains(&(branch_id, key)) {
            self.touched_keys.push((branch_id, key));
        }
    }

    fn generation_guard_for(
        &self,
        branch_id: BranchId,
    ) -> Result<CommitBranchGenerationGuard, TestkitError> {
        let descriptor = self.registry.lookup(branch_id).map_err(testkit_error)?;
        Ok(CommitBranchGenerationGuard::exact(descriptor.generation()))
    }
}

impl FakeWalAppender {
    fn new(fault: CommitScriptDurableFault) -> Self {
        let mode = match fault {
            CommitScriptDurableFault::None
            | CommitScriptDurableFault::ApplyFailureAfterWal
            | CommitScriptDurableFault::VisibleFailureAfterApply => FakeWalMode::Succeed,
            CommitScriptDurableFault::CleanWalFailure => FakeWalMode::CleanFailure,
            CommitScriptDurableFault::UncertainWalFailure => FakeWalMode::UncertainFailure,
            CommitScriptDurableFault::WriterHalted => FakeWalMode::WriterHalted,
            CommitScriptDurableFault::SegmentIdOverflow => FakeWalMode::SegmentIdOverflow,
        };
        Self {
            policy: DurabilityPolicy::Standard,
            mode,
            records: Vec::new(),
        }
    }
}

impl CommitWalAppender for FakeWalAppender {
    fn durability_policy(&self) -> DurabilityPolicy {
        self.policy
    }

    fn append_commit_record(
        &mut self,
        record: &WalRecord,
    ) -> Result<CommitWalAppendFacts, CommitWalAppendError> {
        match self.mode {
            FakeWalMode::Succeed => {
                self.records.push(record.clone());
                Ok(CommitWalAppendFacts::new(0, 0, 1, 0, false))
            }
            FakeWalMode::CleanFailure => Err(CommitWalAppendError::clean(
                CommitRuntimeError::lower_layer(CommitLowerLayer::WalService, "script WAL failure"),
            )),
            FakeWalMode::UncertainFailure => Err(CommitWalAppendError::uncertain(
                CommitRuntimeError::lower_layer(
                    CommitLowerLayer::WalService,
                    "script uncertain WAL failure",
                ),
            )),
            FakeWalMode::WriterHalted => Err(CommitWalAppendError::clean(
                CommitRuntimeError::DurabilityUnavailable {
                    reason: "script WAL writer halted",
                },
            )),
            FakeWalMode::SegmentIdOverflow => Err(CommitWalAppendError::clean(
                CommitRuntimeError::lower_layer(
                    CommitLowerLayer::WalService,
                    "script WAL segment id overflow",
                ),
            )),
        }
    }
}

impl CommitBranchApplyTarget for FaultingBranchTarget<'_> {
    fn branch_id(&self) -> BranchId {
        self.branch.branch_id()
    }

    fn max_commit_version(&self) -> Option<CommitVersion> {
        self.branch.max_commit_version()
    }

    fn capture_read_view(&self) -> CommitRuntimeResult<BranchReadView> {
        self.branch.capture_read_view().map_err(|source| {
            CommitRuntimeError::lower_layer_with(
                CommitLowerLayer::BranchRuntime,
                "script branch read view capture failed",
                source,
            )
        })
    }

    fn append_committed_rows_atomically(
        &mut self,
        rows: Vec<StorageRow>,
    ) -> CommitRuntimeResult<()> {
        if self.fail_apply {
            return Err(CommitRuntimeError::lower_layer(
                CommitLowerLayer::BranchRuntime,
                "script branch apply failure",
            ));
        }
        self.branch
            .append_committed_rows_atomically(rows)
            .map(|_| ())
            .map_err(|source| {
                CommitRuntimeError::lower_layer_with(
                    CommitLowerLayer::BranchRuntime,
                    "script branch apply failed",
                    source,
                )
            })
    }
}

impl CommitVisiblePublisher for FaultingVisiblePublisher<'_> {
    fn visible_version(&self) -> CommitVersion {
        self.visible.visible_version()
    }

    fn publish_from_facts(
        &mut self,
        facts: CommitVisibilityFacts,
    ) -> CommitRuntimeResult<VisibleVersionPublish> {
        if self.fail_publish {
            return Err(CommitRuntimeError::InvalidVisibilityFacts {
                reason: "script visible publish failure",
            });
        }
        self.visible.publish_from_facts(facts)
    }
}

impl ProductionStepOutcome {
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

    fn rejected(category: ModelStepCategory) -> Self {
        Self {
            status: ModelStepStatus::Rejected,
            branch_id: None,
            commit_version: None,
            timestamp: None,
            category,
        }
    }

    fn rejected_for_branch(branch_id: BranchId, category: ModelStepCategory) -> Self {
        Self {
            status: ModelStepStatus::Rejected,
            branch_id: Some(branch_id),
            commit_version: None,
            timestamp: None,
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

fn step_from_commit_result(
    result: CommitRuntimeResult<CommitOutcome>,
    branch_id: BranchId,
    category: ModelStepCategory,
) -> ProductionStepOutcome {
    match result {
        Ok(outcome) => {
            if let (Some(version), Some(timestamp)) =
                (outcome.commit_version(), outcome.commit_timestamp())
            {
                ProductionStepOutcome::success(branch_id, version, timestamp, category)
            } else {
                ProductionStepOutcome::success_no_commit(category)
            }
        }
        Err(error) => ProductionStepOutcome {
            status: ModelStepStatus::Rejected,
            branch_id: Some(branch_id),
            commit_version: None,
            timestamp: None,
            category: classify_commit_error(&error),
        },
    }
}

fn compare_step(
    index: usize,
    operation: CommitScriptOperation,
    expected: ModelStepOutcome,
    actual: ProductionStepOutcome,
) -> Result<(), TestkitError> {
    if expected.status != actual.status {
        return Err(TestkitError::new(format!(
            "commit script step {index} {operation:?} status diverged: model {:?}, runtime {:?}",
            expected.status, actual.status
        )));
    }
    if expected.category != actual.category {
        return Err(TestkitError::new(format!(
            "commit script step {index} {operation:?} category diverged: model {:?}, runtime {:?}",
            expected.category, actual.category
        )));
    }
    if expected.branch_id != actual.branch_id {
        return Err(TestkitError::new(format!(
            "commit script step {index} {operation:?} branch diverged"
        )));
    }
    if expected.status == ModelStepStatus::Succeeded
        && (expected.commit_version != actual.commit_version
            || expected.timestamp != actual.timestamp)
    {
        return Err(TestkitError::new(format!(
            "commit script step {index} {operation:?} commit facts diverged"
        )));
    }
    Ok(())
}

fn classify_commit_error(error: &CommitRuntimeError) -> ModelStepCategory {
    match error {
        CommitRuntimeError::CommitConflict { .. } => ModelStepCategory::ConflictRejection,
        CommitRuntimeError::BranchGuardUnavailable { .. }
        | CommitRuntimeError::CommitQuiesceUnavailable { .. }
        | CommitRuntimeError::UnresolvedDurableCommit { .. }
        | CommitRuntimeError::BranchNotWritable { .. }
        | CommitRuntimeError::BranchNotFound { .. }
        | CommitRuntimeError::BranchGenerationMismatch { .. } => {
            ModelStepCategory::AdmissionRejection
        }
        CommitRuntimeError::DurabilityUncertain { .. }
        | CommitRuntimeError::DurabilityUnavailable { .. }
        | CommitRuntimeError::LowerLayer {
            layer: CommitLowerLayer::WalService,
            ..
        } => ModelStepCategory::DurableWalFailure,
        CommitRuntimeError::DurableButNotVisible { .. }
        | CommitRuntimeError::AppliedButNotVisible { .. } => {
            ModelStepCategory::DurablePostWalFailure
        }
        _ => ModelStepCategory::AdmissionRejection,
    }
}

fn mutation_put(branch_id: BranchId, key: u8, value: u8) -> CommitMutation {
    CommitMutation::put(
        physical_key(branch_id, key),
        vec![value, 0xa5],
        CommitExpiry::None,
        CommitRetentionHint::Append,
    )
}

fn mutation_delete(branch_id: BranchId, key: u8) -> CommitMutation {
    CommitMutation::delete(physical_key(branch_id, key))
}

fn physical_key(branch_id: BranchId, key: u8) -> PhysicalKey {
    PhysicalKey::new(
        branch_id,
        "default",
        StorageSpaceId::engine(0x20).expect("test engine storage space is valid"),
        vec![b'k', key],
    )
    .expect("generated commit-runtime physical key is valid")
}

fn durable_options() -> CommitBatchOptions {
    CommitBatchOptions::new(
        CommitDurabilityMode::Standard,
        CommitConflictValidationMode::Validate,
        CommitDuplicateKeyPolicy::Reject,
        CommitTimestampPolicy::RuntimeGenerated,
        CommitOrigin::StorageRuntime,
    )
}

fn unresolved_kind_tag(kind: CommitUnresolvedDurableKind) -> &'static str {
    match kind {
        CommitUnresolvedDurableKind::DurableNotApplied => "durable-not-applied",
        CommitUnresolvedDurableKind::AppliedNotVisible => "applied-not-visible",
    }
}

fn model_unresolved_kind_tag(kind: ModelUnresolvedDurableKind) -> &'static str {
    match kind {
        ModelUnresolvedDurableKind::DurableNotApplied => "durable-not-applied",
        ModelUnresolvedDurableKind::AppliedNotVisible => "applied-not-visible",
    }
}

fn branch_state_tag(state: CommitBranchState) -> &'static str {
    match state {
        CommitBranchState::Active => "active",
        CommitBranchState::Deleting => "deleting",
        CommitBranchState::Deleted => "deleted",
    }
}

fn lifecycle_step(branch_id: BranchId, success: bool) -> ProductionStepOutcome {
    if success {
        ProductionStepOutcome::success_no_commit_for_branch(
            branch_id,
            ModelStepCategory::BranchLifecycle,
        )
    } else {
        ProductionStepOutcome::rejected_for_branch(branch_id, ModelStepCategory::BranchLifecycle)
    }
}

fn model_branch_state_tag(state: ModelBranchState) -> &'static str {
    match state {
        ModelBranchState::Active => "active",
        ModelBranchState::Deleting => "deleting",
        ModelBranchState::Deleted => "deleted",
    }
}

fn generated_timestamp(index: usize) -> Timestamp {
    let duplicate_timestamp_bucket = index / 2;
    Timestamp::from_micros(
        10_000u64
            .checked_add(
                u64::try_from(duplicate_timestamp_bucket).expect("script index fits in u64") * 10,
            )
            .expect("bounded script timestamp cannot overflow"),
    )
}

fn testkit_error(error: impl std::fmt::Debug) -> TestkitError {
    TestkitError::new(format!("{error:?}"))
}

#[cfg(test)]
mod tests {
    use super::generated_timestamp;

    #[test]
    fn generated_timestamps_include_equal_adjacent_values_for_timeline_tiebreaks() {
        assert_eq!(generated_timestamp(0), generated_timestamp(1));
        assert!(generated_timestamp(2) > generated_timestamp(1));
        assert_eq!(generated_timestamp(2), generated_timestamp(3));
    }
}
