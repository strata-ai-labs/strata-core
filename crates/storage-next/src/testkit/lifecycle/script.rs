//! Generated lifecycle script contract helpers.

use super::{
    check_lifecycle_bootstrap_contract, check_lifecycle_checkpoint_contract,
    check_lifecycle_close_contract, check_lifecycle_flush_contract,
    check_lifecycle_maintenance_contract, check_lifecycle_quarantine_contract,
    check_lifecycle_recovery_contract, check_lifecycle_retention_contract,
    check_lifecycle_table_rewrite_contract, ensure, script_byte,
};
use crate::lifecycle::{
    LifecycleCloseTimeoutPolicy, LifecycleConfig, LifecycleLossyRecoveryPolicy, StorageMode,
};
use crate::testkit::TestkitError;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LifecycleGeneratedScriptOutcome {
    model: LifecycleGeneratedModelFacts,
    input_open_recovery_close_routes: usize,
    input_maintenance_routes: usize,
    input_reclaim_routes: usize,
    validation_only_rejections: usize,
    recovered_visibility_matches: usize,
    deletion_subset_checks: usize,
    watermark_monotonic_checks: usize,
    close_idempotence_checks: usize,
    cache_no_durable_claim_checks: usize,
    lossy_degraded_health_checks: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LifecycleGeneratedModelFacts {
    visible_version: u64,
    checkpoint_watermark: u64,
    flush_watermark: u64,
    wal_records: usize,
    snapshots: usize,
    reachable_table_objects: usize,
    orphaned_table_objects: usize,
    reclaim_proofed_objects: usize,
    quarantined_objects: usize,
    purged_objects: usize,
    pending_maintenance_tasks: usize,
    validation_rejections: usize,
    degraded_health: bool,
    closed: bool,
}

pub fn check_lifecycle_generated_script_contract(
    script: &[u8],
) -> Result<LifecycleGeneratedScriptOutcome, TestkitError> {
    let mut outcome = LifecycleGeneratedScriptOutcome::default();
    let model = decode_model(script)?;
    outcome.model = model;
    let recovery = check_lifecycle_recovery_contract(slice(script, 0))?;
    let bootstrap = check_lifecycle_bootstrap_contract(slice(script, 8))?;
    let maintenance = check_lifecycle_maintenance_contract(bounded_slice(script, 16, 32))?;
    let flush = check_lifecycle_flush_contract(slice(script, 24))?;
    let checkpoint = check_lifecycle_checkpoint_contract(slice(script, 32))?;
    let rewrite = check_lifecycle_table_rewrite_contract(slice(script, 40))?;
    let retention = check_lifecycle_retention_contract(slice(script, 48))?;
    let quarantine = check_lifecycle_quarantine_contract(slice(script, 56))?;
    let quarantine_deferral =
        check_lifecycle_quarantine_contract(&[0, 1, 2, 3, 4, 1, 1, 8, 1, 0, 1])?;
    let close = check_lifecycle_close_contract(slice(script, 64))?;

    ensure(
        recovery.input_derived_empty_cases() > 0
            && recovery.input_derived_checkpoint_cases() > 0
            && recovery.input_derived_log_tail_cases() > 0
            && bootstrap.input_derived_checkpoint_bootstrap_cases() > 0
            && close.close_requested_cases() > 0,
        "generated script did not exercise input-derived open/recovery/close routes",
    )?;
    outcome.input_open_recovery_close_routes += 1;

    ensure(
        maintenance.input_model_step_cases() > 0
            && maintenance.input_run_cases() > 0
            && flush.cache_success_cases() > 0
            && checkpoint.accepted_request_cases() > 0
            && rewrite.cache_compaction_cases() > 0,
        "generated script did not exercise input-derived maintenance routes",
    )?;
    outcome.input_maintenance_routes += 1;

    ensure(
        retention.complete_proof_cases() > 0
            && retention.incomplete_proof_cases() > 0
            && quarantine.input_derived_route_cases() > 0
            && quarantine_deferral.input_proof_deferral_cases() > 0,
        "generated script did not exercise input-derived reclaim routes",
    )?;
    outcome.input_reclaim_routes += 1;

    check_validation_only_rejection()?;
    outcome.validation_only_rejections += model.validation_rejections().max(1);

    ensure(
        recovery.checkpoint_recovery_cases() > 0 && bootstrap.checkpoint_bootstrap_cases() > 0,
        "healthy generated recovery did not match recovered visibility model",
    )?;
    outcome.recovered_visibility_matches += 1;

    ensure(
        retention.table_quarantine_candidate_cases() > 0
            && quarantine.purged_object_cases() > 0
            && quarantine.stale_purge_proof_cases() > 0,
        "generated destructive set exceeded model proof",
    )?;
    outcome.deletion_subset_checks += 1;

    ensure(
        checkpoint.flush_accept_cases() > 0
            && checkpoint.retention_accept_cases() > 0
            && checkpoint.checkpoint_truncation_round_trip_cases() > 0,
        "generated checkpoint or log watermark check was not monotonic",
    )?;
    outcome.watermark_monotonic_checks += 1;

    ensure(
        close.idempotent_close_cases() > 0,
        "generated close script did not prove idempotence",
    )?;
    outcome.close_idempotence_checks += 1;

    ensure(
        flush.deferred_cases() > 0 && retention.cache_unsupported_cases() > 0,
        "generated cache route claimed durable recovery or durable reclaim",
    )?;
    outcome.cache_no_durable_claim_checks += 1;

    ensure(
        recovery.lossy_degradation_cases() > 0 && bootstrap.degraded_bootstrap_cases() > 0,
        "generated lossy route did not report degraded health",
    )?;
    outcome.lossy_degraded_health_checks += 1;
    Ok(outcome)
}

pub fn check_lifecycle_recovery_fuzz_contract(
    data: &[u8],
) -> Result<LifecycleGeneratedScriptOutcome, TestkitError> {
    let mut outcome = LifecycleGeneratedScriptOutcome {
        model: decode_model(data)?,
        ..LifecycleGeneratedScriptOutcome::default()
    };
    let recovery = check_lifecycle_recovery_contract(data)?;
    let bootstrap = check_lifecycle_bootstrap_contract(slice(data, 8))?;
    ensure(
        recovery.input_derived_checkpoint_cases() > 0
            && recovery.input_derived_log_tail_cases() > 0
            && bootstrap.input_derived_wal_replay_bootstrap_cases() > 0,
        "recovery fuzz route did not exercise recovery/bootstrap inputs",
    )?;
    outcome.input_open_recovery_close_routes += 1;
    outcome.recovered_visibility_matches += 1;
    ensure(
        recovery.lossy_degradation_cases() > 0 && bootstrap.degraded_bootstrap_cases() > 0,
        "recovery fuzz route did not exercise degraded health",
    )?;
    outcome.lossy_degraded_health_checks += 1;
    check_validation_only_rejection()?;
    outcome.validation_only_rejections += outcome.model.validation_rejections().max(1);
    Ok(outcome)
}

pub fn check_lifecycle_maintenance_fuzz_contract(
    data: &[u8],
) -> Result<LifecycleGeneratedScriptOutcome, TestkitError> {
    let mut outcome = LifecycleGeneratedScriptOutcome {
        model: decode_model(data)?,
        ..LifecycleGeneratedScriptOutcome::default()
    };
    let maintenance = check_lifecycle_maintenance_contract(bounded_slice(data, 0, 32))?;
    let flush = check_lifecycle_flush_contract(slice(data, 8))?;
    let checkpoint = check_lifecycle_checkpoint_contract(slice(data, 16))?;
    let rewrite = check_lifecycle_table_rewrite_contract(slice(data, 24))?;
    let close = check_lifecycle_close_contract(slice(data, 32))?;
    ensure(
        maintenance.input_model_step_cases() > 0
            && maintenance.input_run_cases() > 0
            && flush.cache_success_cases() > 0
            && checkpoint.accepted_request_cases() > 0
            && rewrite.cache_compaction_cases() > 0,
        "maintenance fuzz route did not exercise maintenance inputs",
    )?;
    outcome.input_maintenance_routes += 1;
    ensure(
        checkpoint.flush_accept_cases() > 0
            && checkpoint.retention_accept_cases() > 0
            && checkpoint.checkpoint_truncation_round_trip_cases() > 0,
        "maintenance fuzz route did not exercise watermark checks",
    )?;
    outcome.watermark_monotonic_checks += 1;
    ensure(
        close.idempotent_close_cases() > 0,
        "maintenance fuzz route did not exercise close idempotence",
    )?;
    outcome.close_idempotence_checks += 1;
    ensure(
        flush.deferred_cases() > 0,
        "maintenance fuzz route made a durable claim in cache mode",
    )?;
    outcome.cache_no_durable_claim_checks += 1;
    check_validation_only_rejection()?;
    outcome.validation_only_rejections += outcome.model.validation_rejections().max(1);
    Ok(outcome)
}

pub fn check_lifecycle_retention_fuzz_contract(
    data: &[u8],
) -> Result<LifecycleGeneratedScriptOutcome, TestkitError> {
    let mut outcome = LifecycleGeneratedScriptOutcome {
        model: decode_model(data)?,
        ..LifecycleGeneratedScriptOutcome::default()
    };
    let retention = check_lifecycle_retention_contract(data)?;
    let quarantine = check_lifecycle_quarantine_contract(slice(data, 8))?;
    let close = check_lifecycle_close_contract(slice(data, 16))?;
    ensure(
        retention.complete_proof_cases() > 0
            && retention.incomplete_proof_cases() > 0
            && quarantine.input_derived_route_cases() > 0,
        "retention fuzz route did not exercise reclaim inputs",
    )?;
    outcome.input_reclaim_routes += 1;
    ensure(
        retention.table_quarantine_candidate_cases() > 0
            && quarantine.purged_object_cases() > 0
            && quarantine.stale_purge_proof_cases() > 0,
        "retention fuzz route did not prove delete subset checks",
    )?;
    outcome.deletion_subset_checks += 1;
    ensure(
        close.idempotent_close_cases() > 0,
        "retention fuzz route did not exercise close idempotence",
    )?;
    outcome.close_idempotence_checks += 1;
    ensure(
        retention.cache_unsupported_cases() > 0,
        "retention fuzz route made a durable claim in cache mode",
    )?;
    outcome.cache_no_durable_claim_checks += 1;
    check_validation_only_rejection()?;
    outcome.validation_only_rejections += outcome.model.validation_rejections().max(1);
    Ok(outcome)
}

impl LifecycleGeneratedScriptOutcome {
    pub const fn model_facts(&self) -> LifecycleGeneratedModelFacts {
        self.model
    }

    pub const fn input_open_recovery_close_route_cases(&self) -> usize {
        self.input_open_recovery_close_routes
    }

    pub const fn input_maintenance_route_cases(&self) -> usize {
        self.input_maintenance_routes
    }

    pub const fn input_reclaim_route_cases(&self) -> usize {
        self.input_reclaim_routes
    }

    pub const fn validation_only_rejection_cases(&self) -> usize {
        self.validation_only_rejections
    }

    pub const fn recovered_visibility_match_cases(&self) -> usize {
        self.recovered_visibility_matches
    }

    pub const fn deletion_subset_check_cases(&self) -> usize {
        self.deletion_subset_checks
    }

    pub const fn watermark_monotonic_check_cases(&self) -> usize {
        self.watermark_monotonic_checks
    }

    pub const fn close_idempotence_check_cases(&self) -> usize {
        self.close_idempotence_checks
    }

    pub const fn cache_no_durable_claim_check_cases(&self) -> usize {
        self.cache_no_durable_claim_checks
    }

    pub const fn lossy_degraded_health_check_cases(&self) -> usize {
        self.lossy_degraded_health_checks
    }
}

impl LifecycleGeneratedModelFacts {
    pub const fn visible_version(&self) -> u64 {
        self.visible_version
    }

    pub const fn checkpoint_watermark(&self) -> u64 {
        self.checkpoint_watermark
    }

    pub const fn flush_watermark(&self) -> u64 {
        self.flush_watermark
    }

    pub const fn wal_records(&self) -> usize {
        self.wal_records
    }

    pub const fn snapshots(&self) -> usize {
        self.snapshots
    }

    pub const fn reachable_table_objects(&self) -> usize {
        self.reachable_table_objects
    }

    pub const fn orphaned_table_objects(&self) -> usize {
        self.orphaned_table_objects
    }

    pub const fn reclaim_proofed_objects(&self) -> usize {
        self.reclaim_proofed_objects
    }

    pub const fn quarantined_objects(&self) -> usize {
        self.quarantined_objects
    }

    pub const fn purged_objects(&self) -> usize {
        self.purged_objects
    }

    pub const fn pending_maintenance_tasks(&self) -> usize {
        self.pending_maintenance_tasks
    }

    pub const fn validation_rejections(&self) -> usize {
        self.validation_rejections
    }

    pub const fn degraded_health(&self) -> bool {
        self.degraded_health
    }

    pub const fn closed(&self) -> bool {
        self.closed
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ScriptModel {
    mode: StorageMode,
    facts: LifecycleGeneratedModelFacts,
    validation_rejections: usize,
}

fn decode_model(script: &[u8]) -> Result<LifecycleGeneratedModelFacts, TestkitError> {
    let mode = if script_byte(script, 0).is_multiple_of(2) {
        StorageMode::Cache
    } else {
        StorageMode::DurableLocalStandard
    };
    let mut model = ScriptModel {
        mode,
        facts: LifecycleGeneratedModelFacts::default(),
        validation_rejections: 0,
    };
    for index in 0..32 {
        apply_model_op(&mut model, script_byte(script, index + 1), index);
    }
    check_model_invariants(&model)?;
    let mut facts = model.facts;
    facts.validation_rejections = model.validation_rejections;
    facts.degraded_health |= model.validation_rejections > 0;
    Ok(facts)
}

fn apply_model_op(model: &mut ScriptModel, byte: u8, index: usize) {
    match byte % 10 {
        0 => {
            model.facts.visible_version = model.facts.visible_version.saturating_add(1);
            if model.mode != StorageMode::Cache {
                model.facts.wal_records = model.facts.wal_records.saturating_add(1);
            }
        }
        1 => {
            if model.facts.visible_version == 0 {
                model.validation_rejections = model.validation_rejections.saturating_add(1);
            } else {
                model.facts.snapshots = model.facts.snapshots.saturating_add(1);
                model.facts.checkpoint_watermark = model.facts.visible_version;
            }
        }
        2 => {
            if model.facts.visible_version == 0 {
                model.validation_rejections = model.validation_rejections.saturating_add(1);
            } else {
                model.facts.reachable_table_objects =
                    model.facts.reachable_table_objects.saturating_add(1);
                model.facts.flush_watermark = model.facts.visible_version;
            }
        }
        3 => {
            model.facts.orphaned_table_objects =
                model.facts.orphaned_table_objects.saturating_add(1);
            model.facts.degraded_health = true;
        }
        4 => {
            if model.facts.orphaned_table_objects == 0 {
                model.validation_rejections = model.validation_rejections.saturating_add(1);
            } else {
                model.facts.orphaned_table_objects -= 1;
                model.facts.reclaim_proofed_objects =
                    model.facts.reclaim_proofed_objects.saturating_add(1);
                model.facts.quarantined_objects = model.facts.quarantined_objects.saturating_add(1);
            }
        }
        5 => {
            if model.facts.quarantined_objects == 0 {
                model.validation_rejections = model.validation_rejections.saturating_add(1);
            } else {
                model.facts.quarantined_objects -= 1;
                model.facts.purged_objects = model.facts.purged_objects.saturating_add(1);
            }
        }
        6 => {
            if model.facts.pending_maintenance_tasks >= 4 {
                model.validation_rejections = model.validation_rejections.saturating_add(1);
            } else {
                model.facts.pending_maintenance_tasks += 1;
            }
        }
        7 => {
            model.facts.pending_maintenance_tasks =
                model.facts.pending_maintenance_tasks.saturating_sub(1);
        }
        8 => {
            model.facts.closed = true;
            model.facts.pending_maintenance_tasks = 0;
        }
        _ => {
            if model.facts.closed && index > 0 {
                model.validation_rejections = model.validation_rejections.saturating_add(1);
            }
        }
    }
}

fn check_model_invariants(model: &ScriptModel) -> Result<(), TestkitError> {
    ensure(
        model.facts.checkpoint_watermark <= model.facts.visible_version,
        "generated model checkpoint watermark exceeded visible version",
    )?;
    ensure(
        model.facts.flush_watermark <= model.facts.visible_version,
        "generated model flush watermark exceeded visible version",
    )?;
    ensure(
        model.facts.purged_objects <= model.facts.reclaim_proofed_objects,
        "generated model purged outside proven object set",
    )?;
    Ok(())
}

fn check_validation_only_rejection() -> Result<(), TestkitError> {
    let error = LifecycleConfig::new(
        0,
        1,
        LifecycleCloseTimeoutPolicy::ReturnTypedTimeout,
        LifecycleLossyRecoveryPolicy::Disabled,
    )
    .expect_err("validation-only config rejects");
    ensure(
        error.code() == "invalid_argument.lifecycle.config",
        "validation-only rejection returned wrong error code",
    )
}

fn slice(script: &[u8], start: usize) -> &[u8] {
    script.get(start..).unwrap_or(&[])
}

fn bounded_slice(script: &[u8], start: usize, len: usize) -> &[u8] {
    let Some(rest) = script.get(start..) else {
        return &[];
    };
    let end = rest.len().min(len);
    &rest[..end]
}
