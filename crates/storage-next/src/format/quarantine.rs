use super::{ByteReader, FormatError, DATABASE_FORMAT_VERSION, MAX_CODEC_ID_LEN};
use crate::object::{ObjectName, MAX_OBJECT_NAME_BYTES};
use std::cmp::Ordering;
use std::collections::HashSet;
use strata_core_next::{BranchId, Timestamp};

const QUARANTINE_INVENTORY_FORMAT: &str = "quarantine_inventory";
const QUARANTINE_INVENTORY_MAGIC: [u8; 4] = *b"STQI";

// The lower bound lets decode reject impossible entry counts before allocating.
const MIN_ENTRY_BYTES: usize = 4 + 1 + 4 + 1 + 8 + 8;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct QuarantineInventory {
    database_id: [u8; 16],
    branch_id: BranchId,
    codec_id: String,
    entries: Vec<QuarantineEntry>,
}

impl QuarantineInventory {
    pub(crate) fn new(
        database_id: [u8; 16],
        branch_id: BranchId,
        codec_id: impl Into<String>,
        mut entries: Vec<QuarantineEntry>,
    ) -> Result<Self, FormatError> {
        entries.sort_by(compare_entries);
        Self::from_parts(database_id, branch_id, codec_id.into(), entries, true)
    }

    fn from_parts(
        database_id: [u8; 16],
        branch_id: BranchId,
        codec_id: String,
        entries: Vec<QuarantineEntry>,
        require_canonical_order: bool,
    ) -> Result<Self, FormatError> {
        validate_codec_id(&codec_id)?;
        validate_entries(&branch_id, &entries, require_canonical_order)?;
        Ok(Self {
            database_id,
            branch_id,
            codec_id,
            entries,
        })
    }

    pub(crate) const fn database_id(&self) -> &[u8; 16] {
        &self.database_id
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) fn codec_id(&self) -> &str {
        &self.codec_id
    }

    pub(crate) fn entries(&self) -> &[QuarantineEntry] {
        &self.entries
    }

    pub(crate) const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct QuarantineEntry {
    object_id: String,
    source_object: ObjectName,
    byte_count: u64,
    quarantined_at: Timestamp,
}

impl QuarantineEntry {
    pub(crate) fn new(
        object_id: impl Into<String>,
        source_object: ObjectName,
        byte_count: u64,
        quarantined_at: Timestamp,
    ) -> Result<Self, FormatError> {
        let object_id = object_id.into();
        validate_object_id(&object_id)?;
        Ok(Self {
            object_id,
            source_object,
            byte_count,
            quarantined_at,
        })
    }

    fn from_decoded(
        object_id: String,
        source_object: ObjectName,
        byte_count: u64,
        quarantined_at: Timestamp,
    ) -> Self {
        Self {
            object_id,
            source_object,
            byte_count,
            quarantined_at,
        }
    }

    pub(crate) fn object_id(&self) -> &str {
        &self.object_id
    }

    pub(crate) fn source_object(&self) -> &ObjectName {
        &self.source_object
    }

    pub(crate) const fn byte_count(&self) -> u64 {
        self.byte_count
    }

