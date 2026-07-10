use super::*;

#[test]
fn table_manifest_round_trips_owned_and_inherited_tables() {
    let manifest = sample_manifest();
    let bytes = encode_table_manifest(&manifest).expect("encode");
    let decoded = decode_table_manifest(&bytes).expect("decode");

    assert_eq!(decoded, manifest);
}

#[test]
fn table_manifest_round_trips_owned_tables() {
    let manifest = TableManifest::new(
        branch(0x11),
        Some(2),
        5,
        vec![
            TableManifestLevel::new(
                BranchLevel::ZERO,
                vec![
                    table_ref(branch(0x11), "newer", 0, b"k2", b"k3"),
                    table_ref(branch(0x11), "older", 1, b"k0", b"k1"),
                ],
            )
            .expect("l0"),
            TableManifestLevel::new(
                BranchLevel::new(1),
                vec![
                    table_ref_at_level(branch(0x11), 1, "l1b", 1, b"m", b"n"),
                    table_ref_at_level(branch(0x11), 1, "l1a", 0, b"a", b"b"),
                ],
            )
            .expect("l1"),
        ],
        vec![],
        vec![],
    )
    .expect("owned manifest");

    let decoded = round_trip(&manifest);
    assert_eq!(decoded, manifest);
    assert_eq!(decoded.levels()[0].level(), BranchLevel::ZERO);
    assert_eq!(
        decoded.levels()[0].tables()[0].table_identity().as_str(),
        "newer"
    );
    assert_eq!(
        decoded.levels()[1].tables()[0].bounds().physical_first(),
        b"a"
    );
}

#[test]
fn table_manifest_round_trips_inherited_layers() {
    let manifest = inherited_manifest();

    let decoded = round_trip(&manifest);
    assert_eq!(decoded.inherited_layers().len(), 2);
    assert_eq!(
        decoded.inherited_layers()[0].source_branch_id(),
        branch(0x22)
    );
    assert_eq!(
        decoded.inherited_layers()[1].source_branch_id(),
        branch(0x33)
    );
}

#[test]
fn table_manifest_round_trips_materialization_provenance() {
    let provenance = TableManifestTableProvenance::materialization_replacement(
        branch(0x44),
        CommitVersion::new(55),
    )
    .expect("provenance");
    let manifest = manifest_with_single_table(table_ref_with_provenance(
        branch(0x11),
        0,
        "replacement",
        0,
        b"a",
        b"b",
        provenance.clone(),
    ));

    let decoded = round_trip(&manifest);
    assert_eq!(decoded.levels()[0].tables()[0].provenance(), &provenance);
}

#[test]
fn table_manifest_canonicalizes_level_order() {
    let branch = branch(0x11);
    let l1 = TableManifestLevel::new(
        BranchLevel::new(1),
        vec![table_ref_at_level(branch, 1, "l1", 0, b"a", b"b")],
    )
    .expect("l1");
    let l0 = TableManifestLevel::new(
        BranchLevel::ZERO,
        vec![table_ref(branch, "l0", 0, b"x", b"y")],
    )
    .expect("l0");

    let manifest =
        TableManifest::new(branch, None, 1, vec![l1, l0], vec![], vec![]).expect("manifest");

    assert_eq!(manifest.levels()[0].level(), BranchLevel::ZERO);
    assert_eq!(manifest.levels()[1].level(), BranchLevel::new(1));
}

#[test]
fn table_manifest_preserves_l0_precedence_order() {
    let branch = branch(0x11);
    let level = TableManifestLevel::new(
        BranchLevel::ZERO,
        vec![
            table_ref(branch, "zzz", 1, b"a", b"b"),
            table_ref(branch, "aaa", 0, b"c", b"d"),
        ],
    )
    .expect("level");

    assert_eq!(level.tables()[0].table_identity().as_str(), "aaa");
    assert_eq!(level.tables()[1].table_identity().as_str(), "zzz");
}

