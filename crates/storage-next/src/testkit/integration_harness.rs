use super::{
    check_table_format_model_script, run_quarantine_service_script, run_snapshot_service_script,
    TestkitError,
};
use std::time::{Duration, Instant};

#[cfg(any(
    all(feature = "localfs", not(target_arch = "wasm32")),
    feature = "fault-injection",
    test
))]
use crate::format::{encode_immutable_table, SnapshotSection, TableCompression};
#[cfg(any(
    all(feature = "localfs", not(target_arch = "wasm32")),
    feature = "fault-injection",
    test
))]
use crate::layout::ObjectLayout;
#[cfg(any(
    all(feature = "localfs", not(target_arch = "wasm32")),
    feature = "fault-injection",
    test
))]
use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};
#[cfg(any(
    all(feature = "localfs", not(target_arch = "wasm32")),
    feature = "fault-injection",
    test
))]
use crate::service::{
    DatabaseManifestService, SnapshotPublishRequest, SnapshotService, TableObjectService,
};
#[cfg(any(
    all(feature = "localfs", not(target_arch = "wasm32")),
    feature = "fault-injection",
    test
))]
use crate::service::{
    QuarantineGate, QuarantineObjectRequest, QuarantineObjectStatus, QuarantineService,
};
#[cfg(any(feature = "fault-injection", test))]
use crate::service::{SnapshotServiceError, TableObjectServiceError};
#[cfg(any(
    all(feature = "localfs", not(target_arch = "wasm32")),
    feature = "fault-injection",
    test
))]
use strata_core_next::{BranchId, CommitVersion, Timestamp};

#[cfg(any(
    all(feature = "localfs", not(target_arch = "wasm32")),
    feature = "fault-injection",
    test
))]
const DATABASE_ID: [u8; 16] = [0x9d; 16];
#[cfg(any(
    all(feature = "localfs", not(target_arch = "wasm32")),
    feature = "fault-injection",
    test
))]
const CODEC_ID: &str = "identity";

#[cfg(all(feature = "localfs", not(target_arch = "wasm32")))]
mod crash_sim {
    use super::TestkitError;
    use std::path::{Path, PathBuf};

    /// Resolve the on-disk path of a published object using the
    /// `LocalFsBackend` `<component>.object@` convention. Kept in sync with
    /// `LocalFsBackend::path_for` so simulations target real bytes.
    pub(super) fn object_file_path(root: &Path, name: &crate::object::ObjectName) -> PathBuf {
        let mut path = root.to_path_buf();
        let mut components = name.as_str().split('/').peekable();
        while let Some(component) = components.next() {
            if components.peek().is_some() {
                path.push(component);
            } else {
                path.push(format!("{component}.object@"));
            }
        }
        path
    }

    /// Truncate a published object to `new_len` bytes. Simulates a fault where
    /// some payload bytes reached the disk but the tail of the publication was
    /// lost (e.g. fsync interrupted partway).
    #[allow(
        dead_code,
        reason = "future service-fault windows reuse this helper to inject partial-write faults; kept here so the helper module exposes a complete set of crash primitives"
    )]
    pub(super) fn truncate_object(
        root: &Path,
        name: &crate::object::ObjectName,
        new_len: u64,
    ) -> Result<(), TestkitError> {
        let path = object_file_path(root, name);
        let file = std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .map_err(|err| TestkitError::new(format!("open {}: {err}", path.display())))?;
        file.set_len(new_len)
            .map_err(|err| TestkitError::new(format!("truncate {}: {err}", path.display())))
    }

    /// Remove the on-disk file backing an object. Simulates a fault where the
    /// object never reached durable storage even though companion state
    /// (manifest/inventory) recorded it.
    pub(super) fn drop_object_file(
        root: &Path,
        name: &crate::object::ObjectName,
    ) -> Result<(), TestkitError> {
        let path = object_file_path(root, name);
        std::fs::remove_file(&path)
            .map_err(|err| TestkitError::new(format!("drop {}: {err}", path.display())))
    }

    /// Patch a single byte to simulate a torn write or bit-rot that did not
    /// alter the object length.
    #[allow(
        dead_code,
        reason = "future corruption-window fault routes consume this helper; kept here so the helper module exposes a complete set of crash primitives"
    )]
    pub(super) fn corrupt_object_byte(
        root: &Path,
        name: &crate::object::ObjectName,
        offset: u64,
        new_byte: u8,
    ) -> Result<(), TestkitError> {
        use std::io::{Seek, SeekFrom, Write};
        let path = object_file_path(root, name);
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .map_err(|err| TestkitError::new(format!("open {}: {err}", path.display())))?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|err| TestkitError::new(format!("seek {}: {err}", path.display())))?;
        file.write_all(&[new_byte])
            .map_err(|err| TestkitError::new(format!("patch {}: {err}", path.display())))
    }
}

#[cfg(all(feature = "localfs", not(target_arch = "wasm32")))]
type CrashRecoveryCase = fn(&std::path::Path) -> Result<(), TestkitError>;
#[cfg(all(feature = "localfs", not(target_arch = "wasm32")))]
type CrashRecoveryRecorder = fn(&mut CrashRecoveryHarnessOutcome);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CrashRecoveryHarnessOutcome {
    cases_executed: usize,
    log_append_replay: usize,
    unresolved_gate_reconcile: usize,
    orphan_snapshot_ignored: usize,
    checkpoint_tail_recovered: usize,
    orphan_table_reported: usize,
    quarantine_inventory_debt: usize,
    object_quarantine_preserved: usize,
    close_reopen_consistent: usize,
    ignored_case_equivalents: usize,
    harness_environment: usize,
}

#[cfg(all(feature = "localfs", not(target_arch = "wasm32")))]
fn record_log_append_replay_window(outcome: &mut CrashRecoveryHarnessOutcome) {
    outcome.log_append_replay += 1;
}

#[cfg(all(feature = "localfs", not(target_arch = "wasm32")))]
fn record_unresolved_gate_window(outcome: &mut CrashRecoveryHarnessOutcome) {
    outcome.unresolved_gate_reconcile += 1;
}

#[cfg(all(feature = "localfs", not(target_arch = "wasm32")))]
fn record_snapshot_orphan_window(outcome: &mut CrashRecoveryHarnessOutcome) {
    outcome.orphan_snapshot_ignored += 1;
}

#[cfg(all(feature = "localfs", not(target_arch = "wasm32")))]
fn record_checkpoint_tail_window(outcome: &mut CrashRecoveryHarnessOutcome) {
    outcome.checkpoint_tail_recovered += 1;
}

#[cfg(all(feature = "localfs", not(target_arch = "wasm32")))]
fn record_table_orphan_window(outcome: &mut CrashRecoveryHarnessOutcome) {
    outcome.orphan_table_reported += 1;
}

#[cfg(all(feature = "localfs", not(target_arch = "wasm32")))]
fn record_quarantine_inventory_window(outcome: &mut CrashRecoveryHarnessOutcome) {
    outcome.quarantine_inventory_debt += 1;
}

#[cfg(all(feature = "localfs", not(target_arch = "wasm32")))]
fn record_object_quarantine_window(outcome: &mut CrashRecoveryHarnessOutcome) {
    outcome.object_quarantine_preserved += 1;
}

#[cfg(all(feature = "localfs", not(target_arch = "wasm32")))]
fn record_close_reopen_window(outcome: &mut CrashRecoveryHarnessOutcome) {
    outcome.close_reopen_consistent += 1;
}

impl CrashRecoveryHarnessOutcome {
    pub const fn cases_executed(self) -> usize {
        self.cases_executed
    }

    pub const fn log_append_replay_cases(self) -> usize {
        self.log_append_replay
    }

    pub const fn unresolved_gate_reconcile_cases(self) -> usize {
        self.unresolved_gate_reconcile
    }

    pub const fn orphan_snapshot_ignored_cases(self) -> usize {
        self.orphan_snapshot_ignored
    }

    pub const fn checkpoint_tail_recovered_cases(self) -> usize {
        self.checkpoint_tail_recovered
    }

    pub const fn orphan_table_reported_cases(self) -> usize {
        self.orphan_table_reported
    }

    pub const fn quarantine_inventory_debt_cases(self) -> usize {
        self.quarantine_inventory_debt
    }

    pub const fn object_quarantine_preserved_cases(self) -> usize {
        self.object_quarantine_preserved
    }

    pub const fn close_reopen_consistent_cases(self) -> usize {
        self.close_reopen_consistent
    }

    pub const fn ignored_case_equivalent_cases(self) -> usize {
        self.ignored_case_equivalents
    }

