//! Executor error-boundary and source-guard tests.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;
use strata_executor::{
    public_error_code_entries, public_error_code_entry, with_error_render_config, Bytes, Command,
    CommitOutcomeStatus, ErrorClass, ErrorCodeRegistryEntry, ErrorReferenceIdSource,
    ErrorRenderConfig, ErrorStatus, Executor, ExecutorError, ExecutorErrorClass, RetryPolicy,
};

#[test]
fn executor_errors_have_stable_public_shape() {
    let mut executor = Executor::open_cache().expect("cache executor opens");

    let invalid_key = executor
        .execute(Command::KvGet {
            branch: None,
            space: None,
            key: Bytes::new(Vec::new()),
            as_of: None,
        })
        .expect_err("empty key fails");
    assert_eq!(invalid_key.class(), ExecutorErrorClass::InvalidInput);
    assert_eq!(invalid_key.public_class(), ErrorClass::InvalidArgument);
    assert_eq!(invalid_key.retry_policy(), RetryPolicy::Never);
    assert_eq!(
        invalid_key.commit_outcome(),
        CommitOutcomeStatus::NotStarted
    );
    assert!(!invalid_key.suggested_fix().is_empty());
    assert!(invalid_key.docs_url().ends_with(invalid_key.code()));
    assert!(invalid_key.reference_id().starts_with("err_local_"));
    assert!(invalid_key.code().contains(".engine."));

    let invalid_space = executor
        .execute(Command::KvPut {
            branch: None,
            space: Some("_system_".to_owned()),
            key: Bytes::from("key"),
            value: Bytes::from("value"),
        })
        .expect_err("reserved space fails");
    assert_eq!(invalid_space.class(), ExecutorErrorClass::InvalidInput);

    let missing_branch = executor
        .execute(Command::KvPut {
            branch: Some("missing".to_owned()),
            space: None,
            key: Bytes::from("key"),
            value: Bytes::from("value"),
        })
        .expect_err("missing branch fails");
    assert_eq!(missing_branch.class(), ExecutorErrorClass::NotFound);

    executor.close().expect("close succeeds");
    let closed = executor
        .execute(Command::KvExists {
            branch: None,
            space: None,
            key: Bytes::from("key"),
        })
        .expect_err("closed executor fails");
    assert_eq!(closed.class(), ExecutorErrorClass::ClosedHandle);
    assert_eq!(closed.public_class(), ErrorClass::FailedPrecondition);
    assert_eq!(closed.retry_policy(), RetryPolicy::Never);
    assert_eq!(closed.commit_outcome(), CommitOutcomeStatus::NotStarted);
}

#[derive(Debug)]
struct FixedReferenceIdSource;

impl ErrorReferenceIdSource for FixedReferenceIdSource {
    fn next_reference_id(&self) -> String {
        "ref_test_000001".to_owned()
    }
}

#[test]
fn executor_error_rendering_uses_injected_boundary_config() {
    let config = ErrorRenderConfig::new(
        "https://docs.example.test/errors/",
        Arc::new(FixedReferenceIdSource),
    );

    let error = with_error_render_config(config, || {
        ExecutorError::invalid_input("invalid_argument.executor.batch_item", "public message")
    });

    assert_eq!(error.reference_id(), "ref_test_000001");
    assert_eq!(
        error.docs_url(),
        "https://docs.example.test/errors/e/invalid_argument.executor.batch_item"
    );
}

#[test]
fn executor_preserves_engine_error_codes_at_public_boundary() {
    let error: ExecutorError = strata_engine::EngineError::closed_runtime("runtime closed").into();

    assert_eq!(error.code(), "failed_precondition.engine.runtime_closed");
    assert_eq!(error.public_class(), ErrorClass::FailedPrecondition);
    assert_eq!(error.commit_outcome(), CommitOutcomeStatus::NotStarted);
    assert!(error.docs_url().ends_with(error.code()));
}

