//! TCP4.10 de-optimization oracle (the `NoREC` analog, seed #2703).
//!
//! Run the same query with and without its accelerating structure and
//! require identical results — the acceleration may change *how*, never
//! *what*. Divergence is a logic bug in whichever path is wrong, detected
//! without a reference engine. Today this also brackets #2703: an inert
//! index trivially satisfies result-equality, and when the index becomes
//! real this same oracle catches index-path corruption the moment it
//! diverges. (Detecting inertness itself needs an acceleration observable —
//! an explain/stats surface — recorded in the charter as deferred.)

use serde_json::{json, Value};

#[path = "parity/support.rs"]
mod support;

fn match_keys(output: &Value) -> Vec<(String, String)> {
    output["data"]
        .as_array()
        .or_else(|| output["data"]["matches"].as_array())
        .unwrap_or_else(|| panic!("query output carries matches: {output}"))
        .iter()
        .map(|m| {
            (
                m["key"].as_str().expect("match carries a key").to_owned(),
                m["score"].to_string(),
            )
        })
        .collect()
}

/// The default vector query path and the index query path must return the
/// same matches (keys, order, scores) — unfiltered and filtered.
#[test]
fn vector_query_and_index_query_agree_exactly() {
    let mut executor = support::executor();
    support::run(
        &mut executor,
        &json!({"type": "vector_create_collection", "collection": "c", "dimension": 4,
                "metric": "cosine"}),
    );
    for index in 0..12_u32 {
        let angle = f64::from(index) * 0.5;
        let kind = if index % 2 == 0 { "even" } else { "odd" };
        support::run(
            &mut executor,
            &json!({"type": "vector_upsert", "collection": "c", "key": format!("v{index}"),
                    "vector": [angle.cos(), angle.sin(), 0.25, 0.125],
                    "metadata": {"kind": kind}}),
        );
    }

    let probe = json!([1.0, 0.1, 0.2, 0.1]);
    for filter in [
        json!(null),
        json!({"conditions": [{"field": "kind", "op": "eq",
                               "value": {"type": "string", "value": "even"}}]}),
    ] {
        let mut default_wire = json!({"type": "vector_query", "collection": "c",
                                      "query": probe, "k": 12});
        let mut index_wire = json!({"type": "vector_index_query", "collection": "c",
                                    "query": probe, "k": 12});
        if !filter.is_null() {
            default_wire["filter"] = filter.clone();
            index_wire["filter"] = filter.clone();
        }
        let default_path = support::run(&mut executor, &default_wire);
        let index_path = support::run(&mut executor, &index_wire);
        assert_eq!(
            match_keys(&default_path),
            match_keys(&index_path),
            "the index path must return exactly what the default path returns \
             (filter: {filter})"
        );
    }
}

/// Creating and dropping a JSON secondary index must never change any query
/// result — before, with, and after the index, the same reads return the
/// same values (#2703's bracket: inert today satisfies this trivially; a
/// future real index diverging here is the bug this oracle exists to catch).
#[test]
fn json_index_presence_never_changes_query_results() {
    let mut executor = support::executor();
    for index in 0..9_u32 {
        support::run(
            &mut executor,
            &json!({"type": "json_set", "key": format!("d{index}"), "path": "$",
                    "value": {"name": format!("n{}", index % 3), "rank": index}}),
        );
    }
    let snapshot = |executor: &mut _| -> Vec<Value> {
        let mut results = Vec::new();
        results.push(support::run(
            executor,
            &json!({"type": "json_list", "prefix": "d", "limit": 100}),
        ));
        results.push(support::run(
            executor,
            &json!({"type": "json_count", "prefix": "d"}),
        ));
        for index in 0..9_u32 {
            results.push(support::run(
                executor,
                &json!({"type": "json_get", "key": format!("d{index}"), "path": "$.name"}),
            ));
        }
        results
    };

    let before = snapshot(&mut executor);
    support::run(
        &mut executor,
        &json!({"type": "json_create_index", "name": "by_name", "field_path": "$.name",
                "index_type": "tag"}),
    );
    let with_index = snapshot(&mut executor);
    assert_eq!(
        before, with_index,
        "creating an index must not change any query result"
    );
    support::run(
        &mut executor,
        &json!({"type": "json_drop_index", "name": "by_name"}),
    );
    let after = snapshot(&mut executor);
    assert_eq!(
        with_index, after,
        "dropping an index must not change any query result"
    );
}
