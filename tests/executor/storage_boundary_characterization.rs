//! Executor storage-boundary characterization.
//!
//! These tests lock public executor behavior at the engine storage boundary.
//! Executor must preserve these outcomes without importing storage directly.

use crate::common::*;
use strata_core::Value;
use strata_executor::{BatchEventEntry, Command, DistanceMetric, Error, Output, Session, Strata};

fn assert_invalid_input_reason(error: Error, expected_reason: &str) {
    match error {
        Error::InvalidInput { reason, .. } => assert_eq!(reason, expected_reason),
        other => panic!("expected invalid input, got {other:?}"),
    }
}

fn assert_missing(output: Output) {
    assert!(
        matches!(output, Output::Maybe(None) | Output::MaybeVersioned(None)),
        "expected missing value, got {output:?}"
    );
}

fn assert_present_value(output: Output, expected: Value) {
    match output {
        Output::Maybe(Some(value)) => assert_eq!(value, expected),
        Output::MaybeVersioned(Some(versioned)) => assert_eq!(versioned.value, expected),
        other => panic!("expected present value {expected:?}, got {other:?}"),
    }
}

fn assert_bool(output: Output, expected: bool) {
    match output {
        Output::Bool(value) => assert_eq!(value, expected),
        other => panic!("expected bool output, got {other:?}"),
    }
}

fn assert_event_count(output: Output, expected: usize) {
    match output {
        Output::VersionedValues(events) => assert_eq!(events.len(), expected),
        other => panic!("expected event list, got {other:?}"),
    }
}

fn assert_batch_success(output: Output, expected_len: usize) -> Vec<u64> {
    match output {
        Output::BatchResults(results) => {
            assert_eq!(results.len(), expected_len);
            results
                .into_iter()
                .map(|result| {
                    assert!(
                        result.error.is_none(),
                        "expected successful batch item, got {result:?}"
                    );
                    result
                        .version
                        .expect("successful batch item should have a version")
                })
                .collect()
        }
        other => panic!("expected batch results, got {other:?}"),
    }
}

#[test]
fn storage_boundary_space_validation_is_executor_visible_for_handle_and_session() {
    let mut db = Strata::cache().expect("cache database should open");
    let expected_reason =
        "Space name can only contain lowercase letters, digits, hyphens, and underscores";

    let error = db
        .set_space("not allowed")
        .expect_err("typed handle should reject invalid space names");
    assert_invalid_input_reason(error, expected_reason);

    let mut session = db.session().expect("session should open");

    let error = session
        .execute(Command::SpaceExists {
            branch: None,
            space: "not allowed".into(),
        })
        .expect_err("SpaceExists should reject invalid space names before storage");
    assert_invalid_input_reason(error, expected_reason);

    let error = session
        .execute(Command::SpaceDelete {
            branch: None,
            space: "not allowed".into(),
            force: true,
        })
        .expect_err("SpaceDelete should reject invalid space names before storage");
    assert_invalid_input_reason(error, expected_reason);

    let error = session
        .set_space("not allowed")
        .expect_err("session should reject invalid space names");
    assert_invalid_input_reason(error, expected_reason);
}

