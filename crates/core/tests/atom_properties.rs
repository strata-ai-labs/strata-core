//! Property tests for core atoms.

#![deny(unsafe_code)]

use proptest::prelude::*;
use std::{cmp::Ordering, time::Duration};
use strata_core::{BranchId, BranchIdError, CommitVersion, Timestamp};

fn raw_u64_with_boundaries() -> impl Strategy<Value = u64> {
    prop_oneof![
        Just(0),
        Just(1),
        Just(u64::MAX - 1),
        Just(u64::MAX),
        any::<u64>()
    ]
}

fn branch_bytes_with_boundaries() -> impl Strategy<Value = [u8; BranchId::BYTE_LEN]> {
    prop_oneof![
        Just([0; BranchId::BYTE_LEN]),
        Just([u8::MAX; BranchId::BYTE_LEN]),
        any::<[u8; BranchId::BYTE_LEN]>(),
    ]
}

fn invalid_numeric_text() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(String::new()),
        any::<u64>().prop_map(|raw| format!("v{raw}")),
        any::<u64>().prop_map(|raw| format!("{raw}x")),
        any::<u64>().prop_map(|raw| format!("+{raw}")),
        any::<u64>().prop_map(|raw| format!("-{raw}")),
        ((u128::from(u64::MAX) + 1)..=u128::MAX).prop_map(|raw| raw.to_string()),
    ]
}

fn uuid_hex_index(hex_index: usize) -> usize {
    hex_index
        + match hex_index {
            0..=7 => 0,
            8..=11 => 1,
            12..=15 => 2,
            16..=19 => 3,
            _ => 4,
        }
}

fn invalid_branch_text() -> impl Strategy<Value = String> {
    const CANONICAL_BRANCH_ID: &str = "00010203-0405-0607-0809-0a0b0c0d0e0f";

    prop_oneof![
        (0usize..36).prop_map(|len| "0".repeat(len)),
        (37usize..72).prop_map(|len| "0".repeat(len)),
        (0usize..32).prop_map(|hex_index| {
            let mut text = CANONICAL_BRANCH_ID.to_owned();
            let byte_index = uuid_hex_index(hex_index);
            text.replace_range(byte_index..=byte_index, "x");
            text
        }),
        prop_oneof![Just(8usize), Just(13), Just(18), Just(23)].prop_map(|byte_index| {
            let mut text = CANONICAL_BRANCH_ID.to_owned();
            text.replace_range(byte_index..=byte_index, "0");
            text
        }),
    ]
}

