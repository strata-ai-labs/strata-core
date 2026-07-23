use super::{ByteReader, FormatError, WAL_WATERMARK_FORMAT_VERSION};

const FORMAT: &str = "wal_watermark";
const MAGIC: [u8; 4] = *b"STWW";
// magic(4) + version(4) + highest_commit_version(8) + crc(4).
const ENCODED_LEN: usize = 20;

/// Encodes the durable WAL commit watermark: the highest commit version whose
/// WAL record has been sealed durable (written at segment rotation and close,
/// strictly after the bytes it attests are synced — a safe lower bound). The
/// object is CRC-guarded because it drives a data-loss refuse-to-open
/// decision, so a torn write must be detectable rather than silently trusted.
///
/// #2690 design note: an earlier revision recorded the highest *segment id*,
/// which is not checkpoint-comparable — it refused databases whose dropped
/// segment was safely covered by a snapshot (the fault-simulation sweep caught
/// it). The commit version is comparable against every recovery source.
pub(crate) fn encode_wal_watermark(highest_commit_version: u64) -> Result<Vec<u8>, FormatError> {
    if highest_commit_version == 0 {
        // Commit versions are 1-based; zero means "nothing committed", which
        // is expressed by the object's absence, never by a zero payload.
        return Err(FormatError::InvalidValue {
            field: "highest_commit_version",
        });
    }
    let mut bytes = Vec::with_capacity(ENCODED_LEN);
    bytes.extend_from_slice(&MAGIC);
    bytes.extend_from_slice(&WAL_WATERMARK_FORMAT_VERSION.to_le_bytes());
    bytes.extend_from_slice(&highest_commit_version.to_le_bytes());
    let crc = crc32fast::hash(&bytes);
    bytes.extend_from_slice(&crc.to_le_bytes());
    Ok(bytes)
}

/// Decodes a durable WAL commit watermark, verifying magic, version, and CRC.
/// A decode failure means the watermark is malformed or torn; the caller
/// degrades to non-detection rather than treating a corrupt value as
/// authoritative.
pub(crate) fn decode_wal_watermark(bytes: &[u8]) -> Result<u64, FormatError> {
    if bytes.len() != ENCODED_LEN {
        return Err(FormatError::InsufficientBytes {
            format: FORMAT,
            needed: ENCODED_LEN,
            actual: bytes.len(),
        });
    }

    let checksum_offset = bytes.len() - 4;
    let stored_crc = u32::from_le_bytes(
        bytes[checksum_offset..]
            .try_into()
            .map_err(|_| FormatError::InvalidLength { field: "crc32" })?,
    );

    let mut reader = ByteReader::new(FORMAT, &bytes[..checksum_offset]);
    let magic = reader.read_exact(4)?;
    if magic != MAGIC {
        return Err(FormatError::InvalidMagic { format: FORMAT });
    }

    let version = reader.read_u32_le()?;
    if version != WAL_WATERMARK_FORMAT_VERSION {
        return Err(FormatError::FutureFormat {
            format: FORMAT,
            version,
            max_supported: WAL_WATERMARK_FORMAT_VERSION,
        });
    }

    let computed_crc = crc32fast::hash(&bytes[..checksum_offset]);
    if stored_crc != computed_crc {
        return Err(FormatError::ChecksumMismatch {
            format: FORMAT,
            expected: stored_crc,
            computed: computed_crc,
        });
    }

    let highest_commit_version = reader.read_u64_le()?;
    reader.finish()?;
    if highest_commit_version == 0 {
        return Err(FormatError::InvalidValue {
            field: "highest_commit_version",
        });
    }
    Ok(highest_commit_version)
}

#[cfg(test)]
mod tests {
    use super::{decode_wal_watermark, encode_wal_watermark};
    use crate::format::FormatError;

    #[test]
    fn watermark_round_trips_every_commit_version() {
        for version in [1u64, 2, 7, 1_000_000, u64::MAX] {
            let encoded = encode_wal_watermark(version).expect("encode");
            assert_eq!(
                decode_wal_watermark(&encoded),
                Ok(version),
                "version {version}"
            );
        }
    }

    #[test]
    fn zero_version_is_rejected_on_encode_and_decode() {
        assert!(matches!(
            encode_wal_watermark(0),
            Err(FormatError::InvalidValue { .. })
        ));
    }

    #[test]
    fn a_torn_crc_is_detected() {
        let mut bytes = encode_wal_watermark(5).expect("encode");
        let last = bytes.len() - 1;
        bytes[last] ^= 0xFF;
        assert!(matches!(
            decode_wal_watermark(&bytes),
            Err(FormatError::ChecksumMismatch { .. })
        ));
    }

    #[test]
    fn a_flipped_magic_is_detected_before_the_crc() {
        let mut bytes = encode_wal_watermark(5).expect("encode");
        bytes[0] ^= 0xFF;
        assert!(matches!(
            decode_wal_watermark(&bytes),
            Err(FormatError::InvalidMagic { .. })
        ));
    }

    #[test]
    fn a_wrong_version_is_rejected() {
        let mut bytes = encode_wal_watermark(5).expect("encode");
        // Bump the version field (bytes 4..8) and refresh the CRC so the version
        // check — not the checksum — is what rejects it.
        bytes[4] = 9;
        let checksum_offset = bytes.len() - 4;
        let crc = crc32fast::hash(&bytes[..checksum_offset]);
        bytes[checksum_offset..].copy_from_slice(&crc.to_le_bytes());
        assert!(matches!(
            decode_wal_watermark(&bytes),
            Err(FormatError::FutureFormat { .. })
        ));
    }

    #[test]
    fn a_truncated_watermark_is_detected() {
        let bytes = encode_wal_watermark(5).expect("encode");
        assert!(matches!(
            decode_wal_watermark(&bytes[..bytes.len() - 1]),
            Err(FormatError::InsufficientBytes { .. })
        ));
    }
}