#[test]
fn serialized_errors_have_v1_status_shape() {
    let error =
        ExecutorError::invalid_input("invalid_argument.executor.batch_item", "public message");
    let encoded = serde_json::to_string(&error).expect("error serializes");
    let status: serde_json::Value = serde_json::from_str(&encoded).expect("json parses");

    assert_eq!(status["class"], "invalid_argument");
    assert_eq!(status["code"], "invalid_argument.executor.batch_item");
    assert_eq!(status["retry_policy"], "never");
    assert_eq!(status["retryable"], false);
    assert_eq!(status["commit_outcome"], "not_started");
    assert_eq!(status["message"], "public message");
    assert_eq!(
        status["suggested_fix"],
        "Correct the batch item input and retry."
    );
    assert_eq!(
        status["docs_url"],
        "https://stratadb.org/e/invalid_argument.executor.batch_item"
    );
    assert!(status["reference_id"]
        .as_str()
        .expect("reference id is a string")
        .starts_with("err_local_"));

    for forbidden in forbidden_lower_layer_terms() {
        assert!(
            !encoded.contains(forbidden),
            "serialized error leaked forbidden term `{forbidden}`: {encoded}"
        );
    }
}

#[test]
fn deserialized_error_status_derives_retryable_from_retry_policy() {
    let status: ErrorStatus = serde_json::from_value(serde_json::json!({
        "class": "unavailable",
        "code": "unavailable.executor.lock_unavailable",
        "retry_policy": "same_request",
        "retryable": false,
        "commit_outcome": "not_started",
        "message": "lock unavailable",
        "suggested_fix": "Retry the same request after the lock becomes available.",
        "docs_url": "https://stratadb.org/e/unavailable.executor.lock_unavailable",
        "reference_id": "err-test-000001"
    }))
    .expect("error status deserializes");

    assert_eq!(status.retry_policy(), RetryPolicy::SameRequest);
    assert!(status.retryable());
}

#[test]
fn unregistered_executor_errors_render_registered_internal_fallback() {
    let error = ExecutorError::invalid_input("invalid_argument.executor.test", "public message");

    assert_eq!(error.code(), "internal.executor.unregistered_code");
    assert_eq!(error.public_class(), ErrorClass::Internal);
    assert_eq!(error.retry_policy(), RetryPolicy::Unknown);
    assert_eq!(error.commit_outcome(), CommitOutcomeStatus::NotApplicable);
    assert!(error
        .status()
        .details()
        .iter()
        .any(|detail| detail.key() == "unregistered_code"
            && detail.value() == "invalid_argument.executor.test"));
}

#[test]
fn explicit_error_status_constructors_normalize_unregistered_codes() {
    let direct = ErrorStatus::new(
        ErrorClass::InvalidArgument,
        "invalid_argument.executor.direct_test",
        RetryPolicy::Never,
        CommitOutcomeStatus::NotStarted,
        "public message",
        "custom fix",
        "err-test-direct",
        None,
        Vec::new(),
        Vec::new(),
    );
    assert_eq!(direct.code(), "internal.executor.unregistered_code");
    assert_eq!(direct.class(), ErrorClass::Internal);
    assert!(direct
        .details()
        .iter()
        .any(|detail| detail.key() == "unregistered_code"
            && detail.value() == "invalid_argument.executor.direct_test"));

    let with_url = ErrorStatus::new_with_docs_url(
        ErrorClass::InvalidArgument,
        "invalid_argument.executor.url_test",
        RetryPolicy::Never,
        CommitOutcomeStatus::NotStarted,
        "public message",
        "custom fix",
        "https://example.invalid/errors#wrong-anchor",
        "err-test-url",
        None,
        Vec::new(),
        Vec::new(),
    );
    assert_eq!(with_url.code(), "internal.executor.unregistered_code");
    assert_eq!(
        with_url.docs_url(),
        "https://stratadb.org/e/internal.executor.unregistered_code"
    );

    let with_registry_url = ErrorStatus::new_with_docs_url(
        ErrorClass::InvalidArgument,
        "invalid_argument.executor.url_test",
        RetryPolicy::Never,
        CommitOutcomeStatus::NotStarted,
        "public message",
        "custom fix",
        "https://docs.example.test/errors/e#wrong-anchor",
        "err-test-url",
        None,
        Vec::new(),
        Vec::new(),
    );
    assert_eq!(
        with_registry_url.docs_url(),
        "https://docs.example.test/errors/e/internal.executor.unregistered_code"
    );
}

#[test]
fn executor_error_from_status_normalizes_unregistered_codes() {
    let status = ErrorStatus::new_with_docs_url(
        ErrorClass::InvalidArgument,
        "invalid_argument.executor.from_status_test",
        RetryPolicy::Never,
        CommitOutcomeStatus::NotStarted,
        "public message",
        "custom fix",
        "https://example.invalid/errors#wrong-anchor",
        "err-test-from-status",
        None,
        Vec::new(),
        Vec::new(),
    );

    let error = ExecutorError::from_status(status);

    assert_eq!(error.code(), "internal.executor.unregistered_code");
    assert_eq!(error.public_class(), ErrorClass::Internal);
    assert_eq!(
        error.docs_url(),
        "https://stratadb.org/e/internal.executor.unregistered_code"
    );
}