#[test]
fn storage_boundary_transaction_omitted_space_uses_session_space_for_kv_json_and_events() {
    let db = create_db();
    let mut session = Session::new(db.clone());
    session
        .set_space("analytics")
        .expect("session space should update before transaction");

    session
        .execute(Command::TxnBegin {
            branch: None,
            options: None,
        })
        .expect("transaction should begin");
    session
        .execute(Command::KvPut {
            branch: None,
            space: None,
            key: "scoped-kv".into(),
            value: Value::Int(7),
        })
        .expect("kv write should use session space");
    let kv_exists = session
        .execute(Command::KvExists {
            branch: None,
            space: None,
            key: "scoped-kv".into(),
        })
        .expect("kv exists should use session space");
    assert_bool(kv_exists, true);
    let kv_scan = session
        .execute(Command::KvScan {
            branch: None,
            space: None,
            start: Some("scoped-".into()),
            limit: None,
        })
        .expect("kv scan should use session space");
    match kv_scan {
        Output::KvScanResult(entries) => {
            assert!(entries.contains(&("scoped-kv".into(), Value::Int(7))));
        }
        other => panic!("expected kv scan result, got {other:?}"),
    }

    session
        .execute(Command::JsonSet {
            branch: None,
            space: None,
            key: "scoped-json".into(),
            path: "$".into(),
            value: Value::String("json-value".into()),
        })
        .expect("json write should use session space");
    let json_exists = session
        .execute(Command::JsonExists {
            branch: None,
            space: None,
            key: "scoped-json".into(),
        })
        .expect("json exists should use session space");
    assert_bool(json_exists, true);

    let event_output = session
        .execute(Command::EventAppend {
            branch: None,
            space: None,
            event_type: "storage_boundary.audit".into(),
            payload: event_payload("phase", Value::String("storage-boundary".into())),
        })
        .expect("event append should use session space");
    let event_sequence = match event_output {
        Output::EventAppendResult { sequence, .. } => sequence,
        other => panic!("expected event append result, got {other:?}"),
    };
    let event_exists = session
        .execute(Command::EventExists {
            branch: None,
            space: None,
            sequence: event_sequence,
        })
        .expect("event exists should use session space");
    assert_bool(event_exists, true);
    let batch_versions = assert_batch_success(
        session
            .execute(Command::EventBatchAppend {
                branch: None,
                space: None,
                entries: vec![
                    BatchEventEntry {
                        event_type: "storage_boundary.batch".into(),
                        payload: event_payload("item", Value::Int(1)),
                    },
                    BatchEventEntry {
                        event_type: "storage_boundary.batch".into(),
                        payload: event_payload("item", Value::Int(2)),
                    },
                ],
            })
            .expect("event batch append should use session space"),
        2,
    );
    for sequence in batch_versions {
        let exists = session
            .execute(Command::EventExists {
                branch: None,
                space: None,
                sequence,
            })
            .expect("batch event exists should use session space");
        assert_bool(exists, true);
    }

    let pending_events = session
        .execute(Command::EventGetByType {
            branch: None,
            space: None,
            event_type: "storage_boundary.audit".into(),
            limit: None,
            after_sequence: None,
            as_of: None,
        })
        .expect("event type lookup should see transaction-local event");
    assert_event_count(pending_events, 1);

    session
        .execute(Command::TxnCommit)
        .expect("transaction should commit");

    let verifier = strata_executor::Executor::new(db);

    let default_kv = verifier
        .execute(Command::KvGet {
            branch: None,
            space: Some("default".into()),
            key: "scoped-kv".into(),
            as_of: None,
        })
        .expect("default-space kv read should succeed");
    assert_missing(default_kv);

    let default_json = verifier
        .execute(Command::JsonGet {
            branch: None,
            space: Some("default".into()),
            key: "scoped-json".into(),
            path: "$".into(),
            as_of: None,
        })
        .expect("default-space json read should succeed");
    assert_missing(default_json);

    let analytics_kv = verifier
        .execute(Command::KvGet {
            branch: None,
            space: Some("analytics".into()),
            key: "scoped-kv".into(),
            as_of: None,
        })
        .expect("analytics kv read should succeed");
    assert_present_value(analytics_kv, Value::Int(7));

    let analytics_json = verifier
        .execute(Command::JsonGet {
            branch: None,
            space: Some("analytics".into()),
            key: "scoped-json".into(),
            path: "$".into(),
            as_of: None,
        })
        .expect("analytics json read should succeed");
    assert_present_value(analytics_json, Value::String("json-value".into()));

    let default_events = verifier
        .execute(Command::EventGetByType {
            branch: None,
            space: Some("default".into()),
            event_type: "storage_boundary.audit".into(),
            limit: None,
            after_sequence: None,
            as_of: None,
        })
        .expect("default-space event read should succeed");
    assert_event_count(default_events, 0);

    let default_batch_events = verifier
        .execute(Command::EventGetByType {
            branch: None,
            space: Some("default".into()),
            event_type: "storage_boundary.batch".into(),
            limit: None,
            after_sequence: None,
            as_of: None,
        })
        .expect("default-space batch event read should succeed");
    assert_event_count(default_batch_events, 0);

    let analytics_events = verifier
        .execute(Command::EventGetByType {
            branch: None,
            space: Some("analytics".into()),
            event_type: "storage_boundary.audit".into(),
            limit: None,
            after_sequence: None,
            as_of: None,
        })
        .expect("analytics event read should succeed");
    assert_event_count(analytics_events, 1);

    let analytics_batch_events = verifier
        .execute(Command::EventGetByType {
            branch: None,
            space: Some("analytics".into()),
            event_type: "storage_boundary.batch".into(),
            limit: None,
            after_sequence: None,
            as_of: None,
        })
        .expect("analytics batch event read should succeed");
    assert_event_count(analytics_batch_events, 2);
}

