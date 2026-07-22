//! TCP4.7 axis: batch-family failure channels (#2701).
//!
//! Every capability offers a batch write, and one contract should govern how
//! a batch reports an invalid item, handles duplicates, and bounds its size.
//! The #2686-era remediation (#2724-#2726) converged kv/json/event on the
//! itemwise channel and standardized duplicate rejection at the executor —
//! asserted here as contracts. Two divergences remain ledgered under #2701:
//! graph aborts wholesale where its siblings report itemwise, and the engine
//! enforces no size cap at all.

use serde_json::json;

#[path = "parity/support.rs"]
mod support;

/// Minimal base64 for the distinct 2-byte keys the no-cap pin needs.
fn base64_2(bytes: [u8; 2]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let chunk = (u32::from(bytes[0]) << 16) | (u32::from(bytes[1]) << 8);
    let mut out = String::new();
    out.push(TABLE[(chunk >> 18) as usize & 0x3f] as char);
    out.push(TABLE[(chunk >> 12) as usize & 0x3f] as char);
    out.push(TABLE[(chunk >> 6) as usize & 0x3f] as char);
    out.push('=');
    out
}

/// Contract that holds today across all four families: an empty batch
/// succeeds (graph requires its graph to exist first — that resolution
/// divergence is pinned in `parity_branch_space.rs`'s #2700 family).
#[test]
fn empty_batches_succeed_across_families() {
    let mut executor = support::executor();
    support::run(
        &mut executor,
        &json!({"type": "kv_batch_put", "entries": []}),
    );
    support::run(
        &mut executor,
        &json!({"type": "json_batch_set", "entries": []}),
    );
    support::run(
        &mut executor,
        &json!({"type": "event_batch_append", "entries": []}),
    );
    support::run(
        &mut executor,
        &json!({"type": "graph_create", "graph": "empty-batch"}),
    );
    support::run(
        &mut executor,
        &json!({"type": "graph_batch_write", "graph": "empty-batch", "operations": []}),
    );
}

/// PIN #2701: kv, json, and event report one invalid item ITEMWISE (batch
/// succeeds, valid items commit, the bad item carries a typed error) — but
/// graph aborts the whole batch. The itemwise legs are the converged
/// contract; the graph leg is the ledgered divergence.
#[test]
fn pin_2701_graph_aborts_where_its_siblings_report_itemwise() {
    support::pinned("batch_invalid_item_channel", 2701);
    let mut executor = support::executor();

    // kv: itemwise — the batch succeeds and the valid entry is readable.
    support::run(
        &mut executor,
        &json!({"type": "kv_batch_put", "entries": [
            {"key": "YQ==", "value": "b25l"},
            {"key": "", "value": "b25l"}
        ]}),
    );
    let kv_get = support::run(&mut executor, &json!({"type": "kv_get", "key": "YQ=="}));
    assert!(
        !kv_get["data"].is_null(),
        "kv: the valid item of a mixed batch commits (itemwise channel): {kv_get}"
    );

    // event: itemwise — the batch succeeds and the valid event is appended.
    support::run(
        &mut executor,
        &json!({"type": "event_batch_append", "entries": [
            {"event_type": "ok.tick", "payload": {}},
            {"event_type": "", "payload": {}}
        ]}),
    );
    let count = support::run(&mut executor, &json!({"type": "event_count"}));
    assert_eq!(
        count["data"]["count"].as_u64(),
        Some(1),
        "event: the valid item of a mixed batch appends (itemwise channel): {count}"
    );

    // json: itemwise since the #2686 batch remediation — the batch reports
    // status=partial with a typed per-item error, and the valid item commits.
    let json_batch = support::run(
        &mut executor,
        &json!({"type": "json_batch_set", "entries": [
            {"key": "good", "path": "$", "value": {"a": 1}},
            {"key": "", "path": "$", "value": {"a": 2}}
        ]}),
    );
    assert_eq!(json_batch["data"]["status"].as_str(), Some("partial"));
    assert_eq!(
        json_batch["data"]["items"][1]["error"]["code"].as_str(),
        Some("invalid_argument.engine.json_document_id")
    );
    let json_get = support::run(
        &mut executor,
        &json!({"type": "json_get", "key": "good", "path": "$"}),
    );
    assert!(
        !json_get["data"].is_null(),
        "json: the valid item of a mixed batch commits (itemwise channel): {json_get}"
    );

    // graph: whole-abort — the batch fails and the valid node does not exist.
    support::run(
        &mut executor,
        &json!({"type": "graph_create", "graph": "mixed"}),
    );
    let graph_code = support::run_err_code(
        &mut executor,
        &json!({"type": "graph_batch_write", "graph": "mixed", "operations": [
            {"type": "upsert_node", "node_id": "good", "data": {}},
            {"type": "upsert_node", "node_id": "", "data": {}}
        ]}),
    );
    assert_eq!(
        graph_code, "invalid_argument.engine.graph_node_id",
        "today: graph aborts the whole batch with the item's error at the top          level; if this fails, #2701's graph leg converged — delete the ledger          entry and extend the itemwise contract to graph"
    );
    let node = support::run(
        &mut executor,
        &json!({"type": "graph_get_node", "graph": "mixed", "node_id": "good"}),
    );
    assert_eq!(
        node["data"]["found"].as_bool(),
        Some(false),
        "graph: the valid item of an aborted batch must not commit: {node}"
    );
}

/// Contract (converged by the #2686 remediation): duplicate keys in one
/// batch are rejected at the executor layer on both the kv and json axes,
/// with parallel family-specific codes.
#[test]
fn duplicate_batch_keys_are_rejected_consistently() {
    let mut executor = support::executor();

    let kv_code = support::run_err_code(
        &mut executor,
        &json!({"type": "kv_batch_put", "entries": [
            {"key": "YQ==", "value": "b25l"},
            {"key": "YQ==", "value": "dHdv"}
        ]}),
    );
    assert_eq!(kv_code, "invalid_argument.executor.kv_batch_duplicate_key");

    let json_code = support::run_err_code(
        &mut executor,
        &json!({"type": "json_batch_set", "entries": [
            {"key": "dup", "path": "$", "value": {"v": 1}},
            {"key": "dup", "path": "$", "value": {"v": 2}}
        ]}),
    );
    assert_eq!(
        json_code,
        "invalid_argument.executor.json_batch_duplicate_key"
    );
}

/// PIN #2701: the engine enforces no item-count cap — a 3000-item batch is
/// accepted. (The caps the issue documents are SDK-side.) If this fails
/// with a typed limit refusal, #2701 grew an engine-side cap — delete the
/// ledger entry and assert the documented limit instead.
#[test]
fn pin_2701_no_engine_side_item_cap() {
    support::pinned("batch_limits", 2701);
    let mut executor = support::executor();
    let entries: Vec<_> = (0..3000_u16)
        .map(|index| json!({"key": base64_2(index.to_be_bytes()), "value": "b25l"}))
        .collect();
    support::run(
        &mut executor,
        &json!({"type": "kv_batch_put", "entries": entries}),
    );
    let count = support::run(&mut executor, &json!({"type": "kv_count"}));
    assert_eq!(
        count["data"].as_u64(),
        Some(3000),
        "all 3000 items committed"
    );
}

/// Ledger guard (entry ⇒ pin): every `batch*` ledger entry is pinned here.
#[test]
fn every_batch_ledger_entry_is_pinned_here() {
    support::assert_ledger_entries_all_pinned(
        "batch",
        &[("batch_invalid_item_channel", 2701), ("batch_limits", 2701)],
    );
}
