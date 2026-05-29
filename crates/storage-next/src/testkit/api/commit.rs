//! Model-backed API commit contracts.

use std::time::Duration;

use strata_core_next::{BranchId, CommitVersion};

use crate::api::{
    CommitBatch, CommitCondition, CommitDurability, CommitDurabilitySummary, CommitExpectedVersion,
    CommitMutation, CommitOptions, PointReadRequest, ReadBound, StorageApiErrorClass, StorageKey,
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
    unsupported_durability: usize,
    closed_runtime_rejections: usize,
    ambiguous_commit_examples: usize,
}

impl StorageApiCommitFaultOutcome {
    pub const fn validation_failures(self) -> usize {
        self.validation_failures
    }

    pub const fn conflicts(self) -> usize {
        self.conflicts
    }

    pub const fn unsupported_durability(self) -> usize {
        self.unsupported_durability
    }

    pub const fn closed_runtime_rejections(self) -> usize {
        self.closed_runtime_rejections
    }

    pub const fn ambiguous_commit_examples(self) -> usize {
        self.ambiguous_commit_examples
    }
}

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

    let validation = CommitBatch::new(DEFAULT_BRANCH_ID, Vec::new(), CommitOptions::default())
        .expect_err("empty commit batch should fail validation");
    if validation.class() != StorageApiErrorClass::InvalidArgument {
        return Err(TestkitError::new("empty commit batch used the wrong class"));
    }
    outcome.validation_failures += 1;

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
    if conflict_error.class() != StorageApiErrorClass::Conflict {
        return Err(TestkitError::new("commit conflict used the wrong class"));
    }
    outcome.conflicts += 1;

    let durable = CommitBatch::new(
        DEFAULT_BRANCH_ID,
        vec![put_mutation(b"durable", &[script_byte(script, 2)])?],
        CommitOptions::default().with_durability(CommitDurability::Standard),
    )
    .map_err(|error| testkit_error(&error))?;
    let unsupported = runtime
        .commit(&durable)
        .expect_err("cache runtime cannot satisfy durable commit");
    if unsupported.class() != StorageApiErrorClass::Unsupported {
        return Err(TestkitError::new(
            "unsupported durability used the wrong class",
        ));
    }
    outcome.unsupported_durability += 1;

    runtime.close().map_err(|error| testkit_error(&error))?;
    let closed = runtime
        .commit(&put_batch(b"closed", &[script_byte(script, 3)])?)
        .expect_err("closed runtime should reject commits");
    if closed.class() != StorageApiErrorClass::FailedPrecondition {
        return Err(TestkitError::new("closed commit used the wrong class"));
    }
    outcome.closed_runtime_rejections += 1;

    let ambiguous = crate::api::StorageApiError::DurableUncertain {
        reason: "durability is uncertain",
    };
    if ambiguous.class() != StorageApiErrorClass::AmbiguousCommit {
        return Err(TestkitError::new(
            "durability uncertainty used the wrong class",
        ));
    }
    outcome.ambiguous_commit_examples += 1;

    Ok(outcome)
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