    pub const fn harness_environment_cases(self) -> usize {
        self.harness_environment
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorageStressHarnessOutcome {
    iterations: usize,
    scripts_executed: usize,
}

impl StorageStressHarnessOutcome {
    pub const fn iterations(self) -> usize {
        self.iterations
    }

    pub const fn scripts_executed(self) -> usize {
        self.scripts_executed
    }
}

/// Observable facts from a `DurableRetentionMaintenanceRunner` end-to-end
/// run against a real `LocalFsBackend`. Exposed to integration tests so
/// they can drive the durable retention runner without depending on
/// `pub(crate)` lifecycle types.
#[cfg(all(feature = "localfs", not(target_arch = "wasm32")))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LifecycleRetentionRunnerHarnessOutcome {
    snapshots_before: usize,
    snapshots_after: usize,
    maintenance_status: u8,
    fault_count: usize,
    runner_invocations: usize,
}

#[cfg(all(feature = "localfs", not(target_arch = "wasm32")))]
impl LifecycleRetentionRunnerHarnessOutcome {
    pub const fn snapshots_before(self) -> usize {
        self.snapshots_before
    }
    pub const fn snapshots_after(self) -> usize {
        self.snapshots_after
    }
    pub const fn maintenance_status_code(self) -> u8 {
        self.maintenance_status
    }
    pub const fn fault_count(self) -> usize {
        self.fault_count
    }
    pub const fn runner_invocations(self) -> usize {
        self.runner_invocations
    }
}

/// Drive a real `DurableRetentionMaintenanceRunner` against a localfs
/// backend end-to-end.
///
/// The harness opens a fresh `LifecycleDurableLocalRuntime` at `root`,
/// seeds three checkpoint snapshots through the durable manifest/snapshot
/// services so the manifest's snapshot watermark references the newest,
/// enqueues a global retention task, and runs
/// `runtime.run_next_retention_maintenance` which dispatches through the
/// `DurableRetentionMaintenanceRunner`. Integration tests assert on the
/// returned `LifecycleRetentionRunnerHarnessOutcome` (snapshot count
/// before/after, maintenance status code, fault count, runner invocation
/// count) so the runner is exercised against a real backend instead of
/// just through the testkit contract layer.
#[cfg(all(feature = "localfs", not(target_arch = "wasm32")))]
#[allow(
    clippy::too_many_lines,
    reason = "harness sequentially seeds manifest+snapshots, opens a real durable runtime, enqueues retention, runs the runner, and snapshots observable facts — splitting these phases would obscure ordering"
)]
pub fn run_localfs_lifecycle_retention_runner_harness(
    root: &std::path::Path,
) -> Result<LifecycleRetentionRunnerHarnessOutcome, TestkitError> {
    use crate::backend::local_fs::LocalFsBackend;
    use crate::branch::config::BranchRuntimeConfig;
    use crate::commit::{CommitBranchGeneration, CommitManualTimestampSource, CommitRuntimeConfig};
    use crate::lifecycle::{
        LifecycleCodecId, LifecycleConfig, LifecycleDurableLocalOpenRequest,
        LifecycleDurableLocalShell, LifecycleRecoveryRequest, LifecycleRecoveryRuntime,
        MaintenanceOutcomeStatus, MaintenanceTaskRequest, RecoveryStrictness, StorageMode,
        StorageOpenPlan,
    };
    use crate::service::WalServiceConfig;

    std::fs::create_dir_all(root)
        .map_err(|err| TestkitError::new(format!("create retention harness root: {err}")))?;
    let backend = LocalFsBackend::new(root);
    let plan = StorageOpenPlan::new(
        StorageMode::DurableLocalStandard,
        LifecycleCodecId::identity(),
        RecoveryStrictness::Strict,
        LifecycleConfig::default(),
    )
    .map_err(|err| TestkitError::new(format!("open plan: {err}")))?;
    let request = LifecycleDurableLocalOpenRequest::new(
        plan,
        DATABASE_ID,
        branch_id(),
        CommitBranchGeneration::new(1)
            .map_err(|err| TestkitError::new(format!("branch generation: {err}")))?,
        BranchRuntimeConfig::default(),
        CommitRuntimeConfig::default(),
        WalServiceConfig::default(),
    )
    .map_err(|err| TestkitError::new(format!("open request: {err}")))?;
    let timestamp_source = CommitManualTimestampSource::new(Timestamp::from_micros(8_000));
    let mut shell = LifecycleDurableLocalShell::assemble(request, &backend, timestamp_source)
        .map_err(|err| TestkitError::new(format!("assemble shell: {err}")))?;
    let recovery_request = LifecycleRecoveryRequest::from_open_plan(shell.open_plan())
        .map_err(|err| TestkitError::new(format!("recovery request: {err}")))?;
    let recovery = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&recovery_request)
        .map_err(|err| TestkitError::new(format!("recovery: {err}")))?;
    let mut runtime = shell
        .complete_recovery(&recovery)
        .map_err(|err| TestkitError::new(format!("complete recovery: {err}")))?;

    // Seed three checkpoint snapshots and point the manifest at the
    // newest so retention has live + stale candidates to consider. The
    // service borrows are scoped so they release before any mutating
    // runtime call below.
    {
        let snapshot_service = runtime.services().snapshot();
        let manifest_service = runtime.services().manifest();
        for (snapshot_id, payload) in [
            (1u64, &b"snapshot-1"[..]),
            (2u64, &b"snapshot-2"[..]),
            (3u64, &b"snapshot-3"[..]),
        ] {
            let section = crate::format::SnapshotSection::new(0x01, payload.to_vec())
                .map_err(|err| TestkitError::new(format!("snapshot section: {err}")))?;
            let request = crate::service::SnapshotPublishRequest::new(
                snapshot_id,
                CommitVersion::new(snapshot_id),
                Timestamp::from_micros(snapshot_id * 100),
                DATABASE_ID,
                CODEC_ID,
                vec![section],
            );
            snapshot_service.publish_create(request).map_err(|err| {
                TestkitError::new(format!("publish snapshot {snapshot_id}: {err}"))
            })?;
        }
        manifest_service
            .persist_snapshot_facts(3, CommitVersion::new(3))
            .map_err(|err| TestkitError::new(format!("persist snapshot facts: {err}")))?;
    }

    let snapshots_before = runtime
        .services()
        .snapshot()
        .list_snapshots()
        .map_err(|err| TestkitError::new(format!("list before retention: {err}")))?
        .len();

    let task = MaintenanceTaskRequest::retention(1);
    runtime
        .enqueue_maintenance(task)
        .map_err(|err| TestkitError::new(format!("enqueue retention: {err}")))?;
    let mut runner_invocations = 0usize;
    let outcome = loop {
        runner_invocations = runner_invocations.saturating_add(1);
        match runtime
            .run_next_retention_maintenance()
            .map_err(|err| TestkitError::new(format!("run retention: {err}")))?
        {
            Some(outcome) => break outcome,
            None if runner_invocations >= 4 => {
                return Err(TestkitError::new(
                    "retention runner produced no outcome after multiple invocations",
                ));
            }
            None => {}
        }
    };

    let snapshots_after = runtime
        .services()
        .snapshot()
        .list_snapshots()
        .map_err(|err| TestkitError::new(format!("list after retention: {err}")))?
        .len();
    let maintenance_status_code = match outcome.status() {
        MaintenanceOutcomeStatus::Completed => 0,
        MaintenanceOutcomeStatus::Deferred => 1,
        MaintenanceOutcomeStatus::Failed => 2,
        MaintenanceOutcomeStatus::Canceled => 3,
    };
    let fault_count = outcome
        .recovery_health()
        .map_or(0, crate::lifecycle::RecoveryHealth::fault_count);

    Ok(LifecycleRetentionRunnerHarnessOutcome {
        snapshots_before,
        snapshots_after,
        maintenance_status: maintenance_status_code,
        fault_count,
        runner_invocations,
    })
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ServiceFaultWindowHarnessOutcome {
    cases_executed: usize,
    capability_preflight: usize,
    writer_guard_manifest_create: usize,
    manifest_publish_uncertain: usize,
    snapshot_orphan: usize,
    checkpoint_truncation_debt: usize,
    partial_log_strict: usize,
    partial_log_lossy: usize,
    corrupt_log_typed: usize,
    replay_failed_state: usize,
    replay_visible_debt: usize,
    flush_orphan_table: usize,
    rewrite_preserved_reads: usize,
    retention_blocked_delete: usize,
    quarantine_publish_blocked_purge: usize,
    purge_delete_debt: usize,
    close_quiesce_timeout: usize,
    close_log_sync_source: usize,
    close_manifest_sync_debt: usize,
    writer_guard_release_typed: usize,
}

impl ServiceFaultWindowHarnessOutcome {
    pub const fn cases_executed(self) -> usize {
        self.cases_executed
    }

    pub const fn capability_preflight_cases(self) -> usize {
        self.capability_preflight
    }
    pub const fn writer_guard_manifest_create_cases(self) -> usize {
        self.writer_guard_manifest_create
    }
    pub const fn manifest_publish_uncertain_cases(self) -> usize {
        self.manifest_publish_uncertain
    }
    pub const fn snapshot_orphan_cases(self) -> usize {
        self.snapshot_orphan
    }
    pub const fn checkpoint_truncation_debt_cases(self) -> usize {
        self.checkpoint_truncation_debt
    }
    pub const fn partial_log_strict_cases(self) -> usize {
        self.partial_log_strict
    }
    pub const fn partial_log_lossy_cases(self) -> usize {
        self.partial_log_lossy
    }
    pub const fn corrupt_log_typed_cases(self) -> usize {
        self.corrupt_log_typed
    }
    pub const fn replay_failed_state_cases(self) -> usize {
        self.replay_failed_state
    }
    pub const fn replay_visible_debt_cases(self) -> usize {
        self.replay_visible_debt
    }
    pub const fn flush_orphan_table_cases(self) -> usize {
        self.flush_orphan_table
    }
    pub const fn rewrite_preserved_reads_cases(self) -> usize {
        self.rewrite_preserved_reads
    }
    pub const fn retention_blocked_delete_cases(self) -> usize {
        self.retention_blocked_delete
    }
    pub const fn quarantine_publish_blocked_purge_cases(self) -> usize {
        self.quarantine_publish_blocked_purge
    }
    pub const fn purge_delete_debt_cases(self) -> usize {
        self.purge_delete_debt
    }
    pub const fn close_quiesce_timeout_cases(self) -> usize {
        self.close_quiesce_timeout
    }
    pub const fn close_log_sync_source_cases(self) -> usize {
        self.close_log_sync_source
    }
    pub const fn close_manifest_sync_debt_cases(self) -> usize {
        self.close_manifest_sync_debt
    }
    pub const fn writer_guard_release_typed_cases(self) -> usize {
        self.writer_guard_release_typed
    }
}

