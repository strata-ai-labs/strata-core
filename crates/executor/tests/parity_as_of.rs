//! TCP4.7 axis: `as_of` × read-command parity (#2702).
//!
//! Time-travel should be one capability with one coverage rule, but `as_of`
//! is accepted by most reads and rejected as an unknown field by a scattered
//! remainder. The coverage gap is ledgered; the semantic contract that holds
//! (a read as-of the write's own commit timestamp equals the live read) is
//! asserted permanently.

use serde_json::json;
use strata_executor::Command;

#[path = "parity/support.rs"]
mod support;

/// Contract that holds today: reading as-of the write's own commit
/// timestamp equals the live read (per the temporal contract, `as_of` at the
/// latest commit is in-window; only beyond-latest raises).
#[test]
fn as_of_at_the_write_timestamp_agrees_with_the_live_read() {
    let mut executor = support::executor();
    let put = support::run(
        &mut executor,
        &json!({"type": "kv_put", "key": "YQ==", "value": "b25l"}),
    );
    let timestamp = put["data"]["commit"]["timestamp"]
        .as_u64()
        .expect("write receipt carries a commit timestamp");

    let live = support::run(&mut executor, &json!({"type": "kv_get", "key": "YQ=="}));
    let as_of = support::run(
        &mut executor,
        &json!({"type": "kv_get", "key": "YQ==", "as_of": timestamp}),
    );
    assert_eq!(
        live["data"], as_of["data"],
        "a read as-of the write's commit timestamp equals the live read"
    );
}

/// PIN #2702: `as_of` coverage is patchy — accepted by `kv_get` and
/// `event_list`, rejected as an unknown field by `kv_exists`, `event_range`,
/// and `event_verify_chain`. (`event_range`'s omission is a recorded
/// deliberate skip pending a contract decision — see the engine comment on
/// its service `range` path — so this pin documents the divergence without
/// implying a fix is owed there.)
#[test]
fn pin_2702_as_of_coverage_is_patchy_across_reads() {
    support::pinned("as_of_coverage", 2702);

    let accepts = |wire: serde_json::Value| {
        serde_json::from_value::<Command>(wire.clone())
            .unwrap_or_else(|err| panic!("must accept as_of ({wire}): {err}"));
    };
    let rejects = |wire: serde_json::Value| {
        assert!(
            serde_json::from_value::<Command>(wire.clone()).is_err(),
            "today: must reject as_of as an unknown field ({wire}); if this \
             fails, the command gained as_of — update the ledger entry"
        );
    };

    accepts(json!({"type": "kv_get", "key": "YQ==", "as_of": 5}));
    accepts(json!({"type": "event_list", "as_of": 5}));

    rejects(json!({"type": "kv_exists", "key": "YQ==", "as_of": 5}));
    rejects(json!({"type": "event_range", "start_seq": 0, "as_of": 5}));
    rejects(json!({"type": "event_verify_chain", "as_of": 5}));
}

/// Ledger guard (entry ⇒ pin): every `as_of*` ledger entry is pinned here.
#[test]
fn every_as_of_ledger_entry_is_pinned_here() {
    support::assert_ledger_entries_all_pinned("as_of", &[("as_of_coverage", 2702)]);
}
