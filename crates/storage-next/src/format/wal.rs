use super::{
    ByteReader, FormatError, WAL_RECORD_ENVELOPE_HEADER_SIZE, WAL_RECORD_FORMAT_VERSION,
    WAL_RECORD_MIN_LEN_AFTER_PREFIX, WAL_SEGMENT_BASE_HEADER_SIZE, WAL_SEGMENT_FORMAT_VERSION,
    WAL_SEGMENT_HEADER_SIZE,
};
use strata_core_next::{BranchId, CommitVersion, Timestamp};

const WAL_ENVELOPE_FORMAT: &str = "wal_record_envelope";
const WAL_RECORD_FORMAT: &str = "wal_record";
const WAL_RECORD_LENGTH_FORMAT: &str = "wal_record_len";
const WAL_SEGMENT_FORMAT: &str = "wal_segment_header";
const WAL_SEGMENT_MAGIC: [u8; 4] = *b"STRA";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct WalSegmentHeader {
    segment_id: u64,
    database_id: [u8; 16],
}

impl WalSegmentHeader {
    pub(crate) const fn new(segment_id: u64, database_id: [u8; 16]) -> Self {
        Self {
            segment_id,
            database_id,
        }
    }

    pub(crate) const fn segment_id(&self) -> u64 {
        self.segment_id
    }

    pub(crate) const fn database_id(&self) -> &[u8; 16] {
        &self.database_id
    }
}

pub(crate) fn encode_wal_segment_header(header: &WalSegmentHeader) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(WAL_SEGMENT_HEADER_SIZE);
    bytes.extend_from_slice(&WAL_SEGMENT_MAGIC);
    bytes.extend_from_slice(&WAL_SEGMENT_FORMAT_VERSION.to_le_bytes());
    bytes.extend_from_slice(&header.segment_id().to_le_bytes());
    bytes.extend_from_slice(header.database_id());
    let crc = crc32fast::hash(&bytes);
    bytes.extend_from_slice(&crc.to_le_bytes());
    bytes
}

