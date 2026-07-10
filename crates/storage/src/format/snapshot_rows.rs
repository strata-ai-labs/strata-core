use super::{decode_storage_row, encode_storage_row, FormatError, SnapshotSection};
use crate::row::StorageRow;

const SNAPSHOT_ROWS_FORMAT: &str = "snapshot_rows";
const SNAPSHOT_ROWS_MAGIC: [u8; 4] = *b"STRR";
const SNAPSHOT_ROWS_VERSION: u32 = 1;
const SNAPSHOT_ROWS_HEADER_SIZE: usize = 12;

pub(crate) const SNAPSHOT_ROW_SECTION_KIND: u8 = 1;

pub(crate) fn encode_snapshot_row_section(
    rows: &[StorageRow],
) -> Result<SnapshotSection, FormatError> {
    let row_count = u32::try_from(rows.len()).map_err(|_| FormatError::InvalidLength {
        field: "snapshot_row_count",
    })?;
    let mut payload = Vec::new();
    payload.extend_from_slice(&SNAPSHOT_ROWS_MAGIC);
    payload.extend_from_slice(&SNAPSHOT_ROWS_VERSION.to_le_bytes());
    payload.extend_from_slice(&row_count.to_le_bytes());
    for row in rows {
        let row_bytes = encode_storage_row(row)?;
        let row_len = u32::try_from(row_bytes.len()).map_err(|_| FormatError::InvalidLength {
            field: "snapshot_row_len",
        })?;
        payload.extend_from_slice(&row_len.to_le_bytes());
        payload.extend_from_slice(&row_bytes);
    }
    SnapshotSection::new(SNAPSHOT_ROW_SECTION_KIND, payload)
}

