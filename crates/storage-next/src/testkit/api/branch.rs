//! Model-backed API branch contract.

use strata_core_next::{BranchId, CommitVersion, Timestamp};

use crate::api::{
    BranchAction, BranchGeneration, BranchOperation, BranchRequest, BranchStatus, BranchSummary,
    CommitBatch, CommitMutation, CommitOptions, PointReadRequest, ReadBound, StorageApiErrorClass,
    StorageKey, StorageOpenOptions, StorageRuntime, StorageSpaceId, StorageValue,
};
use crate::testkit::TestkitError;

const DEFAULT_BRANCH_ID: BranchId = BranchId::from_bytes([0x01; BranchId::BYTE_LEN]);
const ENGINE_STORAGE_SPACE: u8 = 0x20;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StorageApiBranchModelOutcome {
    scripts: usize,
    creates: usize,
    describes: usize,
    lists: usize,
    fork_current: usize,
    fork_at_version: usize,
    fork_at_timestamp: usize,
    clears: usize,
    deletes: usize,
    recreate_transitions: usize,
    invalid_source_rejections: usize,
    read_checks: usize,
}

impl StorageApiBranchModelOutcome {
    pub const fn scripts(self) -> usize {
        self.scripts
    }

    pub const fn creates(self) -> usize {
        self.creates
    }

    pub const fn describes(self) -> usize {
        self.describes
    }

    pub const fn lists(self) -> usize {
        self.lists
    }

    pub const fn fork_current(self) -> usize {
        self.fork_current
    }

    pub const fn fork_at_version(self) -> usize {
        self.fork_at_version
    }

    pub const fn fork_at_timestamp(self) -> usize {
        self.fork_at_timestamp
    }

    pub const fn clears(self) -> usize {
        self.clears
    }

    pub const fn deletes(self) -> usize {
        self.deletes
    }

    pub const fn recreate_transitions(self) -> usize {
        self.recreate_transitions
    }

    pub const fn invalid_source_rejections(self) -> usize {
        self.invalid_source_rejections
    }

    pub const fn read_checks(self) -> usize {
        self.read_checks
    }
}

pub fn check_storage_api_branch_model_contract(
    script: &[u8],
) -> Result<StorageApiBranchModelOutcome, TestkitError> {
    let script = non_empty_script(script);
    let mut runtime = StorageRuntime::open(StorageOpenOptions::default())
        .map_err(testkit_error)?
        .into_runtime();
    let mut outcome = StorageApiBranchModelOutcome {
        scripts: 1,
        ..StorageApiBranchModelOutcome::default()
    };

    let old_value = vec![b'o', script_byte(script, 0)];
    let other_value = vec![b'x', script_byte(script, 1)];
    let new_value = vec![b'n', script_byte(script, 2)];
    let key = vec![b'b', script_byte(script, 3)];

    seed_branch_model(
        &mut runtime,
        &key,
        &old_value,
        &other_value,
        &new_value,
        &mut outcome,
    )?;
    check_fork_routes(&mut runtime, &key, &old_value, &new_value, &mut outcome)?;
    check_catalog_routes(&mut runtime, &key, &mut outcome)?;
    check_recreate_and_invalid_source(&mut runtime, &mut outcome)?;

    Ok(outcome)
}

fn seed_branch_model(
    runtime: &mut StorageRuntime<'_>,
    key: &[u8],
    old_value: &[u8],
    other_value: &[u8],
    new_value: &[u8],
    outcome: &mut StorageApiBranchModelOutcome,
) -> Result<(), TestkitError> {
    commit_put_at(
        runtime,
        DEFAULT_BRANCH_ID,
        key,
        old_value,
        Timestamp::from_micros(10),
    )?;
    create_branch(runtime, branch_id(0x51), outcome)?;
    commit_put_at(
        runtime,
        branch_id(0x51),
        b"other",
        other_value,
        Timestamp::from_micros(20),
    )?;
    commit_put_at(
        runtime,
        DEFAULT_BRANCH_ID,
        key,
        new_value,
        Timestamp::from_micros(30),
    )
}

fn check_fork_routes(
    runtime: &mut StorageRuntime<'_>,
    key: &[u8],
    old_value: &[u8],
    new_value: &[u8],
    outcome: &mut StorageApiBranchModelOutcome,
) -> Result<(), TestkitError> {
    check_fork_current(runtime, key, new_value, outcome)?;
    check_fork_at_version(runtime, key, old_value, outcome)?;
    check_fork_at_timestamp(runtime, key, old_value, outcome)
}