#[test]
fn public_error_registry_has_reviewed_metadata() {
    let mut codes = std::collections::BTreeSet::new();
    for entry in public_error_code_entries() {
        assert!(
            codes.insert(entry.code),
            "duplicate public error code {}",
            entry.code
        );
        assert_eq!(
            entry.docs_slug, entry.code,
            "{} docs slug drifted",
            entry.code
        );
        assert!(
            !entry.suggested_fix.trim().is_empty(),
            "{} has an empty suggested fix",
            entry.code
        );
        assert!(
            !entry.message_template.trim().is_empty(),
            "{} has an empty message template",
            entry.code
        );
        assert!(
            !entry.suggested_fix.contains("Correct the command input"),
            "{} kept a generic executor suggested fix",
            entry.code
        );
        assert!(
            !entry.details_schema.trim().is_empty(),
            "{} has an empty details schema",
            entry.code
        );
        assert_registry_class_matches_prefix(entry);
    }
}

#[test]
fn every_executor_source_error_code_is_registered() {
    let mut unregistered = Vec::new();
    for file in source_files(&crate_root().join("src")) {
        if file.ends_with("error_registry.rs") {
            continue;
        }
        let text = fs::read_to_string(&file).expect("source reads");
        for code in extract_public_error_codes(&text, ".executor.") {
            if public_error_code_entry(&code).is_none() {
                unregistered.push(format!("{}:{}", file.display(), code));
            }
        }
    }

    assert!(
        unregistered.is_empty(),
        "executor source emits unregistered public error codes: {unregistered:?}"
    );
}

#[test]
fn every_inference_error_code_is_registered() {
    let inference_error_source =
        fs::read_to_string(workspace_root().join("crates/inference/src/error.rs"))
            .expect("inference error source reads");
    let unregistered: Vec<String> =
        extract_public_error_codes(&inference_error_source, "inference.")
            .into_iter()
            .filter(|code| public_error_code_entry(code).is_none())
            .collect();

    assert!(
        unregistered.is_empty(),
        "inference source emits unregistered public error codes: {unregistered:?}"
    );
}

