use super::{FormatError, TableManifestExtensionSection};
use strata_core_next::{CommitVersion, Timestamp};

pub(crate) const RETAINED_HISTORY_EXTENSION_KIND: &str = "storage.retained_history";
pub(crate) const RETAINED_HISTORY_EXTENSION_PAYLOAD_LEN: usize = 24;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RetainedHistoryExtensionPayload {
    retained_version_floor: CommitVersion,
    retained_timestamp_floor: Option<Timestamp>,
}

impl RetainedHistoryExtensionPayload {
    pub(crate) const fn new(
        retained_version_floor: CommitVersion,
        retained_timestamp_floor: Option<Timestamp>,
    ) -> Self {
        Self {
            retained_version_floor,
            retained_timestamp_floor,
        }
    }

    pub(crate) const fn retained_version_floor(self) -> CommitVersion {
        self.retained_version_floor
    }

    pub(crate) const fn retained_timestamp_floor(self) -> Option<Timestamp> {
        self.retained_timestamp_floor
    }

    pub(crate) fn to_extension_section(self) -> Result<TableManifestExtensionSection, FormatError> {
        TableManifestExtensionSection::optional(
            RETAINED_HISTORY_EXTENSION_KIND,
            true,
            encode_retained_history_extension_payload(self),
        )
    }
}

pub(crate) fn encode_retained_history_extension_payload(
    payload: RetainedHistoryExtensionPayload,
) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(RETAINED_HISTORY_EXTENSION_PAYLOAD_LEN);
    bytes.extend_from_slice(&payload.retained_version_floor.as_u64().to_le_bytes());
    if let Some(timestamp) = payload.retained_timestamp_floor {
        bytes.push(1);
        bytes.extend_from_slice(&timestamp.as_micros().to_le_bytes());
    } else {
        bytes.push(0);
        bytes.extend_from_slice(&0u64.to_le_bytes());
    }
    bytes.extend_from_slice(&[0u8; 7]);
    debug_assert_eq!(bytes.len(), RETAINED_HISTORY_EXTENSION_PAYLOAD_LEN);
    bytes
}

pub(crate) fn decode_retained_history_extension_payload(
    bytes: &[u8],
) -> Result<RetainedHistoryExtensionPayload, FormatError> {
    if bytes.len() != RETAINED_HISTORY_EXTENSION_PAYLOAD_LEN {
        return Err(FormatError::InvalidLength {
            field: "retained_history_extension_payload",
        });
    }
    let mut version_bytes = [0u8; 8];
    version_bytes.copy_from_slice(&bytes[0..8]);
    let retained_version_floor = CommitVersion::new(u64::from_le_bytes(version_bytes));
    let timestamp_flag = bytes[8];
    let mut timestamp_bytes = [0u8; 8];
    timestamp_bytes.copy_from_slice(&bytes[9..17]);
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
    if bytes[17..RETAINED_HISTORY_EXTENSION_PAYLOAD_LEN]
        .iter()
        .any(|byte| *byte != 0)
    {
        return Err(FormatError::InvalidValue {
            field: "retained_history_reserved_bytes",
        });
    }
    Ok(RetainedHistoryExtensionPayload {
        retained_version_floor,
        retained_timestamp_floor,
    })
}