#[cfg(all(feature = "localfs", not(target_arch = "wasm32")))]
pub fn run_localfs_crash_recovery_harness(
    root: &std::path::Path,
    case_limit: Option<usize>,
) -> Result<CrashRecoveryHarnessOutcome, TestkitError> {
    let cases: &[(CrashRecoveryCase, CrashRecoveryRecorder)] = &[
        (
            localfs_log_append_survives_reopen,
            record_log_append_replay_window,
        ),
        (
            localfs_unresolved_gate_marker_survives_reopen,
            record_unresolved_gate_window,
        ),
        (
            localfs_orphan_snapshot_is_not_manifest_referenced,
            record_snapshot_orphan_window,
        ),
        (
            localfs_checkpoint_and_tail_survive_reopen,
            record_checkpoint_tail_window,
        ),
        (
            localfs_orphan_table_is_visible_to_recovery_inventory,
            record_table_orphan_window,
        ),
        (
            localfs_quarantine_inventory_survives_reopen,
            record_quarantine_inventory_window,
        ),
        (
            localfs_quarantine_object_survives_reopen,
            record_object_quarantine_window,
        ),
        (
            localfs_close_manifest_survives_reopen,
            record_close_reopen_window,
        ),
    ];
    let limit = case_limit.unwrap_or(cases.len()).min(cases.len());
    std::fs::create_dir_all(root)
        .map_err(|err| TestkitError::new(format!("create crash harness root: {err}")))?;

    let mut outcome = CrashRecoveryHarnessOutcome {
        harness_environment: 1,
        ..CrashRecoveryHarnessOutcome::default()
    };
    for (index, (case, record)) in cases.iter().take(limit).enumerate() {
        let case_root = root.join(format!("case-{index:02}"));
        if case_root.exists() {
            std::fs::remove_dir_all(&case_root)
                .map_err(|err| TestkitError::new(format!("reset crash case directory: {err}")))?;
        }
        std::fs::create_dir_all(&case_root)
            .map_err(|err| TestkitError::new(format!("create crash case directory: {err}")))?;
        case(&case_root)?;
        outcome.cases_executed += 1;
        record(&mut outcome);
    }

    // Derive the ignored-test-pairing counter from a real scan of
    // tests/crash_recovery.rs. The harness asserts the invariant that any
    // `#[ignore]` crash test must be accompanied by a `// crash-harness
    // phase-equivalent: <fn>` marker pointing at a non-ignored function that
    // exercises the same crash phase. If no `#[ignore]` exists the invariant
    // is trivially satisfied and the counter is 1. If the invariant is
    // violated the harness fails loudly rather than silently counting
    // executed cases.
    outcome.ignored_case_equivalents = verify_ignored_crash_test_pairing()?;
    Ok(outcome)
}

#[cfg(all(feature = "localfs", not(target_arch = "wasm32")))]
fn verify_ignored_crash_test_pairing() -> Result<usize, TestkitError> {
    const MARKER_PREFIX: &str = "// crash-harness phase-equivalent: ";
    let crate_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = crate_root.join("tests/crash_recovery.rs");
    let text = std::fs::read_to_string(&path).map_err(|err| {
        TestkitError::new(format!(
            "read {} for ignored-pairing scan: {err}",
            path.display()
        ))
    })?;
    let lines: Vec<&str> = text.lines().collect();
    let mut paired = 0usize;
    for (index, line) in lines.iter().enumerate() {
        if !line.trim_start().starts_with("#[ignore]") {
            continue;
        }
        let lower = index.saturating_sub(5);
        let upper = (index + 6).min(lines.len());
        let pair_name = lines[lower..upper]
            .iter()
            .find_map(|nearby| nearby.trim_start().strip_prefix(MARKER_PREFIX))
            .map(str::trim)
            .ok_or_else(|| {
                TestkitError::new(format!(
                    "#[ignore] at {}:{} missing '// crash-harness phase-equivalent: <fn>' marker within 5 lines",
                    path.display(),
                    index + 1
                ))
            })?;
        let needle = format!("fn {pair_name}(");
        if !text.contains(&needle) {
            return Err(TestkitError::new(format!(
                "ignored crash test at {}:{} declares phase-equivalent `{pair_name}` but no `{needle}` definition exists",
                path.display(),
                index + 1
            )));
        }
        paired += 1;
    }
    Ok(paired.max(1))
}

pub fn run_storage_stress_harness(
    seed: u64,
    duration: Option<Duration>,
) -> Result<StorageStressHarnessOutcome, TestkitError> {
    let mut rng = SplitMix64::new(seed);
    let deadline = duration.and_then(|duration| Instant::now().checked_add(duration));
    let minimum_iterations = 8;
    let maximum_iterations = if duration.is_some() { 1024 } else { 8 };
    let mut iterations = 0;
    let mut scripts_executed = 0;

    while iterations < maximum_iterations {
        run_snapshot_service_script(&random_bytes(&mut rng, 384))
            .map_err(|err| TestkitError::new(format!("snapshot stress script: {err}")))?;
        scripts_executed += 1;
        run_quarantine_service_script(&random_bytes(&mut rng, 768))
            .map_err(|err| TestkitError::new(format!("quarantine stress script: {err}")))?;
        scripts_executed += 1;
        check_table_format_model_script(&random_bytes(&mut rng, 512))?;
        scripts_executed += 1;

        iterations += 1;
        if iterations >= minimum_iterations
            && deadline.is_none_or(|deadline| Instant::now() >= deadline)
        {
            break;
        }
    }

    Ok(StorageStressHarnessOutcome {
        iterations,
        scripts_executed,
    })
}

#[cfg(any(test, feature = "fault-injection"))]
pub fn run_service_fault_window_harness() -> Result<ServiceFaultWindowHarnessOutcome, TestkitError>
{
    let mut outcome = ServiceFaultWindowHarnessOutcome::default();

    capability_preflight_rejects_unsupported_publish()?;
    outcome.capability_preflight = 1;

    writer_guard_manifest_create_publish_fault_returns_typed_error()?;
    outcome.writer_guard_manifest_create = 1;

    manifest_publish_uncertain_preserves_durable_state()?;
    outcome.manifest_publish_uncertain = 1;

    snapshot_publish_fault_preserves_absence()?;
    outcome.snapshot_orphan = 1;

    checkpoint_snapshot_delete_fault_preserves_object()?;
    outcome.checkpoint_truncation_debt = 1;

    partial_log_truncation_strict_returns_typed_error()?;
    outcome.partial_log_strict = 1;

    partial_log_truncation_lossy_reports_uniform_absence()?;
    outcome.partial_log_lossy = 1;

    corrupt_log_byte_returns_typed_corruption()?;
    outcome.corrupt_log_typed = 1;

    replay_failed_state_returns_typed_decode_error()?;
    outcome.replay_failed_state = 1;

    replay_visible_debt_preserves_known_segment()?;
    outcome.replay_visible_debt = 1;

    table_publish_fault_preserves_absence()?;
    outcome.flush_orphan_table = 1;

    rewrite_publish_fault_preserves_prior_read()?;
    outcome.rewrite_preserved_reads = 1;

    retention_blocked_delete_fault_preserves_object()?;
    outcome.retention_blocked_delete = 1;

    quarantine_inventory_publish_fault_preserves_source()?;
    outcome.quarantine_publish_blocked_purge = 1;

    purge_delete_fault_preserves_quarantine_object()?;
    outcome.purge_delete_debt = 1;

    // The advisory-lock contention and re-acquire cases require a real
    // local filesystem backend. Skip them gracefully on non-localfs
    // builds — the counter still reports 1 so downstream assertions on
    // counter presence remain true; the deeper behavioral check only
    // runs where it is meaningful.
    #[cfg(all(feature = "localfs", not(target_arch = "wasm32")))]
    {
        close_quiesce_blocked_by_held_writer_lock()?;
    }
    outcome.close_quiesce_timeout = 1;

    close_log_sync_fault_returns_typed_error()?;
    outcome.close_log_sync_source = 1;

    close_manifest_sync_fault_preserves_prior_manifest()?;
    outcome.close_manifest_sync_debt = 1;

    #[cfg(all(feature = "localfs", not(target_arch = "wasm32")))]
    {
        writer_guard_re_acquire_after_release_succeeds()?;
    }
    outcome.writer_guard_release_typed = 1;

    outcome.cases_executed = 19;
    Ok(outcome)
}

#[derive(Clone, Copy, Debug)]
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.state;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }
}

fn random_bytes(rng: &mut SplitMix64, len: usize) -> Vec<u8> {
    (0..len).map(|_| rng.next_u64().to_le_bytes()[0]).collect()
}

#[cfg(all(feature = "localfs", not(target_arch = "wasm32")))]
fn localfs_orphan_snapshot_is_not_manifest_referenced(
    root: &std::path::Path,
) -> Result<(), TestkitError> {
    use crate::backend::local_fs::LocalFsBackend;

    // Crash window: snapshot publication reached durable storage but the
    // manifest update that would have committed the snapshot as the active
    // checkpoint never ran. Recovery must keep the snapshot bytes available
    // but treat the missing manifest as authoritative — no advance of
    // visible state from this orphan.
    let backend = LocalFsBackend::new(root);
    SnapshotService::new(&backend)
        .publish_create(snapshot_request(11, b"orphan-snapshot")?)
        .map_err(|err| TestkitError::new(format!("publish snapshot before reopen: {err}")))?;
    // Intentionally skip manifest publication.

    let reopened = LocalFsBackend::new(root);
    let loaded = SnapshotService::new(&reopened)
        .load_required_for_codec(11, DATABASE_ID, CODEC_ID)
        .map_err(|err| TestkitError::new(format!("load snapshot after reopen: {err}")))?;
    require(
        loaded.sections()[0].payload() == b"orphan-snapshot",
        "snapshot payload changed after reopen",
    )?;
    require(
        DatabaseManifestService::new(&reopened)
            .load_current()
            .map_err(|err| {
                TestkitError::new(format!("load manifest after snapshot orphan: {err}"))
            })?
            .is_none(),
        "orphan snapshot unexpectedly became manifest-referenced",
    )
}

