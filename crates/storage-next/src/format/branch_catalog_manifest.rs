use super::{ByteReader, FormatError, BRANCH_CATALOG_FORMAT_VERSION};
use strata_core_next::BranchId;

const FORMAT: &str = "branch_catalog_manifest";
const MAGIC: [u8; 4] = *b"STBC";
pub(crate) const MAX_BRANCH_CATALOG_ENTRIES: usize = 4096;

// Entry encoding flags
const ENTRY_FLAG_PARENT_PRESENT: u8 = 0b0000_0001;
const ENTRY_FLAG_CREATED_AT_PRESENT: u8 = 0b0000_0010;
const ENTRY_FLAG_DELETED_AT_PRESENT: u8 = 0b0000_0100;
const ENTRY_FLAGS_RESERVED: u8 =
    !(ENTRY_FLAG_PARENT_PRESENT | ENTRY_FLAG_CREATED_AT_PRESENT | ENTRY_FLAG_DELETED_AT_PRESENT);

// Status discriminants. Clearing/Deleting are not yet representable
// (the in-memory catalog collapses them synchronously); reject anything
// outside Active/Deleted at decode so stale or forward-compat catalogs
// never observe an in-flight status that this version cannot honor.
const STATUS_ACTIVE: u8 = 0;
const STATUS_DELETED: u8 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BranchCatalogStatus {
    Active,
    Deleted,
}

impl BranchCatalogStatus {
    const fn discriminant(self) -> u8 {
        match self {
            Self::Active => STATUS_ACTIVE,
            Self::Deleted => STATUS_DELETED,
        }
    }

    const fn from_discriminant(value: u8) -> Option<Self> {
        match value {
            STATUS_ACTIVE => Some(Self::Active),
            STATUS_DELETED => Some(Self::Deleted),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BranchCatalogParent {
    source_branch_id: BranchId,
    fork_version: u64,
}

impl BranchCatalogParent {
    pub(crate) const fn new(source_branch_id: BranchId, fork_version: u64) -> Self {
        Self {
            source_branch_id,
            fork_version,
        }
    }

    pub(crate) const fn source_branch_id(&self) -> BranchId {
        self.source_branch_id
    }

    pub(crate) const fn fork_version(&self) -> u64 {
        self.fork_version
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BranchCatalogEntry {
    branch_id: BranchId,
    generation: u64,
    status: BranchCatalogStatus,
    parent: Option<BranchCatalogParent>,
    created_at: Option<u64>,
    deleted_at: Option<u64>,
    state_revision: u64,
}

impl BranchCatalogEntry {
    pub(crate) fn new(
        branch_id: BranchId,
        generation: u64,
        status: BranchCatalogStatus,
    ) -> Result<Self, FormatError> {
        if generation == 0 {
            return Err(FormatError::InvalidValue {
                field: "branch_catalog_entry.generation",
            });
        }
        Ok(Self {
            branch_id,
            generation,
            status,
            parent: None,
            created_at: None,
            deleted_at: None,
            state_revision: 0,
        })
    }

    pub(crate) fn with_parent(mut self, parent: BranchCatalogParent) -> Self {
        self.parent = Some(parent);
        self
    }

    pub(crate) fn with_created_at(mut self, created_at: u64) -> Result<Self, FormatError> {
        if created_at == 0 {
            return Err(FormatError::InvalidValue {
                field: "branch_catalog_entry.created_at",
            });
        }
        self.created_at = Some(created_at);
        Ok(self)
    }

    pub(crate) fn with_deleted_at(mut self, deleted_at: u64) -> Result<Self, FormatError> {
        if deleted_at == 0 {
            return Err(FormatError::InvalidValue {
                field: "branch_catalog_entry.deleted_at",
            });
        }
        self.deleted_at = Some(deleted_at);
        Ok(self)
    }

    pub(crate) const fn with_state_revision(mut self, state_revision: u64) -> Self {
        self.state_revision = state_revision;
        self
    }

    #[allow(
        dead_code,
        reason = "exposed for recovery rebuild and runtime publish tests"
    )]
    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) const fn status(&self) -> BranchCatalogStatus {
        self.status
    }

    pub(crate) const fn parent(&self) -> Option<BranchCatalogParent> {
        self.parent
    }

    pub(crate) const fn created_at(&self) -> Option<u64> {
        self.created_at
    }

    pub(crate) const fn deleted_at(&self) -> Option<u64> {
        self.deleted_at
    }

    #[allow(
        dead_code,
        reason = "exposed for recovery rebuild and runtime publish tests"
    )]
    pub(crate) const fn state_revision(&self) -> u64 {
        self.state_revision
    }

