use super::*;
use crate::format::FormatError;
use crate::service::wal::WalOperation;

const ENVELOPE_HEADER_SIZE: usize = 8;
const WAL_ENVELOPE_FORMAT: &str = "wal_record_envelope";
const WAL_RECORD_FORMAT: &str = "wal_record";
const WAL_RECORD_LENGTH_FORMAT: &str = "wal_record_len";
const WAL_SEGMENT_FORMAT: &str = "wal_segment_header";
const HEADER_VERSION_OFFSET: usize = 4;
const HEADER_SEGMENT_ID_OFFSET: usize = 8;
const HEADER_CRC_OFFSET: usize = 32;
const RECORD_LEN_OFFSET: usize = WAL_SEGMENT_HEADER_SIZE + ENVELOPE_HEADER_SIZE;
const RECORD_LEN_CRC_OFFSET: usize = RECORD_LEN_OFFSET + 5;
const RECORD_BRANCH_ID_OFFSET: usize = RECORD_LEN_OFFSET + 17;
const RECORD_TIMESTAMP_OFFSET: usize = RECORD_LEN_OFFSET + 33;
const RECORD_PAYLOAD_OFFSET: usize = RECORD_LEN_OFFSET + 41;

fn single_record_service(
    backend: &StoredWalBackend,
    payload: impl Into<Vec<u8>>,
) -> (WalService<'_>, WalRecord) {
    let mut service = WalService::open(
        backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::new(1024),
    )
    .expect("open WAL");
    let record = record(1, payload);
    service.append(&record).expect("append record");
    (service, record)
}

fn two_segment_service(backend: &StoredWalBackend) -> WalService<'_> {
    let mut service = WalService::open(
        backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::new(1024),
    )
    .expect("open WAL");
    let large = vec![0x55; 800];
    service
        .append(&record(1, large.clone()))
        .expect("append first segment record");
    service
        .append(&record(2, large))
        .expect("append second segment record");
    service
}

fn segment_object(segment_id: u64) -> ObjectName {
    ObjectLayout::wal_segment(segment_id).expect("WAL segment")
}

fn segment_bytes_from_backend(backend: &StoredWalBackend, segment_id: u64) -> Vec<u8> {
    backend
        .read_object(&segment_object(segment_id))
        .expect("segment bytes")
}

fn replace_segment(backend: &StoredWalBackend, segment_id: u64, bytes: &[u8]) {
    backend
        .write_object(&segment_object(segment_id), bytes)
        .expect("replace segment bytes");
}