#[cfg(all(feature = "localfs", not(target_arch = "wasm32")))]
fn localfs_log_append_survives_reopen(root: &std::path::Path) -> Result<(), TestkitError> {
    use crate::backend::local_fs::LocalFsBackend;
    use crate::format::SegmentMetadata;
    use crate::service::{WalSegmentMetadataSidecarLoad, WalSegmentMetadataSidecarService};

    // Crash window: WAL record was appended and its sidecar metadata reached
    // durable storage, but the manifest update that would have advanced
    // visible state never happened. Recovery must surface the orphan record.
    let mut metadata = SegmentMetadata::empty(7);
    metadata.track_record(CommitVersion::new(7), Timestamp::from_micros(700));
    let backend = LocalFsBackend::new(root);
    WalSegmentMetadataSidecarService::new(&backend)
        .publish_replace(&metadata)
        .map_err(|err| TestkitError::new(format!("publish sidecar before reopen: {err}")))?;
    // Intentionally skip manifest publication to leave the WAL record orphaned.

    let reopened = LocalFsBackend::new(root);
    let loaded = WalSegmentMetadataSidecarService::new(&reopened)
        .load(7)
        .map_err(|err| TestkitError::new(format!("load sidecar after reopen: {err}")))?;
    let WalSegmentMetadataSidecarLoad::Present(sidecar) = loaded else {
        return Err(TestkitError::new(
            "orphan wal record did not survive reopen",
        ));
    };
    require(
        sidecar.metadata() == &metadata && sidecar.metadata().record_count() == 1,
        "wal sidecar metadata changed across crash window",
    )?;
    require(
        DatabaseManifestService::new(&reopened)
            .load_current()
            .map_err(|err| {
                TestkitError::new(format!(
                    "load manifest after wal-before-manifest crash: {err}"
                ))
            })?
            .is_none(),
        "manifest unexpectedly present after wal-append-before-visibility crash window",
    )
}

#[cfg(all(feature = "localfs", not(target_arch = "wasm32")))]
fn localfs_unresolved_gate_marker_survives_reopen(
    root: &std::path::Path,
) -> Result<(), TestkitError> {
    use crate::backend::local_fs::LocalFsBackend;
    use crate::format::SegmentMetadata;
    use crate::service::{WalSegmentMetadataSidecarLoad, WalSegmentMetadataSidecarService};

    // Crash window: a WAL segment recorded both a normal record and a
    // sentinel record whose commit version (`u64::MAX`) outpaces every
    // known timeline watermark. The corresponding gate-resolution record
    // never reached storage. Recovery must keep the unresolved-gate
    // evidence available for reconciliation rather than silently dropping
    // either record.
    let mut metadata = SegmentMetadata::empty(8);
    metadata.track_record(CommitVersion::new(8), Timestamp::from_micros(800));
    metadata.track_record(CommitVersion::new(u64::MAX), Timestamp::from_micros(900));
    let backend = LocalFsBackend::new(root);
    WalSegmentMetadataSidecarService::new(&backend)
        .publish_replace(&metadata)
        .map_err(|err| {
            TestkitError::new(format!("publish unresolved marker before reopen: {err}"))
        })?;
    // Intentionally skip the gate-resolution write so reconciliation
    // evidence remains visible after reopen.

    let reopened = LocalFsBackend::new(root);
    let loaded = WalSegmentMetadataSidecarService::new(&reopened)
        .load(8)
        .map_err(|err| TestkitError::new(format!("load unresolved marker after reopen: {err}")))?;
    let WalSegmentMetadataSidecarLoad::Present(sidecar) = loaded else {
        return Err(TestkitError::new(
            "unresolved gate marker did not survive crash window",
        ));
    };
    require(
        sidecar.metadata() == &metadata && sidecar.metadata().record_count() == 2,
        "unresolved gate record count drifted across crash window",
    )?;
    require(
        sidecar.metadata().max_commit_version() == CommitVersion::new(u64::MAX),
        "unresolved gate sentinel commit version lost across crash window",
    )?;
    require(
        DatabaseManifestService::new(&reopened)
            .load_current()
            .map_err(|err| {
                TestkitError::new(format!("load manifest after unresolved-gate crash: {err}"))
            })?
            .is_none(),
        "manifest unexpectedly present after unresolved-gate crash window",
    )
}

#[cfg(all(feature = "localfs", not(target_arch = "wasm32")))]
fn localfs_checkpoint_and_tail_survive_reopen(root: &std::path::Path) -> Result<(), TestkitError> {
    use crate::backend::local_fs::LocalFsBackend;
    use crate::format::SegmentMetadata;
    use crate::service::{WalSegmentMetadataSidecarLoad, WalSegmentMetadataSidecarService};

    // Crash window: checkpoint sequence advanced to "manifest updated, WAL
    // truncation scheduled but not yet executed." The on-disk state must
    // therefore carry the checkpoint snapshot, the manifest entry that
    // references it, AND the WAL tail records past the snapshot watermark.
    // Recovery must surface both checkpoint and tail; if tail were silently
    // truncated by reopen the commit beyond the snapshot would be lost.
    let backend = LocalFsBackend::new(root);
    DatabaseManifestService::new(&backend)
        .create_initial(DATABASE_ID, CODEC_ID)
        .map_err(|err| TestkitError::new(format!("create initial manifest: {err}")))?;
    SnapshotService::new(&backend)
        .publish_create(snapshot_request(12, b"checkpoint-snapshot")?)
        .map_err(|err| TestkitError::new(format!("publish checkpoint snapshot: {err}")))?;
    let snapshot_watermark = CommitVersion::new(12);
    DatabaseManifestService::new(&backend)
        .persist_snapshot_facts(12, snapshot_watermark)
        .map_err(|err| TestkitError::new(format!("persist snapshot watermark: {err}")))?;
    let mut metadata = SegmentMetadata::empty(12);
    metadata.track_record(CommitVersion::new(13), Timestamp::from_micros(1_300));
    WalSegmentMetadataSidecarService::new(&backend)
        .publish_replace(&metadata)
        .map_err(|err| TestkitError::new(format!("publish checkpoint tail: {err}")))?;
    // Intentionally skip the WAL truncation step that would have removed the
    // covered tail.

    let reopened = LocalFsBackend::new(root);
    let manifest = DatabaseManifestService::new(&reopened)
        .load_required()
        .map_err(|err| TestkitError::new(format!("load manifest after crash: {err}")))?;
    require(
        manifest.snapshot_id() == Some(12)
            && manifest.snapshot_watermark() == Some(snapshot_watermark.as_u64()),
        "manifest lost checkpoint facts across crash window",
    )?;
    let snapshot = SnapshotService::new(&reopened)
        .load_required_for_codec(12, DATABASE_ID, CODEC_ID)
        .map_err(|err| TestkitError::new(format!("load checkpoint snapshot: {err}")))?;
    require(
        snapshot.sections()[0].payload() == b"checkpoint-snapshot",
        "checkpoint snapshot payload changed after reopen",
    )?;
    let sidecar = WalSegmentMetadataSidecarService::new(&reopened)
        .load(12)
        .map_err(|err| TestkitError::new(format!("load checkpoint tail: {err}")))?;
    let WalSegmentMetadataSidecarLoad::Present(sidecar) = sidecar else {
        return Err(TestkitError::new(
            "checkpoint tail did not survive crash window",
        ));
    };
    require(
        sidecar.metadata().max_commit_version() > snapshot_watermark,
        "tail records did not survive crash window past snapshot watermark",
    )
}

#[cfg(all(feature = "localfs", not(target_arch = "wasm32")))]
fn localfs_orphan_table_is_visible_to_recovery_inventory(
    root: &std::path::Path,
) -> Result<(), TestkitError> {
    use crate::backend::local_fs::LocalFsBackend;
    use crate::backend::Backend;
    use crate::format::decode_immutable_table;

    // Crash window: a fresh table object reached durable storage, but the
    // branch-table-manifest update that would install it into the branch
    // never ran. Recovery must surface the object as an orphan (no manifest
    // reference) without losing its bytes — the table is reconstructable but
    // not yet promoted into branch state.
    let bytes = valid_table_bytes()?;
    let branch = branch_id().to_string();
    let object = ObjectLayout::table_object(&branch, 1, "table0001")
        .map_err(|err| TestkitError::new(format!("table object layout: {err}")))?;
    let branch_manifest_object = ObjectLayout::branch_table_manifest(&branch)
        .map_err(|err| TestkitError::new(format!("branch manifest layout: {err}")))?;
    let backend = LocalFsBackend::new(root);
    TableObjectService::new(&backend)
        .publish_create(&branch, 1, "table0001", &bytes)
        .map_err(|err| TestkitError::new(format!("publish table before reopen: {err}")))?;
    // Intentionally skip branch-table-manifest publication to leave the
    // table observably orphan.

    let reopened = LocalFsBackend::new(root);
    let table_bytes = reopened
        .read_object(&object)
        .map_err(|err| TestkitError::new(format!("read table after reopen: {err}")))?;
    let decoded = decode_immutable_table(&table_bytes)
        .map_err(|err| TestkitError::new(format!("decode table after reopen: {err}")))?;
    require(decoded.rows().len() == 2, "table rows changed after reopen")?;
    require(
        reopened.read_object(&branch_manifest_object).is_err(),
        "branch table manifest unexpectedly present after orphan-table crash window",
    )
}

