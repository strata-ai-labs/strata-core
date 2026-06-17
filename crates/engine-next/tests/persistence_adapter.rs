//! Persistence adapter boundary tests.

use strata_engine_next::{
    BranchName, CacheOpenOptions, Database, EngineErrorClass, KvKey, ProductSpace,
};

#[test]
#[cfg(feature = "testkit")]
fn row_class_assignments_match_registry_contract() {
    assert_eq!(
        strata_engine_next::testkit::row_class_storage_id_for_test("kv").expect("known class"),
        0x20
    );
    assert_eq!(
        strata_engine_next::testkit::row_class_storage_id_for_test("json").expect("known class"),
        0x22
    );
    assert_eq!(
        strata_engine_next::testkit::row_class_storage_id_for_test("json-index")
            .expect("known class"),
        0x24
    );
    assert_eq!(
        strata_engine_next::testkit::row_class_storage_id_for_test("branch").expect("known class"),
        0x30
    );
    assert_eq!(
        strata_engine_next::testkit::row_class_storage_id_for_test("registry")
            .expect("known class"),
        0x32
    );
    assert_eq!(
        strata_engine_next::testkit::row_class_storage_id_for_test("identity")
            .expect("known class"),
        0x34
    );
}

#[test]
fn engine_error_messages_do_not_expose_storage_type_names() {
    let error = BranchName::new("_system_").expect_err("reserved branch rejected");
    assert_engine_error_is_boundary_shaped(&error);

    let error = ProductSpace::new("_system_").expect_err("reserved space rejected");
    assert_engine_error_is_boundary_shaped(&error);

    let error = KvKey::new(Vec::new()).expect_err("empty key rejected");
    assert_engine_error_is_boundary_shaped(&error);
}

#[test]
fn closed_database_rejects_new_operations() {
    let mut database = Database::open_cache(CacheOpenOptions::new())
        .expect("cache open succeeds")
        .into_database();
    database.close().expect("close succeeds");
    let Err(error) = database.branches() else {
        panic!("closed handle accepted branch service");
    };
    assert_eq!(error.class(), EngineErrorClass::ClosedRuntime);

    let Err(error) = database.kv(
        BranchName::new("default").expect("valid branch"),
        ProductSpace::new("default").expect("valid space"),
    ) else {
        panic!("closed handle accepted KV service");
    };
    assert_eq!(error.class(), EngineErrorClass::ClosedRuntime);

    let Err(error) = database.json(
        BranchName::new("default").expect("valid branch"),
        ProductSpace::new("default").expect("valid space"),
    ) else {
        panic!("closed handle accepted JSON service");
    };
    assert_eq!(error.class(), EngineErrorClass::ClosedRuntime);
}

fn assert_engine_error_is_boundary_shaped(error: &strata_engine_next::EngineError) {
    let text = error.to_string();
    for forbidden in [
        "StorageRuntime",
        "CommitBatch",
        "StorageSpaceId",
        "StorageKey",
        "StorageValue",
        "BranchRequest",
        "storage_api",
    ] {
        assert!(
            !text.contains(forbidden),
            "engine error exposed storage detail: {text}"
        );
    }
}
