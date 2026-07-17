//! Hash/Eq consistency and untested arithmetic boundaries (TCP3.1).
//!
//! `BranchId` and `CommitVersion` are documented `Hash` atoms used as map
//! keys across storage and engine; nothing asserted the Eq/Hash contract or
//! key behavior before this slice. The `Timestamp` saturation fallback
//! (`unwrap_or(u64::MAX)`) and unit-truncation directions were likewise
//! unexercised.

use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasher, RandomState};
use std::time::Duration;

use strata_core::{BranchId, CommitVersion, Timestamp};

fn hash_of<T: std::hash::Hash>(value: &T, state: &RandomState) -> u64 {
    state.hash_one(value)
}

#[test]
fn equal_atoms_hash_equal() {
    let state = RandomState::new();
    let branch_a = BranchId::from_bytes([7; 16]);
    let branch_b = BranchId::try_from_slice(&[7; 16]).expect("valid slice");
    assert_eq!(branch_a, branch_b);
    assert_eq!(hash_of(&branch_a, &state), hash_of(&branch_b, &state));

    let version_a = CommitVersion::new(99);
    let version_b = "99".parse::<CommitVersion>().expect("parse");
    assert_eq!(version_a, version_b);
    assert_eq!(hash_of(&version_a, &state), hash_of(&version_b, &state));

    let time_a = Timestamp::from_micros(5_000_000);
    let time_b = Timestamp::from_secs(5);
    assert_eq!(time_a, time_b);
    assert_eq!(hash_of(&time_a, &state), hash_of(&time_b, &state));
}

#[test]
fn atoms_behave_as_map_and_set_keys() {
    let mut branches: HashMap<BranchId, &str> = HashMap::new();
    let branch = BranchId::from_bytes([3; 16]);
    branches.insert(branch, "first");
    branches.insert(BranchId::from_bytes([3; 16]), "second");
    assert_eq!(branches.len(), 1, "equal ids must collide to one entry");
    assert_eq!(branches[&branch], "second");

    let mut versions: HashSet<CommitVersion> = HashSet::new();
    versions.insert(CommitVersion::new(1));
    versions.insert(CommitVersion::new(1));
    versions.insert(CommitVersion::new(2));
    assert_eq!(versions.len(), 2);
    assert!(versions.contains(&"1".parse::<CommitVersion>().expect("parse")));
}

/// The `unwrap_or(u64::MAX)` fallback fires only when a `Duration` holds
/// more microseconds than u64 can carry — the branch the property suite's
/// `Duration::from_micros(u64)` strategy can never reach. A wrong fallback
/// (e.g. `unwrap_or(0)`) would flip the saturation direction silently.
#[test]
fn duration_overflow_saturates_toward_the_bound() {
    let overflowing = Duration::new(u64::MAX, 999_999_999);
    assert!(u64::try_from(overflowing.as_micros()).is_err(), "premise");

    assert_eq!(
        Timestamp::EPOCH.saturating_add(overflowing),
        Timestamp::MAX,
        "adding an overflowing duration saturates to MAX"
    );
    assert_eq!(
        Timestamp::MAX.saturating_sub(overflowing),
        Timestamp::EPOCH,
        "subtracting an overflowing duration saturates to EPOCH"
    );
    assert_eq!(
        Timestamp::from_micros(123).saturating_sub(overflowing),
        Timestamp::EPOCH,
        "overflowing subtraction from a small timestamp reaches the floor"
    );
}

#[test]
fn unit_accessors_truncate_toward_zero_on_non_round_values() {
    let timestamp = Timestamp::from_micros(1_999_999);
    assert_eq!(timestamp.as_millis(), 1_999);
    assert_eq!(timestamp.as_secs(), 1);

    let just_under = Timestamp::from_micros(999);
    assert_eq!(just_under.as_millis(), 0);
    assert_eq!(just_under.as_secs(), 0);
}