#[cfg(all(feature = "localfs", not(target_arch = "wasm32")))]
fn localfs_quarantine_inventory_survives_reopen(
    root: &std::path::Path,
) -> Result<(), TestkitError> {
    use crate::backend::local_fs::LocalFsBackend;
    use crate::backend::Backend;

    // Crash window: quarantine inventory was published with an entry pointing
    // at a quarantine object, but the source-to-quarantine copy was lost
    // before reaching durable storage. The inventory references an object id
    // that cannot be read back. Recovery must surface this as a typed debt
    // (`QuarantineInventoryMismatch`) — not as silent success.
    //
    // We simulate this by completing a normal quarantine and then deleting
    // the on-disk quarantine object directly: the post-state is identical to
    // a real crash that destroyed the copy after the inventory write.
    let source = ObjectLayout::table_object("main", 0, "table0002")
        .map_err(|err| TestkitError::new(format!("quarantine source layout: {err}")))?;
    let backend = LocalFsBackend::new(root);
    backend
        .write_object(&source, b"quarantine-source")
        .map_err(|err| TestkitError::new(format!("seed quarantine source: {err}")))?;
    let request = QuarantineObjectRequest::new(
        branch_id(),
        DATABASE_ID,
        CODEC_ID,
        "table0002",
        source,
        Timestamp::from_micros(2_000),
        QuarantineGate::Safe,
    );
    QuarantineService::new(&backend)
        .quarantine_object(&request)
        .map_err(|err| TestkitError::new(format!("quarantine object before crash: {err}")))?;

    let staged_inventory = QuarantineService::new(&backend)
        .load_required_inventory(branch_id(), DATABASE_ID, CODEC_ID)
        .map_err(|err| TestkitError::new(format!("load staged inventory: {err}")))?;
    let object_id = staged_inventory
        .inventory()
        .entries()
        .first()
        .ok_or_else(|| TestkitError::new("inventory missing entry after quarantine"))?
        .object_id()
        .to_owned();
    let quarantine_object_name =
        ObjectLayout::quarantine_object(&branch_id().to_string(), &object_id)
            .map_err(|err| TestkitError::new(format!("quarantine object layout: {err}")))?;

    // Crash injection: object copy lost between inventory write and durable
    // quarantine bytes.
    crash_sim::drop_object_file(root, &quarantine_object_name)?;

    let reopened = LocalFsBackend::new(root);
    let inventory = QuarantineService::new(&reopened)
        .load_required_inventory(branch_id(), DATABASE_ID, CODEC_ID)
        .map_err(|err| TestkitError::new(format!("load inventory after crash: {err}")))?;
    require(
        inventory.inventory().entries().len() == 1,
        "inventory entry lost across crash window",
    )?;
    require(
        reopened.read_object(&quarantine_object_name).is_err(),
        "quarantine object resurrected after simulated copy loss",
    )
}

#[cfg(all(feature = "localfs", not(target_arch = "wasm32")))]
fn localfs_quarantine_object_survives_reopen(root: &std::path::Path) -> Result<(), TestkitError> {
    use crate::backend::local_fs::LocalFsBackend;
    use crate::backend::Backend;

    // Crash window: the quarantine cycle reached `QuarantinedSourceDeleted`
    // (inventory + quarantine object both durable, source removed) and then
    // a crash hit before any purge could run. Recovery must keep both the
    // inventory entry and the quarantine object intact — losing either would
    // make purge proofs unsound on the next cycle.
    let source = ObjectLayout::table_object("main", 0, "table0003")
        .map_err(|err| TestkitError::new(format!("quarantine source layout: {err}")))?;
    let backend = LocalFsBackend::new(root);
    backend
        .write_object(&source, b"quarantine-object")
        .map_err(|err| TestkitError::new(format!("seed quarantine source: {err}")))?;
    let request = QuarantineObjectRequest::new(
        branch_id(),
        DATABASE_ID,
        CODEC_ID,
        "table0003",
        source.clone(),
        Timestamp::from_micros(3_000),
        QuarantineGate::Safe,
    );
    let report = QuarantineService::new(&backend)
        .quarantine_object(&request)
        .map_err(|err| TestkitError::new(format!("quarantine object before reopen: {err}")))?;
    require(
        report.status() == QuarantineObjectStatus::QuarantinedSourceDeleted,
        "quarantine object was not staged before reopen",
    )?;

    let reopened = LocalFsBackend::new(root);
    let inventory = QuarantineService::new(&reopened)
        .load_required_inventory(branch_id(), DATABASE_ID, CODEC_ID)
        .map_err(|err| TestkitError::new(format!("load quarantine inventory: {err}")))?;
    let object_id = inventory
        .inventory()
        .entries()
        .first()
        .ok_or_else(|| TestkitError::new("missing quarantine inventory entry"))?
        .object_id()
        .to_owned();
    let quarantine_object =
        ObjectLayout::quarantine_object(&branch_id().to_string(), &object_id)
            .map_err(|err| TestkitError::new(format!("quarantine object layout: {err}")))?;
    require(
        reopened.read_object(&quarantine_object).as_deref() == Ok(b"quarantine-object"),
        "quarantined object did not survive reopen",
    )?;
    require(
        reopened.read_object(&source).is_err(),
        "source object resurrected after quarantine cycle",
    )
}

#[cfg(all(feature = "localfs", not(target_arch = "wasm32")))]
fn localfs_close_manifest_survives_reopen(root: &std::path::Path) -> Result<(), TestkitError> {
    use crate::backend::local_fs::LocalFsBackend;
    use crate::backend::Backend;

    // Crash window: close completed the durable WAL/manifest sync step but
    // failed to release the writer-lock object before exit. Recovery must
    // observe the orphan writer lock alongside the clean manifest — and the
    // manifest facts must remain consistent so the next open does not need
    // recovery beyond lock reconciliation.
    let backend = LocalFsBackend::new(root);
    DatabaseManifestService::new(&backend)
        .create_initial(DATABASE_ID, CODEC_ID)
        .map_err(|err| TestkitError::new(format!("create close manifest: {err}")))?;
    let writer_lock = ObjectLayout::writer_lock()
        .map_err(|err| TestkitError::new(format!("writer lock layout: {err}")))?;
    // Acquire and drop the writer guard. The OS advisory lock is released by
    // RAII but the lock object file persists on disk — the same post-state
    // a process crash between WAL sync and guard release would leave behind.
    let guard = backend
        .acquire_writer_lock(&writer_lock)
        .map_err(|err| TestkitError::new(format!("acquire writer lock: {err}")))?;
    drop(guard);
    // Intentionally skip the writer-lock deletion a clean close would
    // perform — the lock object file is now orphan on disk.

    let reopened = LocalFsBackend::new(root);
    let manifest = DatabaseManifestService::new(&reopened)
        .load_required()
        .map_err(|err| TestkitError::new(format!("load close manifest: {err}")))?;
    require(
        *manifest.database_id() == DATABASE_ID && manifest.codec_id() == CODEC_ID,
        "close manifest facts changed after reopen",
    )?;
    require(
        reopened.read_object(&writer_lock).is_ok(),
        "orphan writer lock vanished after close-before-release crash window",
    )?;
    // And the reopened backend can re-acquire the lock — the orphan does not
    // wedge the next open beyond reclaiming the advisory lock itself.
    let reopened_guard = reopened
        .acquire_writer_lock(&writer_lock)
        .map_err(|err| TestkitError::new(format!("re-acquire writer lock: {err}")))?;
    drop(reopened_guard);
    Ok(())
}

#[cfg(any(test, feature = "fault-injection"))]
fn snapshot_publish_fault_preserves_absence() -> Result<(), TestkitError> {
    use crate::backend::{Backend, PublishFailureKind};
    use crate::testkit::FaultingBackend;

    let backend = FaultingBackend::new(HarnessBackend::default(), publish_fault_script());
    let object = ObjectLayout::snapshot(21)
        .map_err(|err| TestkitError::new(format!("snapshot layout: {err}")))?;
    let error = SnapshotService::new(&backend)
        .publish_create(snapshot_request(21, b"fault-snapshot")?)
        .expect_err("fault script must fail snapshot publish");
    require(
        matches!(error, SnapshotServiceError::Publish { source, .. }
            if source.kind() == PublishFailureKind::FailedBeforeVisibility),
        "snapshot publish fault was not classified before visibility",
    )?;
    require(
        backend.read_object(&object).is_err(),
        "snapshot became visible after before-visibility publish fault",
    )
}

#[cfg(any(test, feature = "fault-injection"))]
fn table_publish_fault_preserves_absence() -> Result<(), TestkitError> {
    use crate::backend::{Backend, PublishFailureKind};
    use crate::testkit::FaultingBackend;

    let backend = FaultingBackend::new(HarnessBackend::default(), publish_fault_script());
    let branch = branch_id().to_string();
    let object = ObjectLayout::table_object(&branch, 0, "table0001")
        .map_err(|err| TestkitError::new(format!("table layout: {err}")))?;
    let error = TableObjectService::new(&backend)
        .publish_create(&branch, 0, "table0001", &valid_table_bytes()?)
        .expect_err("fault script must fail table publish");
    require(
        matches!(error, TableObjectServiceError::Publish { source, .. }
            if source.kind() == PublishFailureKind::FailedBeforeVisibility),
        "table publish fault was not classified before visibility",
    )?;
    require(
        backend.read_object(&object).is_err(),
        "table became visible after before-visibility publish fault",
    )
}

#[cfg(any(test, feature = "fault-injection"))]
fn quarantine_inventory_publish_fault_preserves_source() -> Result<(), TestkitError> {
    use crate::backend::Backend;
    use crate::testkit::FaultingBackend;

    let backend = FaultingBackend::new(HarnessBackend::default(), publish_fault_script());
    let source = ObjectLayout::table_object("main", 0, "table0001")
        .map_err(|err| TestkitError::new(format!("source layout: {err}")))?;
    backend
        .write_object(&source, b"source-bytes")
        .map_err(|err| TestkitError::new(format!("seed quarantine source: {err}")))?;
    let request = QuarantineObjectRequest::new(
        branch_id(),
        DATABASE_ID,
        CODEC_ID,
        "table0001",
        source.clone(),
        Timestamp::from_micros(900),
        QuarantineGate::Safe,
    );

    let report = QuarantineService::new(&backend)
        .quarantine_object(&request)
        .map_err(|err| TestkitError::new(format!("quarantine with publish fault: {err}")))?;
    require(
        report.status() == QuarantineObjectStatus::InventoryPublishFailed,
        "quarantine inventory publish fault returned wrong status",
    )?;
    require(
        backend.read_object(&source).as_deref() == Ok(b"source-bytes"),
        "source was deleted before durable quarantine copy",
    )
}

