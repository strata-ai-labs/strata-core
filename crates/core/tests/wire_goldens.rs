//! Golden wire-format vectors for the three durable atoms (TCP3.1).
//!
//! Every serde test before this slice was a round-trip: encode and decode
//! share the implementation, so a representation change (e.g. `BranchId`'s
//! custom `serialize_bytes` becoming a tuple) passes every round-trip while
//! silently changing the durable encoding. These vectors pin the actual
//! bytes in both directions, at canonical and boundary values — the atom
//! layer's share of hard rules #13 (durable format frozen, golden-gated)
//! and #35 (newtype wire stability).
//!
//! The binary vectors are bincode 1.x with default options (fixint, little
//! endian) — the same configuration the existing round-trip tests exercise.

use strata_core::{BranchId, CommitVersion, Timestamp};

const BRANCH_BYTES: [u8; 16] = [
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
];
const BRANCH_TEXT: &str = "00010203-0405-0607-0809-0a0b0c0d0e0f";

/// `bincode(serialize_bytes)` = u64 little-endian length prefix + raw bytes.
const BRANCH_BINCODE: [u8; 24] = [
    16, 0, 0, 0, 0, 0, 0, 0, // length prefix
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
];

#[test]
fn branch_id_json_encoding_is_canonical_uuid_text() {
    let branch = BranchId::from_bytes(BRANCH_BYTES);
    assert_eq!(
        serde_json::to_string(&branch).expect("serialize"),
        format!("\"{BRANCH_TEXT}\"")
    );
    assert_eq!(
        serde_json::from_str::<BranchId>(&format!("\"{BRANCH_TEXT}\"")).expect("deserialize"),
        branch
    );
}

#[test]
fn branch_id_bincode_encoding_is_length_prefixed_raw_bytes() {
    let branch = BranchId::from_bytes(BRANCH_BYTES);
    assert_eq!(
        bincode::serialize(&branch).expect("serialize").as_slice(),
        BRANCH_BINCODE
    );
    assert_eq!(
        bincode::deserialize::<BranchId>(&BRANCH_BINCODE).expect("deserialize"),
        branch
    );
}

#[test]
fn branch_id_boundary_values_hold_their_encodings() {
    // The all-zero and all-ff ids are palindromes: they cannot detect a
    // byte-order change on their own, so the asymmetric third vector below
    // carries that weight (it is what actually fails when encode/decode
    // reverse together — the change every round-trip test sails through).
    for (bytes, text) in [
        ([0u8; 16], "00000000-0000-0000-0000-000000000000"),
        ([0xffu8; 16], "ffffffff-ffff-ffff-ffff-ffffffffffff"),
        (
            [
                0xff, 0xee, 0xdd, 0xcc, 0xbb, 0xaa, 0x99, 0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22,
                0x11, 0x00,
            ],
            "ffeeddcc-bbaa-9988-7766-554433221100",
        ),
    ] {
        let branch = BranchId::from_bytes(bytes);
        assert_eq!(branch.to_string(), text);
        assert_eq!(
            serde_json::to_string(&branch).expect("serialize"),
            format!("\"{text}\"")
        );
        let binary = bincode::serialize(&branch).expect("serialize");
        assert_eq!(&binary[..8], 16u64.to_le_bytes());
        assert_eq!(&binary[8..], bytes);
    }
}

#[test]
fn commit_version_wire_is_a_transparent_u64() {
    for (value, json) in [(0u64, "0"), (42, "42"), (u64::MAX, "18446744073709551615")] {
        let binary = value.to_le_bytes();
        let version = CommitVersion::new(value);
        assert_eq!(serde_json::to_string(&version).expect("serialize"), json);
        assert_eq!(
            serde_json::from_str::<CommitVersion>(json).expect("deserialize"),
            version
        );
        assert_eq!(
            bincode::serialize(&version).expect("serialize").as_slice(),
            binary
        );
        assert_eq!(
            bincode::deserialize::<CommitVersion>(&binary).expect("deserialize"),
            version
        );
    }
}

#[test]
fn timestamp_wire_is_a_transparent_u64_of_micros() {
    for (value, json) in [
        (0u64, "0"),
        (1_700_000_000_000_000, "1700000000000000"),
        (u64::MAX, "18446744073709551615"),
    ] {
        let timestamp = Timestamp::from_micros(value);
        assert_eq!(serde_json::to_string(&timestamp).expect("serialize"), json);
        assert_eq!(
            serde_json::from_str::<Timestamp>(json).expect("deserialize"),
            timestamp
        );
        let binary = value.to_le_bytes();
        assert_eq!(
            bincode::serialize(&timestamp)
                .expect("serialize")
                .as_slice(),
            binary
        );
        assert_eq!(
            bincode::deserialize::<Timestamp>(&binary).expect("deserialize"),
            timestamp
        );
    }
}

/// A truncated binary buffer must fail to decode, never zero-fill.
#[test]
fn truncated_binary_buffers_are_rejected() {
    assert!(bincode::deserialize::<BranchId>(&BRANCH_BINCODE[..23]).is_err());
    assert!(bincode::deserialize::<CommitVersion>(&[1, 2, 3]).is_err());
    assert!(bincode::deserialize::<Timestamp>(&[1, 2, 3, 4, 5, 6, 7]).is_err());
}