#[derive(Clone, Copy, Debug)]
enum FormatErrorExpectation {
    ChecksumMismatch(&'static str),
    FutureFormat {
        format: &'static str,
        version: u32,
        max_supported: u32,
    },
    InsufficientBytes(&'static str),
    InvalidLength(&'static str),
    InvalidMagic(&'static str),
    InvalidValue(&'static str),
    InvalidVersion {
        format: &'static str,
        version: u8,
    },
}

impl FormatErrorExpectation {
    fn assert_matches(self, source: &FormatError) {
        match (self, source) {
            (
                Self::ChecksumMismatch(format),
                FormatError::ChecksumMismatch {
                    format: actual,
                    expected,
                    computed,
                },
            ) => {
                assert_eq!(*actual, format);
                assert_ne!(expected, computed);
            }
            (
                Self::FutureFormat {
                    format,
                    version,
                    max_supported,
                },
                FormatError::FutureFormat {
                    format: actual,
                    version: actual_version,
                    max_supported: actual_max_supported,
                },
            ) => {
                assert_eq!(*actual, format);
                assert_eq!(*actual_version, version);
                assert_eq!(*actual_max_supported, max_supported);
            }
            (
                Self::InsufficientBytes(format),
                FormatError::InsufficientBytes {
                    format: actual,
                    needed,
                    actual: actual_bytes,
                },
            ) => {
                assert_eq!(*actual, format);
                assert!(actual_bytes < needed);
            }
            (Self::InvalidLength(field), FormatError::InvalidLength { field: actual })
            | (Self::InvalidValue(field), FormatError::InvalidValue { field: actual }) => {
                assert_eq!(*actual, field);
            }
            (Self::InvalidMagic(format), FormatError::InvalidMagic { format: actual }) => {
                assert_eq!(*actual, format);
            }
            (
                Self::InvalidVersion { format, version },
                FormatError::InvalidVersion {
                    format: actual,
                    version: actual_version,
                },
            ) => {
                assert_eq!(*actual, format);
                assert_eq!(*actual_version, version);
            }
            (expected, actual) => panic!("expected {expected:?}, got {actual:?}"),
        }
    }
}

fn assert_format_error(
    error: WalServiceError,
    object: &ObjectName,
    expected: FormatErrorExpectation,
) {
    match error {
        WalServiceError::Format {
            operation,
            object: actual,
            source,
        } => {
            assert_eq!(operation, WalOperation::Read);
            assert_eq!(actual, *object);
            expected.assert_matches(&source);
        }
        other => panic!("expected WAL format error, got {other:?}"),
    }
}

fn assert_open_format_error(
    result: Result<WalService<'_>, WalServiceError>,
    object: &ObjectName,
    expected: FormatErrorExpectation,
) {
    assert_format_error(
        open_error(result, "corrupt segment should fail open"),
        object,
        expected,
    );
}

fn assert_read_format_error(
    service: &WalService<'_>,
    object: &ObjectName,
    expected: FormatErrorExpectation,
) {
    assert_format_error(
        service
            .read_all()
            .expect_err("corrupt segment should fail read"),
        object,
        expected,
    );
}

fn assert_latest_tail_truncation(
    service: &WalService<'_>,
    expected_records: &[WalRecord],
    expected_object_size: u64,
) {
    let read = service
        .read_all()
        .expect("latest tail should be recoverable");
    assert_eq!(read.records(), expected_records);
    let truncation = read.truncation().expect("truncation fact");
    assert_eq!(truncation.segment_id(), 1);
    assert!(truncation.valid_end_offset() < truncation.object_size());
    assert_eq!(truncation.object_size(), expected_object_size);
}

fn refresh_segment_header_crc(bytes: &mut [u8]) {
    let crc = crc32fast::hash(&bytes[..HEADER_CRC_OFFSET]);
    bytes[HEADER_CRC_OFFSET..WAL_SEGMENT_HEADER_SIZE].copy_from_slice(&crc.to_le_bytes());
}

fn refresh_envelope_len_crc(bytes: &mut [u8], envelope_offset: usize) {
    let crc = crc32fast::hash(&bytes[envelope_offset..envelope_offset + 4]);
    bytes[envelope_offset + 4..envelope_offset + ENVELOPE_HEADER_SIZE]
        .copy_from_slice(&crc.to_le_bytes());
}

fn refresh_record_len_crc(bytes: &mut [u8]) {
    let crc = crc32fast::hash(&bytes[RECORD_LEN_OFFSET..RECORD_LEN_OFFSET + 4]);
    bytes[RECORD_LEN_CRC_OFFSET..RECORD_LEN_CRC_OFFSET + 4].copy_from_slice(&crc.to_le_bytes());
}

fn refresh_record_payload_crc_at(bytes: &mut [u8], record_offset: usize) {
    let record_len = u32::from_le_bytes(
        bytes[record_offset..record_offset + 4]
            .try_into()
            .expect("record length bytes"),
    ) as usize;
    let payload_start = record_offset + 4;
    let crc_start = payload_start + record_len - 4;
    let crc = crc32fast::hash(&bytes[payload_start..crc_start]);
    bytes[crc_start..crc_start + 4].copy_from_slice(&crc.to_le_bytes());
}

fn refresh_record_payload_crc(bytes: &mut [u8]) {
    refresh_record_payload_crc_at(bytes, RECORD_LEN_OFFSET);
}

fn encoded_record_len_at(bytes: &[u8], envelope_offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[envelope_offset..envelope_offset + 4]
            .try_into()
            .expect("envelope length bytes"),
    )
}

fn second_envelope_offset(bytes: &[u8]) -> usize {
    WAL_SEGMENT_HEADER_SIZE
        + ENVELOPE_HEADER_SIZE
        + encoded_record_len_at(bytes, WAL_SEGMENT_HEADER_SIZE) as usize
}

fn commit_payload_first_row_start_at(record_offset: usize) -> usize {
    let commit_payload_start = record_offset + 41;
    let row_len_offset = commit_payload_start + 4 + 4 + 4;
    row_len_offset + 4
}

fn write_envelope_len_at(bytes: &mut [u8], envelope_offset: usize, encoded_len: u32) {
    bytes[envelope_offset..envelope_offset + 4].copy_from_slice(&encoded_len.to_le_bytes());
    refresh_envelope_len_crc(bytes, envelope_offset);
}

fn truncate_first_envelope_encoded_record(bytes: &mut Vec<u8>, new_encoded_len: u32) {
    write_envelope_len_at(bytes, WAL_SEGMENT_HEADER_SIZE, new_encoded_len);
    bytes.truncate(WAL_SEGMENT_HEADER_SIZE + ENVELOPE_HEADER_SIZE + new_encoded_len as usize);
}

#[test]
fn corrupt_segment_header_magic_fails_open_as_format_error() {
    let backend = StoredWalBackend::new();
    let object = segment_object(1);
    let (_service, _record) = single_record_service(&backend, b"header magic".to_vec());
    let mut bytes = segment_bytes_from_backend(&backend, 1);
    bytes[0] ^= 0xff;
    replace_segment(&backend, 1, &bytes);

    let result = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::new(1024),
    );

    assert_open_format_error(
        result,
        &object,
        FormatErrorExpectation::InvalidMagic(WAL_SEGMENT_FORMAT),
    );
}

#[test]
fn corrupt_segment_header_future_version_fails_open_as_format_error() {
    let backend = StoredWalBackend::new();
    let object = segment_object(1);
    let (_service, _record) = single_record_service(&backend, b"future version".to_vec());
    let mut bytes = segment_bytes_from_backend(&backend, 1);
    bytes[HEADER_VERSION_OFFSET..HEADER_VERSION_OFFSET + 4].copy_from_slice(&99_u32.to_le_bytes());
    refresh_segment_header_crc(&mut bytes);
    replace_segment(&backend, 1, &bytes);

    let result = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::new(1024),
    );

    assert_open_format_error(
        result,
        &object,
        FormatErrorExpectation::FutureFormat {
            format: WAL_SEGMENT_FORMAT,
            version: 99,
            max_supported: 1,
        },
    );
}

#[test]
fn corrupt_segment_header_checksum_fails_open_as_format_error() {
    let backend = StoredWalBackend::new();
    let object = segment_object(1);
    let (_service, _record) = single_record_service(&backend, b"header crc".to_vec());
    let mut bytes = segment_bytes_from_backend(&backend, 1);
    bytes[HEADER_CRC_OFFSET] ^= 0xff;
    replace_segment(&backend, 1, &bytes);

    let result = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::new(1024),
    );

    assert_open_format_error(
        result,
        &object,
        FormatErrorExpectation::ChecksumMismatch(WAL_SEGMENT_FORMAT),
    );
}

#[test]
fn corrupt_segment_header_segment_id_mismatch_is_format_error() {
    let backend = StoredWalBackend::new();
    let object = segment_object(1);
    let (_service, _record) = single_record_service(&backend, b"segment id".to_vec());
    let mut bytes = segment_bytes_from_backend(&backend, 1);
    bytes[HEADER_SEGMENT_ID_OFFSET..HEADER_SEGMENT_ID_OFFSET + 8]
        .copy_from_slice(&2_u64.to_le_bytes());
    refresh_segment_header_crc(&mut bytes);
    replace_segment(&backend, 1, &bytes);

    let result = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::new(1024),
    );