pub(crate) fn decode_snapshot_row_payload(payload: &[u8]) -> Result<Vec<StorageRow>, FormatError> {
    if payload.len() < SNAPSHOT_ROWS_HEADER_SIZE {
        return Err(FormatError::InsufficientBytes {
            format: SNAPSHOT_ROWS_FORMAT,
            needed: SNAPSHOT_ROWS_HEADER_SIZE,
            actual: payload.len(),
        });
    }
    if payload[..4] != SNAPSHOT_ROWS_MAGIC {
        return Err(FormatError::InvalidMagic {
            format: SNAPSHOT_ROWS_FORMAT,
        });
    }
    let version = u32::from_le_bytes(
        payload[4..8]
            .try_into()
            .map_err(|_| FormatError::InvalidLength { field: "version" })?,
    );
    match version {
        SNAPSHOT_ROWS_VERSION => {}
        0 => {
            return Err(FormatError::PreV1Format {
                format: SNAPSHOT_ROWS_FORMAT,
                version,
            });
        }
        version => {
            return Err(FormatError::FutureFormat {
                format: SNAPSHOT_ROWS_FORMAT,
                version,
                max_supported: SNAPSHOT_ROWS_VERSION,
            });
        }
    }
    let row_count = usize::try_from(u32::from_le_bytes(
        payload[8..12]
            .try_into()
            .map_err(|_| FormatError::InvalidLength { field: "row_count" })?,
    ))
    .map_err(|_| FormatError::InvalidLength { field: "row_count" })?;
    let max_possible_rows = (payload.len() - SNAPSHOT_ROWS_HEADER_SIZE) / 4;
    if row_count > max_possible_rows {
        return Err(FormatError::InsufficientBytes {
            format: SNAPSHOT_ROWS_FORMAT,
            needed: SNAPSHOT_ROWS_HEADER_SIZE.saturating_add(row_count.saturating_mul(4)),
            actual: payload.len(),
        });
    }
    let mut rows = Vec::new();
    let mut cursor = SNAPSHOT_ROWS_HEADER_SIZE;
    for _ in 0..row_count {
        let len_end = cursor
            .checked_add(4)
            .ok_or(FormatError::InvalidLength { field: "row_len" })?;
        if payload.len() < len_end {
            return Err(FormatError::InsufficientBytes {
                format: SNAPSHOT_ROWS_FORMAT,
                needed: len_end,
                actual: payload.len(),
            });
        }
        let row_len = usize::try_from(u32::from_le_bytes(
            payload[cursor..len_end]
                .try_into()
                .map_err(|_| FormatError::InvalidLength { field: "row_len" })?,
        ))
        .map_err(|_| FormatError::InvalidLength { field: "row_len" })?;
        cursor = len_end;
        let row_end = cursor
            .checked_add(row_len)
            .ok_or(FormatError::InvalidLength { field: "row_len" })?;
        if payload.len() < row_end {
            return Err(FormatError::InsufficientBytes {
                format: SNAPSHOT_ROWS_FORMAT,
                needed: row_end,
                actual: payload.len(),
            });
        }
        rows.push(decode_storage_row(&payload[cursor..row_end])?);
        cursor = row_end;
    }
    if cursor != payload.len() {
        return Err(FormatError::TrailingData {
            format: SNAPSHOT_ROWS_FORMAT,
            remaining: payload.len() - cursor,
        });
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::row::{PhysicalKey, StorageSpaceId};
    use strata_core::{BranchId, CommitVersion, Timestamp};

    #[test]
    fn snapshot_row_section_round_trips_empty_rows() {
        let section = encode_snapshot_row_section(&[]).expect("encode");

        assert_eq!(section.section_kind(), SNAPSHOT_ROW_SECTION_KIND);
        assert_eq!(
            section.payload(),
            &[
                b'S', b'T', b'R', b'R', // magic
                1, 0, 0, 0, // version
                0, 0, 0, 0, // row count
            ]
        );
        assert_eq!(
            decode_snapshot_row_payload(section.payload()),
            Ok(Vec::new())
        );
    }

    #[test]
    fn snapshot_row_section_round_trips_rows_in_order() {
        let rows = vec![
            put_row("alpha", 41),
            put_row("beta", 42),
            tombstone_row("gamma", 43),
        ];
        let section = encode_snapshot_row_section(&rows).expect("encode");
        let row_bytes: Vec<_> = rows
            .iter()
            .map(|row| encode_storage_row(row).expect("encode row"))
            .collect();
        let mut expected = empty_payload();
        expected[8..12].copy_from_slice(
            &u32::try_from(row_bytes.len())
                .expect("row count fits")
                .to_le_bytes(),
        );
        for bytes in &row_bytes {
            expected.extend_from_slice(
                &u32::try_from(bytes.len())
                    .expect("row len fits")
                    .to_le_bytes(),
            );
            expected.extend_from_slice(bytes);
        }

        assert_eq!(section.payload(), expected.as_slice());
        assert_eq!(decode_snapshot_row_payload(section.payload()), Ok(rows));
    }

    #[test]
    fn snapshot_row_payload_rejects_short_header() {
        assert!(matches!(
            decode_snapshot_row_payload(&[0; 11]),
            Err(FormatError::InsufficientBytes { .. })
        ));
    }

    #[test]
    fn snapshot_row_payload_rejects_bad_magic() {
        let mut payload = empty_payload();
        payload[0] = b'X';

        assert!(matches!(
            decode_snapshot_row_payload(&payload),
            Err(FormatError::InvalidMagic { .. })
        ));
    }

    #[test]
    fn snapshot_row_payload_rejects_future_version() {
        let mut payload = empty_payload();
        payload[4..8].copy_from_slice(&2u32.to_le_bytes());

        assert!(matches!(
            decode_snapshot_row_payload(&payload),
            Err(FormatError::FutureFormat { version: 2, .. })
        ));
    }

    #[test]
    fn snapshot_row_payload_rejects_pre_v1_version() {
        let mut payload = empty_payload();
        payload[4..8].copy_from_slice(&0u32.to_le_bytes());

        assert!(matches!(
            decode_snapshot_row_payload(&payload),
            Err(FormatError::PreV1Format { version: 0, .. })
        ));
    }

    #[test]
    fn snapshot_row_payload_rejects_impossible_row_count() {
        let mut payload = empty_payload();
        payload[8..12].copy_from_slice(&1u32.to_le_bytes());

        assert!(matches!(
            decode_snapshot_row_payload(&payload),
            Err(FormatError::InsufficientBytes { .. })
        ));
    }

    #[test]
    fn snapshot_row_payload_rejects_truncated_row_bytes() {
        let row = put_row("alpha", 41);
        let mut payload = encode_snapshot_row_section(&[row])
            .expect("encode")
            .payload()
            .to_vec();
        payload.pop();

        assert!(matches!(
            decode_snapshot_row_payload(&payload),
            Err(FormatError::InsufficientBytes { .. })
        ));
    }

    #[test]
    fn snapshot_row_payload_rejects_invalid_nested_row() {
        let mut payload = empty_payload();
        payload[8..12].copy_from_slice(&1u32.to_le_bytes());
        payload.extend_from_slice(&1u32.to_le_bytes());
        payload.push(0xff);

        assert!(matches!(
            decode_snapshot_row_payload(&payload),
            Err(FormatError::FutureFormat { .. }
                | FormatError::InsufficientBytes { .. }
                | FormatError::InvalidVersion { .. }
                | FormatError::InvalidMagic { .. })
        ));
    }

    #[test]
    fn snapshot_row_payload_rejects_trailing_bytes() {
        let mut payload = empty_payload();
        payload.push(0);

        assert!(matches!(
            decode_snapshot_row_payload(&payload),
            Err(FormatError::TrailingData { remaining: 1, .. })
        ));
    }

    fn empty_payload() -> Vec<u8> {
        let mut payload = Vec::new();
        payload.extend_from_slice(&SNAPSHOT_ROWS_MAGIC);
        payload.extend_from_slice(&SNAPSHOT_ROWS_VERSION.to_le_bytes());
        payload.extend_from_slice(&0u32.to_le_bytes());
        payload
    }

    fn put_row(key: &str, version: u64) -> StorageRow {
        StorageRow::put(
            physical_key(key),
            CommitVersion::new(version),
            Timestamp::from_micros(1_700_000_000_000_000 + version),
            Timestamp::EPOCH,
            key.as_bytes().to_vec(),
        )
    }

    fn tombstone_row(key: &str, version: u64) -> StorageRow {
        StorageRow::tombstone(
            physical_key(key),
            CommitVersion::new(version),
            Timestamp::from_micros(1_700_000_000_000_000 + version),
        )
    }

    fn physical_key(key: &str) -> PhysicalKey {
        PhysicalKey::new(
            BranchId::from_bytes([0x11; BranchId::BYTE_LEN]),
            "default",
            StorageSpaceId::engine(0x20).expect("space"),
            key.as_bytes().to_vec(),
        )
        .expect("physical key")
    }
}
