use super::*;

#[test]
fn table_manifest_accepts_empty_branch_graph() {
    let manifest = TableManifest::new(branch(0x11), None, 1, vec![], vec![], vec![])
        .expect("empty branch graph");

    assert_eq!(manifest.branch_id(), branch(0x11));
    assert!(manifest.levels().is_empty());
    assert!(manifest.inherited_layers().is_empty());
    assert_eq!(round_trip(&manifest), manifest);
}

#[test]
fn table_manifest_accepts_zero_branch_id_as_opaque_atom() {
    let zero_branch = BranchId::from_bytes([0; BranchId::BYTE_LEN]);
    let manifest = TableManifest::new(zero_branch, None, 1, vec![], vec![], vec![])
        .expect("zero branch is an opaque atom, not an empty sentinel");

    assert_eq!(round_trip(&manifest).branch_id(), zero_branch);
}

#[test]
fn table_manifest_rejects_zero_manifest_sequence_if_reserved() {
    assert_invalid_value(
        TableManifest::new(branch(0x11), None, 0, vec![], vec![], vec![]),
        "manifest_sequence",
    );
}

#[test]
fn table_manifest_rejects_invalid_branch_generation() {
    assert_invalid_value(
        TableManifest::new(branch(0x11), Some(0), 1, vec![], vec![], vec![]),
        "branch_generation",
    );
}

#[test]
fn table_manifest_rejects_duplicate_level() {
    let level_a = decoded_level(BranchLevel::ZERO, vec![]);
    let level_b = decoded_level(BranchLevel::ZERO, vec![]);

    assert_invalid_value(
        TableManifest::from_decoded(
            branch(0x11),
            None,
            1,
            vec![level_a, level_b],
            vec![],
            vec![],
        ),
        "level",
    );
}

