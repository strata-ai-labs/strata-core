//! Durable byte codecs for storage-owned formats.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "format codecs are consumed by durable services added later"
    )
)]

use std::fmt;

mod branch_catalog_manifest;
mod key;
mod manifest;
mod pending_releases_manifest;
pub(crate) mod quarantine;
mod retained_history_extension;
mod segment_metadata;
mod snapshot;
mod snapshot_rows;
mod storage_row;
mod table;
mod table_manifest;
mod wal;
mod watermark;

pub(crate) use branch_catalog_manifest::{
    decode_branch_catalog_manifest, encode_branch_catalog_manifest, BranchCatalogEntry,
    BranchCatalogManifest, BranchCatalogParent, BranchCatalogStatus,
};
pub(crate) use key::{decode_internal_key, encode_internal_key, encode_physical_key};
pub(crate) use manifest::{decode_manifest, encode_manifest, DatabaseManifest};
pub(crate) use pending_releases_manifest::{
    decode_pending_releases_manifest, encode_pending_releases_manifest, PendingReleasesEntry,
    PendingReleasesManifest,
};
pub(crate) use retained_history_extension::{
    decode_retained_history_extension_payload, decode_retained_history_extension_section,
    encode_retained_history_extension_payload, RetainedHistoryExtensionPayload,
};
#[cfg(test)]
pub(crate) use retained_history_extension::{
    RETAINED_HISTORY_EXTENSION_KIND, RETAINED_HISTORY_EXTENSION_PAYLOAD_LEN,
};
pub(crate) use segment_metadata::{
    decode_segment_metadata, encode_segment_metadata, SegmentMetadata,
};
pub(crate) use snapshot::{
    decode_snapshot_container, encode_snapshot_container, visit_snapshot_container_sections,
    SnapshotContainer, SnapshotHeader, SnapshotSection, SnapshotSectionRef,
};
pub(crate) use snapshot_rows::{
    decode_snapshot_row_payload, encode_snapshot_row_section, SNAPSHOT_ROW_SECTION_KIND,
};
pub(crate) use storage_row::{decode_storage_row, encode_storage_row};
#[expect(
    unused_imports,
    reason = "immutable table artifact helpers are consumed by the table runtime layer"
)]
pub(crate) use table::{
    decode_immutable_table, decode_immutable_table_data_block, decode_immutable_table_metadata,
    decode_table_footer_metadata, decode_table_header, encode_immutable_table,
    encode_immutable_table_with_block_compressions, seek_immutable_table_data_block_point,
    ImmutableTable, ImmutableTableMetadata, ImmutableTableStreamingEncoder, TableCompression,
    TableDataBlockPointSeek, TableIndexEntry, MAX_TABLE_BLOCK_DECODED_BYTES,
    MAX_TABLE_BLOCK_ENTRIES, MAX_TABLE_DATA_BLOCKS, MAX_TABLE_FOOTER_SIZE, MAX_TABLE_HEADER_SIZE,
    MAX_TABLE_KEY_BYTES, MAX_TABLE_ROWS, MAX_TABLE_ROW_BYTES,
};
#[allow(
    unused_imports,
    reason = "table manifest codec is consumed by durable reachability wiring and retained-history extension"
)]
pub(crate) use table_manifest::{
    decode_table_manifest, encode_table_manifest, TableManifest, TableManifestExtensionSection,
    TableManifestInheritedLayer, TableManifestInheritedLayerStatus, TableManifestLevel,
    TableManifestTableBounds, TableManifestTableFacts, TableManifestTableProvenance,
    TableManifestTableRef,
};
#[expect(
    unused_imports,
    reason = "direct payload codec exports are used by payload fuzz/testkit routing after record integration"
)]
pub(crate) use wal::{decode_wal_commit_payload, encode_wal_commit_payload, WalCommitPayload};
pub(crate) use wal::{
    decode_wal_record, decode_wal_record_envelope, decode_wal_segment_header, encode_wal_record,
    encode_wal_record_envelope, encode_wal_segment_header, WalRecord, WalRecordEnvelope,
    WalSegmentHeader,
};