    assert_open_format_error(
        result,
        &object,
        FormatErrorExpectation::InvalidValue("segment_id"),
    );
}

#[test]
fn corrupt_segment_header_database_id_mismatch_is_database_mismatch() {
    let backend = StoredWalBackend::new();
    let object = segment_object(1);
    let (_service, _record) = single_record_service(&backend, b"database id".to_vec());
    replace_segment(
        &backend,
        1,
        &encode_wal_segment_header(&WalSegmentHeader::new(1, other_database_id_for_tests())),
    );

    let result = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::new(1024),
    );

    assert_eq!(
        open_error(result, "other database WAL should fail open"),
        WalServiceError::DatabaseMismatch {
            object,
            segment_id: 1,
        }
    );
}

#[test]
fn corrupt_envelope_inflated_length_is_latest_tail_or_non_latest_format_error() {
    let backend = StoredWalBackend::new();
    let mut service = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::new(1024),
    )
    .expect("open WAL");
    let first = record(1, b"valid prefix".to_vec());
    let second = record(2, b"inflated final envelope".to_vec());
    service.append(&first).expect("append first record");
    service.append(&second).expect("append second record");
    let mut bytes = segment_bytes_from_backend(&backend, 1);
    let envelope_offset = second_envelope_offset(&bytes);
    let inflated_len = encoded_record_len_at(&bytes, envelope_offset)
        .checked_add(1)
        .expect("inflate len");
    write_envelope_len_at(&mut bytes, envelope_offset, inflated_len);
    let object_size = bytes.len() as u64;
    replace_segment(&backend, 1, &bytes);

    assert_latest_tail_truncation(&service, std::slice::from_ref(&first), object_size);

    let backend = StoredWalBackend::new();
    let service = two_segment_service(&backend);
    let object = segment_object(1);
    let mut bytes = segment_bytes_from_backend(&backend, 1);
    let inflated_len = encoded_record_len_at(&bytes, WAL_SEGMENT_HEADER_SIZE)
        .checked_add(1)
        .expect("inflate len");
    write_envelope_len_at(&mut bytes, WAL_SEGMENT_HEADER_SIZE, inflated_len);
    replace_segment(&backend, 1, &bytes);

    assert_read_format_error(
        &service,
        &object,
        FormatErrorExpectation::InsufficientBytes(WAL_ENVELOPE_FORMAT),
    );
}

