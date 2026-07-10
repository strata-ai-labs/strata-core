//! Lifecycle mapping for the durable retained-history coverage extension.

use crate::branch::read::BranchTimestampCoverage;
use crate::format::{
    decode_retained_history_extension_payload, decode_retained_history_extension_section,
    encode_retained_history_extension_payload, FormatError, RetainedHistoryExtensionPayload,
    TableManifestExtensionSection,
};
use strata_core::{CommitVersion, Timestamp};

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
        self.to_payload().to_extension_section()
    }

    pub(crate) fn encode(self) -> Vec<u8> {
        encode_retained_history_extension_payload(self.to_payload())
    }

    pub(crate) fn decode(payload: &[u8]) -> Result<Self, FormatError> {
        decode_retained_history_extension_payload(payload).map(Self::from_payload)
    }

    pub(crate) fn from_extension_sections(
        sections: &[TableManifestExtensionSection],
    ) -> Result<Option<Self>, FormatError> {
        decode_retained_history_extension_section(sections)
            .map(|payload| payload.map(Self::from_payload))
    }

    pub(crate) fn to_timestamp_coverage(self) -> BranchTimestampCoverage {
        match self.retained_timestamp_floor {
            Some(floor) => BranchTimestampCoverage::complete_since(floor),
            None => BranchTimestampCoverage::Complete,
        }
    }

    fn to_payload(self) -> RetainedHistoryExtensionPayload {
        RetainedHistoryExtensionPayload::new(
            self.retained_version_floor,
            self.retained_timestamp_floor,
        )
    }

    fn from_payload(payload: RetainedHistoryExtensionPayload) -> Self {
        Self {
            retained_version_floor: payload.retained_version_floor(),
            retained_timestamp_floor: payload.retained_timestamp_floor(),
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
        let mut payload = vec![0u8; crate::format::RETAINED_HISTORY_EXTENSION_PAYLOAD_LEN];
        payload[8] = 0xff;
        assert!(matches!(
            RetainedHistoryFacts::decode(&payload),
            Err(FormatError::InvalidValue { .. })
        ));
    }

    #[test]
    fn decode_rejects_nonzero_reserved_bytes() {
        let mut payload = vec![0u8; crate::format::RETAINED_HISTORY_EXTENSION_PAYLOAD_LEN];
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
