use super::{ByteReader, FormatError, SEGMENT_METADATA_FORMAT_VERSION, SEGMENT_METADATA_SIZE};
use strata_core_next::{CommitVersion, Timestamp};

const SEGMENT_METADATA_FORMAT: &str = "segment_metadata";
const SEGMENT_METADATA_MAGIC: [u8; 4] = *b"STAM";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SegmentMetadata {
    segment_id: u64,
    min_timestamp: Timestamp,
    max_timestamp: Timestamp,
    min_commit_version: CommitVersion,
    max_commit_version: CommitVersion,
    record_count: u64,
}

impl SegmentMetadata {
    pub(crate) const fn empty(segment_id: u64) -> Self {
        Self {
            segment_id,
            min_timestamp: Timestamp::MAX,
            max_timestamp: Timestamp::EPOCH,
            min_commit_version: CommitVersion::MAX,
            max_commit_version: CommitVersion::ZERO,
            record_count: 0,
        }
    }

    pub(crate) fn track_record(&mut self, commit_version: CommitVersion, timestamp: Timestamp) {
        self.min_timestamp = self.min_timestamp.min(timestamp);
        self.max_timestamp = self.max_timestamp.max(timestamp);
        self.min_commit_version = self.min_commit_version.min(commit_version);
        self.max_commit_version = self.max_commit_version.max(commit_version);
        self.record_count += 1;
    }

    pub(crate) const fn segment_id(&self) -> u64 {
        self.segment_id
    }

    pub(crate) const fn min_timestamp(&self) -> Timestamp {
        self.min_timestamp
    }

    pub(crate) const fn max_timestamp(&self) -> Timestamp {
        self.max_timestamp
    }

    pub(crate) const fn min_commit_version(&self) -> CommitVersion {
        self.min_commit_version
    }

    pub(crate) const fn max_commit_version(&self) -> CommitVersion {
        self.max_commit_version
    }

    pub(crate) const fn record_count(&self) -> u64 {
        self.record_count
    }

    pub(crate) const fn is_empty(&self) -> bool {
        self.record_count == 0
    }
}

pub(crate) fn encode_segment_metadata(metadata: &SegmentMetadata) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(SEGMENT_METADATA_SIZE);
    bytes.extend_from_slice(&SEGMENT_METADATA_MAGIC);
    bytes.extend_from_slice(&SEGMENT_METADATA_FORMAT_VERSION.to_le_bytes());
    bytes.extend_from_slice(&metadata.segment_id().to_le_bytes());
    bytes.extend_from_slice(&metadata.min_timestamp().as_micros().to_le_bytes());
    bytes.extend_from_slice(&metadata.max_timestamp().as_micros().to_le_bytes());
    bytes.extend_from_slice(&metadata.min_commit_version().as_u64().to_le_bytes());
    bytes.extend_from_slice(&metadata.max_commit_version().as_u64().to_le_bytes());
    bytes.extend_from_slice(&metadata.record_count().to_le_bytes());
    let crc = crc32fast::hash(&bytes);
    bytes.extend_from_slice(&crc.to_le_bytes());
    bytes
}

pub(crate) fn decode_segment_metadata(bytes: &[u8]) -> Result<SegmentMetadata, FormatError> {
    if bytes.len() < SEGMENT_METADATA_SIZE {
        return Err(FormatError::InsufficientBytes {
            format: SEGMENT_METADATA_FORMAT,
            needed: SEGMENT_METADATA_SIZE,
            actual: bytes.len(),
        });
    }
    if bytes.len() > SEGMENT_METADATA_SIZE {
        return Err(FormatError::TrailingData {
            format: SEGMENT_METADATA_FORMAT,
            remaining: bytes.len() - SEGMENT_METADATA_SIZE,
        });
    }

    let checksum_offset = SEGMENT_METADATA_SIZE - 4;
    let mut reader = ByteReader::new(SEGMENT_METADATA_FORMAT, &bytes[..checksum_offset]);
    if reader.read_exact(4)? != SEGMENT_METADATA_MAGIC {
        return Err(FormatError::InvalidMagic {
            format: SEGMENT_METADATA_FORMAT,
        });
    }

    let version = reader.read_u32_le()?;
    match version {
        SEGMENT_METADATA_FORMAT_VERSION => {}
        0 => {
            return Err(FormatError::PreV1Format {
                format: SEGMENT_METADATA_FORMAT,
                version,
            });
        }
        version => {
            return Err(FormatError::FutureFormat {
                format: SEGMENT_METADATA_FORMAT,
                version,
                max_supported: SEGMENT_METADATA_FORMAT_VERSION,
            });
        }
    }

    let stored_crc = u32::from_le_bytes(
        bytes[checksum_offset..SEGMENT_METADATA_SIZE]
            .try_into()
            .map_err(|_| FormatError::InvalidLength { field: "crc32" })?,
    );
    let computed_crc = crc32fast::hash(&bytes[..checksum_offset]);
    if stored_crc != computed_crc {
        return Err(FormatError::ChecksumMismatch {
            format: SEGMENT_METADATA_FORMAT,
            expected: stored_crc,
            computed: computed_crc,
        });
    }

    let metadata = SegmentMetadata {
        segment_id: reader.read_u64_le()?,
        min_timestamp: Timestamp::from_micros(reader.read_u64_le()?),
        max_timestamp: Timestamp::from_micros(reader.read_u64_le()?),
        min_commit_version: CommitVersion::new(reader.read_u64_le()?),
        max_commit_version: CommitVersion::new(reader.read_u64_le()?),
        record_count: reader.read_u64_le()?,
    };
    reader.finish()?;
    Ok(metadata)
}