    fn encoded_flags(&self) -> u8 {
        let mut flags = 0;
        if self.parent.is_some() {
            flags |= ENTRY_FLAG_PARENT_PRESENT;
        }
        if self.created_at.is_some() {
            flags |= ENTRY_FLAG_CREATED_AT_PRESENT;
        }
        if self.deleted_at.is_some() {
            flags |= ENTRY_FLAG_DELETED_AT_PRESENT;
        }
        flags
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchCatalogManifest {
    database_id: [u8; 16],
    manifest_sequence: u64,
    entries: Vec<BranchCatalogEntry>,
}

impl BranchCatalogManifest {
    pub(crate) fn new(
        database_id: [u8; 16],
        manifest_sequence: u64,
        entries: Vec<BranchCatalogEntry>,
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
        reason = "exposed for recovery rebuild and runtime publish tests"
    )]
    pub(crate) const fn database_id(&self) -> &[u8; 16] {
        &self.database_id
    }

    #[allow(
        dead_code,
        reason = "exposed for recovery rebuild and runtime publish tests"
    )]
    pub(crate) const fn manifest_sequence(&self) -> u64 {
        self.manifest_sequence
    }

    pub(crate) fn entries(&self) -> &[BranchCatalogEntry] {
        &self.entries
    }
}

