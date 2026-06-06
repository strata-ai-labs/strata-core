# Storage Format V1 Goldens

This directory is the checked-in home for storage format V1 golden vectors.

These fixtures are source-controlled evidence for stable durable bytes. Each
fixture records exact field values in `#` comments followed by the expected hex
bytes. Normal tests verify the checked-in hex and must not rewrite it.

Current vectors:

| File | Format | Version | Purpose |
| --- | --- | --- | --- |
| `branch-catalog-manifest-active-and-deleted.hex` | Branch catalog manifest | V1 branch catalog format 1 | Branch catalog with one active branch and one deleted branch. |
| `branch-catalog-manifest-empty.hex` | Branch catalog manifest | V1 branch catalog format 1 | Empty branch catalog for a physical database. |
| `branch-catalog-manifest-single-active.hex` | Branch catalog manifest | V1 branch catalog format 1 | Branch catalog with one active branch, creation time, and state revision. |
| `branch-catalog-manifest-with-parent.hex` | Branch catalog manifest | V1 branch catalog format 1 | Branch catalog with a parent branch reference and fork version. |
| `internal-key-ordinary.hex` | Internal key | V1 key encoding | Ordinary engine-owned storage key with commit version 42. |
| `internal-key-zero-user-byte.hex` | Internal key | V1 key encoding | Storage-owned timeline key with escaped NUL bytes in the user key. |
| `manifest-identity.hex` | Manifest | V1 manifest format 1 | Database manifest with identity codec and recovery watermarks. |
| `pending-releases-manifest-empty.hex` | Pending releases manifest | V1 pending releases format 1 | Empty pending table-release manifest for a physical database. |
| `pending-releases-manifest-multi.hex` | Pending releases manifest | V1 pending releases format 1 | Pending table releases for two branches. |
| `pending-releases-manifest-single.hex` | Pending releases manifest | V1 pending releases format 1 | Pending table release for one branch. |
| `quarantine-inventory-empty.hex` | Quarantine inventory | V1 quarantine inventory format 1 | Empty branch-local quarantine inventory. |
| `quarantine-inventory-multi-entry.hex` | Quarantine inventory | V1 quarantine inventory format 1 | Branch-local quarantine inventory with two canonical entries. |
| `segment-metadata-sidecar.hex` | Segment metadata | V1 metadata format 1 | WAL segment metadata sidecar with timestamp and commit-version ranges. |
| `snapshot-container-single-section.hex` | Snapshot container | V1 snapshot format 1 | Snapshot header, one row-native section envelope, and footer CRC. |
| `snapshot-header-identity.hex` | Snapshot header | V1 snapshot format 1 | Snapshot header with database id, commit-version watermark, and identity codec id. |
| `snapshot-section-empty.hex` | Snapshot section | V1 section envelope | Generic section envelope with an empty payload. |
| `snapshot-watermark-empty.hex` | Snapshot watermark | V1 watermark encoding | Empty snapshot watermark. |
| `snapshot-watermark-present.hex` | Snapshot watermark | V1 watermark encoding | Present snapshot watermark with snapshot and transaction facts. |
| `storage-row-put.hex` | Storage row | V1 row format 1 | Generic put row with value bytes and no expiry. |
| `storage-row-tombstone.hex` | Storage row | V1 row format 1 | Generic tombstone row with no value and no expiry. |
| `immutable-table-one-block.hex` | Immutable table artifact | V1 table format 1 | Complete table with one uncompressed data block. |
| `immutable-table-two-block.hex` | Immutable table artifact | V1 table format 1 | Complete table with two uncompressed data blocks. |
| `table-data-block-one-put-uncompressed-frame.hex` | Table data block frame | V1 table block frame | Uncompressed data frame containing one put row. |
| `table-data-block-put-tombstone-uncompressed-frame.hex` | Table data block frame | V1 table block frame | Uncompressed data frame containing a put row and tombstone row. |
| `table-data-block-zstd-frame.hex` | Table data block frame | V1 table block frame | Zstd-compressed data frame containing a put row and tombstone row. |
| `table-index-block.hex` | Table index block payload | V1 table index payload | Monolithic index payload for a two-block table. |
| `table-properties-block.hex` | Table properties block payload | V1 table properties payload | Table-level facts derived from the two-block table. |
| `wal-commit-payload-one-put.hex` | WAL commit payload | V1 payload format 1 | Row-native commit payload with one put row. |
| `wal-commit-payload-put-tombstone.hex` | WAL commit payload | V1 payload format 1 | Row-native commit payload with a put row followed by a tombstone row. |
| `wal-record-empty-pre-m3f.hex` | WAL record | Historical pre-M3F fixture | Inner WAL record with an empty opaque commit payload; retained only as a malformed V1 fixture. |
| `wal-record-envelope.hex` | WAL record envelope | V1 envelope encoding | Codec-aware outer envelope around an identity-encoded WAL record. |
| `wal-record-payload.hex` | WAL record | V1 record format 1 | Inner WAL record with a row-native commit payload. |
| `wal-segment-header.hex` | WAL segment header | V1 segment format 1 | Segment header with database id, segment id, and CRC. |
