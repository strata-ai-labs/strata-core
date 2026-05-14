use super::{LayoutError, ObjectFamily, ObjectLayout, MAX_TABLE_LEVEL};
use crate::object::ObjectNameError;

#[test]
fn manifest_lock_and_meta_objects_use_reserved_names() {
    assert_eq!(
        ObjectLayout::database_manifest()
            .expect("manifest")
            .as_str(),
        "manifest/current"
    );
    assert_eq!(
        ObjectLayout::writer_lock().expect("writer lock").as_str(),
        "locks/writer"
    );
    assert_eq!(
        ObjectLayout::database_meta()
            .expect("database meta")
            .as_str(),
        "meta/database"
    );
    assert_eq!(
        ObjectLayout::wal_segment_metadata(2)
            .expect("segment metadata")
            .as_str(),
        "meta/wal/0000000000000002"
    );
    assert_eq!(
        ObjectLayout::wal_segment_metadata_prefix()
            .expect("segment metadata prefix")
            .as_str(),
        "meta/wal/"
    );
}

#[test]
fn ordered_ids_use_fixed_width_lower_hex() {
    assert_eq!(
        ObjectLayout::wal_segment(2).expect("wal segment").as_str(),
        "wal/0000000000000002"
    );
    assert_eq!(
        ObjectLayout::wal_segment_metadata(2)
            .expect("segment metadata")
            .as_str(),
        "meta/wal/0000000000000002"
    );
    assert_eq!(
        ObjectLayout::wal_segment(16).expect("wal segment").as_str(),
        "wal/0000000000000010"
    );
    assert!(
        ObjectLayout::wal_segment(2).expect("two")
            < ObjectLayout::wal_segment(16).expect("sixteen")
    );
    assert_eq!(
        ObjectLayout::snapshot(u64::MAX).expect("snapshot").as_str(),
        "snapshots/ffffffffffffffff"
    );
}

#[test]
fn wal_segment_metadata_names_sort_like_segment_ids() {
    for left in sample_u64_values() {
        for right in sample_u64_values() {
            let left_name =
                ObjectLayout::wal_segment_metadata(left).expect("left segment metadata");
            let right_name =
                ObjectLayout::wal_segment_metadata(right).expect("right segment metadata");

            assert_eq!(left_name.cmp(&right_name), left.cmp(&right));
        }
    }
}

#[test]
fn wal_segment_names_sort_like_segment_ids() {
    for left in sample_u64_values() {
        for right in sample_u64_values() {
            let left_name = ObjectLayout::wal_segment(left).expect("left segment");
            let right_name = ObjectLayout::wal_segment(right).expect("right segment");

            assert_eq!(left_name.cmp(&right_name), left.cmp(&right));
        }
    }
}

#[test]
fn snapshot_names_sort_like_snapshot_ids() {
    for left in sample_u64_values() {
        for right in sample_u64_values() {
            let left_name = ObjectLayout::snapshot(left).expect("left snapshot");
            let right_name = ObjectLayout::snapshot(right).expect("right snapshot");

            assert_eq!(left_name.cmp(&right_name), left.cmp(&right));
        }
    }
}

#[test]
fn table_layout_uses_branch_level_and_table_components() {
    assert_eq!(
        ObjectLayout::table_object("branch0001", 12, "table00ff")
            .expect("table")
            .as_str(),
        "tables/branch0001/l0012/table00ff"
    );
    assert_eq!(
        ObjectLayout::branch_table_manifest("branch0001")
            .expect("manifest")
            .as_str(),
        "tables/branch0001/manifest"
    );
    assert_eq!(
        ObjectLayout::branch_table_prefix("branch0001")
            .expect("prefix")
            .as_str(),
        "tables/branch0001/"
    );
}

#[test]
fn table_layout_rejects_level_width_overflow() {
    assert!(matches!(
        ObjectLayout::table_object("branch0001", MAX_TABLE_LEVEL + 1, "table00ff"),
        Err(LayoutError::LevelOutOfRange {
            level: 10_000,
            max: MAX_TABLE_LEVEL
        })
    ));
}

