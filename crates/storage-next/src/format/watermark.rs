use super::{ByteReader, FormatError};
use strata_core_next::{CommitVersion, Timestamp};

const SNAPSHOT_WATERMARK_FORMAT: &str = "snapshot_watermark";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SnapshotWatermark {
    Empty,
    Present {
        snapshot_id: u64,
        watermark_commit_version: CommitVersion,
        updated_at: Timestamp,
    },
}

impl SnapshotWatermark {
    pub(crate) fn present(
        snapshot_id: u64,
        watermark_commit_version: CommitVersion,
        updated_at: Timestamp,
    ) -> Result<Self, FormatError> {
        if snapshot_id == 0 {
            return Err(FormatError::InvalidValue {
                field: "snapshot_id",
            });
        }
        Ok(Self::Present {
            snapshot_id,
            watermark_commit_version,
            updated_at,
        })
    }

    pub(crate) const fn snapshot_id(self) -> Option<u64> {
        match self {
            Self::Empty => None,
            Self::Present { snapshot_id, .. } => Some(snapshot_id),
        }
    }

    pub(crate) const fn watermark_commit_version(self) -> Option<CommitVersion> {
        match self {
            Self::Empty => None,
            Self::Present {
                watermark_commit_version,
                ..
            } => Some(watermark_commit_version),
        }
    }

    pub(crate) const fn updated_at(self) -> Option<Timestamp> {
        match self {
            Self::Empty => None,
            Self::Present { updated_at, .. } => Some(updated_at),
        }
    }

    pub(crate) const fn next_snapshot_id(self) -> u64 {
        match self {
            Self::Empty => 1,
            Self::Present { snapshot_id, .. } => snapshot_id.saturating_add(1),
        }
    }
}

pub(crate) fn encode_snapshot_watermark(
    watermark: SnapshotWatermark,
) -> Result<Vec<u8>, FormatError> {
    match watermark {
        // A one-byte empty encoding keeps the absence of a durable snapshot
        // distinct from a present snapshot with invalid zero facts.
        SnapshotWatermark::Empty => Ok(vec![0]),
        SnapshotWatermark::Present {
            snapshot_id,
            watermark_commit_version,
            updated_at,
        } => {
            if snapshot_id == 0 {
                return Err(FormatError::InvalidValue {
                    field: "snapshot_id",
                });
            }
            let mut bytes = Vec::with_capacity(25);
            bytes.push(1);
            bytes.extend_from_slice(&snapshot_id.to_le_bytes());
            bytes.extend_from_slice(&watermark_commit_version.as_u64().to_le_bytes());
            bytes.extend_from_slice(&updated_at.as_micros().to_le_bytes());
            Ok(bytes)
        }
    }
}

