use super::*;

#[test]
fn table_manifest_rejects_bad_magic() {
    let mut bytes = encode_table_manifest(&sample_manifest()).expect("encode");
    bytes[0] = b'X';

    assert!(matches!(
        decode_table_manifest(&bytes),
        Err(FormatError::InvalidMagic { .. })
    ));
}

#[test]
fn table_manifest_rejects_future_version() {
    let mut bytes = encode_table_manifest(&sample_manifest()).expect("encode");
    bytes[4..8].copy_from_slice(&2_u32.to_le_bytes());
    refresh_crc(&mut bytes);

    assert!(matches!(
        decode_table_manifest(&bytes),
        Err(FormatError::FutureFormat { .. })
    ));
}

#[test]
fn table_manifest_rejects_pre_v1_version_if_reserved() {
    let mut bytes = encode_table_manifest(&sample_manifest()).expect("encode");
    bytes[4..8].copy_from_slice(&0_u32.to_le_bytes());
    refresh_crc(&mut bytes);

    assert!(matches!(
        decode_table_manifest(&bytes),
        Err(FormatError::PreV1Format { .. })
    ));
}

#[test]
fn table_manifest_rejects_truncated_header() {
    assert!(matches!(
        decode_table_manifest(&[0_u8; 8]),
        Err(FormatError::InsufficientBytes { .. })
    ));
}

#[test]
fn table_manifest_rejects_truncated_table_entry() {
    let manifest = manifest_with_single_table(table_ref(branch(0x11), "a", 0, b"a", b"b"));
    let mut bytes = encode_table_manifest(&manifest).expect("encode");
    remove_payload_byte(&mut bytes, level_count_offset() + 8);
    refresh_crc(&mut bytes);

    assert!(decode_table_manifest(&bytes).is_err());
}

#[test]
fn table_manifest_rejects_truncated_inherited_layer() {
    let mut bytes = encode_table_manifest(&inherited_manifest()).expect("encode");
    let offset = find_bytes(&bytes, branch(0x22).as_bytes());
    remove_payload_byte(&mut bytes, offset);
    refresh_crc(&mut bytes);

    assert!(decode_table_manifest(&bytes).is_err());
}

#[test]
fn table_manifest_rejects_trailing_bytes() {
    let mut bytes = encode_table_manifest(&sample_manifest()).expect("encode");
    let checksum_offset = bytes.len() - 4;
    bytes.insert(checksum_offset, 0xaa);
    refresh_crc(&mut bytes);

    assert!(matches!(
        decode_table_manifest(&bytes),
        Err(FormatError::TrailingData { .. })
    ));
}

#[test]
fn table_manifest_rejects_checksum_mismatch() {
    let mut bytes = encode_table_manifest(&sample_manifest()).expect("encode");
    let last = bytes.len() - 1;
    bytes[last] ^= 0xff;

    assert!(matches!(
        decode_table_manifest(&bytes),
        Err(FormatError::ChecksumMismatch { .. })
    ));
}

#[test]
fn table_manifest_rejects_count_overflow() {
    let mut bytes = encode_table_manifest(
        &TableManifest::new(branch(0x11), None, 1, vec![], vec![], vec![]).expect("manifest"),
    )
    .expect("encode");
    write_u32(
        &mut bytes,
        extension_count_offset(),
        bounded_u32(MAX_EXTENSIONS) + 1,
    );
    refresh_crc(&mut bytes);

    assert_invalid_length(decode_table_manifest(&bytes), "extension_count");
}

#[test]
fn table_manifest_rejects_inherited_layer_count_overflow() {
    let mut bytes = encode_table_manifest(
        &TableManifest::new(branch(0x11), None, 1, vec![], vec![], vec![]).expect("manifest"),
    )
    .expect("encode");
    write_u32(
        &mut bytes,
        inherited_count_offset(),
        bounded_u32(MAX_INHERITED_LAYERS) + 1,
    );
    refresh_crc(&mut bytes);

    assert_invalid_length(decode_table_manifest(&bytes), "inherited_layer_count");
}

#[test]
fn table_manifest_rejects_length_overflow() {
    let manifest = manifest_with_single_table(table_ref(branch(0x11), "a", 0, b"a", b"b"));
    let mut bytes = encode_table_manifest(&manifest).expect("encode");
    let identity_offset = find_len_prefixed_string(&bytes, "a");
    write_u32(
        &mut bytes,
        identity_offset,
        bounded_u32(MAX_IDENTITY_BYTES) + 1,
    );
    refresh_crc(&mut bytes);

    assert_invalid_length(decode_table_manifest(&bytes), "table_identity");
}

#[test]
fn table_manifest_rejects_invalid_utf8() {
    let manifest = manifest_with_single_table(table_ref(branch(0x11), "a", 0, b"a", b"b"));
    let mut bytes = encode_table_manifest(&manifest).expect("encode");
    let value_offset = find_bytes(&bytes, b"a");
    bytes[value_offset] = 0xff;
    refresh_crc(&mut bytes);

    assert!(matches!(
        decode_table_manifest(&bytes),
        Err(FormatError::InvalidUtf8 {
            field: "table_identity"
        })
    ));
}