#[test]
fn table_objects_stay_under_their_branch_prefix() {
    for branch in sample_valid_components() {
        for table in sample_valid_components() {
            for level in sample_table_levels() {
                let object =
                    ObjectLayout::table_object(branch, *level, table).expect("table object");
                let prefix = ObjectLayout::branch_table_prefix(branch).expect("branch prefix");
                let expected_level = format!("l{level:04}");
                let components: Vec<_> = object.as_str().split('/').collect();

                assert!(object.as_str().starts_with(prefix.as_str()));
                assert_eq!(
                    ObjectFamily::from_object_name(&object),
                    Some(ObjectFamily::Tables)
                );
                assert_eq!(
                    components,
                    vec!["tables", branch, expected_level.as_str(), table]
                );
            }
        }
    }
}

#[test]
fn temporary_and_quarantine_layouts_are_global_families() {
    assert_eq!(
        ObjectLayout::temporary_object("op0001", "target0002")
            .expect("temporary")
            .as_str(),
        "tmp/op0001/target0002"
    );
    assert_eq!(
        ObjectLayout::operation_temporary_prefix("op0001")
            .expect("temporary prefix")
            .as_str(),
        "tmp/op0001/"
    );
    assert_eq!(
        ObjectLayout::quarantine_object("branch0001", "table00ff")
            .expect("quarantine object")
            .as_str(),
        "quarantine/branch0001/table00ff"
    );
    assert_eq!(
        ObjectLayout::quarantine_manifest("branch0001")
            .expect("quarantine manifest")
            .as_str(),
        "quarantine/branch0001/manifest"
    );
    assert_eq!(
        ObjectLayout::branch_quarantine_prefix("branch0001")
            .expect("quarantine prefix")
            .as_str(),
        "quarantine/branch0001/"
    );
}

#[test]
fn temporary_objects_stay_under_their_operation_prefix() {
    for operation in sample_valid_components() {
        for object_id in sample_valid_components() {
            let object =
                ObjectLayout::temporary_object(operation, object_id).expect("temporary object");
            let prefix =
                ObjectLayout::operation_temporary_prefix(operation).expect("operation prefix");

            assert!(object.as_str().starts_with(prefix.as_str()));
            assert_eq!(
                ObjectFamily::from_object_name(&object),
                Some(ObjectFamily::Temporary)
            );
        }
    }
}

#[test]
fn quarantine_objects_stay_under_their_branch_prefix() {
    for branch in sample_valid_components() {
        for object_id in sample_valid_components() {
            let object =
                ObjectLayout::quarantine_object(branch, object_id).expect("quarantine object");
            let prefix = ObjectLayout::branch_quarantine_prefix(branch).expect("quarantine prefix");

            assert!(object.as_str().starts_with(prefix.as_str()));
            assert_eq!(
                ObjectFamily::from_object_name(&object),
                Some(ObjectFamily::Quarantine)
            );
        }
    }
}

#[test]
fn every_reserved_family_has_a_prefix() {
    let families = [
        (ObjectFamily::Manifest, "manifest/"),
        (ObjectFamily::Wal, "wal/"),
        (ObjectFamily::Tables, "tables/"),
        (ObjectFamily::Snapshots, "snapshots/"),
        (ObjectFamily::Temporary, "tmp/"),
        (ObjectFamily::Quarantine, "quarantine/"),
        (ObjectFamily::Locks, "locks/"),
        (ObjectFamily::Meta, "meta/"),
    ];

    for (family, prefix) in families {
        assert_eq!(
            ObjectLayout::family_prefix(family)
                .expect("family prefix")
                .as_str(),
            prefix
        );
    }

    let direct_prefixes = [
        ObjectLayout::manifest_prefix().expect("manifest prefix"),
        ObjectLayout::wal_prefix().expect("wal prefix"),
        ObjectLayout::table_prefix().expect("table prefix"),
        ObjectLayout::snapshot_prefix().expect("snapshot prefix"),
        ObjectLayout::temporary_prefix().expect("temporary prefix"),
        ObjectLayout::quarantine_prefix().expect("quarantine prefix"),
        ObjectLayout::locks_prefix().expect("locks prefix"),
        ObjectLayout::meta_prefix().expect("meta prefix"),
    ];
    let direct_prefixes: Vec<_> = direct_prefixes
        .iter()
        .map(crate::object::ObjectPrefix::as_str)
        .collect();

    assert_eq!(
        direct_prefixes,
        vec![
            "manifest/",
            "wal/",
            "tables/",
            "snapshots/",
            "tmp/",
            "quarantine/",
            "locks/",
            "meta/",
        ]
    );
}

