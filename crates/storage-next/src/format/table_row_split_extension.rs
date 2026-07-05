//! Durable per-table put/tombstone row-split, carried as an additive manifest
//! extension section.
//!
//! `TableSummaryExtras` has six fields; four (timestamp and physical-key bounds)
//! are already persisted on each table's manifest record. The remaining two — the
//! put/tombstone row split — are persisted here so recovery can rebuild the full
//! summary via [`TableSummaryExtras::from_parts`](crate::table::TableSummaryExtras)
//! without a full row scan (the scan a lazy/disk-resident reader exists to avoid).
//!
//! The payload is **positional**: one 16-byte `(put_rows, tombstone_rows)` entry per
//! table, in the exact order the manifest encodes its tables (top-level levels, then
//! each inherited layer's levels — the order both the writer and recovery walk). A
//! positional array (rather than an identity-keyed map) is what lets the section scale
//! to the manifest's own `MAX_TOTAL_TABLES` cap of 65 536 tables: 65 536 × 16 bytes is
//! exactly the 1 MiB `MAX_SECTION_BYTES` ceiling, whereas duplicating each table's
//! identity would overflow the section past ~17 000 tables. This mirrors the manifest's
//! existing positional table encoding; recovery validates the entry count against the
//! table count it decodes, and the debug provenance oracle cross-checks each
//! reconstructed summary against a full scan.

use super::{FormatError, TableManifestExtensionSection};

pub(crate) const TABLE_ROW_SPLIT_EXTENSION_KIND: &str = "storage.table_row_split";
const TABLE_ROW_SPLIT_ENTRY_LEN: usize = 16;
/// Matches the manifest's `MAX_TOTAL_TABLES`; `MAX * ENTRY_LEN` is exactly the 1 MiB
/// `MAX_SECTION_BYTES` section ceiling, so a full manifest's splits always fit.
const MAX_TABLE_ROW_SPLITS: usize = 65_536;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TableRowSplit {
    put_rows: u64,
    tombstone_rows: u64,
}

impl TableRowSplit {
    pub(crate) const fn new(put_rows: u64, tombstone_rows: u64) -> Self {
        Self {
            put_rows,
            tombstone_rows,
        }
    }
}

/// Build the optional, preserve-on-rewrite section carrying `splits` in table order.
///
/// The caller must pass a non-empty slice (a manifest with no tables emits no
/// section); `MAX_TABLE_ROW_SPLITS` bounds the count so the payload cannot exceed the
/// section-byte ceiling.
pub(crate) fn table_row_split_extension_section(
    splits: &[TableRowSplit],
) -> Result<TableManifestExtensionSection, FormatError> {
    TableManifestExtensionSection::optional(
        TABLE_ROW_SPLIT_EXTENSION_KIND,
        true,
        encode_table_row_split_extension_payload(splits)?,
    )
}

pub(crate) fn encode_table_row_split_extension_payload(
    splits: &[TableRowSplit],
) -> Result<Vec<u8>, FormatError> {
    if splits.is_empty() || splits.len() > MAX_TABLE_ROW_SPLITS {
        return Err(FormatError::InvalidLength {
            field: "table_row_split_count",
        });
    }
    let mut bytes = Vec::with_capacity(splits.len() * TABLE_ROW_SPLIT_ENTRY_LEN);
    for split in splits {
        bytes.extend_from_slice(&split.put_rows.to_le_bytes());
        bytes.extend_from_slice(&split.tombstone_rows.to_le_bytes());
    }
    debug_assert_eq!(bytes.len(), splits.len() * TABLE_ROW_SPLIT_ENTRY_LEN);
    Ok(bytes)
}