#[test]
fn table_manifest_sorts_l1_plus_by_physical_range() {
    let branch = branch(0x11);
    let level = TableManifestLevel::new(
        BranchLevel::new(2),
        vec![
            table_ref_at_level(branch, 2, "late", 1, b"m", b"n"),
            table_ref_at_level(branch, 2, "early", 0, b"a", b"b"),
        ],
    )
    .expect("level");

    assert_eq!(level.tables()[0].table_identity().as_str(), "early");
    assert_eq!(level.tables()[1].table_identity().as_str(), "late");
}

#[test]
fn table_manifest_preserves_inherited_layer_order() {
    let manifest = inherited_manifest();

    assert_eq!(manifest.inherited_layers()[0].order(), 0);
    assert_eq!(
        manifest.inherited_layers()[0].source_branch_id(),
        branch(0x22)
    );
    assert_eq!(manifest.inherited_layers()[1].order(), 1);
    assert_eq!(
        manifest.inherited_layers()[1].source_branch_id(),
        branch(0x33)
    );
}

#[test]
fn table_manifest_preserves_tables_inside_inherited_layer() {
    let decoded = round_trip(&inherited_manifest());
    let layer = &decoded.inherited_layers()[0];

    assert_eq!(
        layer.levels()[0].tables()[0].table_identity().as_str(),
        "ancestor-l0"
    );
    assert_eq!(
        layer.levels()[1].tables()[0].table_identity().as_str(),
        "ancestor-l1"
    );
}

#[test]
fn equivalent_table_manifests_encode_identically() {
    let branch = branch(0x11);
    let l0 = TableManifestLevel::new(
        BranchLevel::ZERO,
        vec![
            table_ref(branch, "b", 1, b"k2", b"k3"),
            table_ref(branch, "a", 0, b"k0", b"k1"),
        ],
    )
    .expect("level");
    let l1 = TableManifestLevel::new(
        BranchLevel::new(1),
        vec![
            table_ref_at_level(branch, 1, "d", 1, b"r2", b"r3"),
            table_ref_at_level(branch, 1, "c", 0, b"r0", b"r1"),
        ],
    )
    .expect("level");
    let left = TableManifest::new(
        branch,
        Some(7),
        2,
        vec![l1.clone(), l0.clone()],
        vec![],
        vec![],
    )
    .expect("left");
    let right =
        TableManifest::new(branch, Some(7), 2, vec![l0, l1], vec![], vec![]).expect("right");

    assert_eq!(
        encode_table_manifest(&left).expect("left bytes"),
        encode_table_manifest(&right).expect("right bytes")
    );
}

#[test]
fn different_l0_order_encodes_differently() {
    let branch = branch(0x11);
    let left_level = TableManifestLevel::new(
        BranchLevel::ZERO,
        vec![
            table_ref(branch, "a", 0, b"k0", b"k1"),
            table_ref(branch, "b", 1, b"k2", b"k3"),
        ],
    )
    .expect("left level");
    let branch_text = branch.to_string();
    let right_level = TableManifestLevel::new(
        BranchLevel::ZERO,
        vec![
            table_ref_with_object(
                "a",
                &format!("tables/{branch_text}/l0000/tb"),
                1,
                b"k0",
                b"k1",
            ),
            table_ref_with_object(
                "b",
                &format!("tables/{branch_text}/l0000/ta"),
                0,
                b"k2",
                b"k3",
            ),
        ],
    )
    .expect("right level");
    let left = TableManifest::new(branch, None, 1, vec![left_level], vec![], vec![]).expect("left");
    let right =
        TableManifest::new(branch, None, 1, vec![right_level], vec![], vec![]).expect("right");

    assert_ne!(
        encode_table_manifest(&left).expect("left bytes"),
        encode_table_manifest(&right).expect("right bytes")
    );
}

#[test]
fn table_manifest_rejects_l1_overlap_and_bad_order() {
    let branch = branch(0x11);
    let overlap = TableManifestLevel::new(
        BranchLevel::new(1),
        vec![
            table_ref_at_level(branch, 1, "a", 0, b"k0", b"k3"),
            table_ref_at_level(branch, 1, "b", 1, b"k3", b"k4"),
        ],
    );
    assert!(matches!(
        overlap,
        Err(FormatError::InvalidValue {
            field: "physical_range_overlap"
        })
    ));
}