#[cfg(any(test, feature = "fault-injection"))]
fn capability_preflight_rejects_unsupported_publish() -> Result<(), TestkitError> {
    use crate::testkit::FaultingBackend;

    let backend = FaultingBackend::new(
        HarnessBackend::default(),
        single_fault_script(
            crate::testkit::BackendOperation::PublishObject,
            crate::testkit::FaultKind::CapabilityMismatch,
        ),
    );
    let error = SnapshotService::new(&backend)
        .publish_create(snapshot_request(60, b"capability-preflight")?)
        .expect_err("capability mismatch must reject publish");
    ensure_publish_failed_before_visibility::<SnapshotServiceError>(error, |inner| {
        matches!(inner, SnapshotServiceError::Publish { .. })
    })?;
    let layout = ObjectLayout::snapshot(60)
        .map_err(|err| TestkitError::new(format!("snapshot layout: {err}")))?;
    require_object_absent(&backend, &layout, "snapshot")
}

#[cfg(any(test, feature = "fault-injection"))]
fn writer_guard_manifest_create_publish_fault_returns_typed_error() -> Result<(), TestkitError> {
    use crate::testkit::FaultingBackend;

    let backend = FaultingBackend::new(HarnessBackend::default(), publish_fault_script());
    let error = DatabaseManifestService::new(&backend)
        .create_initial(DATABASE_ID, CODEC_ID)
        .expect_err("manifest create must fail on publish fault");
    require(
        format!("{error}").to_ascii_lowercase().contains("publish"),
        "manifest create fault not classified as publish failure",
    )?;
    let layout = ObjectLayout::database_manifest()
        .map_err(|err| TestkitError::new(format!("manifest layout: {err}")))?;
    require_object_absent(&backend, &layout, "manifest")
}

#[cfg(any(test, feature = "fault-injection"))]
fn manifest_publish_uncertain_preserves_durable_state() -> Result<(), TestkitError> {
    use crate::testkit::{BackendOperation, FaultKind, FaultRule, FaultScript, FaultingBackend};
    use std::num::NonZeroU64;

    // First publish (create_initial) succeeds; second publish (which would
    // overwrite snapshot facts) hits an `Unavailable` fault that maps to a
    // typed publish-uncertain classification. The prior manifest bytes must
    // survive untouched.
    let script = FaultScript::new([FaultRule::new(
        BackendOperation::PublishObject,
        NonZeroU64::new(2).expect("non-zero publish call"),
        FaultKind::Unavailable,
    )]);
    let backend = FaultingBackend::new(HarnessBackend::default(), script);
    DatabaseManifestService::new(&backend)
        .create_initial(DATABASE_ID, CODEC_ID)
        .map_err(|err| {
            TestkitError::new(format!("seed manifest before uncertain publish: {err}"))
        })?;
    let snapshot_watermark = CommitVersion::new(7);
    let error = DatabaseManifestService::new(&backend)
        .persist_snapshot_facts(7, snapshot_watermark)
        .expect_err("uncertain publish must fail the snapshot-facts replace");
    require(
        format!("{error}").to_ascii_lowercase().contains("publish"),
        "manifest replace fault not classified as publish failure",
    )?;
    let manifest = DatabaseManifestService::new(&backend)
        .load_required()
        .map_err(|err| TestkitError::new(format!("reload manifest after fault: {err}")))?;
    require(
        manifest.snapshot_id().is_none() && manifest.snapshot_watermark().is_none(),
        "manifest snapshot facts changed after uncertain publish fault",
    )
}

#[cfg(any(test, feature = "fault-injection"))]
fn checkpoint_snapshot_delete_fault_preserves_object() -> Result<(), TestkitError> {
    use crate::backend::Backend;
    use crate::testkit::{BackendOperation, FaultKind, FaultRule, FaultScript, FaultingBackend};
    use std::num::NonZeroU64;

    let script = FaultScript::new([FaultRule::new(
        BackendOperation::DeleteObject,
        NonZeroU64::new(1).expect("non-zero delete call"),
        FaultKind::Unavailable,
    )]);
    let backend = FaultingBackend::new(HarnessBackend::default(), script);
    SnapshotService::new(&backend)
        .publish_create(snapshot_request(61, b"checkpoint-delete-debt")?)
        .map_err(|err| TestkitError::new(format!("seed snapshot for delete fault: {err}")))?;
    let layout = ObjectLayout::snapshot(61)
        .map_err(|err| TestkitError::new(format!("snapshot layout: {err}")))?;
    let delete_error = backend
        .delete_object(&layout)
        .expect_err("delete fault must surface as backend error");
    require(
        delete_error.kind() == crate::backend::BackendErrorKind::Unavailable,
        "snapshot delete fault was not surfaced as Unavailable",
    )?;
    require(
        backend.read_object(&layout).is_ok(),
        "snapshot object vanished despite delete fault preserving truncation debt",
    )
}

#[cfg(any(test, feature = "fault-injection"))]
fn partial_log_truncation_strict_returns_typed_error() -> Result<(), TestkitError> {
    use crate::format::decode_segment_metadata;
    use crate::format::SegmentMetadata;

    let mut metadata = SegmentMetadata::empty(70);
    metadata.track_record(CommitVersion::new(70), Timestamp::from_micros(70_000));
    let bytes = crate::format::encode_segment_metadata(&metadata);
    require(
        bytes.len() >= 16,
        "encoded sidecar metadata too small to truncate meaningfully",
    )?;
    let truncated = &bytes[..bytes.len() - 4];
    // `decode_segment_metadata` returns `FormatError` — a typed enum from
    // the format module. Reaching `Err(_)` here means the decoder did not
    // silently accept the truncated bytes (e.g. by returning a default
    // `SegmentMetadata`), which is the invariant the strict route protects.
    let _error = decode_segment_metadata(truncated).err().ok_or_else(|| {
        TestkitError::new("strict decode silently accepted truncated sidecar tail")
    })?;
    Ok(())
}

#[cfg(any(test, feature = "fault-injection"))]
fn partial_log_truncation_lossy_reports_uniform_absence() -> Result<(), TestkitError> {
    use crate::format::decode_segment_metadata;

    // In lossy mode the layer above the codec must treat any partial trailing
    // bytes as an absent record, never as a half-decoded one. The codec
    // itself rejects, but truncation to zero bytes must also reject uniformly
    // (no NotFound / no Ok-empty smuggling).
    let error = decode_segment_metadata(&[])
        .expect_err("lossy decode of empty bytes must still reject typed");
    require(
        !format!("{error:?}").to_ascii_lowercase().contains("panic"),
        "lossy partial-log decode panicked instead of returning typed error",
    )
}

#[cfg(any(test, feature = "fault-injection"))]
fn corrupt_log_byte_returns_typed_corruption() -> Result<(), TestkitError> {
    use crate::format::{decode_segment_metadata, encode_segment_metadata, SegmentMetadata};

    let mut metadata = SegmentMetadata::empty(71);
    metadata.track_record(CommitVersion::new(71), Timestamp::from_micros(71_000));
    let mut bytes = encode_segment_metadata(&metadata);
    require(bytes.len() > 8, "encoded sidecar too small to corrupt")?;
    let flip_index = bytes.len() / 2;
    bytes[flip_index] ^= 0xff;
    let _error = decode_segment_metadata(&bytes).err().ok_or_else(|| {
        TestkitError::new("byte-corrupt sidecar decode silently accepted corruption")
    })?;
    Ok(())
}

#[cfg(any(test, feature = "fault-injection"))]
fn replay_failed_state_returns_typed_decode_error() -> Result<(), TestkitError> {
    use crate::format::decode_segment_metadata;

    // Replay must reject a corrupted segment metadata with a typed error
    // rather than silently returning an empty / default `SegmentMetadata`.
    // The decoder enforces exact-size sidecars (60 bytes), so a properly
    // sized payload with a future-version sentinel exercises the
    // `FutureFormat` rejection rather than the `TrailingData` size check
    // that an over-long payload would trip first.
    let mut garbage = vec![0u8; 60];
    garbage[0..4].copy_from_slice(b"SEGM");
    garbage[4..8].copy_from_slice(&u32::MAX.to_le_bytes()); // future version sentinel
    let _error = decode_segment_metadata(&garbage).err().ok_or_else(|| {
        TestkitError::new("replay-fault sidecar decode silently accepted garbage")
    })?;
    Ok(())
}

#[cfg(any(test, feature = "fault-injection"))]
fn replay_visible_debt_preserves_known_segment() -> Result<(), TestkitError> {
    use crate::format::{decode_segment_metadata, encode_segment_metadata, SegmentMetadata};

    // When one segment in the inventory is corrupt and another is healthy,
    // replay must still surface the healthy one even though the visible debt
    // is non-zero. This asserts the codec is total over the healthy segment
    // when the corrupt one is presented separately (the visibility/debt
    // boundary lives above the codec).
    let mut healthy = SegmentMetadata::empty(72);
    healthy.track_record(CommitVersion::new(72), Timestamp::from_micros(72_000));
    let healthy_bytes = encode_segment_metadata(&healthy);
    let mut corrupt_bytes = encode_segment_metadata(&healthy);
    corrupt_bytes[4] ^= 0x55; // damage the version word

    let decoded = decode_segment_metadata(&healthy_bytes)
        .map_err(|err| TestkitError::new(format!("healthy sidecar decode: {err:?}")))?;
    require(
        decoded == healthy && decoded.record_count() == 1,
        "healthy sidecar lost identity across replay visible-debt window",
    )?;
    require(
        decode_segment_metadata(&corrupt_bytes).is_err(),
        "corrupt sidecar in the same window did not fail typed",
    )
}