    pub(crate) const fn quarantined_at(&self) -> Timestamp {
        self.quarantined_at
    }
}

pub(crate) fn encode_quarantine_inventory(
    inventory: &QuarantineInventory,
) -> Result<Vec<u8>, FormatError> {
    validate_codec_id(inventory.codec_id())?;
    let branch_id = inventory.branch_id();
    validate_entries(&branch_id, inventory.entries(), true)?;

    let codec_len = u32::try_from(inventory.codec_id().len())
        .map_err(|_| FormatError::InvalidLength { field: "codec_id" })?;
    let entry_count =
        u32::try_from(inventory.entries().len()).map_err(|_| FormatError::InvalidLength {
            field: "entry_count",
        })?;

    let mut bytes = Vec::new();
    bytes.extend_from_slice(&QUARANTINE_INVENTORY_MAGIC);
    bytes.extend_from_slice(&DATABASE_FORMAT_VERSION.to_le_bytes());
    bytes.extend_from_slice(inventory.database_id());
    bytes.extend_from_slice(inventory.branch_id().as_bytes());
    bytes.extend_from_slice(&codec_len.to_le_bytes());
    bytes.extend_from_slice(inventory.codec_id().as_bytes());
    bytes.extend_from_slice(&entry_count.to_le_bytes());

    for entry in inventory.entries() {
        write_string(&mut bytes, "object_id", entry.object_id())?;
        write_string(&mut bytes, "source_object", entry.source_object().as_str())?;
        bytes.extend_from_slice(&entry.byte_count().to_le_bytes());
        bytes.extend_from_slice(&entry.quarantined_at().as_micros().to_le_bytes());
    }

    let crc = crc32fast::hash(&bytes);
    bytes.extend_from_slice(&crc.to_le_bytes());
    Ok(bytes)
}

pub(crate) fn decode_quarantine_inventory(
    bytes: &[u8],
) -> Result<QuarantineInventory, FormatError> {
    if bytes.len() < minimum_inventory_len() {
        return Err(FormatError::InsufficientBytes {
            format: QUARANTINE_INVENTORY_FORMAT,
            needed: minimum_inventory_len(),
            actual: bytes.len(),
        });
    }

    let checksum_offset = bytes.len() - 4;
    let stored_crc = u32::from_le_bytes(
        bytes[checksum_offset..]
            .try_into()
            .map_err(|_| FormatError::InvalidLength { field: "crc32" })?,
    );

    let mut reader = ByteReader::new(QUARANTINE_INVENTORY_FORMAT, &bytes[..checksum_offset]);
    if reader.read_exact(4)? != QUARANTINE_INVENTORY_MAGIC {
        return Err(FormatError::InvalidMagic {
            format: QUARANTINE_INVENTORY_FORMAT,
        });
    }

    let version = reader.read_u32_le()?;
    match version {
        DATABASE_FORMAT_VERSION => {}
        0 => {
            return Err(FormatError::PreV1Format {
                format: QUARANTINE_INVENTORY_FORMAT,
                version,
            });
        }
        version => {
            return Err(FormatError::FutureFormat {
                format: QUARANTINE_INVENTORY_FORMAT,
                version,
                max_supported: DATABASE_FORMAT_VERSION,
            });
        }
    }

    let computed_crc = crc32fast::hash(&bytes[..checksum_offset]);
    if stored_crc != computed_crc {
        return Err(FormatError::ChecksumMismatch {
            format: QUARANTINE_INVENTORY_FORMAT,
            expected: stored_crc,
            computed: computed_crc,
        });
    }

    let database_id = read_array_16(&mut reader, "database_id")?;
    let branch_id = BranchId::from_bytes(read_array_16(&mut reader, "branch_id")?);
    let codec_id = read_bounded_string(&mut reader, "codec_id", MAX_CODEC_ID_LEN)?;
    validate_codec_id(&codec_id)?;

    let entry_count =
        usize::try_from(reader.read_u32_le()?).map_err(|_| FormatError::InvalidLength {
            field: "entry_count",
        })?;
    // Do not trust the count until the remaining byte length can hold that many
    // minimally sized entries.
    if entry_count > reader.remaining() / MIN_ENTRY_BYTES {
        return Err(FormatError::InvalidLength {
            field: "entry_count",
        });
    }

    let mut entries = Vec::with_capacity(entry_count);
    for _ in 0..entry_count {
        let object_id = read_bounded_string(&mut reader, "object_id", MAX_OBJECT_NAME_BYTES)?;
        let source_object = ObjectName::new(read_bounded_string(
            &mut reader,
            "source_object",
            MAX_OBJECT_NAME_BYTES,
        )?)
        .map_err(|_| FormatError::InvalidValue {
            field: "source_object",
        })?;
        let byte_count = reader.read_u64_le()?;
        let quarantined_at = Timestamp::from_micros(reader.read_u64_le()?);
        entries.push(QuarantineEntry::from_decoded(
            object_id,
            source_object,
            byte_count,
            quarantined_at,
        ));
    }
    reader.finish()?;

    QuarantineInventory::from_parts(database_id, branch_id, codec_id, entries, true)
}

const fn minimum_inventory_len() -> usize {
    4 + 4 + 16 + 16 + 4 + 4 + 4
}

fn write_string(bytes: &mut Vec<u8>, field: &'static str, value: &str) -> Result<(), FormatError> {
    let len = u32::try_from(value.len()).map_err(|_| FormatError::InvalidLength { field })?;
    bytes.extend_from_slice(&len.to_le_bytes());
    bytes.extend_from_slice(value.as_bytes());
    Ok(())
}

fn read_bounded_string(
    reader: &mut ByteReader<'_>,
    field: &'static str,
    max_len: usize,
) -> Result<String, FormatError> {
    let len =
        usize::try_from(reader.read_u32_le()?).map_err(|_| FormatError::InvalidLength { field })?;
    if len > max_len {
        return Err(FormatError::InvalidLength { field });
    }
    let bytes = reader.read_exact(len)?;
    std::str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|_| FormatError::InvalidUtf8 { field })
}

