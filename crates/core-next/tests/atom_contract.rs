//! Contract tests for core-next atoms.

#![deny(unsafe_code)]

use std::error::Error;
use strata_core_next as _;
use strata_core_next::{
    BranchId, BranchIdError, CommitVersion, ParseCommitVersionError, ParseTimestampError, Timestamp,
};

#[test]
fn crate_is_available_to_integration_tests() {
    assert_eq!(env!("CARGO_PKG_NAME"), "strata-core-next");
}

#[test]
fn commit_version_is_public_contract_atom() {
    let version = CommitVersion::new(5);

    assert_eq!(version.as_u64(), 5);
    assert_eq!(version.to_string(), "5");
    assert_eq!(
        "5".parse::<CommitVersion>().expect("valid version"),
        version
    );
    assert_eq!(
        serde_json::to_string(&version).expect("serialize version"),
        "5"
    );
}

#[test]
fn timestamp_is_public_contract_atom() {
    let timestamp = Timestamp::from_micros(5);

    assert_eq!(timestamp.as_micros(), 5);
    assert_eq!(timestamp.to_string(), "5");
    assert_eq!(
        "5".parse::<Timestamp>().expect("valid timestamp"),
        timestamp
    );
    assert_eq!(
        serde_json::to_string(&timestamp).expect("serialize timestamp"),
        "5"
    );
}

#[test]
fn branch_id_is_public_contract_atom() {
    let branch = BranchId::from_bytes([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]);

    assert_eq!(
        *branch.as_bytes(),
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]
    );
    assert_eq!(branch.to_string(), "00010203-0405-0607-0809-0a0b0c0d0e0f");
    assert_eq!(
        BranchId::parse_str("00010203-0405-0607-0809-0A0B0C0D0E0F").expect("valid branch id"),
        branch
    );
    assert_eq!(
        serde_json::to_string(&branch).expect("serialize branch id"),
        "\"00010203-0405-0607-0809-0a0b0c0d0e0f\""
    );
}

#[test]
fn branch_id_validation_error_is_public_contract() {
    let err = BranchId::try_from_slice(&[0; BranchId::BYTE_LEN - 1])
        .expect_err("short branch id should fail");

    assert_eq!(
        err.to_string(),
        "invalid branch id length: expected 16 bytes, got 15"
    );
    assert!(err.source().is_none());
}

#[test]
fn branch_id_text_validation_error_is_public_contract() {
    let err = BranchId::parse_str("not-a-branch-id").expect_err("invalid text should fail");

    assert_eq!(err, BranchIdError::InvalidText);
    assert_eq!(err.to_string(), "invalid branch id text");
    assert!(err.source().is_none());
}

#[test]
fn commit_version_parse_error_is_public_contract() {
    let err = "v5"
        .parse::<CommitVersion>()
        .expect_err("decorated version should fail");

    let _: &ParseCommitVersionError = &err;
    assert_eq!(err.to_string(), "invalid commit version text");
    assert!(err.source().is_some());

    let signed_err = "+5"
        .parse::<CommitVersion>()
        .expect_err("signed version should fail");
    assert_eq!(signed_err.to_string(), "invalid commit version text");
    assert!(signed_err.source().is_none());
}

#[test]
fn timestamp_parse_error_is_public_contract() {
    let err = "5.0"
        .parse::<Timestamp>()
        .expect_err("decorated timestamp should fail");

    let _: &ParseTimestampError = &err;
    assert_eq!(err.to_string(), "invalid timestamp text");
    assert!(err.source().is_some());

    let signed_err = "+5"
        .parse::<Timestamp>()
        .expect_err("signed timestamp should fail");
    assert_eq!(signed_err.to_string(), "invalid timestamp text");
    assert!(signed_err.source().is_none());
}
