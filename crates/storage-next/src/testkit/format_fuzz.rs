use crate::format::fuzzing;

/// Durable byte decoder available to storage fuzz targets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FormatDecoder {
    Key,
    Manifest,
    SegmentMetadata,
    SnapshotEnvelope,
    StorageRow,
    WalRecord,
    WalSegmentHeader,
    Watermark,
}

/// Result of routing arbitrary bytes through a durable format decoder.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FormatDecodeOutcome {
    Accepted,
    Rejected,
}

/// Routes arbitrary bytes through one storage-next durable format decoder.
///
/// This function is intentionally exposed only through the hidden testkit
/// feature. It exists for fuzz targets and conformance probes, not as a
/// production format API.
pub fn decode_format_bytes(decoder: FormatDecoder, bytes: &[u8]) -> FormatDecodeOutcome {
    let accepted = match decoder {
        FormatDecoder::Key => fuzzing::decode_key(bytes),
        FormatDecoder::Manifest => fuzzing::decode_manifest(bytes),
        FormatDecoder::SegmentMetadata => fuzzing::decode_segment_metadata(bytes),
        FormatDecoder::SnapshotEnvelope => fuzzing::decode_snapshot_envelope(bytes),
        FormatDecoder::StorageRow => fuzzing::decode_storage_row(bytes),
        FormatDecoder::WalRecord => fuzzing::decode_wal_record(bytes),
        FormatDecoder::WalSegmentHeader => fuzzing::decode_wal_segment_header(bytes),
        FormatDecoder::Watermark => fuzzing::decode_watermark(bytes),
    };

    if accepted {
        FormatDecodeOutcome::Accepted
    } else {
        FormatDecodeOutcome::Rejected
    }
}

#[cfg(test)]
mod tests {
    use super::{decode_format_bytes, FormatDecodeOutcome, FormatDecoder};

    #[test]
    fn format_fuzz_decoders_reject_empty_input_without_panicking() {
        for decoder in [
            FormatDecoder::Key,
            FormatDecoder::Manifest,
            FormatDecoder::SegmentMetadata,
            FormatDecoder::SnapshotEnvelope,
            FormatDecoder::StorageRow,
            FormatDecoder::WalRecord,
            FormatDecoder::WalSegmentHeader,
            FormatDecoder::Watermark,
        ] {
            assert_eq!(
                decode_format_bytes(decoder, &[]),
                FormatDecodeOutcome::Rejected
            );
        }
    }
}