#[test]
fn every_registry_docs_url_has_target() {
    let docs_target = workspace_root().join("docs/errors/registry.md");
    let docs = fs::read_to_string(&docs_target).expect("error registry docs target exists");

    assert!(
        docs.contains("# Error Code Registry"),
        "{} is not the error registry docs page",
        docs_target.display()
    );
    for entry in public_error_code_entries() {
        // Stable short per-code slug (first-run D4): the code is the final
        // path segment, so every code resolves to its own docs page.
        let url = format!("https://stratadb.org/e/{}", entry.docs_slug);
        assert!(
            url.ends_with(entry.code),
            "{} docs URL does not include code",
            entry.code
        );
        let anchor = format!(r#"<a id="{}"></a>"#, entry.docs_slug);
        assert!(
            docs.contains(&anchor),
            "{} is missing docs anchor {}",
            entry.code,
            anchor
        );
    }
}

#[test]
fn public_response_fixtures_are_valid_pretty_json_files() {
    for file in response_fixture_files() {
        let text = fs::read_to_string(&file).expect("response fixture reads");
        assert!(
            text.ends_with('\n'),
            "{} must end with a trailing newline",
            file.display()
        );
        assert!(
            text.contains("\n  "),
            "{} must be pretty-printed",
            file.display()
        );
        let _: Value = serde_json::from_str(&text)
            .unwrap_or_else(|error| panic!("{} is invalid JSON: {error}", file.display()));
    }
}

#[test]
fn public_response_fixtures_do_not_leak_lower_layer_details() {
    for file in response_fixture_files() {
        let text = fs::read_to_string(&file).expect("response fixture reads");
        for forbidden in forbidden_response_fixture_terms() {
            assert!(
                !text.contains(forbidden),
                "{} leaked lower-layer response detail `{forbidden}`",
                file.display()
            );
        }
    }
}

#[test]
fn public_response_fixtures_preserve_page_contract() {
    for file in response_fixture_files() {
        let text = fs::read_to_string(&file).expect("response fixture reads");
        let value: Value = serde_json::from_str(&text).expect("response fixture parses");
        assert_page_contract(&value, &file.display().to_string());
    }
}

#[test]
fn public_response_fixtures_never_serialize_string_only_item_errors() {
    for file in response_fixture_files() {
        let text = fs::read_to_string(&file).expect("response fixture reads");
        let value: Value = serde_json::from_str(&text).expect("response fixture parses");
        assert_no_string_error_field(&value, &file.display().to_string());
    }
}

#[test]
fn executor_crate_does_not_depend_on_storage_crates() {
    let manifest = fs::read_to_string(crate_root().join("Cargo.toml")).expect("manifest reads");
    assert!(!manifest.contains("strata-storage"));
    assert!(!manifest.contains("strata_storage"));
}

#[test]
fn executor_sources_do_not_name_lower_layer_types() {
    for file in source_files(&crate_root().join("src")) {
        let text = fs::read_to_string(&file).expect("source reads");
        for forbidden in forbidden_lower_layer_terms() {
            assert!(
                !text.contains(forbidden),
                "{} leaked forbidden term `{forbidden}`",
                file.display()
            );
        }
    }
}

#[test]
fn executor_event_sources_do_not_own_event_product_behavior() {
    for file in source_files(&crate_root().join("src")) {
        if file
            .file_name()
            .is_some_and(|name| name == "idl_tooling.rs" || name == "cli_metadata.rs")
        {
            continue;
        }
        let text = fs::read_to_string(&file).expect("source reads");
        for forbidden in forbidden_event_lower_layer_terms() {
            assert!(
                !text.contains(forbidden),
                "{} leaked forbidden event lower-layer term `{forbidden}`",
                file.display()
            );
        }
    }
}

#[test]
fn executor_vector_sources_do_not_own_index_or_distance_behavior() {
    for file in source_files(&crate_root().join("src")) {
        let text = fs::read_to_string(&file).expect("source reads");
        for forbidden in forbidden_vector_index_lower_layer_terms() {
            assert!(
                !text.contains(forbidden),
                "{} leaked forbidden vector lower-layer term `{forbidden}`",
                file.display()
            );
        }
    }
}

#[test]
fn command_and_output_are_serde_serializable() {
    let command_source =
        fs::read_to_string(crate_root().join("src/command.rs")).expect("command reads");
    let output_source =
        fs::read_to_string(crate_root().join("src/output.rs")).expect("output reads");

    assert!(command_source.contains("Serialize"));
    assert!(command_source.contains("Deserialize"));
    assert!(output_source.contains("Serialize"));
    assert!(output_source.contains("Deserialize"));
}

#[test]
fn convenience_facade_stays_command_shaped() {
    let mut facade = fs::read_to_string(crate_root().join("src/executor/facade.rs"))
        .expect("executor facade reads");
    for file in source_files(&crate_root().join("src/executor/facade")) {
        facade.push_str(&fs::read_to_string(file).expect("executor facade module reads"));
    }

    assert!(facade.contains("self.execute(Command::KvPut"));
    assert!(facade.contains("self.execute(Command::KvBatchPut"));
    assert!(facade.contains("self.execute(Command::JsonSet"));
    assert!(facade.contains("self.execute(Command::JsonGet"));
    assert!(facade.contains("self.execute(Command::JsonDelete"));
    assert!(facade.contains("self.execute(Command::JsonBatchSet"));
    assert!(facade.contains("self.execute(Command::JsonBatchGet"));
    assert!(facade.contains("self.execute(Command::JsonBatchDelete"));
    assert!(facade.contains("self.execute(Command::VectorCreateCollection"));
    assert!(facade.contains("self.execute(Command::VectorUpsert"));
    assert!(facade.contains("self.execute(Command::VectorGet"));
    assert!(facade.contains("self.execute(Command::VectorQuery"));
    assert!(facade.contains("self.execute(Command::VectorBatchUpsert"));
    assert!(facade.contains("self.execute(Command::VectorBatchGet"));
    assert!(facade.contains("self.execute(Command::VectorBatchDelete"));
    assert!(facade.contains("self.execute(Command::EventBatchAppend"));
    assert!(facade.contains("self.execute(Command::EventAppend"));
    assert!(facade.contains("self.execute(Command::EventGet"));
    assert!(facade.contains("self.execute(Command::EventExists"));
    assert!(facade.contains("self.execute(Command::EventLen"));
    assert!(facade.contains("self.execute(Command::EventRange"));
    assert!(facade.contains("self.execute(Command::EventRangeByTime"));
    assert!(facade.contains("self.execute(Command::EventListTypes"));
    assert!(facade.contains("self.execute(Command::EventList"));
    assert!(facade.contains("self.execute(Command::EventVerifyChain"));
    assert!(facade.contains("self.execute(Command::GraphCreate"));
    assert!(facade.contains("self.execute(Command::GraphDelete"));
    assert!(facade.contains("self.execute(Command::GraphList"));
    assert!(facade.contains("self.execute(Command::GraphGetMeta"));
    assert!(facade.contains("self.execute(Command::GraphAddNode"));
    assert!(facade.contains("self.execute(Command::GraphGetNode"));
    assert!(facade.contains("self.execute(Command::GraphRemoveNode"));
    assert!(facade.contains("self.execute(Command::GraphListNodes"));
    assert!(facade.contains("self.execute(Command::GraphAddEdge"));
    assert!(facade.contains("self.execute(Command::GraphGetEdge"));
    assert!(facade.contains("self.execute(Command::GraphRemoveEdge"));
    assert!(facade.contains("self.execute(Command::GraphNeighbors"));
    assert!(facade.contains("self.execute(Command::GraphBindingsForEntity"));
    assert!(facade.contains("self.execute(Command::GraphBatchWrite"));
    assert!(!facade.contains(".kv("));
    assert!(!facade.contains(".json("));
    assert!(!facade.contains(".vector("));
    assert!(!facade.contains(".event("));
    assert!(!facade.contains(".graph("));
    assert!(!facade.contains("json_service("));
    assert!(!facade.contains("vector_service("));
    assert!(!facade.contains("event_service("));
    assert!(!facade.contains("graph_service("));
    assert!(!facade.contains(".put("));
    assert!(!facade.contains(".put_batch("));
    assert!(!facade.contains(".set_or_create("));
    assert!(!facade.contains(".batch_set_or_create("));
    assert!(!facade.contains(".batch_delete_entries("));
    assert!(!facade.contains(".batch_upsert("));
    assert!(!facade.contains(".query("));
    assert!(!facade.contains(".append("));
    assert!(!facade.contains(".batch_append("));
    assert!(!facade.contains(".delete("));
    assert!(!facade.contains(".create_graph("));
    assert!(!facade.contains(".batch_write("));
}

#[test]
fn event_batch_append_handler_uses_engine_batch_api() {
    let source = fs::read_to_string(crate_root().join("src/executor/event.rs"))
        .expect("event handler reads");
    let handler = source
        .split("fn execute_event_batch_append")
        .nth(1)
        .expect("event batch handler is present")
        .split("fn execute_event_append")
        .next()
        .expect("event append handler follows batch handler");

    assert!(handler.contains(".batch_append("));
    assert!(!handler.contains("execute_event_append"));
    assert!(!handler.contains(".append("));
}

#[test]
fn source_contract_uses_kv_specific_value_outputs() {
    let output_source =
        fs::read_to_string(crate_root().join("src/output.rs")).expect("output reads");
    let mut tests_source =
        fs::read_to_string(crate_root().join("tests/command_contract.rs")).expect("tests read");
    for file in source_files(&crate_root().join("tests/command_contract")) {
        tests_source.push_str(&fs::read_to_string(file).expect("command contract module reads"));
    }
    assert!(output_source.contains("KvVersionedValue"));
    assert!(!output_source.contains("KvVersionedValue(Maybe"));
    assert!(output_source.contains("JsonValue(MaybeJsonValue)"));
    assert!(output_source.contains("JsonVersionedValue(MaybeJsonVersionedValue)"));
    assert!(tests_source.contains("MaybeJsonValue::missing"));
    assert!(tests_source.contains("MaybeJsonVersionedValue::missing"));
}

#[test]
fn executor_graph_sources_do_not_own_graph_storage_behavior() {
    for file in source_files(&crate_root().join("src")) {
        let text = fs::read_to_string(&file).expect("source reads");
        for forbidden in forbidden_graph_lower_layer_terms() {
            assert!(
                !text.contains(forbidden),
                "{} leaked forbidden graph lower-layer term `{forbidden}`",
                file.display()
            );
        }
    }
}

#[test]
fn executor_graph_surface_excludes_deferred_old_commands() {
    for file in source_files(&crate_root().join("src")) {
        let text = fs::read_to_string(&file).expect("source reads");
        for forbidden in excluded_graph_command_names() {
            assert!(
                !text.contains(forbidden),
                "{} exposed deferred graph command `{forbidden}`",
                file.display()
            );
        }
    }
}

#[test]
fn executor_admin_surface_excludes_deferred_old_commands() {
    for file in source_files(&crate_root().join("src")) {
        let text = fs::read_to_string(&file).expect("source reads");
        for forbidden in excluded_admin_command_names() {
            assert!(
                !text.contains(forbidden),
                "{} exposed deferred admin command `{forbidden}`",
                file.display()
            );
        }
    }
}

#[test]
fn executor_benchmarks_do_not_bypass_commands() {
    let benchmark_root = workspace_root().join("benchmarks/src/bin");
    if !benchmark_root.exists() {
        return;
    }

    for file in source_files(&benchmark_root) {
        let text = fs::read_to_string(&file).expect("benchmark source reads");
        if !is_executor_benchmark_source(&text) {
            continue;
        }

        assert!(
            text.contains("Command::KvBatchPut")
                || text.contains("Command::JsonBatchSet")
                || text.contains("Command::VectorBatchUpsert")
                || text.contains("Command::EventBatchAppend")
                || text.contains("Command::GraphBatchWrite")
                || text.contains("Command::ArrowImport")
                || text.contains("Command::ArrowExport"),
            "{} must use serialized executor batch commands",
            file.display()
        );
        for forbidden in [
            "strata_storage",
            "StorageRuntime",
            "CommitBatch",
            ".put_batch(",
            ".commit(",
        ] {
            assert!(
                !text.contains(forbidden),
                "{} bypassed executor commands with `{forbidden}`",
                file.display()
            );
        }
    }
}

#[test]
fn executor_arrow_sources_stay_on_serialized_command_boundary() {
    let arrow_root = crate_root().join("src/arrow");
    for file in source_files(&arrow_root) {
        let text = fs::read_to_string(&file).expect("Arrow source reads");
        for forbidden in forbidden_arrow_lower_layer_terms() {
            assert!(
                !text.contains(forbidden),
                "{} bypassed executor commands with `{forbidden}`",
                file.display()
            );
        }
    }

    let import_source =
        fs::read_to_string(arrow_root.join("import.rs")).expect("Arrow import reads");
    assert!(import_source.contains("Command::KvBatchPut"));
    assert!(import_source.contains("Command::JsonBatchSet"));
    assert!(import_source.contains("Command::VectorBatchUpsert"));
    assert!(import_source.contains("Command::VectorListCollections"));

    let export_source =
        fs::read_to_string(arrow_root.join("export.rs")).expect("Arrow export reads");
    assert!(export_source.contains("Command::KvList"));
    assert!(export_source.contains("Command::KvBatchGet"));
    assert!(export_source.contains("Command::JsonList"));
    assert!(export_source.contains("Command::JsonGet"));
    assert!(export_source.contains("Command::EventRange"));
    assert!(export_source.contains("Command::VectorListKeys"));
    assert!(export_source.contains("Command::VectorBatchGet"));
    assert!(export_source.contains("Command::GraphListNodes"));
    assert!(export_source.contains("Command::GraphNeighbors"));
}

fn is_executor_benchmark_source(text: &str) -> bool {
    text.contains("strata_executor")
        || text.contains("Command::Kv")
        || text.contains("Command::Json")
        || text.contains("Command::Vector")
        || text.contains("Command::Event")
        || text.contains("Command::Graph")
        || text.contains("Command::Arrow")
}

fn assert_registry_class_matches_prefix(entry: ErrorCodeRegistryEntry) {
    let prefix = entry.code.split('.').next().expect("code has prefix");
    let expected = match prefix {
        "invalid_argument" => ErrorClass::InvalidArgument,
        "not_found" => ErrorClass::NotFound,
        "already_exists" => ErrorClass::AlreadyExists,
        "failed_precondition" => ErrorClass::FailedPrecondition,
        "access_denied" => ErrorClass::AccessDenied,
        "conflict" => ErrorClass::Conflict,
        "ambiguous_commit" => ErrorClass::AmbiguousCommit,
        "history_unavailable" => ErrorClass::HistoryUnavailable,
        "unsupported" => ErrorClass::Unsupported,
        "resource_exhausted" => ErrorClass::ResourceExhausted,
        "unavailable" => ErrorClass::Unavailable,
        "io" => ErrorClass::Io,
        "corruption" | "data_loss" => ErrorClass::Corruption,
        "serialization" => ErrorClass::Serialization,
        "internal" => ErrorClass::Internal,
        "inference" => return,
        other => panic!("unknown error-code prefix `{other}` in {}", entry.code),
    };
    assert_eq!(entry.class, expected, "{} has wrong class", entry.code);
}

fn extract_public_error_codes(text: &str, infix: &str) -> Vec<String> {
    let mut codes = Vec::new();
    let mut search_start = 0;
    while let Some(relative_index) = text[search_start..].find(infix) {
        let index = search_start + relative_index;
        let mut start = index;
        while start > 0 && is_error_code_char(text.as_bytes()[start - 1]) {
            start -= 1;
        }
        let mut end = index + infix.len();
        while end < text.len() && is_error_code_char(text.as_bytes()[end]) {
            end += 1;
        }
        let delimited = start > 0
            && text.as_bytes()[start - 1] == b'"'
            && end < text.len()
            && text.as_bytes()[end] == b'"';
        if delimited {
            codes.push(text[start..end].to_owned());
        }
        search_start = end;
    }
    codes.sort();
    codes.dedup();
    codes
}

const fn is_error_code_char(byte: u8) -> bool {
    byte == b'_' || byte == b'.' || (byte >= b'a' && byte <= b'z')
}

fn forbidden_lower_layer_terms() -> &'static [&'static str] {
    &[
        "strata-storage",
        "strata_storage",
        "StorageRuntime",
        "CommitBatch",
        "CommitMutation",
        "StorageSpaceId",
        "StorageKey",
        "StorageValue",
        "BranchRequest",
        "Wal",
        "TableRuntime",
        "Lifecycle",
        "Compaction",
        "storage_api",
    ]
}

