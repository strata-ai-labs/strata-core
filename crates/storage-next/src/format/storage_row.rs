use super::{
    key::{append_physical_key, decode_physical_key},
    ByteReader, FormatError, STORAGE_ROW_FLAGS_NONE, STORAGE_ROW_FORMAT_VERSION,
};
use crate::row::StorageRow;
use strata_core_next::{CommitVersion, Timestamp};

const STORAGE_ROW_FORMAT: &str = "storage_row";

pub(crate) fn encode_storage_row(row: &StorageRow) -> Result<Vec<u8>, FormatError> {
    let mut bytes = Vec::with_capacity(storage_row_encode_capacity(row));
    encode_storage_row_into(row, &mut bytes)?;
    Ok(bytes)
}

pub(crate) fn encode_storage_row_into(
    row: &StorageRow,
    bytes: &mut Vec<u8>,
) -> Result<(), FormatError> {
    bytes.clear();
    bytes.reserve(storage_row_encode_capacity(row));
    let value = row.value();
    let value_len =
        u32::try_from(value.len()).map_err(|_| FormatError::InvalidLength { field: "value" })?;

    bytes.push(STORAGE_ROW_FORMAT_VERSION);
    let key_len_offset = bytes.len();
    bytes.extend_from_slice(&[0; 4]);
    let key_start = bytes.len();
    append_physical_key(row.physical_key(), bytes);
    let physical_key_len =
        u32::try_from(bytes.len() - key_start).map_err(|_| FormatError::InvalidLength {
            field: "physical_key",
        })?;
    bytes[key_len_offset..key_len_offset + 4].copy_from_slice(&physical_key_len.to_le_bytes());
    // The row carries commit facts redundantly with the internal key. Recovery
    // and diagnostics can validate row bytes without re-parsing the key suffix.
    bytes.extend_from_slice(&row.commit_version().as_u64().to_le_bytes());
    bytes.extend_from_slice(&row.commit_timestamp().as_micros().to_le_bytes());
    bytes.extend_from_slice(&row.expires_at().as_micros().to_le_bytes());
    bytes.extend_from_slice(&STORAGE_ROW_FLAGS_NONE.to_le_bytes());
    bytes.push(u8::from(row.is_tombstone()));
    bytes.extend_from_slice(&value_len.to_le_bytes());
    bytes.extend_from_slice(value);
    Ok(())
}

fn storage_row_encode_capacity(row: &StorageRow) -> usize {
    let key = row.physical_key();
    let physical_key_capacity = key
        .branch_id()
        .as_bytes()
        .len()
        .saturating_add(key.space().len())
        .saturating_add(1)
        .saturating_add(1)
        .saturating_add(key.user_key().len())
        .saturating_add(2);
    1usize
        .saturating_add(4)
        .saturating_add(physical_key_capacity)
        .saturating_add(8)
        .saturating_add(8)
        .saturating_add(8)
        .saturating_add(4)
        .saturating_add(1)
        .saturating_add(4)
        .saturating_add(row.value().len())
}