fn read_array_16(
    reader: &mut ByteReader<'_>,
    field: &'static str,
) -> Result<[u8; 16], FormatError> {
    let bytes = reader.read_exact(16)?;
    <[u8; 16]>::try_from(bytes).map_err(|_| FormatError::InvalidLength { field })
}

fn validate_codec_id(codec_id: &str) -> Result<(), FormatError> {
    if codec_id.is_empty() || codec_id.len() > MAX_CODEC_ID_LEN {
        return Err(FormatError::InvalidLength { field: "codec_id" });
    }
    if codec_id.as_bytes().contains(&0x00) {
        return Err(FormatError::InvalidUtf8 { field: "codec_id" });
    }
    Ok(())
}

fn validate_entries(
    _branch_id: &BranchId,
    entries: &[QuarantineEntry],
    require_canonical_order: bool,
) -> Result<(), FormatError> {
    let mut object_ids = HashSet::new();
    let mut source_objects = HashSet::new();
    for entry in entries {
        validate_object_id(entry.object_id())?;
        if !object_ids.insert(entry.object_id()) {
            return Err(FormatError::InvalidValue { field: "object_id" });
        }
        if !source_objects.insert(entry.source_object().as_str()) {
            return Err(FormatError::InvalidValue {
                field: "source_object",
            });
        }
    }

    if require_canonical_order {
        for pair in entries.windows(2) {
            if compare_entries(&pair[0], &pair[1]).is_gt() {
                return Err(FormatError::InvalidValue {
                    field: "entry_order",
                });
            }
        }
    }
    Ok(())
}

fn validate_object_id(object_id: &str) -> Result<(), FormatError> {
    if object_id.contains('/') {
        return Err(FormatError::InvalidValue { field: "object_id" });
    }

    ObjectName::new(object_id)
        .map(|_| ())
        .map_err(|_| FormatError::InvalidValue { field: "object_id" })
}

fn compare_entries(left: &QuarantineEntry, right: &QuarantineEntry) -> Ordering {
    left.object_id().cmp(right.object_id()).then_with(|| {
        left.source_object()
            .as_str()
            .cmp(right.source_object().as_str())
    })
}

#[cfg(test)]
#[path = "quarantine/codec_exhaustive_tests.rs"]
mod codec_exhaustive_tests;

#[cfg(test)]
mod tests {
    use super::{
        decode_quarantine_inventory, encode_quarantine_inventory, write_string, QuarantineEntry,
        QuarantineInventory, QUARANTINE_INVENTORY_FORMAT, QUARANTINE_INVENTORY_MAGIC,
    };
    use crate::format::{FormatError, DATABASE_FORMAT_VERSION, MAX_CODEC_ID_LEN};
    use crate::object::{ObjectName, MAX_OBJECT_NAME_BYTES};
    use strata_core_next::{BranchId, Timestamp};

    const DATABASE_ID: [u8; 16] = [
        0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e,
        0x2f,
    ];

