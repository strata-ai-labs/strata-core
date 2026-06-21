#![allow(dead_code)]

use strata_engine_next::{
    BranchName, CacheOpenOptions, Database, DatabaseOpenOutcome, DurableLocalOpenOptions,
    EngineError, EngineErrorClass, EngineResult, KvKey, KvValue, ProductSpace,
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

/// Asserts an engine error's stable status fields in one place.
pub(crate) fn assert_status(
    error: &EngineError,
    class: EngineErrorClass,
    code: &str,
    retryable: bool,
) {
    assert_eq!(error.class(), class, "unexpected error class for `{code}`");
    assert_eq!(error.code(), code, "unexpected error code");
    assert_eq!(
        error.retryable(),
        retryable,
        "unexpected retryable flag for `{code}`"
    );
}

/// Renders an error plus its full source chain into one string.
fn full_error_text(error: &EngineError) -> String {
    let mut text = error.to_string();
    let mut current = std::error::Error::source(error);
    while let Some(source) = current {
        text.push('\n');
        text.push_str(&source.to_string());
        current = source.source();
    }
    text
}

/// Asserts no storage implementation detail leaks through the error or any of
/// its sources. Unlike [`assert_no_storage_type_in_engine_error`], this walks
/// the whole source chain, where the raw storage error would otherwise surface.
pub(crate) fn assert_no_storage_leak(error: &EngineError) {
    let text = full_error_text(error);
    for forbidden in [
        "StorageRuntime",
        "CommitBatch",
        "StorageSpaceId",
        "StorageKey",
        "StorageValue",
        "BranchRequest",
        "StorageApiError",
        "WalService",
        "ManifestService",
        "TableRuntime",
        "storage_api",
    ] {
        assert!(
            !text.contains(forbidden),
            "engine error leaked storage detail `{forbidden}`: {text}"
        );
    }
}

/// Asserts no credential, token, or signed-URL material leaks through the error
/// or its sources. Forward-looking guard for clone and provider work.
pub(crate) fn assert_no_secret_leak(error: &EngineError) {
    let text = full_error_text(error).to_lowercase();
    for forbidden in [
        "authorization",
        "bearer ",
        "api_key",
        "apikey",
        "access_key",
        "signature=",
        "x-amz-",
    ] {
        assert!(
            !text.contains(forbidden),
            "engine error may have leaked a secret (`{forbidden}`)"
        );
    }
}