pub(crate) fn decode_retained_history_extension_section(
    sections: &[TableManifestExtensionSection],
) -> Result<Option<RetainedHistoryExtensionPayload>, FormatError> {
    for section in sections {
        if section.kind() == RETAINED_HISTORY_EXTENSION_KIND {
            return decode_retained_history_extension_payload(section.payload()).map(Some);
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_history_extension_round_trips_with_timestamp_floor() {
        let payload = RetainedHistoryExtensionPayload::new(
            CommitVersion::new(42),
            Some(Timestamp::from_micros(9_001)),
        );
        let encoded = encode_retained_history_extension_payload(payload);
        let mut expected = Vec::new();
        expected.extend_from_slice(&42u64.to_le_bytes());
        expected.push(1);
        expected.extend_from_slice(&9_001u64.to_le_bytes());
        expected.extend_from_slice(&[0u8; 7]);

        assert_eq!(encoded.len(), RETAINED_HISTORY_EXTENSION_PAYLOAD_LEN);
        assert_eq!(encoded, expected);
        assert_eq!(
            decode_retained_history_extension_payload(&encoded),
            Ok(payload)
        );
    }

    #[test]
    fn retained_history_extension_round_trips_without_timestamp_floor() {
        let payload = RetainedHistoryExtensionPayload::new(CommitVersion::new(5), None);
        let encoded = encode_retained_history_extension_payload(payload);

        assert_eq!(
            encoded,
            vec![
                5, 0, 0, 0, 0, 0, 0, 0, // retained version floor
                0, // timestamp absent
                0, 0, 0, 0, 0, 0, 0, 0, // ignored timestamp bytes
                0, 0, 0, 0, 0, 0, 0, // reserved
            ]
        );
        assert_eq!(encoded[8], 0);
        assert_eq!(&encoded[9..17], &0u64.to_le_bytes());
        assert_eq!(
            decode_retained_history_extension_payload(&encoded),
            Ok(payload)
        );
    }

    #[test]
    fn retained_history_extension_builds_preserved_optional_section() {
        let payload = RetainedHistoryExtensionPayload::new(CommitVersion::new(5), None);
        let section = payload.to_extension_section().expect("extension");

        assert_eq!(section.kind(), RETAINED_HISTORY_EXTENSION_KIND);
        assert!(section.preserve_on_rewrite());
        assert_eq!(
            decode_retained_history_extension_payload(section.payload()),
            Ok(payload)
        );
    }

    #[test]
    fn retained_history_extension_decode_rejects_short_payload() {
        assert!(matches!(
            decode_retained_history_extension_payload(&[0; 10]),
            Err(FormatError::InvalidLength { .. })
        ));
    }

    #[test]
    fn retained_history_extension_decode_rejects_long_payload() {
        assert!(matches!(
            decode_retained_history_extension_payload(
                &[0; RETAINED_HISTORY_EXTENSION_PAYLOAD_LEN + 1]
            ),
            Err(FormatError::InvalidLength { .. })
        ));
    }

    #[test]
    fn retained_history_extension_decode_rejects_unknown_timestamp_flag() {
        let mut payload = vec![0u8; RETAINED_HISTORY_EXTENSION_PAYLOAD_LEN];
        payload[8] = 0xff;

        assert!(matches!(
            decode_retained_history_extension_payload(&payload),
            Err(FormatError::InvalidValue {
                field: "retained_history_timestamp_flag"
            })
        ));
    }

    #[test]
    fn retained_history_extension_decode_rejects_nonzero_reserved_bytes() {
        let mut payload = vec![0u8; RETAINED_HISTORY_EXTENSION_PAYLOAD_LEN];
        payload[8] = 1;
        payload[20] = 0x55;

        assert!(matches!(
            decode_retained_history_extension_payload(&payload),
            Err(FormatError::InvalidValue {
                field: "retained_history_reserved_bytes"
            })
        ));
    }

    #[test]
    fn retained_history_extension_decode_accepts_ignored_timestamp_when_flag_is_absent() {
        let mut payload = vec![0u8; RETAINED_HISTORY_EXTENSION_PAYLOAD_LEN];
        payload[0..8].copy_from_slice(&7u64.to_le_bytes());
        payload[8] = 0;
        payload[9..17].copy_from_slice(&99u64.to_le_bytes());

        assert_eq!(
            decode_retained_history_extension_payload(&payload),
            Ok(RetainedHistoryExtensionPayload::new(
                CommitVersion::new(7),
                None
            ))
        );
    }
}