    fn branch_id() -> BranchId {
        BranchId::from_bytes([
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
            0x0e, 0x0f,
        ])
    }

    fn source_object(name: &str) -> ObjectName {
        ObjectName::new(name).expect("valid source object")
    }

    fn entry(object_id: &str, source: &str) -> QuarantineEntry {
        QuarantineEntry::new(
            object_id,
            source_object(source),
            128,
            Timestamp::from_micros(1_700_000_000_000_000),
        )
        .expect("quarantine entry")
    }

    fn inventory(entries: Vec<QuarantineEntry>) -> QuarantineInventory {
        QuarantineInventory::new(DATABASE_ID, branch_id(), "identity", entries)
            .expect("quarantine inventory")
    }

    fn refresh_crc(bytes: &mut Vec<u8>) {
        bytes.truncate(bytes.len() - 4);
        let crc = crc32fast::hash(bytes);
        bytes.extend_from_slice(&crc.to_le_bytes());
    }

    fn encode_inventory_unchecked(entries: &[QuarantineEntry]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&QUARANTINE_INVENTORY_MAGIC);
        bytes.extend_from_slice(&DATABASE_FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&DATABASE_ID);
        bytes.extend_from_slice(branch_id().as_bytes());
        write_string(&mut bytes, "codec_id", "identity").expect("codec id");
        let entry_count = u32::try_from(entries.len()).expect("entry count fits u32");
        bytes.extend_from_slice(&entry_count.to_le_bytes());
        for entry in entries {
            write_string(&mut bytes, "object_id", entry.object_id()).expect("object id");
            write_string(&mut bytes, "source_object", entry.source_object().as_str())
                .expect("source object");
            bytes.extend_from_slice(&entry.byte_count().to_le_bytes());
            bytes.extend_from_slice(&entry.quarantined_at().as_micros().to_le_bytes());
        }
        let crc = crc32fast::hash(&bytes);
        bytes.extend_from_slice(&crc.to_le_bytes());
        bytes
    }

    fn encode_inventory_with_raw_codec(codec_id: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&QUARANTINE_INVENTORY_MAGIC);
        bytes.extend_from_slice(&DATABASE_FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&DATABASE_ID);
        bytes.extend_from_slice(branch_id().as_bytes());
        let codec_len = u32::try_from(codec_id.len()).expect("codec length fits u32");
        bytes.extend_from_slice(&codec_len.to_le_bytes());
        bytes.extend_from_slice(codec_id);
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        let crc = crc32fast::hash(&bytes);
        bytes.extend_from_slice(&crc.to_le_bytes());
        bytes
    }

    fn encode_inventory_unchecked_raw(
        codec_id: &str,
        entries: &[(&str, &str, u64, u64)],
    ) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&QUARANTINE_INVENTORY_MAGIC);
        bytes.extend_from_slice(&DATABASE_FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&DATABASE_ID);
        bytes.extend_from_slice(branch_id().as_bytes());
        write_string(&mut bytes, "codec_id", codec_id).expect("codec id");
        let entry_count = u32::try_from(entries.len()).expect("entry count fits u32");
        bytes.extend_from_slice(&entry_count.to_le_bytes());
        for (object_id, source_object, byte_count, quarantined_at) in entries {
            write_string(&mut bytes, "object_id", object_id).expect("object id");
            write_string(&mut bytes, "source_object", source_object).expect("source object");
            bytes.extend_from_slice(&byte_count.to_le_bytes());
            bytes.extend_from_slice(&quarantined_at.to_le_bytes());
        }
        let crc = crc32fast::hash(&bytes);
        bytes.extend_from_slice(&crc.to_le_bytes());
        bytes
    }

    #[test]
    fn empty_inventory_round_trips() {
        let inventory = inventory(Vec::new());

        assert!(inventory.is_empty());
        assert_eq!(
            decode_quarantine_inventory(
                &encode_quarantine_inventory(&inventory).expect("encode inventory")
            ),
            Ok(inventory)
        );
    }

    #[test]
    fn single_entry_inventory_round_trips() {
        let inventory = inventory(vec![entry("table0001", "tables/main/l0000/table0001")]);

        assert_eq!(inventory.entries()[0].object_id(), "table0001");
        assert_eq!(inventory.entries()[0].byte_count(), 128);
        assert_eq!(
            decode_quarantine_inventory(
                &encode_quarantine_inventory(&inventory).expect("encode inventory")
            ),
            Ok(inventory)
        );
    }

    #[test]
    fn constructor_canonicalizes_entry_order() {
        let inventory = inventory(vec![
            entry("table0002", "tables/main/l0000/table0002"),
            entry("table0001", "tables/main/l0000/table0001"),
        ]);

        assert_eq!(inventory.entries()[0].object_id(), "table0001");
        assert_eq!(inventory.entries()[1].object_id(), "table0002");
    }

    #[test]
    fn decode_rejects_noncanonical_entry_order() {
        let bytes = encode_inventory_unchecked(&[
            entry("table0002", "tables/main/l0000/table0002"),
            entry("table0001", "tables/main/l0000/table0001"),
        ]);

        assert_eq!(
            decode_quarantine_inventory(&bytes),
            Err(FormatError::InvalidValue {
                field: "entry_order"
            })
        );
    }

    #[test]
    fn decode_rejects_invalid_magic() {
        let mut bytes =
            encode_quarantine_inventory(&inventory(Vec::new())).expect("encode inventory");
        bytes[0] = b'X';

        assert_eq!(
            decode_quarantine_inventory(&bytes),
            Err(FormatError::InvalidMagic {
                format: QUARANTINE_INVENTORY_FORMAT
            })
        );
    }

    #[test]
    fn decode_rejects_future_version() {
        let mut bytes =
            encode_quarantine_inventory(&inventory(Vec::new())).expect("encode inventory");
        bytes[4..8].copy_from_slice(&(DATABASE_FORMAT_VERSION + 1).to_le_bytes());

        assert_eq!(
            decode_quarantine_inventory(&bytes),
            Err(FormatError::FutureFormat {
                format: QUARANTINE_INVENTORY_FORMAT,
                version: DATABASE_FORMAT_VERSION + 1,
                max_supported: DATABASE_FORMAT_VERSION
            })
        );
    }

    #[test]
    fn decode_rejects_pre_v1_version() {
        let mut bytes =
            encode_quarantine_inventory(&inventory(Vec::new())).expect("encode inventory");
        bytes[4..8].copy_from_slice(&0_u32.to_le_bytes());

        assert_eq!(
            decode_quarantine_inventory(&bytes),
            Err(FormatError::PreV1Format {
                format: QUARANTINE_INVENTORY_FORMAT,
                version: 0
            })
        );
    }

    #[test]
    fn decode_rejects_checksum_mismatch() {
        let mut bytes = encode_quarantine_inventory(&inventory(vec![entry(
            "table0001",
            "tables/main/l0000/table0001",
        )]))
        .expect("encode inventory");
        bytes[16] ^= 0xff;

        assert!(matches!(
            decode_quarantine_inventory(&bytes),
            Err(FormatError::ChecksumMismatch {
                format: QUARANTINE_INVENTORY_FORMAT,
                ..
            })
        ));
    }

    #[test]
    fn decode_rejects_invalid_codec_utf8() {
        let bytes = encode_inventory_with_raw_codec(&[0xff]);

        assert_eq!(
            decode_quarantine_inventory(&bytes),
            Err(FormatError::InvalidUtf8 { field: "codec_id" })
        );
    }

    #[test]
    fn decode_rejects_oversized_codec_id_before_reading_payload() {
        let bytes = encode_inventory_with_raw_codec(&vec![b'a'; MAX_CODEC_ID_LEN + 1]);

        assert_eq!(
            decode_quarantine_inventory(&bytes),
            Err(FormatError::InvalidLength { field: "codec_id" })
        );
    }

    #[test]
    fn decode_rejects_truncated_header() {
        assert_eq!(
            decode_quarantine_inventory(&[0; 12]),
            Err(FormatError::InsufficientBytes {
                format: QUARANTINE_INVENTORY_FORMAT,
                needed: super::minimum_inventory_len(),
                actual: 12
            })
        );
    }

    #[test]
    fn decode_rejects_truncated_entry_header() {
        let inventory = inventory(vec![entry("table0001", "tables/main/l0000/table0001")]);
        let mut bytes = encode_quarantine_inventory(&inventory).expect("encode inventory");
        bytes.truncate(bytes.len() - 12);
        refresh_crc(&mut bytes);

        assert!(matches!(
            decode_quarantine_inventory(&bytes),
            Err(FormatError::InsufficientBytes {
                format: QUARANTINE_INVENTORY_FORMAT,
                ..
            })
        ));
    }

    #[test]
    fn decode_rejects_truncated_object_id() {
        let mut bytes =
            encode_quarantine_inventory(&inventory(vec![entry("a", "tables/main/l0000/a")]))
                .expect("encode inventory");
        let object_id_len_offset = 4 + 4 + 16 + 16 + 4 + "identity".len() + 4;
        bytes[object_id_len_offset..object_id_len_offset + 4]
            .copy_from_slice(&1_000_u32.to_le_bytes());
        refresh_crc(&mut bytes);

        assert!(matches!(
            decode_quarantine_inventory(&bytes),
            Err(FormatError::InsufficientBytes {
                format: QUARANTINE_INVENTORY_FORMAT,
                ..
            })
        ));
    }

    #[test]
    fn decode_rejects_truncated_source_object() {
        let object_id = "a";
        let mut bytes =
            encode_quarantine_inventory(&inventory(vec![entry(object_id, "tables/main/l0000/a")]))
                .expect("encode inventory");
        let source_object_len_offset =
            4 + 4 + 16 + 16 + 4 + "identity".len() + 4 + 4 + object_id.len();
        bytes[source_object_len_offset..source_object_len_offset + 4]
            .copy_from_slice(&1_000_u32.to_le_bytes());
        refresh_crc(&mut bytes);

        assert!(matches!(
            decode_quarantine_inventory(&bytes),
            Err(FormatError::InsufficientBytes {
                format: QUARANTINE_INVENTORY_FORMAT,
                ..
            })
        ));
    }

    #[test]
    fn decode_rejects_oversized_object_id_before_reading_payload() {
        let object_id = "a".repeat(MAX_OBJECT_NAME_BYTES + 1);
        let bytes = encode_inventory_unchecked_raw(
            "identity",
            &[(
                object_id.as_str(),
                "tables/main/l0000/table0001",
                1,
                Timestamp::from_micros(1).as_micros(),
            )],
        );

        assert_eq!(
            decode_quarantine_inventory(&bytes),
            Err(FormatError::InvalidLength { field: "object_id" })
        );
    }

    #[test]
    fn decode_rejects_oversized_source_object_before_reading_payload() {
        let source_object = "a".repeat(MAX_OBJECT_NAME_BYTES + 1);
        let bytes = encode_inventory_unchecked_raw(
            "identity",
            &[(
                "table0001",
                source_object.as_str(),
                1,
                Timestamp::from_micros(1).as_micros(),
            )],
        );

        assert_eq!(
            decode_quarantine_inventory(&bytes),
            Err(FormatError::InvalidLength {
                field: "source_object"
            })
        );
    }

    #[test]
    fn decode_rejects_trailing_bytes() {
        let mut bytes =
            encode_quarantine_inventory(&inventory(Vec::new())).expect("encode inventory");
        bytes.truncate(bytes.len() - 4);
        bytes.push(0);
        let crc = crc32fast::hash(&bytes);
        bytes.extend_from_slice(&crc.to_le_bytes());

        assert_eq!(
            decode_quarantine_inventory(&bytes),
            Err(FormatError::TrailingData {
                format: QUARANTINE_INVENTORY_FORMAT,
                remaining: 1
            })
        );
    }

    #[test]
    fn entry_accepts_layout_reserved_object_id_for_service_validation() {
        assert!(QuarantineEntry::new(
            "manifest",
            source_object("tables/main/l0000/table0001"),
            1,
            Timestamp::from_micros(1)
        )
        .is_ok());
    }

    #[test]
    fn entry_rejects_invalid_object_id_component() {
        assert_eq!(
            QuarantineEntry::new(
                "bad/object",
                source_object("tables/main/l0000/table0001"),
                1,
                Timestamp::from_micros(1)
            ),
            Err(FormatError::InvalidValue { field: "object_id" })
        );
    }

    #[test]
    fn decode_rejects_empty_object_id() {
        let bytes = encode_inventory_unchecked_raw(
            "identity",
            &[(
                "",
                "tables/main/l0000/table0001",
                1,
                Timestamp::from_micros(1).as_micros(),
            )],
        );

        assert_eq!(
            decode_quarantine_inventory(&bytes),
            Err(FormatError::InvalidValue { field: "object_id" })
        );
    }

    #[test]
    fn decode_rejects_invalid_object_id_bytes() {
        let bytes = encode_inventory_unchecked_raw(
            "identity",
            &[(
                "bad:name",
                "tables/main/l0000/table0001",
                1,
                Timestamp::from_micros(1).as_micros(),
            )],
        );

        assert_eq!(
            decode_quarantine_inventory(&bytes),
            Err(FormatError::InvalidValue { field: "object_id" })
        );
    }

    #[test]
    fn entry_accepts_component_valid_object_id_without_layout_context() {
        let object_id = "a".repeat(980);

        assert!(QuarantineEntry::new(
            object_id,
            source_object("tables/main/l0000/table0001"),
            1,
            Timestamp::from_micros(1)
        )
        .is_ok());
    }

    #[test]
    fn entry_accepts_quarantine_source_object_for_service_validation() {
        assert!(QuarantineEntry::new(
            "table0001",
            source_object("quarantine/main/table0001"),
            1,
            Timestamp::from_micros(1)
        )
        .is_ok());
    }

    #[test]
    fn entry_accepts_unknown_source_family_for_service_validation() {
        assert!(QuarantineEntry::new(
            "table0001",
            source_object("unknown/table0001"),
            1,
            Timestamp::from_micros(1)
        )
        .is_ok());
    }

    #[test]
    fn inventory_rejects_duplicate_object_ids() {
        assert_eq!(
            QuarantineInventory::new(
                DATABASE_ID,
                branch_id(),
                "identity",
                vec![
                    entry("table0001", "tables/main/l0000/table0001"),
                    entry("table0001", "tables/main/l0000/table0002"),
                ]
            ),
            Err(FormatError::InvalidValue { field: "object_id" })
        );
    }

    #[test]
    fn inventory_rejects_duplicate_source_objects() {
        assert_eq!(
            QuarantineInventory::new(
                DATABASE_ID,
                branch_id(),
                "identity",
                vec![
                    entry("table0001", "tables/main/l0000/table0001"),
                    entry("table0002", "tables/main/l0000/table0001"),
                ]
            ),
            Err(FormatError::InvalidValue {
                field: "source_object"
            })
        );
    }

    #[test]
    fn decode_rejects_oversized_entry_count_before_allocating() {
        let mut bytes =
            encode_quarantine_inventory(&inventory(Vec::new())).expect("encode inventory");
        let entry_count_offset = 4 + 4 + 16 + 16 + 4 + "identity".len();
        bytes[entry_count_offset..entry_count_offset + 4].copy_from_slice(&1_u32.to_le_bytes());
        refresh_crc(&mut bytes);

        assert_eq!(
            decode_quarantine_inventory(&bytes),
            Err(FormatError::InvalidLength {
                field: "entry_count"
            })
        );
    }

    #[test]
    fn old_storage_quarantine_magic_is_not_accepted() {
        let mut old_bytes = vec![0; super::minimum_inventory_len()];
        old_bytes[..8].copy_from_slice(b"STRAQRTN");

        assert_eq!(
            decode_quarantine_inventory(&old_bytes),
            Err(FormatError::InvalidMagic {
                format: QUARANTINE_INVENTORY_FORMAT
            })
        );
    }
}
