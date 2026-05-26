//! Encoding for the durable retained-history coverage extension.
//!
//! When row pruning narrows a branch's `BranchTimestampCoverage` to
//! `CompleteSince(floor)`, the lifecycle table-manifest writer emits an
//! optional `TableManifestExtensionSection` (kind = `EXTENSION_KIND`)
//! recording the retained version and timestamp floors so reopening the
//! branch can restore the narrowed coverage rather than silently widening
//! history.
//!
//! Wire format (24 bytes):
//!
//! | offset | width | field                                |
//! |--------|-------|--------------------------------------|
//! | 0      | 8 LE  | `retained_version_floor` (u64)       |
//! | 8      | 1     | timestamp floor flag (0=None, 1=Some)|
//! | 9      | 8 LE  | `retained_timestamp_floor` (micros)  |
//! | 17     | 7     | reserved (must be zero)              |
//!
//! `preserve_on_rewrite` is set so subsequent rewrites carry the floor
//! forward until something explicitly overrides it.

use crate::branch::BranchTimestampCoverage;
use crate::format::{FormatError, TableManifestExtensionSection};
use strata_core_next::{CommitVersion, Timestamp};

pub(crate) const EXTENSION_KIND: &str = "storage.retained_history";

pub(crate) const PAYLOAD_LEN: usize = 24;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RetainedHistoryFacts {
    pub(crate) retained_version_floor: CommitVersion,
    pub(crate) retained_timestamp_floor: Option<Timestamp>,
}

impl RetainedHistoryFacts {
    pub(crate) fn from_timestamp_coverage(
        coverage: BranchTimestampCoverage,
        retained_version_floor: CommitVersion,
    ) -> Option<Self> {
        match coverage {
            BranchTimestampCoverage::CompleteSince { earliest_timestamp } => Some(Self {
                retained_version_floor,
                retained_timestamp_floor: Some(earliest_timestamp),
            }),
            BranchTimestampCoverage::Complete | BranchTimestampCoverage::Unknown => None,
        }
    }

    pub(crate) fn to_extension_section(self) -> Result<TableManifestExtensionSection, FormatError> {
        TableManifestExtensionSection::optional(EXTENSION_KIND, true, self.encode())
    }

    pub(crate) fn encode(self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(PAYLOAD_LEN);
        bytes.extend_from_slice(&self.retained_version_floor.as_u64().to_le_bytes());
        if let Some(timestamp) = self.retained_timestamp_floor {
            bytes.push(1);
            bytes.extend_from_slice(&timestamp.as_micros().to_le_bytes());
        } else {
            bytes.push(0);
            bytes.extend_from_slice(&0u64.to_le_bytes());
        }
        bytes.extend_from_slice(&[0u8; 7]);
        debug_assert_eq!(bytes.len(), PAYLOAD_LEN);
        bytes
    }

    pub(crate) fn decode(payload: &[u8]) -> Result<Self, FormatError> {
        if payload.len() != PAYLOAD_LEN {
            return Err(FormatError::InvalidLength {
                field: "retained_history_extension_payload",
            });
        }
        let mut version_bytes = [0u8; 8];
        version_bytes.copy_from_slice(&payload[0..8]);
        let retained_version_floor = CommitVersion::new(u64::from_le_bytes(version_bytes));
        let timestamp_flag = payload[8];
        let mut timestamp_bytes = [0u8; 8];
        timestamp_bytes.copy_from_slice(&payload[9..17]);
        let timestamp_micros = u64::from_le_bytes(timestamp_bytes);
        let retained_timestamp_floor = match timestamp_flag {
            0 => None,
            1 => Some(Timestamp::from_micros(timestamp_micros)),
            _ => {
                return Err(FormatError::InvalidValue {
                    field: "retained_history_timestamp_flag",
                });
            }
        };
        if payload[17..PAYLOAD_LEN].iter().any(|byte| *byte != 0) {
            return Err(FormatError::InvalidValue {
                field: "retained_history_reserved_bytes",
            });
        }
        Ok(Self {
            retained_version_floor,
            retained_timestamp_floor,
        })
    }

    pub(crate) fn from_extension_sections(
        sections: &[TableManifestExtensionSection],
    ) -> Result<Option<Self>, FormatError> {
        for section in sections {
            let kind: &str = section.kind();
            if kind == EXTENSION_KIND {
                return Self::decode(section.payload()).map(Some);
            }
        }
        Ok(None)
    }

    pub(crate) fn to_timestamp_coverage(self) -> BranchTimestampCoverage {
        match self.retained_timestamp_floor {
            Some(floor) => BranchTimestampCoverage::complete_since(floor),
            None => BranchTimestampCoverage::Complete,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_with_timestamp_floor() {
        let facts = RetainedHistoryFacts {
            retained_version_floor: CommitVersion::new(42),
            retained_timestamp_floor: Some(Timestamp::from_micros(9_001)),
        };
        let encoded = facts.encode();
        let decoded = RetainedHistoryFacts::decode(&encoded).expect("decode");
        assert_eq!(decoded, facts);
    }

    #[test]
    fn round_trip_without_timestamp_floor() {
        let facts = RetainedHistoryFacts {
            retained_version_floor: CommitVersion::new(5),
            retained_timestamp_floor: None,
        };
        let encoded = facts.encode();
        let decoded = RetainedHistoryFacts::decode(&encoded).expect("decode");
        assert_eq!(decoded, facts);
    }

    #[test]
    fn decode_rejects_short_payload() {
        assert!(matches!(
            RetainedHistoryFacts::decode(&[0; 10]),
            Err(FormatError::InvalidLength { .. })
        ));
    }

    #[test]
    fn decode_rejects_unknown_flag() {
        let mut payload = vec![0u8; PAYLOAD_LEN];
        payload[8] = 0xff;
        assert!(matches!(
            RetainedHistoryFacts::decode(&payload),
            Err(FormatError::InvalidValue { .. })
        ));
    }

    #[test]
    fn decode_rejects_nonzero_reserved_bytes() {
        let mut payload = vec![0u8; PAYLOAD_LEN];
        payload[8] = 1;
        payload[20] = 0x55;
        assert!(matches!(
            RetainedHistoryFacts::decode(&payload),
            Err(FormatError::InvalidValue { .. })
        ));
    }

    #[test]
    fn from_timestamp_coverage_emits_when_narrowed() {
        let coverage = BranchTimestampCoverage::complete_since(Timestamp::from_micros(100));
        let facts = RetainedHistoryFacts::from_timestamp_coverage(coverage, CommitVersion::new(5))
            .expect("present");
        assert_eq!(facts.retained_version_floor, CommitVersion::new(5));
        assert_eq!(
            facts.retained_timestamp_floor,
            Some(Timestamp::from_micros(100))
        );
    }

    #[test]
    fn from_timestamp_coverage_skips_complete() {
        assert!(RetainedHistoryFacts::from_timestamp_coverage(
            BranchTimestampCoverage::Complete,
            CommitVersion::new(0)
        )
        .is_none());
    }

    #[test]
    fn from_timestamp_coverage_skips_unknown() {
        assert!(RetainedHistoryFacts::from_timestamp_coverage(
            BranchTimestampCoverage::Unknown,
            CommitVersion::new(0)
        )
        .is_none());
    }
}