fn forbidden_event_lower_layer_terms() -> &'static [&'static str] {
    &[
        "compute_event_hash",
        "EventRecordEnvelope",
        "EventLogMetadata",
        "EventHash",
        "encode_event_record",
        "decode_event_record",
        "encode_event_metadata",
        "decode_event_metadata",
        "event_raw_rows",
        "event_rows(",
        "event_address",
        "type_index_address",
        "metadata_address",
        "StoragePersistence",
        "PersistenceReadRow",
        "RowMutation",
        "RowAddress",
        "ReadSelector",
        "CommitPlan",
        "sha2",
        "Sha256",
        "shadow_embedding",
        "embedding_runtime",
        "export_hook",
        "ExportService",
        "SearchIndex",
        "search_index",
    ]
}

fn forbidden_graph_lower_layer_terms() -> &'static [&'static str] {
    &[
        "strata_engine::data::graph",
        "strata_engine::graph",
        "GraphMetadataRecord",
        "GraphNodeRecord",
        "GraphEdgeRecord",
        "GraphBindingRecord",
        "encode_graph_",
        "decode_graph_",
        "graph_metadata_row",
        "node_rows(",
        "edge_rows(",
        "reverse_edge_rows(",
        "binding_rows_for_space",
        "binding_address",
        "node_address",
        "edge_address",
        "reverse_edge_address",
        // GO4 lifted the ontology deferral and GA5 the analytics deferral:
        // commands delegate to the engine's D4 ontology and adjacency
        // snapshot surfaces.
    ]
}