#[cfg(any(test, feature = "testkit"))]
pub(crate) mod fuzzing;

#[cfg(test)]
mod tests;

const BRANCH_CATALOG_FORMAT_VERSION: u32 = 1;
const DATABASE_FORMAT_VERSION: u32 = 1;
const PENDING_RELEASES_FORMAT_VERSION: u32 = 1;
const MAX_CODEC_ID_LEN: usize = 256;
const SEGMENT_METADATA_FORMAT_VERSION: u32 = 1;
const SEGMENT_METADATA_SIZE: usize = 60;
const SNAPSHOT_FOOTER_SIZE: usize = 4;
const SNAPSHOT_HEADER_SIZE: usize = 64;
const SNAPSHOT_FORMAT_VERSION: u32 = 1;
const SNAPSHOT_SECTION_HEADER_SIZE: usize = 9;
const STORAGE_ROW_FORMAT_VERSION: u8 = 1;
const STORAGE_ROW_FLAGS_NONE: u32 = 0;
const WAL_RECORD_ENVELOPE_HEADER_SIZE: usize = 8;
const WAL_RECORD_FORMAT_VERSION: u8 = 1;
// Minimum V1 WAL record length after the 4-byte length prefix: fixed record
// fields plus the smallest row-native commit payload containing one row.
const WAL_RECORD_MIN_LEN_AFTER_PREFIX: usize = 116;
const WAL_SEGMENT_BASE_HEADER_SIZE: usize = 32;
const WAL_SEGMENT_FORMAT_VERSION: u32 = 1;
pub(crate) const WAL_SEGMENT_HEADER_SIZE: usize = 36;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum FormatError {
    InsufficientBytes {
        format: &'static str,
        needed: usize,
        actual: usize,
    },
    InvalidEscape {
        field: &'static str,
    },
    MissingTerminator {
        field: &'static str,
    },
    InvalidUtf8 {
        field: &'static str,
    },
    InvalidLength {
        field: &'static str,
    },
    InvalidValue {
        field: &'static str,
    },
    InvalidMagic {
        format: &'static str,
    },
    InvalidVersion {
        format: &'static str,
        version: u8,
    },
    PreV1Format {
        format: &'static str,
        version: u32,
    },
    FutureFormat {
        format: &'static str,
        version: u32,
        max_supported: u32,
    },
    InvalidBool {
        field: &'static str,
        value: u8,
    },
    ChecksumMismatch {
        format: &'static str,
        expected: u32,
        computed: u32,
    },
    UnsupportedFlags {
        format: &'static str,
        flags: u32,
    },
    UnsupportedCompression {
        format: &'static str,
        codec: u8,
    },
    CompressionFailed {
        format: &'static str,
    },
    DecompressionFailed {
        format: &'static str,
    },
    InvalidTombstonePayload {
        field: &'static str,
    },
    InvalidStorageSpaceId {
        raw: u8,
    },
    StorageReservedSpaceId {
        raw: u8,
    },
    InvalidSpace {
        reason: &'static str,
    },
    TrailingData {
        format: &'static str,
        remaining: usize,
    },
}