#[test]
fn family_detection_uses_first_component_only() {
    let table = ObjectLayout::table_object("branch0001", 0, "table0001").expect("table");
    let quarantine =
        ObjectLayout::quarantine_object("branch0001", "table0001").expect("quarantine");

    assert_eq!(
        ObjectFamily::from_object_name(&table),
        Some(ObjectFamily::Tables)
    );
    assert_eq!(
        ObjectFamily::from_object_name(&quarantine),
        Some(ObjectFamily::Quarantine)
    );
}

#[test]
fn invalid_components_cannot_escape_layout_namespace() {
    assert!(matches!(
        ObjectLayout::table_object("branch/child", 0, "table0001"),
        Err(LayoutError::ComponentContainsSeparator { role: "branch" })
    ));
    assert!(matches!(
        ObjectLayout::temporary_object("op0001", ".."),
        Err(LayoutError::InvalidComponent {
            role: "temporary object",
            source: ObjectNameError::TraversalComponent
        })
    ));
    assert!(matches!(
        ObjectLayout::quarantine_object("", "table0001"),
        Err(LayoutError::EmptyComponent { role: "branch" })
    ));
    assert!(matches!(
        ObjectLayout::branch_table_manifest("branch:bad"),
        Err(LayoutError::InvalidComponent {
            role: "branch",
            source: ObjectNameError::InvalidByte(b':')
        })
    ));
}

#[test]
fn invalid_components_are_rejected_before_name_construction() {
    for component in sample_invalid_components() {
        assert!(ObjectLayout::branch_table_prefix(component).is_err());
        assert!(ObjectLayout::table_object(component, 0, "table0001").is_err());
        assert!(ObjectLayout::table_object("branch0001", 0, component).is_err());
        assert!(ObjectLayout::temporary_object(component, "target0001").is_err());
        assert!(ObjectLayout::quarantine_object("branch0001", component).is_err());
    }
}

#[test]
fn follower_state_names_are_not_part_of_target_layout() {
    let layout_texts = [
        ObjectLayout::database_manifest()
            .expect("database manifest")
            .to_string(),
        ObjectLayout::wal_segment(1)
            .expect("wal segment")
            .to_string(),
        ObjectLayout::branch_table_manifest("branch0001")
            .expect("branch table manifest")
            .to_string(),
        ObjectLayout::table_object("branch0001", 0, "table0001")
            .expect("table object")
            .to_string(),
        ObjectLayout::snapshot(1).expect("snapshot").to_string(),
        ObjectLayout::temporary_object("op0001", "target0001")
            .expect("temporary")
            .to_string(),
        ObjectLayout::quarantine_manifest("branch0001")
            .expect("quarantine manifest")
            .to_string(),
        ObjectLayout::quarantine_object("branch0001", "table0001")
            .expect("quarantine")
            .to_string(),
        ObjectLayout::writer_lock()
            .expect("writer lock")
            .to_string(),
        ObjectLayout::database_meta()
            .expect("database meta")
            .to_string(),
        ObjectLayout::wal_segment_metadata(1)
            .expect("segment metadata")
            .to_string(),
    ];
    let forbidden_fragments = [
        "follower_state",
        "follower_audit",
        "MANIFEST",
        "wal-",
        "snap-",
        "segments.manifest",
        "quarantine.manifest",
        "__quarantine__",
    ];

    for text in layout_texts {
        for forbidden in forbidden_fragments {
            assert!(
                !text.contains(forbidden),
                "layout text {text:?} contains retired fragment {forbidden:?}"
            );
        }
    }
}

fn sample_u64_values() -> Vec<u64> {
    let mut values = vec![0, 1, 2, 15, 16, 255, 256, u64::from(u32::MAX), u64::MAX];
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    for _ in 0..32 {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        values.push(state);
    }
    values.sort_unstable();
    values.dedup();
    values
}

fn sample_valid_components() -> &'static [&'static str] {
    &[
        "a",
        "branch0001",
        "table-0001",
        "object_0001",
        "ABCxyz019",
        "component-with-32-characters-ok",
    ]
}

fn sample_table_levels() -> &'static [u32] {
    &[0, 1, 9, 10, 99, 100, 999, 1_000, MAX_TABLE_LEVEL]
}

fn sample_invalid_components() -> &'static [&'static str] {
    &[
        "",
        ".",
        "..",
        "/absolute",
        "branch/child",
        "branch//child",
        "branch:child",
        "branch child",
        "branch\\child",
        "branch/current/",
    ]
}