pub(crate) fn decode_table_row_split_extension_payload(
    bytes: &[u8],
) -> Result<Vec<TableRowSplit>, FormatError> {
    if bytes.is_empty() || bytes.len() % TABLE_ROW_SPLIT_ENTRY_LEN != 0 {
        return Err(FormatError::InvalidLength {
            field: "table_row_split_payload",
        });
    }
    let count = bytes.len() / TABLE_ROW_SPLIT_ENTRY_LEN;
    if count > MAX_TABLE_ROW_SPLITS {
        return Err(FormatError::InvalidLength {
            field: "table_row_split_count",
        });
    }
    let mut splits = Vec::with_capacity(count);
    for chunk in bytes.chunks_exact(TABLE_ROW_SPLIT_ENTRY_LEN) {
        let mut put_bytes = [0u8; 8];
        put_bytes.copy_from_slice(&chunk[0..8]);
        let mut tombstone_bytes = [0u8; 8];
        tombstone_bytes.copy_from_slice(&chunk[8..16]);
        splits.push(TableRowSplit::new(
            u64::from_le_bytes(put_bytes),
            u64::from_le_bytes(tombstone_bytes),
        ));
    }
    Ok(splits)
}

/// Look up the row-split section (by kind) among a manifest's decoded extension
/// sections, returning `None` when absent (an older or table-less manifest).
pub(crate) fn decode_table_row_split_extension_section(
    sections: &[TableManifestExtensionSection],
) -> Result<Option<Vec<TableRowSplit>>, FormatError> {
    for section in sections {
        if section.kind() == TABLE_ROW_SPLIT_EXTENSION_KIND {
            return decode_table_row_split_extension_payload(section.payload()).map(Some);
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_row_split_extension_round_trips() {
        let splits = vec![
            TableRowSplit::new(7, 0),
            TableRowSplit::new(0, 3),
            TableRowSplit::new(4_096, 12),
        ];
        let encoded = encode_table_row_split_extension_payload(&splits).expect("encode");

        assert_eq!(encoded.len(), splits.len() * TABLE_ROW_SPLIT_ENTRY_LEN);
        assert_eq!(
            decode_table_row_split_extension_payload(&encoded),
            Ok(splits)
        );
    }

    #[test]
    fn table_row_split_extension_builds_preserved_optional_section() {
        let splits = vec![TableRowSplit::new(1, 2)];
        let section = table_row_split_extension_section(&splits).expect("section");

        assert_eq!(section.kind(), TABLE_ROW_SPLIT_EXTENSION_KIND);
        assert!(section.preserve_on_rewrite());
        assert_eq!(
            decode_table_row_split_extension_payload(section.payload()),
            Ok(splits)
        );
    }

    #[test]
    fn table_row_split_extension_section_lookup_returns_none_when_absent() {
        assert_eq!(decode_table_row_split_extension_section(&[]), Ok(None));
    }

    #[test]
    fn table_row_split_extension_encode_rejects_empty() {
        assert!(matches!(
            encode_table_row_split_extension_payload(&[]),
            Err(FormatError::InvalidLength {
                field: "table_row_split_count"
            })
        ));
    }

    #[test]
    fn table_row_split_extension_decode_rejects_empty_payload() {
        assert!(matches!(
            decode_table_row_split_extension_payload(&[]),
            Err(FormatError::InvalidLength {
                field: "table_row_split_payload"
            })
        ));
    }

    #[test]
    fn table_row_split_extension_decode_rejects_misaligned_payload() {
        assert!(matches!(
            decode_table_row_split_extension_payload(&[0u8; TABLE_ROW_SPLIT_ENTRY_LEN + 1]),
            Err(FormatError::InvalidLength {
                field: "table_row_split_payload"
            })
        ));
    }

    #[test]
    fn table_row_split_extension_full_manifest_cap_fits_section_ceiling() {
        // MAX_TABLE_ROW_SPLITS entries must encode within the section-byte ceiling that
        // `table_row_split_extension_section` (via `optional`) enforces.
        let splits = vec![TableRowSplit::new(1, 1); MAX_TABLE_ROW_SPLITS];
        let section = table_row_split_extension_section(&splits).expect("cap must fit section");
        assert_eq!(
            decode_table_row_split_extension_payload(section.payload()).map(|s| s.len()),
            Ok(MAX_TABLE_ROW_SPLITS)
        );
    }
}