pub(crate) fn encode_branch_catalog_manifest(
    manifest: &BranchCatalogManifest,
) -> Result<Vec<u8>, FormatError> {
    validate_manifest_sequence(manifest.manifest_sequence)?;
    validate_entries(&manifest.entries)?;
    let entry_count =
        u32::try_from(manifest.entries.len()).map_err(|_| FormatError::InvalidLength {
            field: "branch_catalog_entry_count",
        })?;

    // Header (36 bytes) + entries + crc trailer (4 bytes).
    let mut bytes = Vec::with_capacity(40 + manifest.entries.len() * 34);
    bytes.extend_from_slice(&MAGIC);
    bytes.extend_from_slice(&BRANCH_CATALOG_FORMAT_VERSION.to_le_bytes());
    bytes.extend_from_slice(&manifest.database_id);
    bytes.extend_from_slice(&manifest.manifest_sequence.to_le_bytes());
    bytes.extend_from_slice(&entry_count.to_le_bytes());

    for entry in &manifest.entries {
        bytes.extend_from_slice(entry.branch_id.as_bytes());
        bytes.extend_from_slice(&entry.generation.to_le_bytes());
        bytes.push(entry.status.discriminant());
        bytes.push(entry.encoded_flags());
        bytes.extend_from_slice(&entry.state_revision.to_le_bytes());
        if let Some(parent) = entry.parent {
            bytes.extend_from_slice(parent.source_branch_id.as_bytes());
            bytes.extend_from_slice(&parent.fork_version.to_le_bytes());
        }
        if let Some(created_at) = entry.created_at {
            bytes.extend_from_slice(&created_at.to_le_bytes());
        }
        if let Some(deleted_at) = entry.deleted_at {
            bytes.extend_from_slice(&deleted_at.to_le_bytes());
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
pub(crate) fn decode_branch_catalog_manifest(
    bytes: &[u8],
) -> Result<BranchCatalogManifest, FormatError> {
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
        BRANCH_CATALOG_FORMAT_VERSION => {}
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
                max_supported: BRANCH_CATALOG_FORMAT_VERSION,
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
            field: "branch_catalog_manifest_sequence",
        });
    }

    let entry_count_raw = reader.read_u32_le()?;
    let entry_count = usize::try_from(entry_count_raw).map_err(|_| FormatError::InvalidLength {
        field: "branch_catalog_entry_count",
    })?;
    if entry_count > MAX_BRANCH_CATALOG_ENTRIES {
        return Err(FormatError::InvalidLength {
            field: "branch_catalog_entry_count",
        });
    }

    let mut entries = Vec::with_capacity(entry_count);
    let mut previous_branch_bytes: Option<[u8; BranchId::BYTE_LEN]> = None;
    for _ in 0..entry_count {
        let branch_bytes_slice = reader.read_exact(BranchId::BYTE_LEN)?;
        let branch_bytes =
            <[u8; BranchId::BYTE_LEN]>::try_from(branch_bytes_slice).map_err(|_| {
                FormatError::InvalidLength {
                    field: "branch_catalog_entry.branch_id",
                }
            })?;
        // Entries are stored in strictly ascending branch_id byte order so
        // recovery can detect duplicates and the encoding is canonical.
        if let Some(prev) = previous_branch_bytes {
            if branch_bytes <= prev {
                return Err(FormatError::InvalidValue {
                    field: "branch_catalog_entries_order",
                });
            }
        }
        previous_branch_bytes = Some(branch_bytes);
        let branch_id = BranchId::from_bytes(branch_bytes);

        let generation = reader.read_u64_le()?;
        if generation == 0 {
            return Err(FormatError::InvalidValue {
                field: "branch_catalog_entry.generation",
            });
        }

        let status_byte = reader.read_u8()?;
        let status = BranchCatalogStatus::from_discriminant(status_byte).ok_or(
            FormatError::InvalidValue {
                field: "branch_catalog_entry.status",
            },
        )?;

        let flags = reader.read_u8()?;
        if flags & ENTRY_FLAGS_RESERVED != 0 {
            return Err(FormatError::UnsupportedFlags {
                format: FORMAT,
                flags: u32::from(flags),
            });
        }
        let state_revision = reader.read_u64_le()?;

        let parent =
            if flags & ENTRY_FLAG_PARENT_PRESENT != 0 {
                let source_bytes_slice = reader.read_exact(BranchId::BYTE_LEN)?;
                let source_bytes = <[u8; BranchId::BYTE_LEN]>::try_from(source_bytes_slice)
                    .map_err(|_| FormatError::InvalidLength {
                        field: "branch_catalog_entry.parent.source_branch_id",
                    })?;
                let fork_version = reader.read_u64_le()?;
                Some(BranchCatalogParent {
                    source_branch_id: BranchId::from_bytes(source_bytes),
                    fork_version,
                })
            } else {
                None
            };

        let created_at = if flags & ENTRY_FLAG_CREATED_AT_PRESENT != 0 {
            let value = reader.read_u64_le()?;
            if value == 0 {
                return Err(FormatError::InvalidValue {
                    field: "branch_catalog_entry.created_at",
                });
            }
            Some(value)
        } else {
            None
        };

        let deleted_at = if flags & ENTRY_FLAG_DELETED_AT_PRESENT != 0 {
            let value = reader.read_u64_le()?;
            if value == 0 {
                return Err(FormatError::InvalidValue {
                    field: "branch_catalog_entry.deleted_at",
                });
            }
            Some(value)
        } else {
            None
        };

        entries.push(BranchCatalogEntry {
            branch_id,
            generation,
            status,
            parent,
            created_at,
            deleted_at,
            state_revision,
        });
    }

    reader.finish()?;

    Ok(BranchCatalogManifest {
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
            field: "branch_catalog_manifest_sequence",
        });
    }
    Ok(())
}