#[cfg(test)]
mod tests {
    use super::{
        decode_segment_metadata, encode_segment_metadata, SegmentMetadata, SEGMENT_METADATA_FORMAT,
    };
    use crate::format::{FormatError, SEGMENT_METADATA_FORMAT_VERSION};
    use strata_core_next::{CommitVersion, Timestamp};

    fn metadata() -> SegmentMetadata {
        let mut metadata = SegmentMetadata::empty(9);
        metadata.track_record(CommitVersion::new(7), Timestamp::from_micros(1_700));
        metadata.track_record(CommitVersion::new(11), Timestamp::from_micros(1_900));
        metadata
    }

    #[test]
    fn empty_metadata_has_sentinel_ranges() {
        let metadata = SegmentMetadata::empty(42);

        assert_eq!(metadata.segment_id(), 42);
        assert_eq!(metadata.min_timestamp(), Timestamp::MAX);
        assert_eq!(metadata.max_timestamp(), Timestamp::EPOCH);
        assert_eq!(metadata.min_commit_version(), CommitVersion::MAX);
        assert_eq!(metadata.max_commit_version(), CommitVersion::ZERO);
        assert!(metadata.is_empty());
    }

    #[test]
    fn segment_metadata_round_trips() {
        let metadata = metadata();

        assert_eq!(
            decode_segment_metadata(&encode_segment_metadata(&metadata)),
            Ok(metadata)
        );
    }

    #[test]
    fn decode_rejects_invalid_magic() {
        let mut bytes = encode_segment_metadata(&metadata());
        bytes[0] = b'X';

        assert_eq!(
            decode_segment_metadata(&bytes),
            Err(FormatError::InvalidMagic {
                format: SEGMENT_METADATA_FORMAT
            })
        );
    }

    #[test]
    fn decode_rejects_future_version() {
        let mut bytes = encode_segment_metadata(&metadata());
        bytes[4..8].copy_from_slice(&(SEGMENT_METADATA_FORMAT_VERSION + 1).to_le_bytes());

        assert_eq!(
            decode_segment_metadata(&bytes),
            Err(FormatError::FutureFormat {
                format: SEGMENT_METADATA_FORMAT,
                version: SEGMENT_METADATA_FORMAT_VERSION + 1,
                max_supported: SEGMENT_METADATA_FORMAT_VERSION
            })
        );
    }

    #[test]
    fn decode_rejects_pre_v1_version() {
        let mut bytes = encode_segment_metadata(&metadata());
        bytes[4..8].copy_from_slice(&0u32.to_le_bytes());

        assert_eq!(
            decode_segment_metadata(&bytes),
            Err(FormatError::PreV1Format {
                format: SEGMENT_METADATA_FORMAT,
                version: 0
            })
        );
    }

    #[test]
    fn decode_rejects_checksum_mismatch() {
        let mut bytes = encode_segment_metadata(&metadata());
        bytes[20] ^= 0xff;

        assert!(matches!(
            decode_segment_metadata(&bytes),
            Err(FormatError::ChecksumMismatch {
                format: SEGMENT_METADATA_FORMAT,
                ..
            })
        ));
    }

    #[test]
    fn decode_rejects_trailing_bytes() {
        let mut bytes = encode_segment_metadata(&metadata());
        bytes.push(0);

        assert_eq!(
            decode_segment_metadata(&bytes),
            Err(FormatError::TrailingData {
                format: SEGMENT_METADATA_FORMAT,
                remaining: 1
            })
        );
    }
}