fn forbidden_vector_index_lower_layer_terms() -> &'static [&'static str] {
    &[
        "strata_engine::data::vector",
        "VectorArtifact",
        "FlatVectorArtifact",
        "HnswVectorArtifact",
        "HnswRuntimeIndex",
        "VectorIndexManifest",
        "VectorIndexPolicy",
        "encode_flat_vector_artifact",
        "decode_flat_vector_artifact",
        "encode_hnsw_vector_artifact",
        "decode_hnsw_vector_artifact",
        "vector_score",
        "fast_hnsw",
        "Hnsw",
        "HNSW",
    ]
}

fn forbidden_arrow_lower_layer_terms() -> &'static [&'static str] {
    &[
        "strata_storage",
        "strata-storage",
        "strata_engine::data",
        "StorageRuntime",
        "CommitBatch",
        "CommitMutation",
        "RowMutation",
        "StorageKey",
        "StorageValue",
        "Wal",
        "TableRuntime",
        "Lifecycle",
        "Compaction",
        "database.",
        "self.database",
        ".kv(",
        ".json(",
        ".vector(",
        "kv_service(",
        "json_service(",
        "vector_service(",
        "event_service(",
        "graph_service(",
    ]
}

fn forbidden_response_fixture_terms() -> &'static [&'static str] {
    &[
        "strata_storage",
        "strata-storage",
        "StorageRuntime",
        "StorageKey",
        "StorageValue",
        "RowAddress",
        "CommitBatch",
        "CommitMutation",
        "engine-artifacts",
        "engine_artifacts",
        ".wal",
        ".sst",
        "/Users/",
        "\\Users\\",
        "target/debug",
        "_system_",
    ]
}

