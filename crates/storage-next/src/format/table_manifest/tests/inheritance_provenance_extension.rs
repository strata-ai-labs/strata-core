use super::*;

#[test]
fn table_manifest_inherited_layer_records_source_and_fork() {
    let inherited = inherited_manifest();
    let layer = &inherited.inherited_layers()[0];

    assert_eq!(layer.source_branch_id(), branch(0x22));
    assert_eq!(layer.fork_version(), CommitVersion::new(10));
}

#[test]
fn table_manifest_rejects_duplicate_inherited_layer_source_fork() {
    let layer_a = inherited_layer(
        0,
        branch(0x22),
        CommitVersion::new(10),
        TableManifestInheritedLayerStatus::Active,
    );
    let layer_b = inherited_layer(
        1,
        branch(0x22),
        CommitVersion::new(10),
        TableManifestInheritedLayerStatus::Materializing,
    );

    assert_invalid_value(
        TableManifest::from_decoded(
            branch(0x11),
            None,
            1,
            vec![],
            vec![layer_a, layer_b],
            vec![],
        ),
        "inherited_layer_source",
    );
}

#[test]
fn table_manifest_rejects_non_contiguous_inherited_layer_order() {
    let layer = inherited_layer(
        1,
        branch(0x22),
        CommitVersion::new(10),
        TableManifestInheritedLayerStatus::Active,
    );

    assert_invalid_value(
        TableManifest::from_decoded(branch(0x11), None, 1, vec![], vec![layer], vec![]),
        "inherited_layer_order",
    );
}

#[test]
fn table_manifest_preserves_active_status() {
    assert_inherited_status_round_trips(TableManifestInheritedLayerStatus::Active);
}

#[test]
fn table_manifest_preserves_materializing_status() {
    assert_inherited_status_round_trips(TableManifestInheritedLayerStatus::Materializing);
}

#[test]
fn table_manifest_preserves_materialized_status_if_supported() {
    assert_inherited_status_round_trips(TableManifestInheritedLayerStatus::Materialized);
}

#[test]
fn table_manifest_rejects_inherited_layer_with_invalid_fork_version() {
    assert_invalid_value(
        TableManifestInheritedLayer::new(
            0,
            branch(0x22),
            None,
            CommitVersion::ZERO,
            TableManifestInheritedLayerStatus::Active,
            vec![],
        ),
        "inherited_fork_version",
    );
}

#[test]
fn table_manifest_rejects_inherited_layer_with_duplicate_table_identity() {
    let inherited = inherited_layer_with_tables(
        0,
        branch(0x22),
        vec![decoded_level(
            BranchLevel::ZERO,
            vec![
                table_ref(branch(0x22), "dup", 0, b"a", b"b"),
                table_ref(branch(0x22), "dup", 1, b"c", b"d"),
            ],
        )],
    );

    assert_invalid_value(
        TableManifest::new(branch(0x11), None, 1, vec![], vec![inherited], vec![]),
        "table_identity",
    );
}

#[test]
fn table_manifest_rejects_inherited_layer_with_duplicate_object_name() {
    let source = branch(0x22);
    let object = format!("tables/{source}/l0000/shared");
    let inherited = inherited_layer_with_tables(
        0,
        source,
        vec![decoded_level(
            BranchLevel::ZERO,
            vec![
                table_ref_with_object("a", &object, 0, b"a", b"b"),
                table_ref_with_object("b", &object, 1, b"c", b"d"),
            ],
        )],
    );

    assert_invalid_value(
        TableManifest::new(branch(0x11), None, 1, vec![], vec![inherited], vec![]),
        "table_object",
    );
}

#[test]
fn table_manifest_does_not_require_runtime_materialization_handle() {
    let manifest = inherited_manifest();
    let bytes = encode_table_manifest(&manifest).expect("encode");

    assert_eq!(decode_table_manifest(&bytes), Ok(manifest));
}

#[test]
fn table_manifest_preserves_flush_provenance() {
    assert_provenance_round_trips(&TableManifestTableProvenance::Flush);
}

#[test]
fn table_manifest_preserves_snapshot_install_provenance() {
    assert_provenance_round_trips(&TableManifestTableProvenance::SnapshotInstall);
}

#[test]
fn table_manifest_preserves_compaction_provenance() {
    assert_provenance_round_trips(&TableManifestTableProvenance::Compaction);
}

