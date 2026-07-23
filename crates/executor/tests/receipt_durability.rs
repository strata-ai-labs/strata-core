//! Fix #2756 — the wire receipt attests per-commit durability truthfully.
//!
//! The former `commit.durable` boolean reported Standard-mode (buffered
//! WAL) acknowledgements as durable; SDK callers treated them as
//! crash-safe and SIGKILL erased them silently. The wire now carries the
//! four-state attestation: `not_durable` (cache), `standard` (durable
//! after the next sync point — the mode's documented loss window),
//! `always` (synced before acknowledgement), `uncertain`. Engine-level
//! survival semantics are proven in
//! `crates/engine/tests/commit_durability.rs`; this target pins the wire
//! strings SDK callers actually switch on.

use serde_json::{json, Value};
use strata_executor::{Command, DurabilityMode, DurableLocalOpenOptions, Executor};

fn put_receipt_durability(executor: &mut Executor) -> String {
    let command: Command = serde_json::from_value(json!({
        "type": "kv_put",
        "key": "aw==",
        "value": "dg==",
    }))
    .expect("wire JSON parses");
    let output = executor.execute(command).expect("put succeeds");
    let envelope = serde_json::to_value(&output).expect("serialize output");
    envelope["data"]["commit"]["durability"]
        .as_str()
        .unwrap_or_else(|| panic!("receipt carries a durability string: {envelope}"))
        .to_owned()
}

#[test]
fn cache_receipts_attest_not_durable() {
    let mut executor = Executor::open_cache().expect("cache executor opens");
    assert_eq!(put_receipt_durability(&mut executor), "not_durable");
}

#[test]
fn standard_receipts_attest_standard() {
    let dir = tempfile::tempdir().expect("tmp");
    let mut executor =
        Executor::open_durable_local(dir.path().join("db")).expect("durable executor opens");
    // The #2756 defect: this said `durable: true` while the commit sat in a
    // user-space WAL buffer.
    assert_eq!(put_receipt_durability(&mut executor), "standard");
}

#[test]
fn always_receipts_attest_always() {
    let dir = tempfile::tempdir().expect("tmp");
    let mut executor = Executor::open_durable_local_with_options(
        dir.path().join("db"),
        DurableLocalOpenOptions::new().with_durability(DurabilityMode::Always),
    )
    .expect("always-mode executor opens");
    assert_eq!(put_receipt_durability(&mut executor), "always");
}

/// The old field must not quietly coexist with the new one: a receipt
/// carrying `durable` would mean the fold came back.
#[test]
fn receipts_do_not_carry_the_retired_durable_boolean() {
    let mut executor = Executor::open_cache().expect("cache executor opens");
    let command: Command = serde_json::from_value(json!({
        "type": "kv_put",
        "key": "aw==",
        "value": "dg==",
    }))
    .expect("wire JSON parses");
    let output = executor.execute(command).expect("put succeeds");
    let envelope = serde_json::to_value(&output).expect("serialize output");
    assert_eq!(
        envelope["data"]["commit"].get("durable"),
        None::<&Value>,
        "the retired boolean reappeared on the wire: {envelope}"
    );
}
