#![allow(dead_code)]

use strata_engine_next::{
    BranchName, CacheOpenOptions, Database, DatabaseOpenOutcome, DurableLocalOpenOptions,
    EngineResult, KvKey, KvValue, ProductSpace,
};

pub(crate) fn open_cache_database() -> EngineResult<Database> {
    Database::open_cache(CacheOpenOptions::new()).map(DatabaseOpenOutcome::into_database)
}

pub(crate) fn open_durable_database(path: &std::path::Path) -> EngineResult<Database> {
    Database::open_local(path, DurableLocalOpenOptions::new())
        .map(DatabaseOpenOutcome::into_database)
}

pub(crate) fn branch(name: &str) -> BranchName {
    BranchName::new(name).expect("valid branch name")
}

pub(crate) fn space(name: &str) -> ProductSpace {
    ProductSpace::new(name).expect("valid product space")
}

pub(crate) fn key(bytes: &[u8]) -> KvKey {
    KvKey::new(bytes).expect("valid key")
}

pub(crate) fn value(bytes: &[u8]) -> KvValue {
    KvValue::new(bytes)
}

pub(crate) fn assert_branch_value(
    database: &mut Database,
    branch_name: &str,
    space_name: &str,
    key_bytes: &[u8],
    expected: &[u8],
) {
    let mut kv = database
        .kv(branch(branch_name), space(space_name))
        .expect("KV service opens");
    let value = kv
        .get(&key(key_bytes))
        .expect("KV read succeeds")
        .expect("value exists");
    assert_eq!(value.as_bytes(), expected);
}

pub(crate) fn assert_default_branch_exists(database: &mut Database) {
    let summary = database
        .branches()
        .expect("branch service opens")
        .get(&branch("default"))
        .expect("default branch exists");
    assert_eq!(summary.name().as_str(), "default");
    assert_eq!(summary.generation(), 1);
}

pub(crate) fn assert_no_storage_type_in_engine_error(error: &strata_engine_next::EngineError) {
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