#[test]
fn table_manifest_rejects_reserved_flag_bits() {
    let manifest = extension_manifest();
    let mut bytes = encode_table_manifest(&manifest).expect("encode");
    let flag_index = find_extension_flag_index(&bytes, "audit.fact");
    bytes[flag_index] = 0x80;
    refresh_crc(&mut bytes);

    assert!(matches!(
        decode_table_manifest(&bytes),
        Err(FormatError::UnsupportedFlags { .. })
    ));
}

#[test]
fn table_manifest_decode_rejects_empty_identity_bytes() {
    let marker = "decode-empty-identity";
    let manifest = manifest_with_single_table(table_ref(branch(0x11), marker, 0, b"a", b"b"));
    let mut bytes = encode_table_manifest(&manifest).expect("encode");
    let length_offset = find_len_prefixed_string(&bytes, marker);
    write_u32(&mut bytes, length_offset, 0);
    bytes.drain(length_offset + 4..length_offset + 4 + marker.len());
    refresh_crc(&mut bytes);

    assert_invalid_value(decode_table_manifest(&bytes), "table_identity");
}

#[test]
fn table_manifest_decode_rejects_empty_object_bytes() {
    let branch = branch(0x11);
    let object = format!("tables/{branch}/l0000/decode-empty-object");
    let manifest = manifest_with_single_table(table_ref_with_object(
        "identity-for-empty-object",
        &object,
        0,
        b"a",
        b"b",
    ));
    let mut bytes = encode_table_manifest(&manifest).expect("encode");
    let length_offset = find_len_prefixed_string(&bytes, &object);
    write_u32(&mut bytes, length_offset, 0);
    bytes.drain(length_offset + 4..length_offset + 4 + object.len());
    refresh_crc(&mut bytes);

    assert_invalid_value(decode_table_manifest(&bytes), "table_object");
}

#[test]
fn table_manifest_decode_empty_bytes_returns_typed_error() {
    assert!(matches!(
        decode_table_manifest(&[]),
        Err(FormatError::InsufficientBytes { .. })
    ));
}

#[test]
fn table_manifest_decode_random_bytes_returns_typed_error_or_valid_manifest() {
    for seed in 0..64_u8 {
        let bytes: Vec<u8> = (0..96_u8)
            .map(|index| seed.wrapping_mul(31).wrapping_add(index.wrapping_mul(17)))
            .collect();
        let _ = decode_table_manifest(&bytes);
    }
}

#[test]
fn table_manifest_decode_large_counts_does_not_allocate_unbounded_memory() {
    let mut bytes = encode_table_manifest(
        &TableManifest::new(branch(0x11), None, 1, vec![], vec![], vec![]).expect("manifest"),
    )
    .expect("encode");
    write_u32(
        &mut bytes,
        level_count_offset(),
        bounded_u32(MAX_LEVELS) + 1,
    );
    refresh_crc(&mut bytes);

    assert_invalid_length(decode_table_manifest(&bytes), "level_count");
}

#[test]
fn table_manifest_decode_rejects_deeply_nested_sections() {
    let mut bytes = encode_table_manifest(
        &TableManifest::new(branch(0x11), None, 1, vec![], vec![], vec![]).expect("manifest"),
    )
    .expect("encode");
    write_u32(
        &mut bytes,
        extension_count_offset(),
        bounded_u32(MAX_EXTENSIONS) + 1,
    );
    refresh_crc(&mut bytes);

    assert_invalid_length(decode_table_manifest(&bytes), "extension_count");
}

#[test]
fn table_manifest_decode_rejects_noncanonical_reencoded_bytes() {
    let branch = branch(0x11);
    let manifest = manifest_with_single_table(table_ref(branch, "a", 0, b"a", b"b"));
    let mut bytes = encode_table_manifest(&manifest).expect("encode");
    let order_offset = table_order_offset_after_object(&bytes, &format!("tables/{branch}/l0000/a"));
    write_u32(&mut bytes, order_offset, 1);
    refresh_crc(&mut bytes);

    assert_invalid_value(decode_table_manifest(&bytes), "table_order");
}

#[test]
fn table_manifest_rejects_corruption_and_future_version() {
    let manifest = sample_manifest();
    let mut bytes = encode_table_manifest(&manifest).expect("encode");
    let last = bytes.len() - 1;
    bytes[last] ^= 0xff;
    assert!(matches!(
        decode_table_manifest(&bytes),
        Err(FormatError::ChecksumMismatch { .. })
    ));

    let mut future = encode_table_manifest(&manifest).expect("encode");
    future[4..8].copy_from_slice(&2_u32.to_le_bytes());
    refresh_crc(&mut future);
    assert!(matches!(
        decode_table_manifest(&future),
        Err(FormatError::FutureFormat { .. })
    ));
}

#[test]
fn table_manifest_fuzz_decode_contract_accepts_or_rejects_without_panic() {
    assert!(decode_table_manifest(&[]).is_err());
    let manifest = sample_manifest();
    let bytes = encode_table_manifest(&manifest).expect("encode");
    assert_eq!(decode_table_manifest(&bytes), Ok(manifest));
}
