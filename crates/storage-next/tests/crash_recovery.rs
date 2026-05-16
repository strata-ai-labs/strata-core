//! Process-level crash recovery harness entry point.

#![deny(unsafe_code)]

mod common;

#[cfg(all(feature = "testkit", feature = "localfs", not(target_arch = "wasm32")))]
#[test]
fn crash_recovery_harness_reopens_durable_local_objects() {
    let case_limit = common::crash_case_limit()
        .unwrap_or_else(|error| panic!("invalid crash harness environment: {error}"));
    let keep_test_dir = common::keep_test_dir()
        .unwrap_or_else(|error| panic!("invalid crash harness environment: {error}"));
    let test_root_override = common::test_root_override()
        .unwrap_or_else(|error| panic!("invalid crash harness environment: {error}"));

    let tempdir = test_root_override
        .is_none()
        .then(|| tempfile::tempdir().expect("temp crash harness root"));
    let root = test_root_override.unwrap_or_else(|| {
        tempdir
            .as_ref()
            .expect("tempdir exists without override")
            .path()
            .to_path_buf()
    });

    let outcome =
        strata_storage_next::testkit::run_localfs_crash_recovery_harness(&root, case_limit)
            .expect("crash recovery harness");

    assert!(outcome.cases_executed() > 0 || case_limit == Some(0));
    if keep_test_dir {
        if let Some(tempdir) = tempdir {
            eprintln!("keeping crash recovery harness root at {}", root.display());
            let _ = tempdir.keep();
        }
    }
}