impl fmt::Display for FormatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InsufficientBytes {
                format,
                needed,
                actual,
            } => write!(formatter, "{format} needs {needed} bytes; found {actual}"),
            Self::InvalidEscape { field } => write!(formatter, "{field} has invalid escape bytes"),
            Self::MissingTerminator { field } => write!(formatter, "{field} is not terminated"),
            Self::InvalidUtf8 { field } => write!(formatter, "{field} is not valid UTF-8"),
            Self::InvalidLength { field } => write!(formatter, "{field} length is invalid"),
            Self::InvalidValue { field } => write!(formatter, "{field} value is invalid"),
            Self::InvalidMagic { format } => write!(formatter, "{format} magic is invalid"),
            Self::InvalidVersion { format, version } => {
                write!(formatter, "{format} version {version} is unsupported")
            }
            Self::PreV1Format { format, version } => {
                write!(formatter, "{format} version {version} is pre-V1")
            }
            Self::FutureFormat {
                format,
                version,
                max_supported,
            } => write!(
                formatter,
                "{format} version {version} is newer than supported version {max_supported}"
            ),
            Self::InvalidBool { field, value } => {
                write!(formatter, "{field} has invalid boolean byte {value}")
            }
            Self::ChecksumMismatch {
                format,
                expected,
                computed,
            } => write!(
                formatter,
                "{format} checksum mismatch: expected 0x{expected:08x}, computed 0x{computed:08x}"
            ),
            Self::UnsupportedFlags { format, flags } => {
                write!(formatter, "{format} flags 0x{flags:08x} are unsupported")
            }
            Self::UnsupportedCompression { format, codec } => {
                write!(
                    formatter,
                    "{format} compression codec {codec} is unsupported"
                )
            }
            Self::CompressionFailed { format } => write!(formatter, "{format} compression failed"),
            Self::DecompressionFailed { format } => {
                write!(formatter, "{format} decompression failed")
            }
            Self::InvalidTombstonePayload { field } => {
                write!(formatter, "tombstone row must not carry {field}")
            }
            Self::InvalidStorageSpaceId { raw } => {
                write!(formatter, "storage space id 0x{raw:02x} is invalid")
            }
            Self::StorageReservedSpaceId { raw } => {
                write!(
                    formatter,
                    "storage space id 0x{raw:02x} is reserved for storage"
                )
            }
            Self::InvalidSpace { reason } => write!(formatter, "physical key space {reason}"),
            Self::TrailingData { format, remaining } => {
                write!(formatter, "{format} has {remaining} trailing bytes")
            }
        }
    }
}

impl std::error::Error for FormatError {}

struct ByteReader<'a> {
    format: &'static str,
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> ByteReader<'a> {
    const fn new(format: &'static str, bytes: &'a [u8]) -> Self {
        Self {
            format,
            bytes,
            cursor: 0,
        }
    }

    const fn remaining(&self) -> usize {
        self.bytes.len() - self.cursor
    }

    fn read_exact(&mut self, len: usize) -> Result<&'a [u8], FormatError> {
        let end = self
            .cursor
            .checked_add(len)
            .ok_or(FormatError::InvalidLength { field: self.format })?;
        if end > self.bytes.len() {
            return Err(FormatError::InsufficientBytes {
                format: self.format,
                needed: end,
                actual: self.bytes.len(),
            });
        }

        let value = &self.bytes[self.cursor..end];
        self.cursor = end;
        Ok(value)
    }

    fn read_u8(&mut self) -> Result<u8, FormatError> {
        Ok(self.read_exact(1)?[0])
    }

    fn read_u16_le(&mut self) -> Result<u16, FormatError> {
        let bytes = self.read_exact(2)?;
        let bytes = <[u8; 2]>::try_from(bytes)
            .map_err(|_| FormatError::InvalidLength { field: self.format })?;
        Ok(u16::from_le_bytes(bytes))
    }

    fn read_u32_le(&mut self) -> Result<u32, FormatError> {
        let bytes = self.read_exact(4)?;
        let bytes = <[u8; 4]>::try_from(bytes)
            .map_err(|_| FormatError::InvalidLength { field: self.format })?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn read_u64_le(&mut self) -> Result<u64, FormatError> {
        let bytes = self.read_exact(8)?;
        let bytes = <[u8; 8]>::try_from(bytes)
            .map_err(|_| FormatError::InvalidLength { field: self.format })?;
        Ok(u64::from_le_bytes(bytes))
    }

    fn finish(&self) -> Result<(), FormatError> {
        let remaining = self.remaining();
        if remaining == 0 {
            Ok(())
        } else {
            Err(FormatError::TrailingData {
                format: self.format,
                remaining,
            })
        }
    }
}