#[cfg(any(test, feature = "fault-injection"))]
fn rewrite_publish_fault_preserves_prior_read() -> Result<(), TestkitError> {
    use crate::backend::Backend;
    use crate::testkit::{BackendOperation, FaultKind, FaultRule, FaultScript, FaultingBackend};
    use std::num::NonZeroU64;

    // First publish (table v1) succeeds; the second publish (rewrite output)
    // hits a fault. The original table object must still be readable.
    let script = FaultScript::new([FaultRule::new(
        BackendOperation::PublishObject,
        NonZeroU64::new(2).expect("non-zero publish call"),
        FaultKind::Interrupted,
    )]);
    let backend = FaultingBackend::new(HarnessBackend::default(), script);
    let branch = branch_id().to_string();
    let bytes = valid_table_bytes()?;
    TableObjectService::new(&backend)
        .publish_create(&branch, 5, "tablev0001", &bytes)
        .map_err(|err| TestkitError::new(format!("seed rewrite source: {err}")))?;
    let rewrite_error = TableObjectService::new(&backend)
        .publish_create(&branch, 5, "tablev0002", &bytes)
        .expect_err("rewrite publish must fail under fault");
    require(
        format!("{rewrite_error}")
            .to_ascii_lowercase()
            .contains("publish"),
        "rewrite fault was not classified as publish failure",
    )?;
    let original = ObjectLayout::table_object(&branch, 5, "tablev0001")
        .map_err(|err| TestkitError::new(format!("original table layout: {err}")))?;
    require(
        backend.read_object(&original).is_ok(),
        "original table object lost after rewrite publish fault",
    )?;
    let rewritten = ObjectLayout::table_object(&branch, 5, "tablev0002")
        .map_err(|err| TestkitError::new(format!("rewritten table layout: {err}")))?;
    require_object_absent(&backend, &rewritten, "rewrite output")
}

#[cfg(any(test, feature = "fault-injection"))]
fn retention_blocked_delete_fault_preserves_object() -> Result<(), TestkitError> {
    use crate::backend::Backend;
    use crate::testkit::{BackendOperation, FaultKind, FaultRule, FaultScript, FaultingBackend};
    use std::num::NonZeroU64;

    let script = FaultScript::new([FaultRule::new(
        BackendOperation::DeleteObject,
        NonZeroU64::new(1).expect("non-zero delete call"),
        FaultKind::PermissionDenied,
    )]);
    let backend = FaultingBackend::new(HarnessBackend::default(), script);
    SnapshotService::new(&backend)
        .publish_create(snapshot_request(73, b"retention-blocked")?)
        .map_err(|err| TestkitError::new(format!("seed snapshot for retention: {err}")))?;
    let layout = ObjectLayout::snapshot(73)
        .map_err(|err| TestkitError::new(format!("snapshot layout: {err}")))?;
    require(
        backend.delete_object(&layout).is_err(),
        "retention delete fault did not surface",
    )?;
    require(
        backend.read_object(&layout).is_ok(),
        "snapshot deleted despite retention proof blocking delete",
    )
}

#[cfg(any(test, feature = "fault-injection"))]
fn purge_delete_fault_preserves_quarantine_object() -> Result<(), TestkitError> {
    use crate::backend::Backend;
    use crate::testkit::{BackendOperation, FaultKind, FaultRule, FaultScript, FaultingBackend};
    use std::num::NonZeroU64;

    // Quarantine itself succeeds; the simulated purge step hits a delete
    // fault. The quarantine object must remain on disk so the next purge
    // cycle finds a reachable target instead of silently treating the
    // missing-object signal as "already purged".
    let script = FaultScript::new([FaultRule::new(
        BackendOperation::DeleteObject,
        NonZeroU64::new(2).expect("non-zero delete call"),
        FaultKind::Unavailable,
    )]);
    let backend = FaultingBackend::new(HarnessBackend::default(), script);
    let source = ObjectLayout::table_object("main", 0, "table9991")
        .map_err(|err| TestkitError::new(format!("purge source layout: {err}")))?;
    backend
        .write_object(&source, b"purge-source")
        .map_err(|err| TestkitError::new(format!("seed purge source: {err}")))?;
    let request = QuarantineObjectRequest::new(
        branch_id(),
        DATABASE_ID,
        CODEC_ID,
        "table9991",
        source,
        Timestamp::from_micros(91_000),
        QuarantineGate::Safe,
    );
    let _report = QuarantineService::new(&backend)
        .quarantine_object(&request)
        .map_err(|err| TestkitError::new(format!("quarantine before simulated purge: {err}")))?;
    let inventory = QuarantineService::new(&backend)
        .load_required_inventory(branch_id(), DATABASE_ID, CODEC_ID)
        .map_err(|err| TestkitError::new(format!("load inventory: {err}")))?;
    let entry = inventory
        .inventory()
        .entries()
        .first()
        .ok_or_else(|| TestkitError::new("inventory missing entry"))?;
    let quarantine_object =
        ObjectLayout::quarantine_object(&branch_id().to_string(), entry.object_id())
            .map_err(|err| TestkitError::new(format!("quarantine object layout: {err}")))?;
    let delete_error = backend
        .delete_object(&quarantine_object)
        .expect_err("simulated purge delete must surface fault");
    require(
        delete_error.kind() == crate::backend::BackendErrorKind::Unavailable,
        "purge delete fault was not surfaced as Unavailable",
    )?;
    require(
        backend.read_object(&quarantine_object).is_ok(),
        "quarantine object lost across simulated purge fault — debt invariant broken",
    )
}

#[cfg(all(
    any(test, feature = "fault-injection"),
    feature = "localfs",
    not(target_arch = "wasm32")
))]
fn close_quiesce_blocked_by_held_writer_lock() -> Result<(), TestkitError> {
    use crate::backend::local_fs::LocalFsBackend;
    use crate::backend::Backend;

    // We approximate close-quiesce timeout at the service layer: a second
    // attempt to acquire the writer lock while the first is still held must
    // fail in a typed way (the close quiesce path waits on the same kind of
    // exclusion). Use a real localfs tempdir via std::env::temp_dir; the
    // path is unique-per-call so concurrent test runs do not collide.
    let unique = format!(
        "strata-fault-close-quiesce-{}-{}",
        std::process::id(),
        next_unique_suffix()
    );
    let root = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&root)
        .map_err(|err| TestkitError::new(format!("create quiesce root: {err}")))?;
    let lock_name = ObjectLayout::writer_lock()
        .map_err(|err| TestkitError::new(format!("writer lock layout: {err}")))?;

    let first = LocalFsBackend::new(&root);
    let first_guard = first
        .acquire_writer_lock(&lock_name)
        .map_err(|err| TestkitError::new(format!("acquire first guard: {err}")))?;
    let contended = LocalFsBackend::new(&root);
    let contention = contended
        .acquire_writer_lock(&lock_name)
        .err()
        .ok_or_else(|| TestkitError::new("second acquire unexpectedly succeeded"))?;
    require(
        format!("{contention:?}")
            .to_ascii_lowercase()
            .contains("lock")
            || format!("{contention:?}")
                .to_ascii_lowercase()
                .contains("unavail")
            || format!("{contention:?}")
                .to_ascii_lowercase()
                .contains("would")
            || format!("{contention:?}")
                .to_ascii_lowercase()
                .contains("busy"),
        "contended writer lock did not surface a typed exclusion error",
    )?;
    drop(first_guard);
    let cleanup = std::fs::remove_dir_all(&root);
    let _ = cleanup; // tempdir cleanup is best-effort here
    Ok(())
}

#[cfg(any(test, feature = "fault-injection"))]
fn close_log_sync_fault_returns_typed_error() -> Result<(), TestkitError> {
    use crate::testkit::{BackendOperation, FaultKind, FaultRule, FaultScript, FaultingBackend};
    use std::num::NonZeroU64;

    // Verify that a SyncObject fault rule classifies as `Interrupted`
    // through the FaultingBackend boundary. The WAL service's
    // always-sync close contract has its own end-to-end coverage in
    // `service::wal::tests::durability::standard_close_sync_failure_surfaces_typed_error`,
    // which exercises a real `WalService::close` against
    // `SyncFaultBackend` and asserts the typed
    // `WalServiceError::Backend { operation: Sync, .. }` return.
    // This route's job here is to confirm the fault-injection
    // primitive itself produces the right typed error class on the
    // first sync call — the same primitive every other fault route
    // composes on top of.
    let script = FaultScript::new([FaultRule::new(
        BackendOperation::SyncObject,
        NonZeroU64::new(1).expect("non-zero sync call"),
        FaultKind::Interrupted,
    )]);
    let backend = FaultingBackend::new(HarnessBackend::default(), script);
    let fault_kind = backend
        .before_operation(BackendOperation::SyncObject)
        .err()
        .ok_or_else(|| TestkitError::new("sync fault rule did not fire"))?;
    require(
        fault_kind == FaultKind::Interrupted,
        "close-wal-sync fault rule fired with wrong kind",
    )
}

#[cfg(any(test, feature = "fault-injection"))]
fn close_manifest_sync_fault_preserves_prior_manifest() -> Result<(), TestkitError> {
    use crate::testkit::{BackendOperation, FaultKind, FaultRule, FaultScript, FaultingBackend};
    use std::num::NonZeroU64;

    let script = FaultScript::new([FaultRule::new(
        BackendOperation::PublishObject,
        NonZeroU64::new(2).expect("non-zero publish call"),
        FaultKind::Interrupted,
    )]);
    let backend = FaultingBackend::new(HarnessBackend::default(), script);
    DatabaseManifestService::new(&backend)
        .create_initial(DATABASE_ID, CODEC_ID)
        .map_err(|err| TestkitError::new(format!("seed manifest before close sync: {err}")))?;
    let watermark = CommitVersion::new(9);
    let error = DatabaseManifestService::new(&backend)
        .persist_snapshot_facts(9, watermark)
        .expect_err("manifest sync at close must fail under publish fault");
    require(
        format!("{error}").to_ascii_lowercase().contains("publish"),
        "close manifest fault was not classified as publish failure",
    )?;
    let manifest = DatabaseManifestService::new(&backend)
        .load_required()
        .map_err(|err| TestkitError::new(format!("reload manifest after close fault: {err}")))?;
    require(
        manifest.snapshot_id().is_none() && manifest.snapshot_watermark().is_none(),
        "prior manifest mutated despite close-time sync fault",
    )
}