fn validate_entries(entries: &[BranchCatalogEntry]) -> Result<(), FormatError> {
    if entries.len() > MAX_BRANCH_CATALOG_ENTRIES {
        return Err(FormatError::InvalidLength {
            field: "branch_catalog_entry_count",
        });
    }
    let mut previous: Option<&[u8; BranchId::BYTE_LEN]> = None;
    for entry in entries {
        if entry.generation == 0 {
            return Err(FormatError::InvalidValue {
                field: "branch_catalog_entry.generation",
            });
        }
        if let Some(value) = entry.created_at {
            if value == 0 {
                return Err(FormatError::InvalidValue {
                    field: "branch_catalog_entry.created_at",
                });
            }
        }
        if let Some(value) = entry.deleted_at {
            if value == 0 {
                return Err(FormatError::InvalidValue {
                    field: "branch_catalog_entry.deleted_at",
                });
            }
        }
        let bytes = entry.branch_id.as_bytes();
        if let Some(prev) = previous {
            if bytes <= prev {
                return Err(FormatError::InvalidValue {
                    field: "branch_catalog_entries_order",
                });
            }
        }
        previous = Some(bytes);
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
        [0xAB; 16]
    }

    #[test]
    fn branch_catalog_manifest_encodes_empty() {
        let manifest =
            BranchCatalogManifest::new(database_id(), 1, Vec::new()).expect("empty manifest");
        let encoded = encode_branch_catalog_manifest(&manifest).expect("encode");
        // header (36 bytes) + crc (4 bytes) = 40 bytes
        assert_eq!(encoded.len(), 40);
        let decoded = decode_branch_catalog_manifest(&encoded).expect("decode");
        assert_eq!(decoded, manifest);
    }

    #[test]
    fn branch_catalog_manifest_decode_rejects_trailing_data() {
        let manifest = BranchCatalogManifest::new(database_id(), 1, Vec::new()).expect("manifest");
        let mut encoded = encode_branch_catalog_manifest(&manifest).expect("encode");
        encoded.push(0); // extra byte
        let err = decode_branch_catalog_manifest(&encoded).expect_err("trailing data rejected");
        assert!(matches!(err, FormatError::ChecksumMismatch { .. }));
    }

    #[test]
    fn branch_catalog_manifest_decode_rejects_bad_magic() {
        let manifest = BranchCatalogManifest::new(database_id(), 1, Vec::new()).expect("manifest");
        let mut encoded = encode_branch_catalog_manifest(&manifest).expect("encode");
        encoded[0] = b'X';
        let err = decode_branch_catalog_manifest(&encoded).expect_err("bad magic");
        assert!(matches!(err, FormatError::InvalidMagic { .. }));
    }

    #[test]
    fn branch_catalog_manifest_decode_rejects_unsupported_version() {
        let mut bytes = Vec::with_capacity(40);
        bytes.extend_from_slice(&MAGIC);
        bytes.extend_from_slice(&99_u32.to_le_bytes());
        bytes.extend_from_slice(&database_id());
        bytes.extend_from_slice(&1_u64.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        let crc = crc32fast::hash(&bytes);
        bytes.extend_from_slice(&crc.to_le_bytes());
        let err = decode_branch_catalog_manifest(&bytes).expect_err("unsupported version");
        assert!(matches!(err, FormatError::FutureFormat { .. }));
    }

    #[test]
    fn branch_catalog_manifest_decode_rejects_zero_generation() {
        let mut bytes = Vec::with_capacity(80);
        bytes.extend_from_slice(&MAGIC);
        bytes.extend_from_slice(&BRANCH_CATALOG_FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&database_id());
        bytes.extend_from_slice(&1_u64.to_le_bytes());
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        // entry with zero generation
        bytes.extend_from_slice(branch(0x10).as_bytes());
        bytes.extend_from_slice(&0_u64.to_le_bytes()); // generation = 0 (invalid)
        bytes.push(STATUS_ACTIVE);
        bytes.push(0); // flags
        bytes.extend_from_slice(&0_u64.to_le_bytes()); // state_revision
        let crc = crc32fast::hash(&bytes);
        bytes.extend_from_slice(&crc.to_le_bytes());
        let err = decode_branch_catalog_manifest(&bytes).expect_err("zero generation");
        assert!(matches!(
            err,
            FormatError::InvalidValue {
                field: "branch_catalog_entry.generation",
            }
        ));
    }

    #[test]
    fn branch_catalog_manifest_decode_rejects_too_many_entries() {
        let mut bytes = Vec::with_capacity(40);
        bytes.extend_from_slice(&MAGIC);
        bytes.extend_from_slice(&BRANCH_CATALOG_FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&database_id());
        bytes.extend_from_slice(&1_u64.to_le_bytes());
        let too_many: u32 =
            u32::try_from(MAX_BRANCH_CATALOG_ENTRIES).expect("test cap fits in u32") + 1;
        bytes.extend_from_slice(&too_many.to_le_bytes());
        let crc = crc32fast::hash(&bytes);
        bytes.extend_from_slice(&crc.to_le_bytes());
        let err = decode_branch_catalog_manifest(&bytes).expect_err("too many entries");
        assert!(matches!(
            err,
            FormatError::InvalidLength {
                field: "branch_catalog_entry_count",
            }
        ));
    }

    #[test]
    fn branch_catalog_manifest_round_trip_active() {
        let entry = BranchCatalogEntry::new(branch(0x10), 3, BranchCatalogStatus::Active)
            .expect("entry")
            .with_created_at(7)
            .expect("created_at")
            .with_state_revision(2);
        let manifest = BranchCatalogManifest::new(database_id(), 5, vec![entry]).expect("manifest");
        let encoded = encode_branch_catalog_manifest(&manifest).expect("encode");
        let decoded = decode_branch_catalog_manifest(&encoded).expect("decode");
        assert_eq!(decoded, manifest);
        assert_eq!(decoded.entries()[0].status(), BranchCatalogStatus::Active);
        assert_eq!(decoded.entries()[0].generation(), 3);
        assert_eq!(decoded.entries()[0].created_at(), Some(7));
    }

    #[test]
    fn branch_catalog_manifest_round_trip_active_and_deleted() {
        let active = BranchCatalogEntry::new(branch(0x10), 1, BranchCatalogStatus::Active)
            .expect("active entry");
        let deleted = BranchCatalogEntry::new(branch(0x22), 4, BranchCatalogStatus::Deleted)
            .expect("deleted entry")
            .with_deleted_at(11)
            .expect("deleted_at");
        let manifest =
            BranchCatalogManifest::new(database_id(), 7, vec![active, deleted]).expect("manifest");
        let encoded = encode_branch_catalog_manifest(&manifest).expect("encode");
        let decoded = decode_branch_catalog_manifest(&encoded).expect("decode");
        assert_eq!(decoded, manifest);
        assert_eq!(decoded.entries().len(), 2);
        assert_eq!(decoded.entries()[0].status(), BranchCatalogStatus::Active);
        assert_eq!(decoded.entries()[1].status(), BranchCatalogStatus::Deleted);
        assert_eq!(decoded.entries()[1].deleted_at(), Some(11));
    }

    #[test]
    fn branch_catalog_manifest_round_trip_with_parent() {
        let entry = BranchCatalogEntry::new(branch(0x40), 2, BranchCatalogStatus::Active)
            .expect("entry")
            .with_parent(BranchCatalogParent::new(branch(0x10), 9))
            .with_created_at(10)
            .expect("created_at");
        let manifest = BranchCatalogManifest::new(database_id(), 3, vec![entry]).expect("manifest");
        let encoded = encode_branch_catalog_manifest(&manifest).expect("encode");
        let decoded = decode_branch_catalog_manifest(&encoded).expect("decode");
        assert_eq!(decoded, manifest);
        let parent = decoded.entries()[0].parent().expect("parent");
        assert_eq!(parent.source_branch_id(), branch(0x10));
        assert_eq!(parent.fork_version(), 9);
    }

    #[test]
    fn branch_catalog_manifest_entries_sorted_deterministically() {
        // Construction with unsorted entries fails (branch_id order enforced).
        let a = BranchCatalogEntry::new(branch(0x40), 1, BranchCatalogStatus::Active).expect("a");
        let b = BranchCatalogEntry::new(branch(0x10), 1, BranchCatalogStatus::Active).expect("b");
        let err = BranchCatalogManifest::new(database_id(), 1, vec![a, b])
            .expect_err("unsorted entries rejected");
        assert!(matches!(
            err,
            FormatError::InvalidValue {
                field: "branch_catalog_entries_order",
            }
        ));
    }

    /// Helper for emitting golden vector hex content. Print with
    /// `cargo test -p strata-storage-next --lib format::branch_catalog_manifest::tests::emit_goldens_for_capture -- --ignored --nocapture`
    /// Each line of output is a comment + hex stream to copy into a .hex file.
    #[test]
    #[ignore = "manual capture for golden vector generation"]
    fn emit_goldens_for_capture() {
        use std::fmt::Write;
        fn print_hex(name: &str, manifest: &BranchCatalogManifest) {
            let bytes = encode_branch_catalog_manifest(manifest).expect("encode");
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
        let empty = BranchCatalogManifest::new(database_id(), 1, Vec::new()).unwrap();
        print_hex("branch-catalog-manifest-empty.hex", &empty);

        let active = BranchCatalogEntry::new(branch(0x10), 3, BranchCatalogStatus::Active)
            .unwrap()
            .with_created_at(7)
            .unwrap()
            .with_state_revision(2);
        let single = BranchCatalogManifest::new(database_id(), 5, vec![active]).unwrap();
        print_hex("branch-catalog-manifest-single-active.hex", &single);

        let active = BranchCatalogEntry::new(branch(0x10), 1, BranchCatalogStatus::Active).unwrap();
        let deleted = BranchCatalogEntry::new(branch(0x22), 4, BranchCatalogStatus::Deleted)
            .unwrap()
            .with_deleted_at(11)
            .unwrap();
        let mixed = BranchCatalogManifest::new(database_id(), 7, vec![active, deleted]).unwrap();
        print_hex("branch-catalog-manifest-active-and-deleted.hex", &mixed);

        let with_parent = BranchCatalogEntry::new(branch(0x40), 2, BranchCatalogStatus::Active)
            .unwrap()
            .with_parent(BranchCatalogParent::new(branch(0x10), 9))
            .with_created_at(10)
            .unwrap();
        let parented = BranchCatalogManifest::new(database_id(), 3, vec![with_parent]).unwrap();
        print_hex("branch-catalog-manifest-with-parent.hex", &parented);
    }

    #[test]
    fn branch_catalog_manifest_decode_rejects_unsorted_entries() {
        // Forge bytes with two entries in descending branch_id order.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&MAGIC);
        bytes.extend_from_slice(&BRANCH_CATALOG_FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&database_id());
        bytes.extend_from_slice(&1_u64.to_le_bytes());
        bytes.extend_from_slice(&2_u32.to_le_bytes());
        // Entry 1: branch 0x40
        bytes.extend_from_slice(branch(0x40).as_bytes());
        bytes.extend_from_slice(&1_u64.to_le_bytes());
        bytes.push(STATUS_ACTIVE);
        bytes.push(0);
        bytes.extend_from_slice(&0_u64.to_le_bytes());
        // Entry 2: branch 0x10 (out of order)
        bytes.extend_from_slice(branch(0x10).as_bytes());
        bytes.extend_from_slice(&1_u64.to_le_bytes());
        bytes.push(STATUS_ACTIVE);
        bytes.push(0);
        bytes.extend_from_slice(&0_u64.to_le_bytes());
        let crc = crc32fast::hash(&bytes);
        bytes.extend_from_slice(&crc.to_le_bytes());
        let err = decode_branch_catalog_manifest(&bytes).expect_err("unsorted rejected");
        assert!(matches!(
            err,
            FormatError::InvalidValue {
                field: "branch_catalog_entries_order",
            }
        ));
    }
}