#[test]
fn corrupt_envelope_shortened_length_is_inner_record_format_error() {
    let backend = StoredWalBackend::new();
    let (service, _record) = single_record_service(&backend, b"short envelope length".to_vec());
    let object = segment_object(1);
    let mut bytes = segment_bytes_from_backend(&backend, 1);
    let shortened_len = encoded_record_len_at(&bytes, WAL_SEGMENT_HEADER_SIZE)
        .checked_sub(1)
        .expect("shorten envelope length");

    // The envelope header is internally valid after the CRC refresh, but it
    // frames an impossible truncated inner record rather than a recoverable
    // latest tail.
    write_envelope_len_at(&mut bytes, WAL_SEGMENT_HEADER_SIZE, shortened_len);
    replace_segment(&backend, 1, &bytes);

    assert_read_format_error(
        &service,
        &object,
        FormatErrorExpectation::InsufficientBytes(WAL_RECORD_FORMAT),
    );
}

#[test]
fn corrupt_envelope_length_crc_fails_as_format_error() {
    let backend = StoredWalBackend::new();
    let (service, _record) = single_record_service(&backend, b"envelope crc".to_vec());
    let object = segment_object(1);
    let mut bytes = segment_bytes_from_backend(&backend, 1);
    bytes[WAL_SEGMENT_HEADER_SIZE + 4] ^= 0xff;
    replace_segment(&backend, 1, &bytes);

    assert_read_format_error(
        &service,
        &object,
        FormatErrorExpectation::ChecksumMismatch(WAL_ENVELOPE_FORMAT),
    );
}

#[test]
fn corrupt_truncated_final_envelope_is_latest_tail_fact() {
    let backend = StoredWalBackend::new();
    let mut service = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::new(1024),
    )
    .expect("open WAL");
    let first = record(1, b"valid prefix".to_vec());
    let second = record(2, b"truncated final envelope".to_vec());
    service.append(&first).expect("append first record");
    service.append(&second).expect("append second record");
    let mut bytes = segment_bytes_from_backend(&backend, 1);
    bytes.pop().expect("remove one final envelope byte");
    let object_size = bytes.len() as u64;
    replace_segment(&backend, 1, &bytes);

    assert_latest_tail_truncation(&service, std::slice::from_ref(&first), object_size);
}

#[test]
fn corrupt_truncated_non_latest_envelope_is_format_error() {
    let backend = StoredWalBackend::new();
    let service = two_segment_service(&backend);
    let object = segment_object(1);
    let mut bytes = segment_bytes_from_backend(&backend, 1);
    bytes.pop().expect("remove one non-latest envelope byte");
    replace_segment(&backend, 1, &bytes);

    assert_read_format_error(
        &service,
        &object,
        FormatErrorExpectation::InsufficientBytes(WAL_ENVELOPE_FORMAT),
    );
}

#[test]
fn corrupt_envelope_encoded_record_byte_is_format_error() {
    let backend = StoredWalBackend::new();
    let (service, _record) = single_record_service(&backend, b"encoded record".to_vec());
    let object = segment_object(1);
    let mut bytes = segment_bytes_from_backend(&backend, 1);
    bytes[RECORD_LEN_OFFSET + 9] ^= 0xff;
    replace_segment(&backend, 1, &bytes);

    assert_read_format_error(
        &service,
        &object,
        FormatErrorExpectation::ChecksumMismatch(WAL_RECORD_FORMAT),
    );
}