#[test]
fn table_manifest_rejects_invalid_level() {
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
fn table_manifest_rejects_duplicate_table_identity() {
    let branch = branch(0x11);
    let branch_text = branch.to_string();
    let level = TableManifestLevel::new(
        BranchLevel::ZERO,
        vec![
            table_ref_with_object(
                "dup",
                &format!("tables/{branch_text}/l0000/a"),
                0,
                b"a",
                b"b",
            ),
            table_ref_with_object(
                "dup",
                &format!("tables/{branch_text}/l0000/b"),
                1,
                b"c",
                b"d",
            ),
        ],
    )
    .expect("level");

    assert_invalid_value(
        TableManifest::new(branch, None, 1, vec![level], vec![], vec![]),
        "table_identity",
    );
}

#[test]
fn table_manifest_rejects_duplicate_object_name() {
    let branch = branch(0x11);
    let object = format!("tables/{branch}/l0000/shared");
    let level = TableManifestLevel::new(
        BranchLevel::ZERO,
        vec![
            table_ref_with_object("a", &object, 0, b"a", b"b"),
            table_ref_with_object("b", &object, 1, b"c", b"d"),
        ],
    )
    .expect("level");

    assert_invalid_value(
        TableManifest::new(branch, None, 1, vec![level], vec![], vec![]),
        "table_object",
    );
}

#[test]
fn table_manifest_rejects_owned_and_inherited_duplicate_table_identity() {
    let inherited = inherited_layer_with_tables(
        0,
        branch(0x22),
        vec![TableManifestLevel::new(
            BranchLevel::ZERO,
            vec![table_ref(branch(0x22), "shared", 0, b"c", b"d")],
        )
        .expect("inherited level")],
    );
    let owned = TableManifestLevel::new(
        BranchLevel::ZERO,
        vec![table_ref(branch(0x11), "shared", 0, b"a", b"b")],
    )
    .expect("owned level");

    assert_invalid_value(
        TableManifest::new(branch(0x11), None, 1, vec![owned], vec![inherited], vec![]),
        "table_identity",
    );
}

#[test]
fn table_manifest_rejects_empty_table_entry() {
    assert!(TableIdentity::new("").is_err());
    assert!(ObjectName::new("").is_err());
}

#[test]
fn table_manifest_rejects_zero_row_count() {
    assert_invalid_value(
        TableManifestTableFacts::new(
            128,
            0,
            1,
            CommitVersion::new(1),
            CommitVersion::new(2),
            None,
            None,
        ),
        "row_count",
    );
}

#[test]
fn table_manifest_rejects_commit_min_greater_than_max() {
    assert_invalid_value(
        TableManifestTableFacts::new(
            128,
            1,
            1,
            CommitVersion::new(3),
            CommitVersion::new(2),
            None,
            None,
        ),
        "commit_range",
    );
}

#[test]
fn table_manifest_rejects_timestamp_min_greater_than_max() {
    assert_invalid_value(
        TableManifestTableFacts::new(
            128,
            1,
            1,
            CommitVersion::new(1),
            CommitVersion::new(2),
            Some(Timestamp::from_micros(20)),
            Some(Timestamp::from_micros(10)),
        ),
        "timestamp_range",
    );
}

#[test]
fn table_manifest_rejects_invalid_physical_bounds() {
    assert_invalid_value(
        TableManifestTableBounds::new(
            b"z".to_vec(),
            b"a".to_vec(),
            b"a:i0".to_vec(),
            b"z:i9".to_vec(),
        ),
        "physical_bounds",
    );
}

#[test]
fn table_manifest_rejects_invalid_internal_bounds() {
    assert_invalid_value(
        TableManifestTableBounds::new(
            b"a".to_vec(),
            b"z".to_vec(),
            b"z:i9".to_vec(),
            b"a:i0".to_vec(),
        ),
        "internal_bounds",
    );
}

#[test]
fn table_manifest_accepts_layout_valid_table_object_name() {
    let branch = branch(0x11);
    let object = format!("tables/{branch}/l0003/tablev0001");

    assert!(TableManifestTableRef::new(
        TableIdentity::new("tablev0001").expect("identity"),
        ObjectName::new(object).expect("object"),
        0,
        table_facts(),
        table_bounds(b"a", b"b"),
        TableManifestTableProvenance::Flush,
    )
    .is_ok());
}

#[test]
fn table_manifest_rejects_absolute_path_object_name() {
    assert!(ObjectName::new("/tables/branch/l0000/table").is_err());
}

#[test]
fn table_manifest_rejects_parent_component_object_name() {
    assert!(ObjectName::new("tables/branch/../table").is_err());
}

#[test]
fn table_manifest_rejects_empty_object_component() {
    assert!(ObjectName::new("tables//l0000/table").is_err());
}

#[test]
fn table_manifest_rejects_manifest_object_used_as_table_object() {
    assert_table_object_rejected("manifest/current");
}

#[test]
fn table_manifest_rejects_snapshot_object_used_as_table_object() {
    assert_table_object_rejected("snapshots/0000000000000001");
}

#[test]
fn table_manifest_rejects_quarantine_object_used_as_table_object() {
    assert_table_object_rejected("quarantine/branch/object");
}

#[test]
fn table_manifest_rejects_duplicate_identity_and_object() {
    let branch = branch(0x11);
    let branch_text = branch.to_string();
    let duplicate_identity = TableManifest::new(
        branch,
        None,
        1,
        vec![TableManifestLevel::new(
            BranchLevel::ZERO,
            vec![
                table_ref_with_object(
                    "a",
                    &format!("tables/{branch_text}/l0000/ta"),
                    0,
                    b"k0",
                    b"k1",
                ),
                table_ref_with_object(
                    "a",
                    &format!("tables/{branch_text}/l0000/tb"),
                    1,
                    b"k2",
                    b"k3",
                ),
            ],
        )
        .expect("level")],
        vec![],
        vec![],
    );
    assert!(matches!(
        duplicate_identity,
        Err(FormatError::InvalidValue {
            field: "table_identity"
        })
    ));

    let duplicate_object = TableManifest::new(
        branch,
        None,
        1,
        vec![TableManifestLevel::new(
            BranchLevel::ZERO,
            vec![
                table_ref_with_object(
                    "a",
                    &format!("tables/{branch_text}/l0000/ta"),
                    0,
                    b"k0",
                    b"k1",
                ),
                table_ref_with_object(
                    "b",
                    &format!("tables/{branch_text}/l0000/ta"),
                    1,
                    b"k2",
                    b"k3",
                ),
            ],
        )
        .expect("level")],
        vec![],
        vec![],
    );
    assert!(matches!(
        duplicate_object,
        Err(FormatError::InvalidValue {
            field: "table_object"
        })
    ));
}

#[test]
fn table_manifest_rejects_non_table_object_names() {
    let facts = TableManifestTableFacts::new(
        128,
        4,
        1,
        CommitVersion::new(1),
        CommitVersion::new(4),
        None,
        None,
    )
    .expect("facts");
    let bounds = TableManifestTableBounds::new(
        b"k0".to_vec(),
        b"k1".to_vec(),
        b"k0:i0".to_vec(),
        b"k1:i9".to_vec(),
    )
    .expect("bounds");
    let result = TableManifestTableRef::new(
        TableIdentity::new("a").expect("identity"),
        ObjectName::new("snapshots/0000000000000001").expect("object"),
        0,
        facts,
        bounds,
        TableManifestTableProvenance::Flush,
    );
    assert!(matches!(
        result,
        Err(FormatError::InvalidValue {
            field: "table_object"
        })
    ));
}

#[test]
fn table_manifest_rejects_cross_branch_table_objects() {
    let manifest_branch = branch(0x11);
    let other_branch = branch(0x22);
    let result = TableManifest::new(
        manifest_branch,
        None,
        1,
        vec![TableManifestLevel::new(
            BranchLevel::ZERO,
            vec![table_ref(other_branch, "a", 0, b"k0", b"k1")],
        )
        .expect("level")],
        vec![],
        vec![],
    );

    assert!(matches!(
        result,
        Err(FormatError::InvalidValue {
            field: "table_object_branch"
        })
    ));
}

#[test]
fn table_manifest_rejects_wrong_table_object_shape_or_level() {
    let branch = branch(0x11);
    let branch_text = branch.to_string();
    for object in [
        format!("tables/{branch_text}/manifest/foo"),
        format!("tables/{branch_text}/level-zero/table"),
        format!("tables/{branch_text}/l0000/table/extra"),
    ] {
        assert!(matches!(
            TableManifestTableRef::new(
                TableIdentity::new("shape").expect("identity"),
                ObjectName::new(object).expect("object"),
                0,
                table_facts(),
                table_bounds(b"k0", b"k1"),
                TableManifestTableProvenance::Flush,
            ),
            Err(FormatError::InvalidValue {
                field: "table_object"
            })
        ));
    }

    let result = TableManifest::new(
        branch,
        None,
        1,
        vec![TableManifestLevel::new(
            BranchLevel::new(1),
            vec![table_ref(branch, "wrong_level", 0, b"k0", b"k1")],
        )
        .expect("level")],
        vec![],
        vec![],
    );
    assert!(matches!(
        result,
        Err(FormatError::InvalidValue {
            field: "table_object_level"
        })
    ));
}