pub(crate) fn decode_snapshot_watermark(bytes: &[u8]) -> Result<SnapshotWatermark, FormatError> {
    let mut reader = ByteReader::new(SNAPSHOT_WATERMARK_FORMAT, bytes);
    let has_data = reader.read_u8()?;
    match has_data {
        0 => {
            reader.finish()?;
            Ok(SnapshotWatermark::Empty)
        }
        1 => {
            let snapshot_id = reader.read_u64_le()?;
            let watermark_commit_version = CommitVersion::new(reader.read_u64_le()?);
            let updated_at = Timestamp::from_micros(reader.read_u64_le()?);
            reader.finish()?;
            SnapshotWatermark::present(snapshot_id, watermark_commit_version, updated_at)
        }
        value => Err(FormatError::InvalidBool {
            field: "watermark_has_data",
            value,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        decode_snapshot_watermark, encode_snapshot_watermark, SnapshotWatermark,
        SNAPSHOT_WATERMARK_FORMAT,
    };
    use crate::format::FormatError;
    use strata_core_next::{CommitVersion, Timestamp};

    #[test]
    fn empty_watermark_round_trips() {
        assert_eq!(
            decode_snapshot_watermark(
                &encode_snapshot_watermark(SnapshotWatermark::Empty).expect("encode watermark")
            ),
            Ok(SnapshotWatermark::Empty)
        );
    }

    #[test]
    fn present_watermark_round_trips() {
        let watermark =
            SnapshotWatermark::present(3, CommitVersion::new(42), Timestamp::from_micros(1_700))
                .expect("watermark");

        assert_eq!(
            decode_snapshot_watermark(
                &encode_snapshot_watermark(watermark).expect("encode watermark")
            ),
            Ok(watermark)
        );
    }

    #[test]
    fn present_watermark_reports_facts() {
        let watermark =
            SnapshotWatermark::present(3, CommitVersion::new(42), Timestamp::from_micros(1_700))
                .expect("watermark");

        assert_eq!(watermark.snapshot_id(), Some(3));
        assert_eq!(
            watermark.watermark_commit_version(),
            Some(CommitVersion::new(42))
        );
        assert_eq!(watermark.updated_at(), Some(Timestamp::from_micros(1_700)));
        assert_eq!(watermark.next_snapshot_id(), 4);
        assert_eq!(SnapshotWatermark::Empty.next_snapshot_id(), 1);
    }

    #[test]
    fn decode_rejects_invalid_presence_byte() {
        assert_eq!(
            decode_snapshot_watermark(&[7]),
            Err(FormatError::InvalidBool {
                field: "watermark_has_data",
                value: 7
            })
        );
    }

    #[test]
    fn present_watermark_rejects_zero_snapshot_id() {
        assert_eq!(
            SnapshotWatermark::present(0, CommitVersion::new(42), Timestamp::from_micros(1_700)),
            Err(FormatError::InvalidValue {
                field: "snapshot_id"
            })
        );
    }

    #[test]
    fn encode_rejects_direct_present_watermark_with_zero_snapshot_id() {
        assert_eq!(
            encode_snapshot_watermark(SnapshotWatermark::Present {
                snapshot_id: 0,
                watermark_commit_version: CommitVersion::new(42),
                updated_at: Timestamp::from_micros(1_700),
            }),
            Err(FormatError::InvalidValue {
                field: "snapshot_id"
            })
        );
    }

    #[test]
    fn decode_rejects_present_watermark_with_zero_snapshot_id() {
        let mut bytes = vec![1];
        bytes.extend_from_slice(&0u64.to_le_bytes());
        bytes.extend_from_slice(&42u64.to_le_bytes());
        bytes.extend_from_slice(&1_700u64.to_le_bytes());

        assert_eq!(
            decode_snapshot_watermark(&bytes),
            Err(FormatError::InvalidValue {
                field: "snapshot_id"
            })
        );
    }

    #[test]
    fn decode_rejects_truncated_present_watermark() {
        assert_eq!(
            decode_snapshot_watermark(&[1, 0, 0]),
            Err(FormatError::InsufficientBytes {
                format: SNAPSHOT_WATERMARK_FORMAT,
                needed: 9,
                actual: 3
            })
        );
    }

    #[test]
    fn decode_rejects_trailing_bytes_after_empty_watermark() {
        assert_eq!(
            decode_snapshot_watermark(&[0, 0]),
            Err(FormatError::TrailingData {
                format: SNAPSHOT_WATERMARK_FORMAT,
                remaining: 1
            })
        );
    }

    #[test]
    fn decode_rejects_trailing_bytes_after_present_watermark() {
        let mut bytes = encode_snapshot_watermark(
            SnapshotWatermark::present(3, CommitVersion::new(42), Timestamp::from_micros(1_700))
                .expect("watermark"),
        )
        .expect("encode watermark");
        bytes.push(0);

        assert_eq!(
            decode_snapshot_watermark(&bytes),
            Err(FormatError::TrailingData {
                format: SNAPSHOT_WATERMARK_FORMAT,
                remaining: 1
            })
        );
    }
}