#[test]
fn storage_boundary_space_delete_force_clears_target_space_and_preserves_sibling_space() {
    let executor = create_executor();

    for space in ["tenant_x", "tenant_y"] {
        executor
            .execute(Command::KvPut {
                branch: None,
                space: Some(space.into()),
                key: "kv".into(),
                value: Value::String(format!("{space}-kv")),
            })
            .expect("kv write should succeed");
        executor
            .execute(Command::JsonSet {
                branch: None,
                space: Some(space.into()),
                key: "doc".into(),
                path: "$.name".into(),
                value: Value::String(format!("{space}-json")),
            })
            .expect("json write should succeed");
        executor
            .execute(Command::EventAppend {
                branch: None,
                space: Some(space.into()),
                event_type: "storage_boundary.delete".into(),
                payload: event_payload("space", Value::String(space.into())),
            })
            .expect("event append should succeed");
        executor
            .execute(Command::VectorCreateCollection {
                branch: None,
                space: Some(space.into()),
                collection: "embeddings".into(),
                dimension: 3,
                metric: DistanceMetric::Cosine,
            })
            .expect("vector collection should be created");
        executor
            .execute(Command::VectorUpsert {
                branch: None,
                space: Some(space.into()),
                collection: "embeddings".into(),
                key: "v1".into(),
                vector: vec![1.0, 0.0, 0.0],
                metadata: None,
            })
            .expect("vector upsert should succeed");
        executor
            .execute(Command::GraphCreate {
                branch: None,
                space: Some(space.into()),
                graph: "g".into(),
                cascade_policy: None,
            })
            .expect("graph should be created");
        executor
            .execute(Command::GraphAddNode {
                branch: None,
                space: Some(space.into()),
                graph: "g".into(),
                node_id: "n1".into(),
                entity_ref: None,
                properties: None,
                object_type: None,
            })
            .expect("graph node should be added");
    }

    let non_forced = executor
        .execute(Command::SpaceDelete {
            branch: None,
            space: "tenant_x".into(),
            force: false,
        })
        .expect_err("non-empty target space should require force");
    assert!(
        matches!(non_forced, Error::ConstraintViolation { .. }),
        "expected constraint violation, got {non_forced:?}"
    );

    executor
        .execute(Command::SpaceDelete {
            branch: None,
            space: "tenant_x".into(),
            force: true,
        })
        .expect("forced space delete should succeed");

    let tenant_x_kv = executor
        .execute(Command::KvGet {
            branch: None,
            space: Some("tenant_x".into()),
            key: "kv".into(),
            as_of: None,
        })
        .expect("tenant_x kv read should succeed");
    assert_missing(tenant_x_kv);

    let tenant_x_json = executor
        .execute(Command::JsonGet {
            branch: None,
            space: Some("tenant_x".into()),
            key: "doc".into(),
            path: "$.name".into(),
            as_of: None,
        })
        .expect("tenant_x json read should succeed");
    assert_missing(tenant_x_json);

    let tenant_x_events = executor
        .execute(Command::EventGetByType {
            branch: None,
            space: Some("tenant_x".into()),
            event_type: "storage_boundary.delete".into(),
            limit: None,
            after_sequence: None,
            as_of: None,
        })
        .expect("tenant_x event read should succeed");
    assert_event_count(tenant_x_events, 0);

    let tenant_x_collections = executor
        .execute(Command::VectorListCollections {
            branch: None,
            space: Some("tenant_x".into()),
        })
        .expect("tenant_x vector collection list should succeed");
    match tenant_x_collections {
        Output::VectorCollectionList(collections) => assert!(collections.is_empty()),
        other => panic!("expected vector collection list, got {other:?}"),
    }

    let tenant_x_graphs = executor
        .execute(Command::GraphList {
            branch: None,
            space: Some("tenant_x".into()),
        })
        .expect("tenant_x graph list should succeed");
    match tenant_x_graphs {
        Output::Keys(graphs) => assert!(graphs.is_empty()),
        other => panic!("expected graph list, got {other:?}"),
    }

    let tenant_y_kv = executor
        .execute(Command::KvGet {
            branch: None,
            space: Some("tenant_y".into()),
            key: "kv".into(),
            as_of: None,
        })
        .expect("tenant_y kv read should succeed");
    assert_present_value(tenant_y_kv, Value::String("tenant_y-kv".into()));

    let tenant_y_json = executor
        .execute(Command::JsonGet {
            branch: None,
            space: Some("tenant_y".into()),
            key: "doc".into(),
            path: "$.name".into(),
            as_of: None,
        })
        .expect("tenant_y json read should succeed");
    assert_present_value(tenant_y_json, Value::String("tenant_y-json".into()));

    let tenant_y_events = executor
        .execute(Command::EventGetByType {
            branch: None,
            space: Some("tenant_y".into()),
            event_type: "storage_boundary.delete".into(),
            limit: None,
            after_sequence: None,
            as_of: None,
        })
        .expect("tenant_y event read should succeed");
    assert_event_count(tenant_y_events, 1);

    let tenant_y_collections = executor
        .execute(Command::VectorListCollections {
            branch: None,
            space: Some("tenant_y".into()),
        })
        .expect("tenant_y vector collection list should succeed");
    match tenant_y_collections {
        Output::VectorCollectionList(collections) => {
            assert_eq!(collections.len(), 1);
            assert_eq!(collections[0].name, "embeddings");
            assert_eq!(collections[0].count, 1);
        }
        other => panic!("expected vector collection list, got {other:?}"),
    }

    let tenant_y_graphs = executor
        .execute(Command::GraphList {
            branch: None,
            space: Some("tenant_y".into()),
        })
        .expect("tenant_y graph list should succeed");
    match tenant_y_graphs {
        Output::Keys(graphs) => assert_eq!(graphs, vec!["g".to_string()]),
        other => panic!("expected graph list, got {other:?}"),
    }
}