pub(crate) fn decode_storage_row(bytes: &[u8]) -> Result<StorageRow, FormatError> {
    let mut reader = ByteReader::new(STORAGE_ROW_FORMAT, bytes);
    let version = reader.read_u8()?;
    if version != STORAGE_ROW_FORMAT_VERSION {
        return Err(FormatError::InvalidVersion {
            format: STORAGE_ROW_FORMAT,
            version,
        });
    }

    let physical_key_len =
        usize::try_from(reader.read_u32_le()?).map_err(|_| FormatError::InvalidLength {
            field: "physical_key",
        })?;
    let physical_key = decode_physical_key(reader.read_exact(physical_key_len)?)?;
    let commit_version = CommitVersion::new(reader.read_u64_le()?);
    let commit_timestamp = Timestamp::from_micros(reader.read_u64_le()?);
    let expires_at = Timestamp::from_micros(reader.read_u64_le()?);
    let row_flags = reader.read_u32_le()?;
    if row_flags != STORAGE_ROW_FLAGS_NONE {
        // Reserved flags fail closed until a later format version assigns
        // semantics to them.
        return Err(FormatError::UnsupportedFlags {
            format: STORAGE_ROW_FORMAT,
            flags: row_flags,
        });
    }

    let tombstone = match reader.read_u8()? {
        0 => false,
        1 => true,
        value => {
            return Err(FormatError::InvalidBool {
                field: "is_tombstone",
                value,
            });
        }
    };
    let value_len = usize::try_from(reader.read_u32_le()?)
        .map_err(|_| FormatError::InvalidLength { field: "value" })?;
    if tombstone {
        // Tombstones carry only deletion intent and commit facts. Expiry and
        // value bytes would make delete semantics ambiguous.
        if expires_at != Timestamp::EPOCH {
            return Err(FormatError::InvalidTombstonePayload { field: "expiry" });
        }
        if value_len != 0 {
            return Err(FormatError::InvalidTombstonePayload { field: "value" });
        }
        reader.finish()?;
        Ok(StorageRow::tombstone(
            physical_key,
            commit_version,
            commit_timestamp,
        ))
    } else {
        let value = reader.read_exact(value_len)?.to_vec();
        reader.finish()?;
        Ok(StorageRow::put(
            physical_key,
            commit_version,
            commit_timestamp,
            expires_at,
            value,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{decode_storage_row, encode_storage_row, FormatError, STORAGE_ROW_FORMAT};
    use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};
    use strata_core_next::{BranchId, CommitVersion, Timestamp};

    fn physical_key() -> PhysicalKey {
        PhysicalKey::new(
            BranchId::from_bytes([7; BranchId::BYTE_LEN]),
            "default",
            StorageSpaceId::engine(0x20).expect("engine id"),
            b"alpha".to_vec(),
        )
        .expect("physical key")
    }

    #[test]
    fn storage_row_put_round_trips() {
        let row = StorageRow::put(
            physical_key(),
            CommitVersion::new(42),
            Timestamp::from_micros(11),
            Timestamp::from_micros(99),
            b"value".to_vec(),
        );

        assert_eq!(
            decode_storage_row(&encode_storage_row(&row).expect("encode row")),
            Ok(row)
        );
    }

    #[test]
    fn storage_row_tombstone_round_trips_without_value_or_expiry() {
        let row = StorageRow::tombstone(
            physical_key(),
            CommitVersion::new(43),
            Timestamp::from_micros(12),
        );

        assert_eq!(
            decode_storage_row(&encode_storage_row(&row).expect("encode row")),
            Ok(row)
        );
    }

    #[test]
    fn decode_rejects_unknown_row_format_version() {
        let row = StorageRow::put(
            physical_key(),
            CommitVersion::new(42),
            Timestamp::from_micros(11),
            Timestamp::EPOCH,
            b"value".to_vec(),
        );
        let mut bytes = encode_storage_row(&row).expect("encode row");
        bytes[0] = 2;

        assert_eq!(
            decode_storage_row(&bytes),
            Err(FormatError::InvalidVersion {
                format: STORAGE_ROW_FORMAT,
                version: 2
            })
        );
    }

    #[test]
    fn decode_rejects_nonzero_reserved_flags() {
        let row = StorageRow::put(
            physical_key(),
            CommitVersion::new(42),
            Timestamp::from_micros(11),
            Timestamp::EPOCH,
            b"value".to_vec(),
        );
        let mut bytes = encode_storage_row(&row).expect("encode row");
        let key_len = u32::from_le_bytes(bytes[1..5].try_into().expect("key len")) as usize;
        let flags_offset = 1 + 4 + key_len + 8 + 8 + 8;
        bytes[flags_offset] = 1;

        assert_eq!(
            decode_storage_row(&bytes),
            Err(FormatError::UnsupportedFlags {
                format: STORAGE_ROW_FORMAT,
                flags: 1
            })
        );
    }

    #[test]
    fn decode_rejects_invalid_tombstone_byte() {
        let row = StorageRow::put(
            physical_key(),
            CommitVersion::new(42),
            Timestamp::from_micros(11),
            Timestamp::EPOCH,
            b"value".to_vec(),
        );
        let mut bytes = encode_storage_row(&row).expect("encode row");
        let key_len = u32::from_le_bytes(bytes[1..5].try_into().expect("key len")) as usize;
        let tombstone_offset = 1 + 4 + key_len + 8 + 8 + 8 + 4;
        bytes[tombstone_offset] = 7;

        assert_eq!(
            decode_storage_row(&bytes),
            Err(FormatError::InvalidBool {
                field: "is_tombstone",
                value: 7
            })
        );
    }

    #[test]
    fn decode_rejects_tombstone_payload_data() {
        let row = StorageRow::put(
            physical_key(),
            CommitVersion::new(42),
            Timestamp::from_micros(11),
            Timestamp::EPOCH,
            b"value".to_vec(),
        );
        let mut bytes = encode_storage_row(&row).expect("encode row");
        let key_len = u32::from_le_bytes(bytes[1..5].try_into().expect("key len")) as usize;
        let tombstone_offset = 1 + 4 + key_len + 8 + 8 + 8 + 4;
        bytes[tombstone_offset] = 1;

        assert_eq!(
            decode_storage_row(&bytes),
            Err(FormatError::InvalidTombstonePayload { field: "value" })
        );
    }

    #[test]
    fn decode_rejects_tombstone_value_len_before_value_allocation() {
        let row = StorageRow::tombstone(
            physical_key(),
            CommitVersion::new(42),
            Timestamp::from_micros(11),
        );
        let mut bytes = encode_storage_row(&row).expect("encode row");
        let key_len = u32::from_le_bytes(bytes[1..5].try_into().expect("key len")) as usize;
        let value_len_offset = 1 + 4 + key_len + 8 + 8 + 8 + 4 + 1;
        bytes[value_len_offset..value_len_offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());

        assert_eq!(
            decode_storage_row(&bytes),
            Err(FormatError::InvalidTombstonePayload { field: "value" })
        );
    }

    #[test]
    fn decode_rejects_tombstone_expiry() {
        let row = StorageRow::tombstone(
            physical_key(),
            CommitVersion::new(42),
            Timestamp::from_micros(11),
        );
        let mut bytes = encode_storage_row(&row).expect("encode row");
        let key_len = u32::from_le_bytes(bytes[1..5].try_into().expect("key len")) as usize;
        let expiry_offset = 1 + 4 + key_len + 8 + 8;
        bytes[expiry_offset] = 1;

        assert_eq!(
            decode_storage_row(&bytes),
            Err(FormatError::InvalidTombstonePayload { field: "expiry" })
        );
    }
}