#[test]
fn corrupt_inner_record_length_fails_as_format_error() {
    let backend = StoredWalBackend::new();
    let (service, _record) = single_record_service(&backend, b"record length".to_vec());
    let object = segment_object(1);
    let mut bytes = segment_bytes_from_backend(&backend, 1);
    bytes[RECORD_LEN_OFFSET..RECORD_LEN_OFFSET + 4].copy_from_slice(&1_u32.to_le_bytes());
    refresh_record_len_crc(&mut bytes);
    replace_segment(&backend, 1, &bytes);

    assert_read_format_error(
        &service,
        &object,
        FormatErrorExpectation::InvalidLength(WAL_RECORD_LENGTH_FORMAT),
    );
}

#[test]
fn corrupt_inner_record_length_crc_fails_as_format_error() {
    let backend = StoredWalBackend::new();
    let (service, _record) = single_record_service(&backend, b"record len crc".to_vec());
    let object = segment_object(1);
    let mut bytes = segment_bytes_from_backend(&backend, 1);
    bytes[RECORD_LEN_CRC_OFFSET] ^= 0xff;
    replace_segment(&backend, 1, &bytes);

    assert_read_format_error(
        &service,
        &object,
        FormatErrorExpectation::ChecksumMismatch(WAL_RECORD_LENGTH_FORMAT),
    );
}

#[test]
fn corrupt_inner_record_payload_crc_fails_as_format_error() {
    let backend = StoredWalBackend::new();
    let (service, _record) = single_record_service(&backend, b"payload crc".to_vec());
    let object = segment_object(1);
    let mut bytes = segment_bytes_from_backend(&backend, 1);
    let last = bytes.len() - 1;
    bytes[last] ^= 0xff;
    replace_segment(&backend, 1, &bytes);

    assert_read_format_error(
        &service,
        &object,
        FormatErrorExpectation::ChecksumMismatch(WAL_RECORD_FORMAT),
    );
}

#[test]
fn corrupt_inner_record_payload_byte_fails_as_format_error() {
    let backend = StoredWalBackend::new();
    let (service, _record) = single_record_service(&backend, b"payload byte".to_vec());
    let object = segment_object(1);
    let mut bytes = segment_bytes_from_backend(&backend, 1);
    bytes[RECORD_PAYLOAD_OFFSET] ^= 0xff;
    replace_segment(&backend, 1, &bytes);

    assert_read_format_error(
        &service,
        &object,
        FormatErrorExpectation::ChecksumMismatch(WAL_RECORD_FORMAT),
    );
}

#[test]
fn corrupt_commit_payload_magic_with_refreshed_record_crc_is_format_error() {
    let backend = StoredWalBackend::new();
    let service = two_segment_service(&backend);
    let object = segment_object(1);
    let mut bytes = segment_bytes_from_backend(&backend, 1);
    bytes[RECORD_PAYLOAD_OFFSET] ^= 0xff;
    refresh_record_payload_crc(&mut bytes);
    replace_segment(&backend, 1, &bytes);

    assert_read_format_error(
        &service,
        &object,
        FormatErrorExpectation::InvalidMagic("wal_commit_payload"),
    );
}

#[test]
fn corrupt_nested_row_version_with_refreshed_record_crc_is_format_error() {
    let backend = StoredWalBackend::new();
    let service = two_segment_service(&backend);
    let object = segment_object(1);
    let mut bytes = segment_bytes_from_backend(&backend, 1);
    let row_start = commit_payload_first_row_start_at(RECORD_LEN_OFFSET);
    bytes[row_start] = 9;
    refresh_record_payload_crc(&mut bytes);
    replace_segment(&backend, 1, &bytes);

    assert_read_format_error(
        &service,
        &object,
        FormatErrorExpectation::InvalidVersion {
            format: "storage_row",
            version: 9,
        },
    );
}

#[test]
fn corrupt_inner_record_branch_id_byte_fails_as_format_error() {
    let backend = StoredWalBackend::new();
    let (service, _record) = single_record_service(&backend, b"branch id".to_vec());
    let object = segment_object(1);
    let mut bytes = segment_bytes_from_backend(&backend, 1);
    bytes[RECORD_BRANCH_ID_OFFSET] ^= 0xff;
    replace_segment(&backend, 1, &bytes);

    assert_read_format_error(
        &service,
        &object,
        FormatErrorExpectation::ChecksumMismatch(WAL_RECORD_FORMAT),
    );
}

