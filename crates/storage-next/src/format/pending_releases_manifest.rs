use super::{ByteReader, FormatError, PENDING_RELEASES_FORMAT_VERSION};
use strata_core_next::BranchId;

const FORMAT: &str = "pending_releases_manifest";
const MAGIC: [u8; 4] = *b"STPR";
pub(crate) const MAX_PENDING_RELEASE_ENTRIES: usize = 4096;
pub(crate) const MAX_RELEASED_TABLES_PER_ENTRY: usize = 4096;
pub(crate) const MAX_TABLE_IDENTITY_LEN: usize = 256;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PendingReleasesEntry {
    branch_id: BranchId,
    released_tables: Vec<String>,
}

impl PendingReleasesEntry {
    pub(crate) fn new(
        branch_id: BranchId,
        released_tables: Vec<String>,
    ) -> Result<Self, FormatError> {
        validate_released_tables(&released_tables)?;
        Ok(Self {
            branch_id,
            released_tables,
        })
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) fn released_tables(&self) -> &[String] {
        &self.released_tables
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PendingReleasesManifest {
    database_id: [u8; 16],
    manifest_sequence: u64,
    entries: Vec<PendingReleasesEntry>,
}

impl PendingReleasesManifest {
    pub(crate) fn new(
        database_id: [u8; 16],
        manifest_sequence: u64,
        entries: Vec<PendingReleasesEntry>,
    ) -> Result<Self, FormatError> {
        validate_manifest_sequence(manifest_sequence)?;
        validate_entries(&entries)?;
        Ok(Self {
            database_id,
            manifest_sequence,
            entries,
        })
    }

    #[allow(
        dead_code,
        reason = "database_id is asserted by recovery integrity tests"
    )]
    pub(crate) const fn database_id(&self) -> &[u8; 16] {
        &self.database_id
    }

    pub(crate) const fn manifest_sequence(&self) -> u64 {
        self.manifest_sequence
    }

    pub(crate) fn entries(&self) -> &[PendingReleasesEntry] {
        &self.entries
    }
}

pub(crate) fn encode_pending_releases_manifest(
    manifest: &PendingReleasesManifest,
) -> Result<Vec<u8>, FormatError> {
    validate_manifest_sequence(manifest.manifest_sequence)?;
    validate_entries(&manifest.entries)?;
    let entry_count =
        u32::try_from(manifest.entries.len()).map_err(|_| FormatError::InvalidLength {
            field: "pending_releases_entry_count",
        })?;

    // Header (36 bytes) + entries + crc trailer (4 bytes).
    let mut bytes = Vec::with_capacity(40);
    bytes.extend_from_slice(&MAGIC);
    bytes.extend_from_slice(&PENDING_RELEASES_FORMAT_VERSION.to_le_bytes());
    bytes.extend_from_slice(&manifest.database_id);
    bytes.extend_from_slice(&manifest.manifest_sequence.to_le_bytes());
    bytes.extend_from_slice(&entry_count.to_le_bytes());

    for entry in &manifest.entries {
        bytes.extend_from_slice(entry.branch_id.as_bytes());
        let released_count =
            u32::try_from(entry.released_tables.len()).map_err(|_| FormatError::InvalidLength {
                field: "pending_releases_entry.released_tables_count",
            })?;
        bytes.extend_from_slice(&released_count.to_le_bytes());
        for identity in &entry.released_tables {
            let identity_bytes = identity.as_bytes();
            let identity_len =
                u32::try_from(identity_bytes.len()).map_err(|_| FormatError::InvalidLength {
                    field: "pending_releases_entry.released_table_identity_len",
                })?;
            bytes.extend_from_slice(&identity_len.to_le_bytes());
            bytes.extend_from_slice(identity_bytes);
        }
    }

    let crc = crc32fast::hash(&bytes);
    bytes.extend_from_slice(&crc.to_le_bytes());
    Ok(bytes)
}

