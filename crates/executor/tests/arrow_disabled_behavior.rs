//! Arrow command behavior when the Arrow feature is not compiled.

#![cfg(not(feature = "arrow"))]

use strata_executor::{
    ArrowExportPrimitive, ArrowFileFormat, ArrowImportTarget, Command, Executor, ExecutorErrorClass,
};
use tempfile::TempDir;

#[test]
fn arrow_commands_return_stable_feature_disabled_errors_without_feature() {
    let temp = TempDir::new().expect("temp dir");
    let input_path = temp.path().join("input.csv");
    std::fs::write(&input_path, "key,value\nalpha,one\n").expect("write input");
    let output_path = temp.path().join("output.csv");
    let mut executor = Executor::open_cache().expect("cache executor opens");

    let error = executor
        .execute(Command::ArrowImport {
            branch: None,
            space: None,
            file_path: input_path.to_string_lossy().into_owned(),
            format: Some(ArrowFileFormat::Csv),
            target: ArrowImportTarget::Kv,
            key_column: None,
            value_column: None,
            collection: None,
            graph: None,
        })
        .expect_err("feature disabled import fails");
    assert_eq!(error.class(), ExecutorErrorClass::InvalidInput);
    assert_eq!(
        error.code(),
        "invalid_argument.executor.arrow_feature_disabled"
    );

    let error = executor
        .execute(Command::ArrowExport {
            branch: None,
            space: None,
            primitive: ArrowExportPrimitive::Kv,
            format: ArrowFileFormat::Csv,
            path: output_path.to_string_lossy().into_owned(),
            prefix: None,
            limit: None,
            collection: None,
            graph: None,
            event_type: None,
        })
        .expect_err("feature disabled export fails");
    assert_eq!(error.class(), ExecutorErrorClass::InvalidInput);
    assert_eq!(
        error.code(),
        "invalid_argument.executor.arrow_feature_disabled"
    );
}

#[test]
fn missing_arrow_import_input_is_reported_before_feature_disabled() {
    let mut executor = Executor::open_cache().expect("cache executor opens");
    let error = executor
        .execute(Command::ArrowImport {
            branch: None,
            space: None,
            file_path: "missing.csv".to_owned(),
            format: Some(ArrowFileFormat::Csv),
            target: ArrowImportTarget::Kv,
            key_column: None,
            value_column: None,
            collection: None,
            graph: None,
        })
        .expect_err("missing input fails");
    assert_eq!(error.class(), ExecutorErrorClass::InvalidInput);
    assert_eq!(
        error.code(),
        "invalid_argument.executor.arrow_input_missing"
    );
}