#[test]
fn table_manifest_rejects_non_contiguous_l0_order() {
    let branch = branch(0x11);
    let table = table_ref(branch, "a", 1, b"a", b"b");

    assert_invalid_value(
        TableManifestLevel::from_decoded(BranchLevel::ZERO, vec![table]),
        "table_order",
    );
}

#[test]
fn table_manifest_rejects_duplicate_l0_order() {
    let branch = branch(0x11);
    let tables = vec![
        table_ref(branch, "a", 0, b"a", b"b"),
        table_ref(branch, "b", 0, b"c", b"d"),
    ];

    assert_invalid_value(
        TableManifestLevel::from_decoded(BranchLevel::ZERO, tables),
        "table_order",
    );
}

#[test]
fn table_manifest_rejects_l1_plus_overlap() {
    table_manifest_rejects_l1_overlap_and_bad_order();
}

#[test]
fn table_manifest_rejects_l1_plus_out_of_order_ranges() {
    let branch = branch(0x11);
    let tables = vec![
        table_ref_at_level(branch, 1, "late", 0, b"m", b"n"),
        table_ref_at_level(branch, 1, "early", 1, b"a", b"b"),
    ];

    assert_invalid_value(
        TableManifestLevel::from_decoded(BranchLevel::new(1), tables),
        "physical_range_order",
    );
}

#[test]
fn table_manifest_allows_l0_overlapping_ranges() {
    let branch = branch(0x11);
    let level = TableManifestLevel::new(
        BranchLevel::ZERO,
        vec![
            table_ref(branch, "a", 0, b"k", b"z"),
            table_ref(branch, "b", 1, b"k", b"z"),
        ],
    )
    .expect("overlapping l0");

    assert_eq!(level.tables().len(), 2);
}

#[test]
fn table_manifest_allows_distinct_versions_of_same_logical_key_when_ranges_are_valid() {
    let branch = branch(0x11);
    let level = TableManifestLevel::new(
        BranchLevel::new(1),
        vec![
            table_ref_at_level(branch, 1, "old", 0, b"k#v1", b"k#v1"),
            table_ref_at_level(branch, 1, "new", 1, b"k#v2", b"k#v2"),
        ],
    )
    .expect("distinct physical ranges");

    assert_eq!(level.tables().len(), 2);
}

#[test]
fn table_manifest_allows_sparse_owned_levels_until_branch_policy() {
    let branch = branch(0x11);
    let l1 = decoded_level(
        BranchLevel::new(1),
        vec![table_ref_at_level(branch, 1, "l1", 0, b"a", b"b")],
    );
    let manifest = TableManifest::from_decoded(branch, None, 1, vec![l1], vec![], vec![])
        .expect("format permits sparse levels until branch policy tightens");

    assert_eq!(manifest.levels()[0].level(), BranchLevel::new(1));
}

#[test]
fn table_manifest_structured_mutation_matrix_round_trips_or_rejects() {
    for seed in 1..16_u8 {
        let manifest = generated_manifest(seed);
        let bytes = encode_table_manifest(&manifest).expect("encode generated manifest");
        assert_eq!(decode_table_manifest(&bytes), Ok(manifest));

        for mutation in [
            ManifestMutation::Magic,
            ManifestMutation::Version,
            ManifestMutation::Checksum,
            ManifestMutation::Count,
            ManifestMutation::ObjectName,
            ManifestMutation::Order,
            ManifestMutation::Range,
            ManifestMutation::Status,
            ManifestMutation::SectionFlags,
        ] {
            let mut mutated = bytes.clone();
            mutation.apply(&mut mutated, seed);
            assert!(
                decode_table_manifest(&mutated).is_err(),
                "mutation {mutation:?} accepted for seed {seed}"
            );
        }
    }
}