fn excluded_graph_command_names() -> &'static [&'static str] {
    &[
        // GO4 ported the ontology surface onto 8 commands, GA5 the
        // analytics surface onto 6, and GI3 bulk ingest; the v0.6
        // per-kind read commands below stay excluded by design — the one
        // canonical read is GraphGetOntology (status + all definitions).
        "GraphGetObjectType",
        "GraphListObjectTypes",
        "GraphGetLinkType",
        "GraphListLinkTypes",
        "GraphListOntologyTypes",
    ]
}

fn excluded_admin_command_names() -> &'static [&'static str] {
    &[
        "Command::Flush",
        "Flush {",
        "Command::Compact",
        "Compact {",
        "Command::TimeRange",
        "TimeRange {",
        "Command::DurabilityCounters",
        "DurabilityCounters",
        "ConfigSetAutoEmbed",
        "AutoEmbedStatus",
        "EmbedStatus",
        "ReindexEmbeddings",
        "ConfigureModel",
        "ConfigureSet",
        "RetentionApply",
        "RetentionPreview",
        "RetentionStats",
    ]
}

fn source_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_source_files(root, &mut files);
    files
}

fn response_fixture_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_files_with_extension(
        &crate_root().join("tests/fixtures/responses/v1"),
        "json",
        &mut files,
    );
    files.sort();
    files
}