#[test]
fn corrupt_inner_record_branch_id_truncation_fails_as_format_error() {
    let backend = StoredWalBackend::new();
    let (service, _record) = single_record_service(&backend, b"branch truncation".to_vec());
    let object = segment_object(1);
    let mut bytes = segment_bytes_from_backend(&backend, 1);
    let truncated_len =
        u32::try_from(RECORD_BRANCH_ID_OFFSET - RECORD_LEN_OFFSET + 4).expect("truncated length");
    truncate_first_envelope_encoded_record(&mut bytes, truncated_len);
    replace_segment(&backend, 1, &bytes);

    assert_read_format_error(
        &service,
        &object,
        FormatErrorExpectation::InsufficientBytes(WAL_RECORD_FORMAT),
    );
}

#[test]
fn corrupt_inner_record_timestamp_truncation_fails_as_format_error() {
    let backend = StoredWalBackend::new();
    let (service, _record) = single_record_service(&backend, b"timestamp truncation".to_vec());
    let object = segment_object(1);
    let mut bytes = segment_bytes_from_backend(&backend, 1);
    let truncated_len =
        u32::try_from(RECORD_TIMESTAMP_OFFSET - RECORD_LEN_OFFSET + 2).expect("truncated length");
    truncate_first_envelope_encoded_record(&mut bytes, truncated_len);
    replace_segment(&backend, 1, &bytes);

    assert_read_format_error(
        &service,
        &object,
        FormatErrorExpectation::InsufficientBytes(WAL_RECORD_FORMAT),
    );
}

#[test]
fn corrupt_trailing_garbage_is_latest_tail_or_non_latest_format_error() {
    let backend = StoredWalBackend::new();
    let (service, first) = single_record_service(&backend, b"latest garbage".to_vec());
    let mut bytes = segment_bytes_from_backend(&backend, 1);
    bytes.extend_from_slice(&[0xaa, 0xbb, 0xcc]);
    let object_size = bytes.len() as u64;
    replace_segment(&backend, 1, &bytes);

    assert_latest_tail_truncation(&service, std::slice::from_ref(&first), object_size);

    let backend = StoredWalBackend::new();
    let service = two_segment_service(&backend);
    let object = segment_object(1);
    let mut bytes = segment_bytes_from_backend(&backend, 1);
    bytes.extend_from_slice(&[0xaa, 0xbb, 0xcc]);
    replace_segment(&backend, 1, &bytes);

    assert_read_format_error(
        &service,
        &object,
        FormatErrorExpectation::InsufficientBytes(WAL_ENVELOPE_FORMAT),
    );
}

#[test]
fn truncated_latest_segment_inside_commit_payload_row_bytes_is_tail_fact() {
    let backend = StoredWalBackend::new();
    let mut service = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::new(1024),
    )
    .expect("open WAL");
    let first = record(1, b"valid prefix".to_vec());
    let second = record(2, b"commit payload row tail".to_vec());
    service.append(&first).expect("append first record");
    service.append(&second).expect("append second record");
    let mut bytes = segment_bytes_from_backend(&backend, 1);
    let second_record_offset = second_envelope_offset(&bytes) + ENVELOPE_HEADER_SIZE;
    let row_start = commit_payload_first_row_start_at(second_record_offset);
    bytes.truncate(row_start + 1);
    let object_size = bytes.len() as u64;
    replace_segment(&backend, 1, &bytes);

    assert_latest_tail_truncation(&service, std::slice::from_ref(&first), object_size);
}

#[test]
fn truncated_non_latest_segment_inside_commit_payload_row_bytes_is_format_error() {
    let backend = StoredWalBackend::new();
    let mut service = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::new(1024),
    )
    .expect("open WAL");
    let first = record(1, b"valid prefix".to_vec());
    let second = record(2, b"commit payload row non-latest".to_vec());
    service.append(&first).expect("append first record");
    service.append(&second).expect("append second record");
    let mut bytes = segment_bytes_from_backend(&backend, 1);
    let second_record_offset = second_envelope_offset(&bytes) + ENVELOPE_HEADER_SIZE;
    let row_start = commit_payload_first_row_start_at(second_record_offset);
    bytes.truncate(row_start + 1);
    replace_segment(&backend, 1, &bytes);
    let segment_two = segment_object(2);
    backend
        .write_object(&segment_two, &segment_bytes(2, &[]))
        .expect("seed newer segment");
    let service = WalService::open(
        &backend,
        database_id(),
        2,
        DurabilityPolicy::Standard,
        WalServiceConfig::new(1024),
    )
    .expect("open newer segment");

    assert_read_format_error(
        &service,
        &segment_object(1),
        FormatErrorExpectation::InsufficientBytes(WAL_ENVELOPE_FORMAT),
    );
}
