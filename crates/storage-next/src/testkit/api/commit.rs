//! Model-backed API commit contracts.

use std::error::Error;
use std::fmt;
use std::time::Duration;

use strata_core_next::{BranchId, CommitVersion};

use crate::api::{
    CommitBatch, CommitCondition, CommitDurabilitySummary, CommitExpectedVersion, CommitMutation,
    CommitOptions, PointReadRequest, ReadBound, StorageApiError, StorageApiErrorClass, StorageKey,
    StorageOpenOptions, StorageRuntime, StorageSpaceId, StorageValue,
};
use crate::testkit::TestkitError;

const DEFAULT_BRANCH_ID: BranchId = BranchId::from_bytes([0x01; BranchId::BYTE_LEN]);
const ENGINE_STORAGE_SPACE: u8 = 0x20;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StorageApiCommitModelOutcome {
    scripts: usize,
    commits: usize,
    puts: usize,
    deletes: usize,
    conditions: usize,
    conflicts: usize,
    ttl_roundtrips: usize,
}

impl StorageApiCommitModelOutcome {
    pub const fn scripts(self) -> usize {
        self.scripts
    }

    pub const fn commits(self) -> usize {
        self.commits
    }

    pub const fn puts(self) -> usize {
        self.puts
    }

    pub const fn deletes(self) -> usize {
        self.deletes
    }

    pub const fn conditions(self) -> usize {
        self.conditions
    }

    pub const fn conflicts(self) -> usize {
        self.conflicts
    }