fn check_fork_current(
    runtime: &mut StorageRuntime<'_>,
    key: &[u8],
    new_value: &[u8],
    outcome: &mut StorageApiBranchModelOutcome,
) -> Result<(), TestkitError> {
    let current_child = branch_id(0x52);
    let current = runtime
        .branch(&request(
            current_child,
            BranchAction::ForkCurrent {
                source: DEFAULT_BRANCH_ID,
            },
            Some(BranchGeneration::new(1)),
        ))
        .map_err(testkit_error)?;
    if current.operation() != BranchOperation::Forked
        || current.fork_version() != Some(CommitVersion::new(3))
    {
        return Err(TestkitError::new("fork-current facts disagreed with model"));
    }
    outcome.fork_current += 1;
    assert_read_value(runtime, current_child, key, Some(new_value), outcome)
}

fn check_fork_at_version(
    runtime: &mut StorageRuntime<'_>,
    key: &[u8],
    old_value: &[u8],
    outcome: &mut StorageApiBranchModelOutcome,
) -> Result<(), TestkitError> {
    let version_child = branch_id(0x53);
    let version = runtime
        .branch(&request(
            version_child,
            BranchAction::ForkAtVersion {
                source: DEFAULT_BRANCH_ID,
                version: CommitVersion::new(2),
            },
            Some(BranchGeneration::new(1)),
        ))
        .map_err(testkit_error)?;
    if version.operation() != BranchOperation::Forked
        || version.fork_version() != Some(CommitVersion::new(2))
    {
        return Err(TestkitError::new(
            "fork-at-version facts disagreed with model",
        ));
    }
    outcome.fork_at_version += 1;
    assert_read_value(runtime, version_child, key, Some(old_value), outcome)
}

fn check_fork_at_timestamp(
    runtime: &mut StorageRuntime<'_>,
    key: &[u8],
    old_value: &[u8],
    outcome: &mut StorageApiBranchModelOutcome,
) -> Result<(), TestkitError> {
    let timestamp_child = branch_id(0x54);
    let timestamp = runtime
        .branch(&request(
            timestamp_child,
            BranchAction::ForkAtTimestamp {
                source: DEFAULT_BRANCH_ID,
                timestamp: Timestamp::from_micros(20),
            },
            Some(BranchGeneration::new(1)),
        ))
        .map_err(testkit_error)?;
    if timestamp.operation() != BranchOperation::Forked
        || timestamp.fork_version() != Some(CommitVersion::new(1))
    {
        return Err(TestkitError::new(
            "fork-at-timestamp facts disagreed with model",
        ));
    }
    outcome.fork_at_timestamp += 1;
    assert_read_value(runtime, timestamp_child, key, Some(old_value), outcome)
}

fn check_catalog_routes(
    runtime: &mut StorageRuntime<'_>,
    key: &[u8],
    outcome: &mut StorageApiBranchModelOutcome,
) -> Result<(), TestkitError> {
    let current_child = branch_id(0x52);
    let described = runtime
        .branch(&request(current_child, BranchAction::Describe, None))
        .map_err(testkit_error)?;
    if described.operation() != BranchOperation::Described {
        return Err(TestkitError::new(
            "describe did not report described operation",
        ));
    }
    outcome.describes += 1;

    let listed = runtime
        .branch(&request(DEFAULT_BRANCH_ID, BranchAction::List, None))
        .map_err(testkit_error)?;
    if listed.operation() != BranchOperation::Listed || listed.branches().len() < 5 {
        return Err(TestkitError::new("list did not surface generated branches"));
    }
    outcome.lists += 1;

    let cleared = runtime
        .branch(&request(
            current_child,
            BranchAction::Clear,
            Some(BranchGeneration::new(1)),
        ))
        .map_err(testkit_error)?;
    if cleared.operation() != BranchOperation::Cleared {
        return Err(TestkitError::new("clear did not report cleared operation"));
    }
    outcome.clears += 1;
    assert_read_value(runtime, current_child, key, None, outcome)
}

