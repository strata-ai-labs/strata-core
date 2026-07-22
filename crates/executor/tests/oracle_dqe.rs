//! TCP4.10 write-read predicate consistency (the DQE analog).
//!
//! A read and a write sharing one predicate must agree on which rows the
//! predicate selects — the read predicts exactly what the write touches.
//! The verdict is the agreement itself; no expected values are authored.

use serde_json::json;

#[path = "parity/support.rs"]
mod support;

/// `vector_query(filter)` predicts exactly what `vector_delete_by_filter`
/// removes: the deleted count equals the predicted match count, the
/// predicate's rows are gone afterward, and the complement is untouched.
#[test]
fn vector_filter_reads_predict_filter_deletes() {
    let mut executor = support::executor();
    support::run(
        &mut executor,
        &json!({"type": "vector_create_collection", "collection": "c", "dimension": 2,
                "metric": "euclidean"}),
    );
    for index in 0..10_u32 {
        let kind = if index < 4 { "drop" } else { "keep" };
        support::run(
            &mut executor,
            &json!({"type": "vector_upsert", "collection": "c", "key": format!("v{index}"),
                    "vector": [f64::from(index), 1.0], "metadata": {"kind": kind}}),
        );
    }
    let filter = json!({"conditions": [{"field": "kind", "op": "eq",
                                        "value": {"type": "string", "value": "drop"}}]});

    let predicted = support::run(
        &mut executor,
        &json!({"type": "vector_query", "collection": "c", "query": [0.0, 1.0], "k": 100,
                "filter": filter}),
    );
    let predicted_keys = predicted["data"]
        .as_array()
        .unwrap_or_else(|| panic!("query carries matches: {predicted}"))
        .len();
    assert_eq!(predicted_keys, 4, "the predicate selects the planted rows");

    let deleted = support::run(
        &mut executor,
        &json!({"type": "vector_delete_by_filter", "collection": "c", "filter": filter}),
    );
    let deleted_count = deleted["data"]["effect"]["affected_count"]
        .as_u64()
        .unwrap_or_else(|| panic!("delete_by_filter reports affected_count: {deleted}"));
    assert_eq!(
        usize::try_from(deleted_count).expect("small count"),
        predicted_keys,
        "the write touches exactly the rows the read predicted"
    );

    let residue = support::run(
        &mut executor,
        &json!({"type": "vector_query", "collection": "c", "query": [0.0, 1.0], "k": 100,
                "filter": filter}),
    );
    assert_eq!(
        residue["data"].as_array().map(Vec::len),
        Some(0),
        "the predicate's rows are gone after the predicated delete"
    );

    let count = support::run(
        &mut executor,
        &json!({"type": "vector_count", "collection": "c"}),
    );
    assert_eq!(
        count["data"].as_u64(),
        Some(6),
        "the complement is untouched"
    );
}

/// `json_list(prefix)` predicts exactly what `json_batch_delete` of those
/// keys removes: the prefix count reaches zero and the complement survives.
#[test]
fn json_prefix_reads_predict_batch_deletes() {
    let mut executor = support::executor();
    for index in 0..6_u32 {
        support::run(
            &mut executor,
            &json!({"type": "json_set", "key": format!("del{index}"), "path": "$",
                    "value": {"i": index}}),
        );
        support::run(
            &mut executor,
            &json!({"type": "json_set", "key": format!("keep{index}"), "path": "$",
                    "value": {"i": index}}),
        );
    }
    let listed = support::run(
        &mut executor,
        &json!({"type": "json_list", "prefix": "del", "limit": 100}),
    );
    let keys: Vec<String> = listed["data"]["items"]
        .as_array()
        .unwrap_or_else(|| panic!("json_list carries items: {listed}"))
        .iter()
        .map(|item| item.as_str().expect("items are keys").to_owned())
        .collect();
    assert_eq!(keys.len(), 6, "the predicate selects the planted rows");

    let entries: Vec<_> = keys
        .iter()
        .map(|key| json!({"key": key, "path": "$"}))
        .collect();
    support::run(
        &mut executor,
        &json!({"type": "json_batch_delete", "entries": entries}),
    );

    let gone = support::run(
        &mut executor,
        &json!({"type": "json_count", "prefix": "del"}),
    );
    assert_eq!(
        gone["data"].as_u64(),
        Some(0),
        "the predicate's rows are gone"
    );
    let kept = support::run(
        &mut executor,
        &json!({"type": "json_count", "prefix": "keep"}),
    );
    assert_eq!(
        kept["data"].as_u64(),
        Some(6),
        "the complement is untouched"
    );
}