pub(crate) fn decode_wal_segment_header(
    bytes: &[u8],
    expected_segment_id: Option<u64>,
) -> Result<(WalSegmentHeader, usize), FormatError> {
    if bytes.len() < WAL_SEGMENT_HEADER_SIZE {
        return Err(FormatError::InsufficientBytes {
            format: WAL_SEGMENT_FORMAT,
            needed: WAL_SEGMENT_HEADER_SIZE,
            actual: bytes.len(),
        });
    }

    let mut reader = ByteReader::new(WAL_SEGMENT_FORMAT, &bytes[..WAL_SEGMENT_BASE_HEADER_SIZE]);
    if reader.read_exact(4)? != WAL_SEGMENT_MAGIC {
        return Err(FormatError::InvalidMagic {
            format: WAL_SEGMENT_FORMAT,
        });
    }

    let version = reader.read_u32_le()?;
    match version {
        WAL_SEGMENT_FORMAT_VERSION => {}
        0 | 2 | 3 => {
            return Err(FormatError::PreV1Format {
                format: WAL_SEGMENT_FORMAT,
                version,
            });
        }
        version => {
            return Err(FormatError::FutureFormat {
                format: WAL_SEGMENT_FORMAT,
                version,
                max_supported: WAL_SEGMENT_FORMAT_VERSION,
            });
        }
    }

    let computed_crc = crc32fast::hash(&bytes[..WAL_SEGMENT_BASE_HEADER_SIZE]);
    let stored_crc = u32::from_le_bytes(
        bytes[WAL_SEGMENT_BASE_HEADER_SIZE..WAL_SEGMENT_HEADER_SIZE]
            .try_into()
            .map_err(|_| FormatError::InvalidLength {
                field: "wal_segment_crc32",
            })?,
    );
    if stored_crc != computed_crc {
        return Err(FormatError::ChecksumMismatch {
            format: WAL_SEGMENT_FORMAT,
            expected: stored_crc,
            computed: computed_crc,
        });
    }

    let segment_id = reader.read_u64_le()?;
    if expected_segment_id.is_some_and(|expected| expected != segment_id) {
        return Err(FormatError::InvalidValue {
            field: "segment_id",
        });
    }
    let database_id = reader.read_exact(16)?;
    let database_id =
        <[u8; 16]>::try_from(database_id).map_err(|_| FormatError::InvalidLength {
            field: "database_id",
        })?;
    reader.finish()?;

    Ok((
        WalSegmentHeader {
            segment_id,
            database_id,
        },
        WAL_SEGMENT_HEADER_SIZE,
    ))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WalRecord {
    commit_version: CommitVersion,
    branch_id: BranchId,
    commit_timestamp: Timestamp,
    commit_payload: Vec<u8>,
}

impl WalRecord {
    pub(crate) fn new(
        commit_version: CommitVersion,
        branch_id: BranchId,
        commit_timestamp: Timestamp,
        commit_payload: impl Into<Vec<u8>>,
    ) -> Self {
        Self {
            commit_version,
            branch_id,
            commit_timestamp,
            commit_payload: commit_payload.into(),
        }
    }

    pub(crate) const fn commit_version(&self) -> CommitVersion {
        self.commit_version
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn commit_timestamp(&self) -> Timestamp {
        self.commit_timestamp
    }

    pub(crate) fn commit_payload(&self) -> &[u8] {
        &self.commit_payload
    }
}

pub(crate) fn encode_wal_record(record: &WalRecord) -> Result<Vec<u8>, FormatError> {
    let mut bytes = Vec::new();
    encode_wal_record_into(record, &mut bytes)?;
    Ok(bytes)
}

pub(crate) fn encode_wal_record_into(
    record: &WalRecord,
    bytes: &mut Vec<u8>,
) -> Result<(), FormatError> {
    bytes.clear();
    let payload_len = 1usize
        .checked_add(4)
        .and_then(|len| len.checked_add(8))
        .and_then(|len| len.checked_add(BranchId::BYTE_LEN))
        .and_then(|len| len.checked_add(8))
        .and_then(|len| len.checked_add(record.commit_payload().len()))
        .ok_or(FormatError::InvalidLength {
            field: WAL_RECORD_FORMAT,
        })?;
    let record_len = payload_len
        .checked_add(4)
        .ok_or(FormatError::InvalidLength {
            field: WAL_RECORD_FORMAT,
        })?;
    let record_len = u32::try_from(record_len).map_err(|_| FormatError::InvalidLength {
        field: WAL_RECORD_FORMAT,
    })?;
    let record_len_bytes = record_len.to_le_bytes();

    let capacity = 4usize
        .checked_add(record_len as usize)
        .ok_or(FormatError::InvalidLength {
            field: WAL_RECORD_FORMAT,
        })?;
    bytes.reserve(capacity);
    bytes.extend_from_slice(&record_len_bytes);

    let payload_start = bytes.len();
    bytes.push(WAL_RECORD_FORMAT_VERSION);
    bytes.extend_from_slice(&[0; 4]);
    bytes.extend_from_slice(&record.commit_version().as_u64().to_le_bytes());
    bytes.extend_from_slice(record.branch_id().as_bytes());
    bytes.extend_from_slice(&record.commit_timestamp().as_micros().to_le_bytes());
    bytes.extend_from_slice(record.commit_payload());

    let len_crc = crc32fast::hash(&record_len_bytes);
    bytes[payload_start + 1..payload_start + 5].copy_from_slice(&len_crc.to_le_bytes());

    let payload_crc = crc32fast::hash(&bytes[payload_start..]);
    bytes.extend_from_slice(&payload_crc.to_le_bytes());
    Ok(())
}

pub(crate) fn decode_wal_record(bytes: &[u8]) -> Result<(WalRecord, usize), FormatError> {
    if bytes.len() < 5 {
        return Err(FormatError::InsufficientBytes {
            format: WAL_RECORD_FORMAT,
            needed: 5,
            actual: bytes.len(),
        });
    }

    let record_len = read_wal_record_len(bytes)?;
    verify_wal_record_len_crc(bytes)?;
    validate_wal_record_len(record_len)?;

    let total_len = 4usize
        .checked_add(record_len)
        .ok_or(FormatError::InvalidLength {
            field: WAL_RECORD_LENGTH_FORMAT,
        })?;
    if bytes.len() < total_len {
        return Err(FormatError::InsufficientBytes {
            format: WAL_RECORD_FORMAT,
            needed: total_len,
            actual: bytes.len(),
        });
    }

    let payload = verify_wal_record_payload_crc(&bytes[4..total_len], record_len)?;
    validate_wal_record_version(payload[0])?;
    Ok((decode_wal_record_payload(payload)?, total_len))
}

fn read_wal_record_len(bytes: &[u8]) -> Result<usize, FormatError> {
    Ok(u32::from_le_bytes(
        bytes[..4]
            .try_into()
            .map_err(|_| FormatError::InvalidLength {
                field: WAL_RECORD_LENGTH_FORMAT,
            })?,
    ) as usize)
}

fn validate_wal_record_len(record_len: usize) -> Result<(), FormatError> {
    if record_len < WAL_RECORD_MIN_LEN_AFTER_PREFIX {
        return Err(FormatError::InvalidLength {
            field: WAL_RECORD_LENGTH_FORMAT,
        });
    }
    Ok(())
}

fn validate_wal_record_version(version: u8) -> Result<(), FormatError> {
    match version {
        WAL_RECORD_FORMAT_VERSION => Ok(()),
        0 | 2 => Err(FormatError::PreV1Format {
            format: WAL_RECORD_FORMAT,
            version: u32::from(version),
        }),
        version => Err(FormatError::FutureFormat {
            format: WAL_RECORD_FORMAT,
            version: u32::from(version),
            max_supported: u32::from(WAL_RECORD_FORMAT_VERSION),
        }),
    }
}

fn verify_wal_record_len_crc(bytes: &[u8]) -> Result<(), FormatError> {
    if bytes.len() < 9 {
        return Err(FormatError::InsufficientBytes {
            format: WAL_RECORD_FORMAT,
            needed: 9,
            actual: bytes.len(),
        });
    }
    let stored_len_crc =
        u32::from_le_bytes(
            bytes[5..9]
                .try_into()
                .map_err(|_| FormatError::InvalidLength {
                    field: WAL_RECORD_LENGTH_FORMAT,
                })?,
        );
    let computed_len_crc = crc32fast::hash(&bytes[..4]);
    if stored_len_crc == computed_len_crc {
        Ok(())
    } else {
        Err(FormatError::ChecksumMismatch {
            format: WAL_RECORD_LENGTH_FORMAT,
            expected: stored_len_crc,
            computed: computed_len_crc,
        })
    }
}

fn verify_wal_record_payload_crc(
    payload_with_crc: &[u8],
    record_len: usize,
) -> Result<&[u8], FormatError> {
    let payload = &payload_with_crc[..record_len - 4];
    let stored_payload_crc =
        u32::from_le_bytes(payload_with_crc[record_len - 4..].try_into().map_err(|_| {
            FormatError::InvalidLength {
                field: "wal_record_crc32",
            }
        })?);
    let computed_payload_crc = crc32fast::hash(payload);
    if stored_payload_crc == computed_payload_crc {
        Ok(payload)
    } else {
        Err(FormatError::ChecksumMismatch {
            format: WAL_RECORD_FORMAT,
            expected: stored_payload_crc,
            computed: computed_payload_crc,
        })
    }
}

fn decode_wal_record_payload(payload: &[u8]) -> Result<WalRecord, FormatError> {
    let commit_version =
        CommitVersion::new(u64::from_le_bytes(payload[5..13].try_into().map_err(
            |_| FormatError::InvalidLength {
                field: "commit_version",
            },
        )?));
    let branch_id = BranchId::try_from_slice(&payload[13..29])
        .map_err(|_| FormatError::InvalidLength { field: "branch_id" })?;
    let commit_timestamp =
        Timestamp::from_micros(u64::from_le_bytes(payload[29..37].try_into().map_err(
            |_| FormatError::InvalidLength {
                field: "commit_timestamp",
            },
        )?));
    let commit_payload = payload[37..].to_vec();

    Ok(WalRecord {
        commit_version,
        branch_id,
        commit_timestamp,
        commit_payload,
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WalRecordEnvelope {
    encoded_record: Vec<u8>,
}

impl WalRecordEnvelope {
    pub(crate) fn new(encoded_record: impl Into<Vec<u8>>) -> Result<Self, FormatError> {
        let encoded_record = encoded_record.into();
        if encoded_record.is_empty() {
            return Err(FormatError::InvalidLength {
                field: WAL_ENVELOPE_FORMAT,
            });
        }
        Ok(Self { encoded_record })
    }

    pub(crate) fn encoded_record(&self) -> &[u8] {
        &self.encoded_record
    }
}

pub(crate) fn encode_wal_record_envelope(
    envelope: &WalRecordEnvelope,
) -> Result<Vec<u8>, FormatError> {
    let encoded_len =
        u32::try_from(envelope.encoded_record().len()).map_err(|_| FormatError::InvalidLength {
            field: WAL_ENVELOPE_FORMAT,
        })?;
    let encoded_len_bytes = encoded_len.to_le_bytes();
    let encoded_len_crc = crc32fast::hash(&encoded_len_bytes);

    let capacity = WAL_RECORD_ENVELOPE_HEADER_SIZE
        .checked_add(envelope.encoded_record().len())
        .ok_or(FormatError::InvalidLength {
            field: WAL_ENVELOPE_FORMAT,
        })?;
    let mut bytes = Vec::with_capacity(capacity);
    bytes.extend_from_slice(&encoded_len_bytes);
    bytes.extend_from_slice(&encoded_len_crc.to_le_bytes());
    bytes.extend_from_slice(envelope.encoded_record());
    Ok(bytes)
}

pub(crate) fn decode_wal_record_envelope(
    bytes: &[u8],
) -> Result<(WalRecordEnvelope, usize), FormatError> {
    if bytes.len() < WAL_RECORD_ENVELOPE_HEADER_SIZE {
        return Err(FormatError::InsufficientBytes {
            format: WAL_ENVELOPE_FORMAT,
            needed: WAL_RECORD_ENVELOPE_HEADER_SIZE,
            actual: bytes.len(),
        });
    }

    let encoded_len =
        u32::from_le_bytes(
            bytes[..4]
                .try_into()
                .map_err(|_| FormatError::InvalidLength {
                    field: WAL_ENVELOPE_FORMAT,
                })?,
        ) as usize;
    if encoded_len == 0 {
        return Err(FormatError::InvalidLength {
            field: WAL_ENVELOPE_FORMAT,
        });
    }
    let stored_len_crc =
        u32::from_le_bytes(
            bytes[4..8]
                .try_into()
                .map_err(|_| FormatError::InvalidLength {
                    field: WAL_ENVELOPE_FORMAT,
                })?,
        );
    let computed_len_crc = crc32fast::hash(&bytes[..4]);
    if stored_len_crc != computed_len_crc {
        return Err(FormatError::ChecksumMismatch {
            format: WAL_ENVELOPE_FORMAT,
            expected: stored_len_crc,
            computed: computed_len_crc,
        });
    }

    let total_len = WAL_RECORD_ENVELOPE_HEADER_SIZE
        .checked_add(encoded_len)
        .ok_or(FormatError::InvalidLength {
            field: WAL_ENVELOPE_FORMAT,
        })?;
    if bytes.len() < total_len {
        return Err(FormatError::InsufficientBytes {
            format: WAL_ENVELOPE_FORMAT,
            needed: total_len,
            actual: bytes.len(),
        });
    }

    Ok((
        WalRecordEnvelope {
            encoded_record: bytes[WAL_RECORD_ENVELOPE_HEADER_SIZE..total_len].to_vec(),
        },
        total_len,
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        decode_wal_record, decode_wal_record_envelope, decode_wal_segment_header,
        encode_wal_record, encode_wal_record_envelope, encode_wal_segment_header, WalRecord,
        WalRecordEnvelope, WalSegmentHeader, WAL_ENVELOPE_FORMAT, WAL_RECORD_FORMAT,
        WAL_RECORD_LENGTH_FORMAT, WAL_SEGMENT_FORMAT,
    };
    use crate::format::{
        FormatError, WAL_RECORD_FORMAT_VERSION, WAL_SEGMENT_FORMAT_VERSION, WAL_SEGMENT_HEADER_SIZE,
    };
    use strata_core_next::{BranchId, CommitVersion, Timestamp};

    fn segment_header() -> WalSegmentHeader {
        WalSegmentHeader::new(
            5,
            [
                0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d,
                0x2e, 0x2f,
            ],
        )
    }

    fn branch_id() -> BranchId {
        BranchId::from_bytes([
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
            0x0e, 0x0f,
        ])
    }

    fn record(payload: impl Into<Vec<u8>>) -> WalRecord {
        WalRecord::new(
            CommitVersion::new(41),
            branch_id(),
            Timestamp::from_micros(1_700_000_000_123_456),
            payload,
        )
    }

    fn refresh_header_crc(bytes: &mut [u8]) {
        let crc = crc32fast::hash(&bytes[..32]);
        bytes[32..36].copy_from_slice(&crc.to_le_bytes());
    }

    fn refresh_record_len_crc(bytes: &mut [u8]) {
        let crc = crc32fast::hash(&bytes[..4]);
        bytes[5..9].copy_from_slice(&crc.to_le_bytes());
    }

    fn refresh_record_payload_crc(bytes: &mut [u8]) {
        let record_len =
            u32::from_le_bytes(bytes[..4].try_into().expect("record length bytes")) as usize;
        let payload_start = 4;
        let crc_start = payload_start + record_len - 4;
        let crc = crc32fast::hash(&bytes[payload_start..crc_start]);
        bytes[crc_start..crc_start + 4].copy_from_slice(&crc.to_le_bytes());
    }

    #[test]
    fn wal_segment_header_round_trips() {
        let header = segment_header();
        let bytes = encode_wal_segment_header(&header);

        assert_eq!(bytes.len(), WAL_SEGMENT_HEADER_SIZE);
        assert_eq!(
            decode_wal_segment_header(&bytes, Some(header.segment_id())),
            Ok((header, WAL_SEGMENT_HEADER_SIZE))
        );
    }

    #[test]
    fn wal_segment_header_decode_consumes_header_only() {
        let header = segment_header();
        let mut bytes = encode_wal_segment_header(&header);
        bytes.extend_from_slice(b"record bytes");

        assert_eq!(
            decode_wal_segment_header(&bytes, Some(header.segment_id())),
            Ok((header, WAL_SEGMENT_HEADER_SIZE))
        );
    }

    #[test]
    fn wal_segment_header_rejects_invalid_magic() {
        let mut bytes = encode_wal_segment_header(&segment_header());
        bytes[0] = b'X';

        assert_eq!(
            decode_wal_segment_header(&bytes, None),
            Err(FormatError::InvalidMagic {
                format: WAL_SEGMENT_FORMAT
            })
        );
    }

    #[test]
    fn wal_segment_header_rejects_pre_v1_versions() {
        for version in [0u32, 2, 3] {
            let mut bytes = encode_wal_segment_header(&segment_header());
            bytes[4..8].copy_from_slice(&version.to_le_bytes());
            refresh_header_crc(&mut bytes);

            assert_eq!(
                decode_wal_segment_header(&bytes, None),
                Err(FormatError::PreV1Format {
                    format: WAL_SEGMENT_FORMAT,
                    version
                })
            );
        }
    }

    #[test]
    fn wal_segment_header_rejects_future_version() {
        let mut bytes = encode_wal_segment_header(&segment_header());
        bytes[4..8].copy_from_slice(&(WAL_SEGMENT_FORMAT_VERSION + 8).to_le_bytes());
        refresh_header_crc(&mut bytes);

        assert_eq!(
            decode_wal_segment_header(&bytes, None),
            Err(FormatError::FutureFormat {
                format: WAL_SEGMENT_FORMAT,
                version: WAL_SEGMENT_FORMAT_VERSION + 8,
                max_supported: WAL_SEGMENT_FORMAT_VERSION
            })
        );
    }

    #[test]
    fn wal_segment_header_rejects_checksum_mismatch() {
        let mut bytes = encode_wal_segment_header(&segment_header());
        bytes[20] ^= 0xff;

        assert!(matches!(
            decode_wal_segment_header(&bytes, None),
            Err(FormatError::ChecksumMismatch {
                format: WAL_SEGMENT_FORMAT,
                ..
            })
        ));
    }

    #[test]
    fn wal_segment_header_rejects_segment_id_mismatch() {
        let bytes = encode_wal_segment_header(&segment_header());

        assert_eq!(
            decode_wal_segment_header(&bytes, Some(6)),
            Err(FormatError::InvalidValue {
                field: "segment_id"
            })
        );
    }

    #[test]
    fn wal_segment_header_rejects_truncated_header() {
        let bytes = encode_wal_segment_header(&segment_header());

        assert_eq!(
            decode_wal_segment_header(&bytes[..WAL_SEGMENT_HEADER_SIZE - 1], None),
            Err(FormatError::InsufficientBytes {
                format: WAL_SEGMENT_FORMAT,
                needed: WAL_SEGMENT_HEADER_SIZE,
                actual: WAL_SEGMENT_HEADER_SIZE - 1
            })
        );
    }

    #[test]
    fn wal_record_round_trips_empty_payload() {
        let record = record(Vec::new());
        let bytes = encode_wal_record(&record).expect("encode record");

        assert_eq!(decode_wal_record(&bytes), Ok((record, bytes.len())));
    }

    #[test]
    fn wal_record_round_trips_non_empty_payload() {
        let record = record(b"payload".to_vec());
        let bytes = encode_wal_record(&record).expect("encode record");

        assert_eq!(decode_wal_record(&bytes), Ok((record, bytes.len())));
    }

    #[test]
    fn wal_record_decode_consumes_one_record_from_sequence() {
        let first = record(b"first".to_vec());
        let second = record(b"second".to_vec());
        let first_bytes = encode_wal_record(&first).expect("first record");
        let second_bytes = encode_wal_record(&second).expect("second record");
        let mut sequence = first_bytes.clone();
        sequence.extend_from_slice(&second_bytes);

        assert_eq!(decode_wal_record(&sequence), Ok((first, first_bytes.len())));
    }

    #[test]
    fn wal_record_rejects_length_crc_mismatch_before_trusting_length() {
        let mut bytes = encode_wal_record(&record(b"payload".to_vec())).expect("encode record");
        bytes[0] = 0xff;

        assert!(matches!(
            decode_wal_record(&bytes),
            Err(FormatError::ChecksumMismatch {
                format: WAL_RECORD_LENGTH_FORMAT,
                ..
            })
        ));
    }

    #[test]
    fn wal_record_rejects_payload_crc_mismatch() {
        let mut bytes = encode_wal_record(&record(b"payload".to_vec())).expect("encode record");
        bytes[20] ^= 0xff;

        assert!(matches!(
            decode_wal_record(&bytes),
            Err(FormatError::ChecksumMismatch {
                format: WAL_RECORD_FORMAT,
                ..
            })
        ));
    }

    #[test]
    fn wal_record_rejects_pre_v1_development_version() {
        let mut bytes = encode_wal_record(&record(b"payload".to_vec())).expect("encode record");
        bytes[4] = 2;
        refresh_record_payload_crc(&mut bytes);

        assert_eq!(
            decode_wal_record(&bytes),
            Err(FormatError::PreV1Format {
                format: WAL_RECORD_FORMAT,
                version: 2
            })
        );
    }

    #[test]
    fn wal_record_rejects_future_version() {
        let mut bytes = encode_wal_record(&record(b"payload".to_vec())).expect("encode record");
        bytes[4] = WAL_RECORD_FORMAT_VERSION + 8;
        refresh_record_payload_crc(&mut bytes);

        assert_eq!(
            decode_wal_record(&bytes),
            Err(FormatError::FutureFormat {
                format: WAL_RECORD_FORMAT,
                version: u32::from(WAL_RECORD_FORMAT_VERSION + 8),
                max_supported: u32::from(WAL_RECORD_FORMAT_VERSION)
            })
        );
    }

    #[test]
    fn wal_record_rejects_too_small_record_length() {
        let mut bytes = encode_wal_record(&record(Vec::new())).expect("encode record");
        bytes[..4].copy_from_slice(&1u32.to_le_bytes());
        refresh_record_len_crc(&mut bytes);

        assert_eq!(
            decode_wal_record(&bytes),
            Err(FormatError::InvalidLength {
                field: WAL_RECORD_LENGTH_FORMAT
            })
        );
    }

    #[test]
    fn wal_record_rejects_truncated_record() {
        let bytes = encode_wal_record(&record(b"payload".to_vec())).expect("encode record");

        assert_eq!(
            decode_wal_record(&bytes[..bytes.len() - 1]),
            Err(FormatError::InsufficientBytes {
                format: WAL_RECORD_FORMAT,
                needed: bytes.len(),
                actual: bytes.len() - 1
            })
        );
    }

    #[test]
    fn wal_record_envelope_round_trips() {
        let record_bytes = encode_wal_record(&record(b"payload".to_vec())).expect("encode record");
        let envelope = WalRecordEnvelope::new(record_bytes.clone()).expect("envelope");
        let envelope_bytes = encode_wal_record_envelope(&envelope).expect("encode envelope");

        assert_eq!(
            decode_wal_record_envelope(&envelope_bytes),
            Ok((envelope, envelope_bytes.len()))
        );
    }

    #[test]
    fn wal_record_envelope_decode_consumes_one_frame_from_sequence() {
        let record_bytes = encode_wal_record(&record(b"payload".to_vec())).expect("encode record");
        let envelope = WalRecordEnvelope::new(record_bytes.clone()).expect("envelope");
        let envelope_bytes = encode_wal_record_envelope(&envelope).expect("encode envelope");
        let mut sequence = envelope_bytes.clone();
        sequence.extend_from_slice(&envelope_bytes);

        assert_eq!(
            decode_wal_record_envelope(&sequence),
            Ok((envelope, envelope_bytes.len()))
        );
    }

    #[test]
    fn wal_record_envelope_rejects_empty_encoded_record() {
        assert_eq!(
            WalRecordEnvelope::new(Vec::new()),
            Err(FormatError::InvalidLength {
                field: WAL_ENVELOPE_FORMAT
            })
        );

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&crc32fast::hash(&0u32.to_le_bytes()).to_le_bytes());

        assert_eq!(
            decode_wal_record_envelope(&bytes),
            Err(FormatError::InvalidLength {
                field: WAL_ENVELOPE_FORMAT
            })
        );
    }

    #[test]
    fn wal_record_envelope_rejects_length_crc_mismatch() {
        let record_bytes = encode_wal_record(&record(b"payload".to_vec())).expect("encode record");
        let envelope = WalRecordEnvelope::new(record_bytes).expect("envelope");
        let mut bytes = encode_wal_record_envelope(&envelope).expect("encode envelope");
        bytes[4] ^= 0xff;

        assert!(matches!(
            decode_wal_record_envelope(&bytes),
            Err(FormatError::ChecksumMismatch {
                format: WAL_ENVELOPE_FORMAT,
                ..
            })
        ));
    }

    #[test]
    fn wal_record_envelope_rejects_truncated_payload() {
        let record_bytes = encode_wal_record(&record(b"payload".to_vec())).expect("encode record");
        let envelope = WalRecordEnvelope::new(record_bytes).expect("envelope");
        let bytes = encode_wal_record_envelope(&envelope).expect("encode envelope");

        assert_eq!(
            decode_wal_record_envelope(&bytes[..bytes.len() - 1]),
            Err(FormatError::InsufficientBytes {
                format: WAL_ENVELOPE_FORMAT,
                needed: bytes.len(),
                actual: bytes.len() - 1
            })
        );
    }
}