fn check_recreate_and_invalid_source(
    runtime: &mut StorageRuntime<'_>,
    outcome: &mut StorageApiBranchModelOutcome,
) -> Result<(), TestkitError> {
    let other = branch_id(0x51);
    let deleted = runtime
        .branch(&request(
            other,
            BranchAction::Delete,
            Some(BranchGeneration::new(1)),
        ))
        .map_err(testkit_error)?;
    if deleted.operation() != BranchOperation::Deleted {
        return Err(TestkitError::new("delete did not report deleted operation"));
    }
    outcome.deletes += 1;

    let recreated = runtime
        .branch(&request(
            other,
            BranchAction::Create,
            Some(BranchGeneration::new(2)),
        ))
        .map_err(testkit_error)?;
    if recreated.generation_before() != Some(BranchGeneration::new(1))
        || recreated.generation_after() != Some(BranchGeneration::new(2))
        || recreated.branch().map(BranchSummary::status) != Some(BranchStatus::Active)
    {
        return Err(TestkitError::new(
            "recreate generation facts disagreed with model",
        ));
    }
    outcome.creates += 1;
    outcome.recreate_transitions += 1;

    let invalid_source = runtime
        .branch(&request(
            branch_id(0x55),
            BranchAction::ForkCurrent {
                source: BranchId::from_bytes([0; BranchId::BYTE_LEN]),
            },
            Some(BranchGeneration::new(1)),
        ))
        .expect_err("zero source branch id should fail");
    if invalid_source.class() != StorageApiErrorClass::InvalidArgument {
        return Err(TestkitError::new(
            "invalid source branch id used the wrong class",
        ));
    }
    outcome.invalid_source_rejections += 1;
    Ok(())
}

fn create_branch(
    runtime: &mut StorageRuntime<'_>,
    branch_id: BranchId,
    outcome: &mut StorageApiBranchModelOutcome,
) -> Result<(), TestkitError> {
    let created = runtime
        .branch(&request(
            branch_id,
            BranchAction::Create,
            Some(BranchGeneration::new(1)),
        ))
        .map_err(testkit_error)?;
    if created.operation() != BranchOperation::Created {
        return Err(TestkitError::new("create did not report created operation"));
    }
    outcome.creates += 1;
    Ok(())
}

fn commit_put_at(
    runtime: &mut StorageRuntime<'_>,
    branch_id: BranchId,
    key: &[u8],
    value: &[u8],
    timestamp: Timestamp,
) -> Result<(), TestkitError> {
    runtime
        .commit_for_test(&put_batch(branch_id, key, value)?, timestamp)
        .map(|_| ())
        .map_err(testkit_error)
}

fn assert_read_value(
    runtime: &StorageRuntime<'_>,
    branch_id: BranchId,
    key: &[u8],
    expected: Option<&[u8]>,
    outcome: &mut StorageApiBranchModelOutcome,
) -> Result<(), TestkitError> {
    let actual = runtime
        .read_point(&PointReadRequest::new(
            branch_id,
            engine_space()?,
            api_key(key)?,
            ReadBound::Latest,
        ))
        .map_err(testkit_error)?
        .row()
        .and_then(|row| row.value().map(|value| value.as_bytes().to_vec()));
    if actual.as_deref() != expected {
        return Err(TestkitError::new("branch read disagreed with model"));
    }
    outcome.read_checks += 1;
    Ok(())
}

fn put_batch(branch_id: BranchId, key: &[u8], value: &[u8]) -> Result<CommitBatch, TestkitError> {
    CommitBatch::new(
        branch_id,
        vec![CommitMutation::Put {
            storage_space: engine_space()?,
            key: api_key(key)?,
            value: StorageValue::new(value.to_vec()),
            ttl: None,
        }],
        CommitOptions::default(),
    )
    .map_err(testkit_error)
}

fn request(
    branch_id: BranchId,
    action: BranchAction,
    generation: Option<BranchGeneration>,
) -> BranchRequest {
    BranchRequest::new(branch_id, action, generation)
}

const fn branch_id(byte: u8) -> BranchId {
    BranchId::from_bytes([byte; BranchId::BYTE_LEN])
}

fn engine_space() -> Result<StorageSpaceId, TestkitError> {
    StorageSpaceId::new(vec![ENGINE_STORAGE_SPACE]).map_err(testkit_error)
}

fn api_key(bytes: &[u8]) -> Result<StorageKey, TestkitError> {
    StorageKey::new(bytes.to_vec()).map_err(testkit_error)
}

fn non_empty_script(script: &[u8]) -> &[u8] {
    if script.is_empty() {
        &[0_u8]
    } else {
        script
    }
}

fn script_byte(script: &[u8], index: usize) -> u8 {
    script[index % script.len()]
}

fn testkit_error(error: impl std::fmt::Display) -> TestkitError {
    TestkitError::new(error.to_string())
}