#[allow(
    clippy::too_many_lines,
    reason = "single-pass decoder reads header + entry blocks; splitting would split related validation"
)]
pub(crate) fn decode_pending_releases_manifest(
    bytes: &[u8],
) -> Result<PendingReleasesManifest, FormatError> {
    if bytes.len() < minimum_manifest_len() {
        return Err(FormatError::InsufficientBytes {
            format: FORMAT,
            needed: minimum_manifest_len(),
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
    match version {
        PENDING_RELEASES_FORMAT_VERSION => {}
        0 => {
            return Err(FormatError::PreV1Format {
                format: FORMAT,
                version,
            });
        }
        version => {
            return Err(FormatError::FutureFormat {
                format: FORMAT,
                version,
                max_supported: PENDING_RELEASES_FORMAT_VERSION,
            });
        }
    }

    let computed_crc = crc32fast::hash(&bytes[..checksum_offset]);
    if stored_crc != computed_crc {
        return Err(FormatError::ChecksumMismatch {
            format: FORMAT,
            expected: stored_crc,
            computed: computed_crc,
        });
    }

    let database_id_slice = reader.read_exact(16)?;
    let database_id =
        <[u8; 16]>::try_from(database_id_slice).map_err(|_| FormatError::InvalidLength {
            field: "database_id",
        })?;

    let manifest_sequence = reader.read_u64_le()?;
    if manifest_sequence == 0 {
        return Err(FormatError::InvalidValue {
            field: "pending_releases_manifest_sequence",
        });
    }

    let entry_count_raw = reader.read_u32_le()?;
    let entry_count = usize::try_from(entry_count_raw).map_err(|_| FormatError::InvalidLength {
        field: "pending_releases_entry_count",
    })?;
    if entry_count > MAX_PENDING_RELEASE_ENTRIES {
        return Err(FormatError::InvalidLength {
            field: "pending_releases_entry_count",
        });
    }

    let mut entries = Vec::with_capacity(entry_count);
    let mut previous_branch_bytes: Option<[u8; BranchId::BYTE_LEN]> = None;
    for _ in 0..entry_count {
        let branch_bytes_slice = reader.read_exact(BranchId::BYTE_LEN)?;
        let branch_bytes =
            <[u8; BranchId::BYTE_LEN]>::try_from(branch_bytes_slice).map_err(|_| {
                FormatError::InvalidLength {
                    field: "pending_releases_entry.branch_id",
                }
            })?;
        if let Some(prev) = previous_branch_bytes {
            if branch_bytes <= prev {
                return Err(FormatError::InvalidValue {
                    field: "pending_releases_entries_order",
                });
            }
        }
        previous_branch_bytes = Some(branch_bytes);
        let branch_id = BranchId::from_bytes(branch_bytes);

        let released_count_raw = reader.read_u32_le()?;
        let released_count =
            usize::try_from(released_count_raw).map_err(|_| FormatError::InvalidLength {
                field: "pending_releases_entry.released_tables_count",
            })?;
        if released_count > MAX_RELEASED_TABLES_PER_ENTRY {
            return Err(FormatError::InvalidLength {
                field: "pending_releases_entry.released_tables_count",
            });
        }

        let mut released_tables = Vec::with_capacity(released_count);
        for _ in 0..released_count {
            let identity_len_raw = reader.read_u32_le()?;
            let identity_len =
                usize::try_from(identity_len_raw).map_err(|_| FormatError::InvalidLength {
                    field: "pending_releases_entry.released_table_identity_len",
                })?;
            if identity_len == 0 || identity_len > MAX_TABLE_IDENTITY_LEN {
                return Err(FormatError::InvalidLength {
                    field: "pending_releases_entry.released_table_identity_len",
                });
            }
            let identity_slice = reader.read_exact(identity_len)?;
            let identity = std::str::from_utf8(identity_slice)
                .map_err(|_| FormatError::InvalidValue {
                    field: "pending_releases_entry.released_table_identity",
                })?
                .to_owned();
            released_tables.push(identity);
        }

        entries.push(PendingReleasesEntry {
            branch_id,
            released_tables,
        });
    }

    reader.finish()?;

    Ok(PendingReleasesManifest {
        database_id,
        manifest_sequence,
        entries,
    })
}

const fn minimum_manifest_len() -> usize {
    // magic + version + database_id + manifest_sequence + entry_count + crc
    4 + 4 + 16 + 8 + 4 + 4
}

fn validate_manifest_sequence(manifest_sequence: u64) -> Result<(), FormatError> {
    if manifest_sequence == 0 {
        return Err(FormatError::InvalidValue {
            field: "pending_releases_manifest_sequence",
        });
    }
    Ok(())
}

fn validate_entries(entries: &[PendingReleasesEntry]) -> Result<(), FormatError> {
    if entries.len() > MAX_PENDING_RELEASE_ENTRIES {
        return Err(FormatError::InvalidLength {
            field: "pending_releases_entry_count",
        });
    }
    let mut previous: Option<&[u8; BranchId::BYTE_LEN]> = None;
    for entry in entries {
        validate_released_tables(&entry.released_tables)?;
        let bytes = entry.branch_id.as_bytes();
        if let Some(prev) = previous {
            if bytes <= prev {
                return Err(FormatError::InvalidValue {
                    field: "pending_releases_entries_order",
                });
            }
        }
        previous = Some(bytes);
    }
    Ok(())
}

fn validate_released_tables(released_tables: &[String]) -> Result<(), FormatError> {
    if released_tables.len() > MAX_RELEASED_TABLES_PER_ENTRY {
        return Err(FormatError::InvalidLength {
            field: "pending_releases_entry.released_tables_count",
        });
    }
    for identity in released_tables {
        let identity_bytes = identity.as_bytes();
        if identity_bytes.is_empty() || identity_bytes.len() > MAX_TABLE_IDENTITY_LEN {
            return Err(FormatError::InvalidLength {
                field: "pending_releases_entry.released_table_identity_len",
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn branch(byte: u8) -> BranchId {
        BranchId::from_bytes([byte; BranchId::BYTE_LEN])
    }

    fn database_id() -> [u8; 16] {
        [0xCD; 16]
    }

    #[test]
    fn pending_releases_manifest_encodes_empty() {
        let manifest =
            PendingReleasesManifest::new(database_id(), 1, Vec::new()).expect("empty manifest");
        let encoded = encode_pending_releases_manifest(&manifest).expect("encode");
        // header (36 bytes) + crc (4 bytes) = 40 bytes
        assert_eq!(encoded.len(), 40);
        let decoded = decode_pending_releases_manifest(&encoded).expect("decode");
        assert_eq!(decoded, manifest);
    }

    #[test]
    fn pending_releases_manifest_round_trips_single_entry() {
        let entry =
            PendingReleasesEntry::new(branch(0x21), vec!["table-alpha".to_owned()]).expect("entry");
        let manifest =
            PendingReleasesManifest::new(database_id(), 5, vec![entry]).expect("manifest");
        let encoded = encode_pending_releases_manifest(&manifest).expect("encode");
        let decoded = decode_pending_releases_manifest(&encoded).expect("decode");
        assert_eq!(decoded, manifest);
    }

    #[test]
    fn pending_releases_manifest_round_trips_multiple_entries() {
        let entry_a = PendingReleasesEntry::new(
            branch(0x11),
            vec!["table-a-1".to_owned(), "table-a-2".to_owned()],
        )
        .expect("entry a");
        let entry_b =
            PendingReleasesEntry::new(branch(0x22), vec!["table-b-1".to_owned()]).expect("entry b");
        let manifest = PendingReleasesManifest::new(database_id(), 9, vec![entry_a, entry_b])
            .expect("manifest");
        let encoded = encode_pending_releases_manifest(&manifest).expect("encode");
        let decoded = decode_pending_releases_manifest(&encoded).expect("decode");
        assert_eq!(decoded, manifest);
    }

    #[test]
    fn pending_releases_manifest_rejects_bad_magic() {
        let manifest =
            PendingReleasesManifest::new(database_id(), 1, Vec::new()).expect("manifest");
        let mut encoded = encode_pending_releases_manifest(&manifest).expect("encode");
        encoded[0] = b'X';
        let err = decode_pending_releases_manifest(&encoded).expect_err("bad magic rejected");
        assert!(matches!(err, FormatError::InvalidMagic { .. }));
    }

    #[test]
    fn pending_releases_manifest_rejects_future_version() {
        let manifest =
            PendingReleasesManifest::new(database_id(), 1, Vec::new()).expect("manifest");
        let mut encoded = encode_pending_releases_manifest(&manifest).expect("encode");
        // Version field is at offset 4..8.
        encoded[4] = 99;
        let err = decode_pending_releases_manifest(&encoded).expect_err("future version rejected");
        assert!(matches!(err, FormatError::FutureFormat { .. }));
    }

    #[test]
    fn pending_releases_manifest_rejects_zero_sequence() {
        let result = PendingReleasesManifest::new(database_id(), 0, Vec::new());
        assert!(matches!(result, Err(FormatError::InvalidValue { .. })));
    }

    #[test]
    fn pending_releases_manifest_rejects_unsorted_entries() {
        let entry_a = PendingReleasesEntry::new(branch(0x44), Vec::new()).expect("entry a");
        let entry_b = PendingReleasesEntry::new(branch(0x22), Vec::new()).expect("entry b");
        let result = PendingReleasesManifest::new(database_id(), 1, vec![entry_a, entry_b]);
        assert!(matches!(result, Err(FormatError::InvalidValue { .. })));
    }

    #[test]
    fn pending_releases_manifest_rejects_empty_identity() {
        let result = PendingReleasesEntry::new(branch(0x21), vec![String::new()]);
        assert!(matches!(result, Err(FormatError::InvalidLength { .. })));
    }

    #[test]
    fn pending_releases_manifest_rejects_trailing_data() {
        let manifest =
            PendingReleasesManifest::new(database_id(), 1, Vec::new()).expect("manifest");
        let mut encoded = encode_pending_releases_manifest(&manifest).expect("encode");
        encoded.push(0);
        let err = decode_pending_releases_manifest(&encoded).expect_err("trailing data rejected");
        assert!(matches!(err, FormatError::ChecksumMismatch { .. }));
    }

    #[test]
    fn pending_releases_manifest_rejects_oversized_entry_count() {
        // Build encoded bytes with a giant entry_count field.
        let manifest =
            PendingReleasesManifest::new(database_id(), 1, Vec::new()).expect("manifest");
        let mut encoded = encode_pending_releases_manifest(&manifest).expect("encode");
        // Strip the 4-byte CRC trailer so we can tamper with the entry_count
        // and reattach a fresh CRC over the modified body.
        let crc_len = 4;
        encoded.truncate(encoded.len() - crc_len);
        let entry_count_offset = encoded.len() - 4;
        let oversized = u32::try_from(MAX_PENDING_RELEASE_ENTRIES + 1)
            .expect("oversized fixture fits in u32");
        encoded[entry_count_offset..entry_count_offset + 4]
            .copy_from_slice(&oversized.to_le_bytes());
        let fresh_crc = crc32fast::hash(&encoded);
        encoded.extend_from_slice(&fresh_crc.to_le_bytes());
        let err =
            decode_pending_releases_manifest(&encoded).expect_err("oversized entry_count rejected");
        assert!(matches!(err, FormatError::InvalidLength { .. }));
    }

    /// Helper for emitting golden vector hex content. Print with
    /// `cargo test -p strata-storage-next --lib format::pending_releases_manifest::tests::emit_goldens_for_capture -- --ignored --nocapture`
    /// Each line of output is a comment + hex stream to copy into a .hex file.
    #[test]
    #[ignore = "manual capture for golden vector generation"]
    fn emit_goldens_for_capture() {
        use std::fmt::Write;
        fn print_hex(name: &str, manifest: &PendingReleasesManifest) {
            let bytes = encode_pending_releases_manifest(manifest).expect("encode");
            let mut hex = String::with_capacity(bytes.len() * 2);
            for byte in &bytes {
                write!(hex, "{byte:02x}").expect("hex write");
            }
            println!("# {name}");
            for chunk in hex.as_bytes().chunks(64) {
                println!("{}", std::str::from_utf8(chunk).expect("ascii"));
            }
            println!();
        }
        let empty =
            PendingReleasesManifest::new(database_id(), 1, Vec::new()).expect("empty manifest");
        print_hex("pending-releases-manifest-empty.hex", &empty);

        let single_entry = PendingReleasesEntry::new(branch(0x21), vec!["table-alpha".to_owned()])
            .expect("single entry");
        let single = PendingReleasesManifest::new(database_id(), 5, vec![single_entry])
            .expect("single manifest");
        print_hex("pending-releases-manifest-single.hex", &single);

        let multi_a = PendingReleasesEntry::new(
            branch(0x11),
            vec!["table-a-1".to_owned(), "table-a-2".to_owned()],
        )
        .expect("multi entry a");
        let multi_b = PendingReleasesEntry::new(branch(0x22), vec!["table-b-1".to_owned()])
            .expect("multi entry b");
        let multi = PendingReleasesManifest::new(database_id(), 9, vec![multi_a, multi_b])
            .expect("multi manifest");
        print_hex("pending-releases-manifest-multi.hex", &multi);
    }
}