proptest! {
    #[test]
    fn commit_version_parse_display_and_serde_round_trip(raw in raw_u64_with_boundaries()) {
        let version = CommitVersion::new(raw);

        prop_assert_eq!(version.as_u64(), raw);
        prop_assert_eq!(
            version.to_string().parse::<CommitVersion>().expect("display should parse"),
            version
        );
        prop_assert_eq!(
            serde_json::from_str::<CommitVersion>(
                &serde_json::to_string(&version).expect("json serialize")
            ).expect("json deserialize"),
            version
        );
        prop_assert_eq!(
            bincode::deserialize::<CommitVersion>(
                &bincode::serialize(&version).expect("binary serialize")
            ).expect("binary deserialize"),
            version
        );
    }

    #[test]
    fn commit_version_ordering_matches_raw_value(
        left in raw_u64_with_boundaries(),
        right in raw_u64_with_boundaries(),
    ) {
        let expected = left.cmp(&right);
        let observed = CommitVersion::new(left).cmp(&CommitVersion::new(right));

        prop_assert_eq!(observed, expected);
    }

    #[test]
    fn commit_version_checked_next_matches_checked_add(raw in raw_u64_with_boundaries()) {
        let expected = raw.checked_add(1).map(CommitVersion::new);

        prop_assert_eq!(CommitVersion::new(raw).checked_next(), expected);
    }

    #[test]
    fn commit_version_rejects_generated_invalid_text(text in invalid_numeric_text()) {
        prop_assert!(text.parse::<CommitVersion>().is_err());
    }

    #[test]
    fn timestamp_parse_display_and_serde_round_trip(raw in raw_u64_with_boundaries()) {
        let timestamp = Timestamp::from_micros(raw);

        prop_assert_eq!(timestamp.as_micros(), raw);
        prop_assert_eq!(
            timestamp.to_string().parse::<Timestamp>().expect("display should parse"),
            timestamp
        );
        prop_assert_eq!(
            serde_json::from_str::<Timestamp>(
                &serde_json::to_string(&timestamp).expect("json serialize")
            ).expect("json deserialize"),
            timestamp
        );
        prop_assert_eq!(
            bincode::deserialize::<Timestamp>(
                &bincode::serialize(&timestamp).expect("binary serialize")
            ).expect("binary deserialize"),
            timestamp
        );
    }

    #[test]
    fn timestamp_ordering_matches_raw_microseconds(
        left in raw_u64_with_boundaries(),
        right in raw_u64_with_boundaries(),
    ) {
        let expected = left.cmp(&right);
        let observed = Timestamp::from_micros(left).cmp(&Timestamp::from_micros(right));

        prop_assert_eq!(observed, expected);
    }

    #[test]
    fn timestamp_duration_since_matches_checked_sub(
        left in raw_u64_with_boundaries(),
        right in raw_u64_with_boundaries(),
    ) {
        let later = Timestamp::from_micros(left);
        let earlier = Timestamp::from_micros(right);

        match left.cmp(&right) {
            Ordering::Less => prop_assert_eq!(later.duration_since(earlier), None),
            Ordering::Equal | Ordering::Greater => {
                prop_assert_eq!(
                    later.duration_since(earlier),
                    Some(Duration::from_micros(left - right))
                );
            }
        }
    }

    #[test]
    fn timestamp_larger_unit_constructors_saturate(raw in raw_u64_with_boundaries()) {
        prop_assert_eq!(
            Timestamp::from_millis(raw).as_micros(),
            raw.saturating_mul(1_000)
        );
        prop_assert_eq!(
            Timestamp::from_secs(raw).as_micros(),
            raw.saturating_mul(1_000_000)
        );
    }

    #[test]
    fn timestamp_duration_since_epoch_preserves_representable_durations(
        micros in raw_u64_with_boundaries(),
    ) {
        prop_assert_eq!(
            Timestamp::from_duration_since_epoch(Duration::from_micros(micros)).as_micros(),
            micros
        );
    }

    #[test]
    fn timestamp_saturating_add_and_sub_match_raw_arithmetic(
        base in raw_u64_with_boundaries(),
        delta in raw_u64_with_boundaries(),
    ) {
        let timestamp = Timestamp::from_micros(base);
        let duration = Duration::from_micros(delta);

        prop_assert_eq!(
            timestamp.saturating_add(duration).as_micros(),
            base.saturating_add(delta)
        );
        prop_assert_eq!(
            timestamp.saturating_sub(duration).as_micros(),
            base.saturating_sub(delta)
        );
    }

    #[test]
    fn timestamp_rejects_generated_invalid_text(text in invalid_numeric_text()) {
        prop_assert!(text.parse::<Timestamp>().is_err());
    }

    #[test]
    fn branch_id_parse_display_and_serde_round_trip(bytes in branch_bytes_with_boundaries()) {
        let branch = BranchId::from_bytes(bytes);

        prop_assert_eq!(*branch.as_bytes(), bytes);
        prop_assert_eq!(
            branch.to_string().parse::<BranchId>().expect("display should parse"),
            branch
        );
        prop_assert_eq!(
            serde_json::from_str::<BranchId>(
                &serde_json::to_string(&branch).expect("json serialize")
            ).expect("json deserialize"),
            branch
        );
        prop_assert_eq!(
            bincode::deserialize::<BranchId>(
                &bincode::serialize(&branch).expect("binary serialize")
            ).expect("binary deserialize"),
            branch
        );
    }

    #[test]
    fn branch_id_uppercase_display_text_round_trips(bytes in branch_bytes_with_boundaries()) {
        let branch = BranchId::from_bytes(bytes);
        let uppercase = branch.to_string().to_ascii_uppercase();

        prop_assert_eq!(BranchId::parse_str(&uppercase).expect("uppercase should parse"), branch);
    }

    #[test]
    fn branch_id_rejects_generated_invalid_text(text in invalid_branch_text()) {
        prop_assert_eq!(BranchId::parse_str(&text), Err(BranchIdError::InvalidText));
    }

    #[test]
    fn branch_id_rejects_generated_invalid_byte_lengths(
        len in prop_oneof![0usize..BranchId::BYTE_LEN, BranchId::BYTE_LEN + 1..64],
    ) {
        let bytes = vec![0; len];

        prop_assert_eq!(
            BranchId::try_from_slice(&bytes),
            Err(BranchIdError::InvalidLength {
                expected: BranchId::BYTE_LEN,
                actual: len,
            })
        );
    }
}
