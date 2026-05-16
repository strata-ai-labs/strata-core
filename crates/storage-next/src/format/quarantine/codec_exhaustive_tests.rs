use super::{
    decode_quarantine_inventory, write_string, QuarantineEntry, QuarantineInventory,
    QUARANTINE_INVENTORY_MAGIC,
};
use crate::format::{FormatError, DATABASE_FORMAT_VERSION};
use crate::object::ObjectName;
use strata_core_next::{BranchId, Timestamp};

const DATABASE_ID: [u8; 16] = [
    0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e, 0x2f,
];

fn branch_id() -> BranchId {
    BranchId::from_bytes([
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
        0x0f,
    ])
}

fn table_source_object(table_id: &str) -> ObjectName {
    ObjectName::new(format!("source/main/l0000/{table_id}")).expect("source object")
}

fn table_source_name(table_id: &str) -> String {
    table_source_object(table_id).as_str().to_owned()
}

fn entry(object_id: &str, source: ObjectName) -> QuarantineEntry {
    QuarantineEntry::new(
        object_id,
        source,
        128,
        Timestamp::from_micros(1_700_000_000_000_000),
    )
    .expect("quarantine entry")
}

fn encode_inventory_unchecked_raw_with_branch(
    branch_id: BranchId,
    entries: &[(&str, &str, u64, u64)],
) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&QUARANTINE_INVENTORY_MAGIC);
    bytes.extend_from_slice(&DATABASE_FORMAT_VERSION.to_le_bytes());
    bytes.extend_from_slice(&DATABASE_ID);
    bytes.extend_from_slice(branch_id.as_bytes());
    write_string(&mut bytes, "codec_id", "identity").expect("codec id");
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

fn encode_inventory_unchecked_raw(entries: &[(&str, &str, u64, u64)]) -> Vec<u8> {
    encode_inventory_unchecked_raw_with_branch(branch_id(), entries)
}

#[test]
fn decode_rejects_object_id_separator_in_durable_bytes() {
    let source = table_source_name("table0001");
    let bytes = encode_inventory_unchecked_raw(&[(
        "bad/object",
        source.as_str(),
        1,
        Timestamp::from_micros(1).as_micros(),
    )]);

    assert_eq!(
        decode_quarantine_inventory(&bytes),
        Err(FormatError::InvalidValue { field: "object_id" })
    );
}

#[test]
fn decode_preserves_layout_reserved_object_id_for_service_validation() {
    let object_id: String = ['m', 'a', 'n', 'i', 'f', 'e', 's', 't']
        .into_iter()
        .collect();
    let source = table_source_name("table0001");
    let bytes = encode_inventory_unchecked_raw(&[(
        object_id.as_str(),
        source.as_str(),
        1,
        Timestamp::from_micros(1).as_micros(),
    )]);

    assert_eq!(
        decode_quarantine_inventory(&bytes)
            .map(|inventory| inventory.entries()[0].object_id().to_owned()),
        Ok(object_id)
    );
}

#[test]
fn decode_preserves_component_valid_object_id_that_service_layout_may_reject() {
    let object_id = "a".repeat(980);
    let source = table_source_name("table0001");
    let bytes = encode_inventory_unchecked_raw(&[(
        object_id.as_str(),
        source.as_str(),
        1,
        Timestamp::from_micros(1).as_micros(),
    )]);

    assert_eq!(
        decode_quarantine_inventory(&bytes)
            .map(|inventory| { inventory.entries()[0].object_id().to_owned() }),
        Ok(object_id)
    );
}

#[test]
fn decode_preserves_source_object_for_service_validation() {
    let source = table_source_name("table0001");
    let bytes = encode_inventory_unchecked_raw(&[(
        "table0001",
        source.as_str(),
        1,
        Timestamp::from_micros(1).as_micros(),
    )]);

    assert_eq!(
        decode_quarantine_inventory(&bytes)
            .map(|inventory| { inventory.entries()[0].source_object().as_str().to_owned() }),
        Ok(source)
    );
}

#[test]
fn decode_preserves_unknown_source_family_for_service_validation() {
    let bytes = encode_inventory_unchecked_raw(&[(
        "table0001",
        "unknown/table0001",
        1,
        Timestamp::from_micros(1).as_micros(),
    )]);

    assert_eq!(
        decode_quarantine_inventory(&bytes)
            .map(|inventory| { inventory.entries()[0].source_object().as_str().to_owned() }),
        Ok("unknown/table0001".to_owned())
    );
}

#[test]
fn decode_rejects_duplicate_object_ids_in_durable_bytes() {
    let source_one = table_source_name("table0001");
    let source_two = table_source_name("table0002");
    let bytes = encode_inventory_unchecked_raw(&[
        (
            "table0001",
            source_one.as_str(),
            1,
            Timestamp::from_micros(1).as_micros(),
        ),
        (
            "table0001",
            source_two.as_str(),
            2,
            Timestamp::from_micros(2).as_micros(),
        ),
    ]);

    assert_eq!(
        decode_quarantine_inventory(&bytes),
        Err(FormatError::InvalidValue { field: "object_id" })
    );
}

#[test]
fn decode_rejects_duplicate_source_objects_in_durable_bytes() {
    let source = table_source_name("table0001");
    let bytes = encode_inventory_unchecked_raw(&[
        (
            "table0001",
            source.as_str(),
            1,
            Timestamp::from_micros(1).as_micros(),
        ),
        (
            "table0002",
            source.as_str(),
            2,
            Timestamp::from_micros(2).as_micros(),
        ),
    ]);

    assert_eq!(
        decode_quarantine_inventory(&bytes),
        Err(FormatError::InvalidValue {
            field: "source_object"
        })
    );
}

#[test]
fn durable_inventory_stores_branch_id_as_raw_bytes() {
    let raw_branch = BranchId::from_bytes([
        0xff, 0xee, 0xdd, 0xcc, 0xbb, 0xaa, 0x99, 0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11,
        0x00,
    ]);
    let bytes = encode_inventory_unchecked_raw_with_branch(raw_branch, &[]);

    assert!(BranchId::parse_str("not-a-branch-id").is_err());
    assert_eq!(
        decode_quarantine_inventory(&bytes).map(|inventory| inventory.branch_id()),
        Ok(raw_branch)
    );
}

#[test]
fn inventory_constructor_still_rejects_duplicate_object_ids() {
    assert_eq!(
        QuarantineInventory::new(
            DATABASE_ID,
            branch_id(),
            "identity",
            vec![
                entry("table0001", table_source_object("table0001")),
                entry("table0001", table_source_object("table0002")),
            ],
        ),
        Err(FormatError::InvalidValue { field: "object_id" })
    );
}