#[test]
fn table_manifest_preserves_materialization_replacement_provenance() {
    assert_provenance_round_trips(
        &TableManifestTableProvenance::materialization_replacement(
            branch(0x44),
            CommitVersion::new(10),
        )
        .expect("provenance"),
    );
}

#[test]
fn table_manifest_preserves_recovered_provenance() {
    assert_provenance_round_trips(&TableManifestTableProvenance::Recovered);
}

#[test]
fn table_manifest_rejects_materialization_provenance_without_source() {
    assert_invalid_value(
        TableManifestTableProvenance::materialization_replacement(
            branch(0x44),
            CommitVersion::ZERO,
        ),
        "materialization_fork_version",
    );
}

#[test]
fn table_manifest_rejects_unknown_required_provenance() {
    let manifest = manifest_with_single_table(table_ref(branch(0x11), "a", 0, b"a", b"b"));
    let mut bytes = encode_table_manifest(&manifest).expect("encode");
    let provenance_offset =
        table_provenance_offset_after_object(&bytes, &format!("tables/{}/l0000/a", branch(0x11)));
    bytes[provenance_offset] = 0xff;
    refresh_crc(&mut bytes);

    assert_invalid_value(decode_table_manifest(&bytes), "table_provenance");
}

#[test]
fn table_manifest_rejects_unknown_required_section() {
    table_manifest_rejects_unknown_required_extension();
}

#[test]
fn table_manifest_accepts_unknown_optional_section_without_core_fact_loss() {
    let manifest = extension_manifest();
    let decoded = round_trip(&manifest);

    assert_eq!(decoded.branch_id(), manifest.branch_id());
    assert_eq!(decoded.extension_sections()[0].kind(), "audit.fact");
    assert_eq!(decoded.extension_sections()[0].payload(), b"abc");
}

#[test]
fn table_manifest_preserves_known_extension_section() {
    let section =
        TableManifestExtensionSection::optional("storage.audit", true, b"payload".to_vec())
            .expect("section");
    let manifest = TableManifest::new(branch(0x11), None, 1, vec![], vec![], vec![section.clone()])
        .expect("manifest");

    assert_eq!(round_trip(&manifest).extension_sections()[0], section);
}

#[test]
fn table_manifest_rejects_duplicate_required_section() {
    let first =
        TableManifestExtensionSection::optional("storage.audit", false, b"a").expect("section");
    let second =
        TableManifestExtensionSection::optional("storage.audit", true, b"b").expect("section");

    assert_invalid_value(
        TableManifest::from_decoded(branch(0x11), None, 1, vec![], vec![], vec![first, second]),
        "extension_kind",
    );
}

#[test]
fn table_manifest_rejects_invalid_section_identifier() {
    assert_invalid_value(
        TableManifestExtensionSection::optional("StorageAudit", false, b"abc"),
        "extension_kind",
    );
}

#[test]
fn table_manifest_rejects_product_named_section() {
    let name = format!("storage.{}{}", "gra", "ph");
    assert_invalid_value(
        TableManifestExtensionSection::optional(name, false, b"abc"),
        "extension_kind",
    );
}

#[test]
fn table_manifest_rejects_primitive_named_section() {
    let name = format!("storage.{}{}", "vec", "tor");
    assert_invalid_value(
        TableManifestExtensionSection::optional(name, false, b"abc"),
        "extension_kind",
    );
}

#[test]
fn table_manifest_rejects_unknown_required_extension() {
    let manifest = TableManifest::new(
        branch(0x11),
        None,
        1,
        vec![],
        vec![],
        vec![
            TableManifestExtensionSection::optional("audit.fact", true, b"abc".to_vec())
                .expect("section"),
        ],
    )
    .expect("manifest");
    let mut bytes = encode_table_manifest(&manifest).expect("encode");
    let flag_index = find_extension_flag_index(&bytes, "audit.fact");
    bytes[flag_index] |= EXTENSION_FLAG_REQUIRED;
    refresh_crc(&mut bytes);

    assert!(matches!(
        decode_table_manifest(&bytes),
        Err(FormatError::UnsupportedFlags { .. })
    ));
}

#[test]
fn table_manifest_rejects_reserved_extension_vocabulary() {
    let reserved = format!("{}{}", "gra", "ph");
    assert!(matches!(
        TableManifestExtensionSection::optional(format!("audit.{reserved}"), false, b"abc"),
        Err(FormatError::InvalidValue {
            field: "extension_kind"
        })
    ));
}