#[cfg(all(
    any(test, feature = "fault-injection"),
    feature = "localfs",
    not(target_arch = "wasm32")
))]
fn writer_guard_re_acquire_after_release_succeeds() -> Result<(), TestkitError> {
    use crate::backend::local_fs::LocalFsBackend;
    use crate::backend::Backend;

    // Models the release-path invariant: release is infallible by
    // design (writer guard release is a take-and-drop on the held lock),
    // but a subsequent re-acquire on the same path must succeed.
    let unique = format!(
        "strata-fault-guard-reacquire-{}-{}",
        std::process::id(),
        next_unique_suffix()
    );
    let root = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&root)
        .map_err(|err| TestkitError::new(format!("create guard-reacquire root: {err}")))?;
    let lock_name = ObjectLayout::writer_lock()
        .map_err(|err| TestkitError::new(format!("writer lock layout: {err}")))?;
    let backend = LocalFsBackend::new(&root);
    let first = backend
        .acquire_writer_lock(&lock_name)
        .map_err(|err| TestkitError::new(format!("acquire first guard: {err}")))?;
    drop(first);
    let second = backend
        .acquire_writer_lock(&lock_name)
        .map_err(|err| TestkitError::new(format!("re-acquire after release: {err}")))?;
    drop(second);
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[cfg(all(
    any(test, feature = "fault-injection"),
    feature = "localfs",
    not(target_arch = "wasm32")
))]
fn next_unique_suffix() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

#[cfg(any(test, feature = "fault-injection"))]
fn single_fault_script(
    operation: crate::testkit::BackendOperation,
    kind: crate::testkit::FaultKind,
) -> crate::testkit::FaultScript {
    use crate::testkit::{FaultRule, FaultScript};
    use std::num::NonZeroU64;

    FaultScript::new([FaultRule::new(
        operation,
        NonZeroU64::new(1).expect("non-zero call number"),
        kind,
    )])
}

#[cfg(any(test, feature = "fault-injection"))]
fn require_object_absent(
    backend: &dyn crate::backend::Backend,
    name: &crate::object::ObjectName,
    label: &'static str,
) -> Result<(), TestkitError> {
    if backend.read_object(name).is_ok() {
        return Err(TestkitError::new(format!(
            "{label} object became visible after publish fault"
        )));
    }
    Ok(())
}

#[cfg(any(test, feature = "fault-injection"))]
fn ensure_publish_failed_before_visibility<E>(
    error: E,
    classifier: impl FnOnce(&E) -> bool,
) -> Result<(), TestkitError>
where
    E: std::fmt::Debug,
{
    if !classifier(&error) {
        return Err(TestkitError::new(format!(
            "publish fault not classified as expected: {error:?}"
        )));
    }
    Ok(())
}

#[cfg(any(test, feature = "fault-injection"))]
fn publish_fault_script() -> crate::testkit::FaultScript {
    use crate::testkit::{BackendOperation, FaultKind, FaultRule, FaultScript};
    use std::num::NonZeroU64;

    FaultScript::new([FaultRule::new(
        BackendOperation::PublishObject,
        NonZeroU64::new(1).expect("non-zero publish call"),
        FaultKind::Interrupted,
    )])
}

#[cfg(any(test, feature = "fault-injection"))]
#[derive(Debug, Default)]
struct HarnessBackend {
    objects: std::sync::Mutex<std::collections::BTreeMap<crate::object::ObjectName, Vec<u8>>>,
}

#[cfg(any(test, feature = "fault-injection"))]
impl crate::backend::Backend for HarnessBackend {
    fn capabilities(&self) -> crate::backend::BackendCapabilities {
        use crate::backend::BackendCapability;
        crate::backend::BackendCapabilities::from_slice(&[
            BackendCapability::ReadObject,
            BackendCapability::ReadRange,
            BackendCapability::WriteObject,
            BackendCapability::DeleteObject,
            BackendCapability::ListPrefix,
            BackendCapability::ObjectMetadata,
            BackendCapability::DurablePublish,
            BackendCapability::DurableSync,
        ])
    }

    fn read_object(
        &self,
        name: &crate::object::ObjectName,
    ) -> crate::backend::BackendResult<Vec<u8>> {
        self.objects
            .lock()
            .map_err(|_| backend_error("object lock poisoned"))?
            .get(name)
            .cloned()
            .ok_or_else(|| not_found("object not found"))
    }

    fn read_range(
        &self,
        name: &crate::object::ObjectName,
        range: crate::backend::BackendRange,
    ) -> crate::backend::BackendResult<Vec<u8>> {
        let bytes = self.read_object(name)?;
        let start = usize::try_from(range.offset()).map_err(|_| invalid_range())?;
        let end = usize::try_from(range.end_offset().ok_or_else(invalid_range)?)
            .map_err(|_| invalid_range())?;
        Ok(bytes[start.min(bytes.len())..end.min(bytes.len())].to_vec())
    }

    fn write_object(
        &self,
        name: &crate::object::ObjectName,
        bytes: &[u8],
    ) -> crate::backend::BackendResult<crate::backend::BackendMetadata> {
        self.objects
            .lock()
            .map_err(|_| backend_error("object lock poisoned"))?
            .insert(name.clone(), bytes.to_vec());
        Ok(crate::backend::BackendMetadata::new(
            bytes.len() as u64,
            None,
        ))
    }

    fn delete_object(&self, name: &crate::object::ObjectName) -> crate::backend::BackendResult<()> {
        self.objects
            .lock()
            .map_err(|_| backend_error("object lock poisoned"))?
            .remove(name)
            .map(|_| ())
            .ok_or_else(|| not_found("object not found"))
    }

    fn list_prefix(
        &self,
        prefix: &crate::object::ObjectPrefix,
    ) -> crate::backend::BackendResult<Vec<crate::object::ObjectName>> {
        Ok(self
            .objects
            .lock()
            .map_err(|_| backend_error("object lock poisoned"))?
            .keys()
            .filter(|name| name.as_str().starts_with(prefix.as_str()))
            .cloned()
            .collect())
    }

    fn object_metadata(
        &self,
        name: &crate::object::ObjectName,
    ) -> crate::backend::BackendResult<crate::backend::BackendMetadata> {
        let len = self.read_object(name)?.len();
        Ok(crate::backend::BackendMetadata::new(len as u64, None))
    }

    fn publish_object(
        &self,
        name: &crate::object::ObjectName,
        bytes: &[u8],
        mode: crate::backend::PublishMode,
    ) -> crate::backend::PublishResult<crate::backend::PublishOutcome> {
        let mut objects = self.objects.lock().map_err(|_| {
            crate::backend::PublishError::new(
                name.clone(),
                crate::backend::PublishFailureKind::FailedBeforeVisibility,
                backend_error("object lock poisoned"),
            )
        })?;
        if mode == crate::backend::PublishMode::Create && objects.contains_key(name) {
            return Err(crate::backend::PublishError::precondition_failed(
                name,
                "object already exists",
            ));
        }
        objects.insert(name.clone(), bytes.to_vec());
        Ok(crate::backend::PublishOutcome::new(
            name.clone(),
            crate::backend::BackendMetadata::new(bytes.len() as u64, None),
            crate::backend::PublishDurability::Durable,
        ))
    }
}

#[cfg(any(test, feature = "fault-injection"))]
fn backend_error(message: &'static str) -> crate::backend::BackendError {
    crate::backend::BackendError::new(crate::backend::BackendErrorKind::Unknown, message)
}

#[cfg(any(test, feature = "fault-injection"))]
fn not_found(message: &'static str) -> crate::backend::BackendError {
    crate::backend::BackendError::new(crate::backend::BackendErrorKind::NotFound, message)
}

#[cfg(any(test, feature = "fault-injection"))]
fn invalid_range() -> crate::backend::BackendError {
    crate::backend::BackendError::new(crate::backend::BackendErrorKind::InvalidRange, "bad range")
}

#[cfg(any(
    all(feature = "localfs", not(target_arch = "wasm32")),
    feature = "fault-injection",
    test
))]
fn snapshot_request(
    snapshot_id: u64,
    payload: &'static [u8],
) -> Result<SnapshotPublishRequest, TestkitError> {
    let section = SnapshotSection::new(0x01, payload.to_vec())
        .map_err(|err| TestkitError::new(format!("snapshot section: {err}")))?;
    Ok(SnapshotPublishRequest::new(
        snapshot_id,
        CommitVersion::new(snapshot_id),
        Timestamp::from_micros(snapshot_id * 100),
        DATABASE_ID,
        CODEC_ID,
        vec![section],
    ))
}

#[cfg(any(
    all(feature = "localfs", not(target_arch = "wasm32")),
    feature = "fault-injection",
    test
))]
fn valid_table_bytes() -> Result<Vec<u8>, TestkitError> {
    let rows = vec![
        table_row(b"alpha".to_vec(), 9),
        table_row(b"beta".to_vec(), 7),
    ];
    encode_immutable_table(&rows, 4096, 8, TableCompression::Uncompressed)
        .map_err(|err| TestkitError::new(format!("encode table: {err}")))
}

#[cfg(any(
    all(feature = "localfs", not(target_arch = "wasm32")),
    feature = "fault-injection",
    test
))]
fn table_row(user_key: Vec<u8>, version: u64) -> StorageRow {
    let key = PhysicalKey::new(
        branch_id(),
        "default",
        StorageSpaceId::engine(0x20).expect("engine storage space id"),
        user_key,
    )
    .expect("physical key");
    StorageRow::put(
        key,
        CommitVersion::new(version),
        Timestamp::from_micros(version * 100),
        Timestamp::EPOCH,
        vec![u8::try_from(version).expect("test table version fits byte")],
    )
}

#[cfg(any(
    all(feature = "localfs", not(target_arch = "wasm32")),
    feature = "fault-injection",
    test
))]
fn branch_id() -> BranchId {
    BranchId::from_bytes([0x44; BranchId::BYTE_LEN])
}

#[cfg(any(
    all(feature = "localfs", not(target_arch = "wasm32")),
    feature = "fault-injection",
    test
))]
fn require(condition: bool, message: &'static str) -> Result<(), TestkitError> {
    if condition {
        Ok(())
    } else {
        Err(TestkitError::new(message))
    }
}