    pub const fn ttl_roundtrips(self) -> usize {
        self.ttl_roundtrips
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StorageApiCommitFaultOutcome {
    validation_failures: usize,
    conflicts: usize,
    before_allocation_failures: usize,
    after_allocation_failures: usize,
    wal_append_failures: usize,
    forced_durability_uncertainties: usize,
    branch_apply_failures: usize,
    visibility_publication_failures: usize,
}

impl StorageApiCommitFaultOutcome {
    pub const fn validation_failures(self) -> usize {
        self.validation_failures
    }

    pub const fn conflicts(self) -> usize {
        self.conflicts
    }

    pub const fn before_allocation_failures(self) -> usize {
        self.before_allocation_failures
    }

    pub const fn after_allocation_failures(self) -> usize {
        self.after_allocation_failures
    }

    pub const fn wal_append_failures(self) -> usize {
        self.wal_append_failures
    }

    pub const fn forced_durability_uncertainties(self) -> usize {
        self.forced_durability_uncertainties
    }

    pub const fn branch_apply_failures(self) -> usize {
        self.branch_apply_failures
    }

    pub const fn visibility_publication_failures(self) -> usize {
        self.visibility_publication_failures
    }

    pub const fn total_routes(self) -> usize {
        self.validation_failures
            + self.conflicts
            + self.before_allocation_failures
            + self.after_allocation_failures
            + self.wal_append_failures
            + self.forced_durability_uncertainties
            + self.branch_apply_failures
            + self.visibility_publication_failures
    }
}

const COMMIT_FAULT_VALIDATION: u8 = 0;
const COMMIT_FAULT_CONFLICT: u8 = 1;
const COMMIT_FAULT_BEFORE_ALLOCATION: u8 = 2;
const COMMIT_FAULT_AFTER_ALLOCATION: u8 = 3;
const COMMIT_FAULT_WAL_APPEND: u8 = 4;
const COMMIT_FAULT_FORCED_DURABILITY: u8 = 5;
const COMMIT_FAULT_BRANCH_APPLY: u8 = 6;
const COMMIT_FAULT_VISIBILITY_PUBLICATION: u8 = 7;

pub fn check_storage_api_commit_model_contract(
    script: &[u8],
) -> Result<StorageApiCommitModelOutcome, TestkitError> {
    let script = non_empty_script(script);
    let mut runtime = StorageRuntime::open(StorageOpenOptions::default())
        .map_err(|error| testkit_error(&error))?
        .into_runtime();
    let mut outcome = StorageApiCommitModelOutcome {
        scripts: 1,
        ..StorageApiCommitModelOutcome::default()
    };

    let first = runtime
        .commit(&put_batch(b"key-a", &[script[0]])?)
        .map_err(|error| testkit_error(&error))?;
    outcome.commits += 1;
    outcome.puts += 1;
    assert_read_value(&runtime, b"key-a", &[script[0]])?;

    let conditioned = put_batch(b"key-a", &[script_byte(script, 1)])?
        .with_conditions(vec![CommitCondition::expected_present(
            engine_space()?,
            api_key(b"key-a")?,
            first.commit_version(),
        )])
        .map_err(|error| testkit_error(&error))?;
    let second = runtime
        .commit(&conditioned)
        .map_err(|error| testkit_error(&error))?;
    outcome.commits += 1;
    outcome.puts += 1;
    outcome.conditions += 1;
    if second.put_count() != 1 || second.durability() != CommitDurabilitySummary::NotDurable {
        return Err(TestkitError::new(
            "commit summary did not preserve put and durability facts",
        ));
    }

    let ttl = Duration::from_micros(1 + u64::from(script_byte(script, 2)));
    let ttl_batch = CommitBatch::new(
        DEFAULT_BRANCH_ID,
        vec![CommitMutation::Put {
            storage_space: engine_space()?,
            key: api_key(b"ttl")?,
            value: StorageValue::new(vec![script_byte(script, 3)]),
            ttl: Some(ttl),
        }],
        CommitOptions::default(),
    )
    .map_err(|error| testkit_error(&error))?;
    let ttl_summary = runtime
        .commit(&ttl_batch)
        .map_err(|error| testkit_error(&error))?;
    let ttl_row = runtime
        .read_point(&PointReadRequest::new(
            DEFAULT_BRANCH_ID,
            engine_space()?,
            api_key(b"ttl")?,
            ReadBound::Latest,
        ))
        .map_err(|error| testkit_error(&error))?
        .row()
        .cloned()
        .ok_or_else(|| TestkitError::new("TTL commit did not produce a visible row"))?;
    if ttl_row.expires_at() != Some(ttl_summary.commit_timestamp().saturating_add(ttl)) {
        return Err(TestkitError::new("TTL expiration did not round-trip"));
    }
    outcome.commits += 1;
    outcome.puts += 1;
    outcome.ttl_roundtrips += 1;

    runtime
        .commit(&delete_batch(b"key-a")?)
        .map_err(|error| testkit_error(&error))?;
    outcome.commits += 1;
    outcome.deletes += 1;

    let conflict = put_batch(b"key-a", b"conflict")?
        .with_conditions(vec![CommitCondition::new(
            engine_space()?,
            api_key(b"key-a")?,
            CommitExpectedVersion::Present(CommitVersion::new(99)),
        )])
        .map_err(|error| testkit_error(&error))?;
    let error = runtime
        .commit(&conflict)
        .expect_err("stale expected version should conflict");
    if error.class() != StorageApiErrorClass::Conflict {
        return Err(TestkitError::new(
            "stale expected version was not a conflict",
        ));
    }
    outcome.conditions += 1;
    outcome.conflicts += 1;

    Ok(outcome)
}

pub fn check_storage_api_commit_fault_contract(
    script: &[u8],
) -> Result<StorageApiCommitFaultOutcome, TestkitError> {
    let script = non_empty_script(script);
    let mut outcome = StorageApiCommitFaultOutcome::default();
    match route_from_script(script) {
        COMMIT_FAULT_VALIDATION => check_validation_fault(&mut outcome)?,
        COMMIT_FAULT_CONFLICT => check_conflict_fault(script, &mut outcome)?,
        COMMIT_FAULT_BEFORE_ALLOCATION => check_before_allocation_fault(&mut outcome)?,
        COMMIT_FAULT_AFTER_ALLOCATION => check_after_allocation_fault(&mut outcome)?,
        COMMIT_FAULT_WAL_APPEND => check_wal_append_fault(&mut outcome)?,
        COMMIT_FAULT_FORCED_DURABILITY => check_forced_durability_fault(&mut outcome)?,
        COMMIT_FAULT_BRANCH_APPLY => check_branch_apply_fault(&mut outcome)?,
        COMMIT_FAULT_VISIBILITY_PUBLICATION => {
            check_visibility_publication_fault(&mut outcome)?;
        }
        _ => unreachable!("commit fault route is normalized by route_from_script"),
    }

    Ok(outcome)
}

fn check_validation_fault(outcome: &mut StorageApiCommitFaultOutcome) -> Result<(), TestkitError> {
    let validation = CommitBatch::new(DEFAULT_BRANCH_ID, Vec::new(), CommitOptions::default())
        .expect_err("empty commit batch should fail validation");
    require_class(
        &validation,
        StorageApiErrorClass::InvalidArgument,
        "empty commit batch used the wrong class",
    )?;
    outcome.validation_failures += 1;
    Ok(())
}

fn check_conflict_fault(
    script: &[u8],
    outcome: &mut StorageApiCommitFaultOutcome,
) -> Result<(), TestkitError> {
    let mut runtime = StorageRuntime::open(StorageOpenOptions::default())
        .map_err(|error| testkit_error(&error))?
        .into_runtime();
    runtime
        .commit(&put_batch(b"fault", &[script[0]])?)
        .map_err(|error| testkit_error(&error))?;
    let conflict = put_batch(b"fault", &[script_byte(script, 1)])?
        .with_conditions(vec![CommitCondition::expected_absent(
            engine_space()?,
            api_key(b"fault")?,
        )])
        .map_err(|error| testkit_error(&error))?;
    let conflict_error = runtime
        .commit(&conflict)
        .expect_err("expected-absent condition should conflict");
    require_class(
        &conflict_error,
        StorageApiErrorClass::Conflict,
        "commit conflict used the wrong class",
    )?;
    outcome.conflicts += 1;
    Ok(())
}

fn check_before_allocation_fault(
    outcome: &mut StorageApiCommitFaultOutcome,
) -> Result<(), TestkitError> {
    let error = crate::api::map_commit_error_for_test(
        crate::commit::CommitRuntimeError::timestamp_unavailable_with(
            "timestamp source failed before allocation",
            FaultSource("timestamp source failed"),
        ),
    );
    require_class(
        &error,
        StorageApiErrorClass::Internal,
        "pre-allocation failure used the wrong class",
    )?;
    require_source(&error, "pre-allocation failure lost its source")?;
    outcome.before_allocation_failures += 1;
    Ok(())
}

fn check_after_allocation_fault(
    outcome: &mut StorageApiCommitFaultOutcome,
) -> Result<(), TestkitError> {
    let error = crate::api::map_commit_error_for_test(
        crate::commit::CommitRuntimeError::DurabilityUncertain {
            branch_id: DEFAULT_BRANCH_ID,
            commit_version: CommitVersion::new(1),
            reason: "commit failed after allocation before mutation",
            source: None,
        },
    );
    require_class(
        &error,
        StorageApiErrorClass::AmbiguousCommit,
        "post-allocation failure used the wrong class",
    )?;
    outcome.after_allocation_failures += 1;
    Ok(())
}

fn check_wal_append_fault(outcome: &mut StorageApiCommitFaultOutcome) -> Result<(), TestkitError> {
    let error = crate::api::map_commit_error_for_test(
        crate::commit::CommitRuntimeError::durability_uncertain_with(
            DEFAULT_BRANCH_ID,
            CommitVersion::new(2),
            "WAL append did not complete",
            FaultSource("WAL append failed"),
        ),
    );
    require_ambiguous_with_source(&error, "WAL append failure")?;
    outcome.wal_append_failures += 1;
    Ok(())
}

fn check_forced_durability_fault(
    outcome: &mut StorageApiCommitFaultOutcome,
) -> Result<(), TestkitError> {
    let error = crate::api::map_commit_error_for_test(
        crate::commit::CommitRuntimeError::durability_uncertain_with(
            DEFAULT_BRANCH_ID,
            CommitVersion::new(3),
            "forced durability did not complete",
            FaultSource("forced durability failed"),
        ),
    );
    require_ambiguous_with_source(&error, "forced durability failure")?;
    outcome.forced_durability_uncertainties += 1;
    Ok(())
}

fn check_branch_apply_fault(
    outcome: &mut StorageApiCommitFaultOutcome,
) -> Result<(), TestkitError> {
    let error = crate::api::map_commit_error_for_test(
        crate::commit::CommitRuntimeError::durable_but_not_visible_with(
            DEFAULT_BRANCH_ID,
            CommitVersion::new(4),
            "branch apply failed after durable record",
            FaultSource("branch apply failed"),
        ),
    );
    require_ambiguous_with_source(&error, "branch apply failure")?;
    outcome.branch_apply_failures += 1;
    Ok(())
}

fn check_visibility_publication_fault(
    outcome: &mut StorageApiCommitFaultOutcome,
) -> Result<(), TestkitError> {
    let error = crate::api::map_commit_error_for_test(
        crate::commit::CommitRuntimeError::durable_but_not_visible_with(
            DEFAULT_BRANCH_ID,
            CommitVersion::new(5),
            "visibility publication failed after durable record",
            FaultSource("visibility publication failed"),
        ),
    );
    require_ambiguous_with_source(&error, "visibility publication failure")?;
    outcome.visibility_publication_failures += 1;
    Ok(())
}

fn route_from_script(script: &[u8]) -> u8 {
    let label = String::from_utf8_lossy(script).to_ascii_lowercase();
    if label.contains("validation") {
        COMMIT_FAULT_VALIDATION
    } else if label.contains("conflict") {
        COMMIT_FAULT_CONFLICT
    } else if label.contains("before") {
        COMMIT_FAULT_BEFORE_ALLOCATION
    } else if label.contains("after") {
        COMMIT_FAULT_AFTER_ALLOCATION
    } else if label.contains("wal") {
        COMMIT_FAULT_WAL_APPEND
    } else if label.contains("forced") {
        COMMIT_FAULT_FORCED_DURABILITY
    } else if label.contains("branch") {
        COMMIT_FAULT_BRANCH_APPLY
    } else if label.contains("visibility") {
        COMMIT_FAULT_VISIBILITY_PUBLICATION
    } else {
        script[0] % 8
    }
}

fn require_class(
    error: &StorageApiError,
    expected: StorageApiErrorClass,
    message: &'static str,
) -> Result<(), TestkitError> {
    if error.class() == expected {
        Ok(())
    } else {
        Err(TestkitError::new(message))
    }
}

fn require_source(error: &StorageApiError, message: &'static str) -> Result<(), TestkitError> {
    if error.source().is_some() {
        Ok(())
    } else {
        Err(TestkitError::new(message))
    }
}

fn require_ambiguous_with_source(
    error: &StorageApiError,
    label: &'static str,
) -> Result<(), TestkitError> {
    require_class(
        error,
        StorageApiErrorClass::AmbiguousCommit,
        "ambiguous commit fault used the wrong class",
    )?;
    require_source(error, label)
}

fn put_batch(key: &[u8], value: &[u8]) -> Result<CommitBatch, TestkitError> {
    CommitBatch::new(
        DEFAULT_BRANCH_ID,
        vec![put_mutation(key, value)?],
        CommitOptions::default(),
    )
    .map_err(|error| testkit_error(&error))
}

fn delete_batch(key: &[u8]) -> Result<CommitBatch, TestkitError> {
    CommitBatch::new(
        DEFAULT_BRANCH_ID,
        vec![CommitMutation::Delete {
            storage_space: engine_space()?,
            key: api_key(key)?,
        }],
        CommitOptions::default(),
    )
    .map_err(|error| testkit_error(&error))
}

fn put_mutation(key: &[u8], value: &[u8]) -> Result<CommitMutation, TestkitError> {
    Ok(CommitMutation::Put {
        storage_space: engine_space()?,
        key: api_key(key)?,
        value: StorageValue::new(value.to_vec()),
        ttl: None,
    })
}

fn assert_read_value(
    runtime: &StorageRuntime<'_>,
    key: &[u8],
    expected: &[u8],
) -> Result<(), TestkitError> {
    let row = runtime
        .read_point(&PointReadRequest::new(
            DEFAULT_BRANCH_ID,
            engine_space()?,
            api_key(key)?,
            ReadBound::Latest,
        ))
        .map_err(|error| testkit_error(&error))?
        .row()
        .cloned()
        .ok_or_else(|| TestkitError::new("committed row was not visible"))?;
    if row.value().map(crate::api::StorageValue::as_bytes) != Some(expected) {
        return Err(TestkitError::new("committed row value did not match"));
    }
    Ok(())
}

fn non_empty_script(script: &[u8]) -> &[u8] {
    if script.is_empty() {
        &[0]
    } else {
        script
    }
}

fn script_byte(script: &[u8], index: usize) -> u8 {
    script[index % script.len()]
}

fn engine_space() -> Result<StorageSpaceId, TestkitError> {
    StorageSpaceId::new(vec![ENGINE_STORAGE_SPACE]).map_err(|error| testkit_error(&error))
}

fn api_key(key: &[u8]) -> Result<StorageKey, TestkitError> {
    StorageKey::new(key.to_vec()).map_err(|error| testkit_error(&error))
}

fn testkit_error(error: &crate::api::StorageApiError) -> TestkitError {
    TestkitError::new(error.to_string())
}

#[derive(Debug)]
struct FaultSource(&'static str);

impl fmt::Display for FaultSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl Error for FaultSource {}
