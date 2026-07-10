//! `HubClone` behavior when the hub feature is not compiled (e.g. the
//! wasm build, which cannot carry the native HTTP client stack).

#![cfg(not(feature = "hub"))]

use strata_executor::{Command, Executor, ExecutorErrorClass};

#[test]
fn hub_clone_returns_a_stable_feature_disabled_error_without_feature() {
    let mut executor = Executor::open_cache().expect("cache executor opens");
    let error = executor
        .execute(Command::HubClone {
            dataset: "titanic".to_owned(),
            branch: None,
            dest: "unused".to_owned(),
            hub_url: None,
        })
        .expect_err("feature disabled clone fails");
    assert_eq!(error.class(), ExecutorErrorClass::InvalidInput);
    assert_eq!(
        error.code(),
        "invalid_argument.executor.hub_feature_disabled"
    );
}