fn collect_source_files(root: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(root).expect("directory reads") {
        let entry = entry.expect("directory entry reads");
        let path = entry.path();
        if path.is_dir() {
            collect_source_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

fn collect_files_with_extension(root: &Path, extension: &str, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(root).expect("directory reads") {
        let entry = entry.expect("directory entry reads");
        let path = entry.path();
        if path.is_dir() {
            collect_files_with_extension(&path, extension, files);
        } else if path
            .extension()
            .is_some_and(|actual_extension| actual_extension == extension)
        {
            files.push(path);
        }
    }
}

fn assert_page_contract(value: &Value, path: &str) {
    match value {
        Value::Object(object) => {
            if let Some(has_more) = object.get("has_more") {
                let has_more = has_more
                    .as_bool()
                    .unwrap_or_else(|| panic!("{path}: has_more must be a boolean"));
                let cursor = object
                    .get("cursor")
                    .unwrap_or_else(|| panic!("{path}: page object is missing cursor"));
                if has_more {
                    assert!(
                        !cursor.is_null(),
                        "{path}: continued page must carry a non-null cursor"
                    );
                } else {
                    assert!(
                        cursor.is_null(),
                        "{path}: terminal page must serialize cursor as null"
                    );
                }
            }
            for (key, nested) in object {
                assert_page_contract(nested, &format!("{path}.{key}"));
            }
        }
        Value::Array(items) => {
            for (index, nested) in items.iter().enumerate() {
                assert_page_contract(nested, &format!("{path}[{index}]"));
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn assert_no_string_error_field(value: &Value, path: &str) {
    match value {
        Value::Object(object) => {
            if let Some(error) = object.get("error") {
                assert!(
                    error.is_null() || error.is_object(),
                    "{path}: public error fields must be null or structured ErrorStatus objects"
                );
            }
            for (key, nested) in object {
                assert_no_string_error_field(nested, &format!("{path}.{key}"));
            }
        }
        Value::Array(items) => {
            for (index, nested) in items.iter().enumerate() {
                assert_no_string_error_field(nested, &format!("{path}[{index}]"));
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn workspace_root() -> PathBuf {
    crate_root()
        .parent()
        .and_then(Path::parent)
        .expect("crate is under workspace crates directory")
        .to_path_buf()
}
