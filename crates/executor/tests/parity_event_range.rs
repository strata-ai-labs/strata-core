//! TCP4.7 axis: event range direction × endpoint parity.
//!
//! `event_range` (sequence-addressed) and `event_range_by_time`
//! (timestamp-addressed) are parallel views of one log and should agree on
//! window semantics; `direction: reverse` should walk the same window
//! backwards. Two ledgered divergences today: #2694 (reverse anchors
//! `start_seq` as the inclusive upper bound) and #2695 (`end_seq` exclusive
//! vs `end_ts` inclusive).

use serde_json::{json, Value};
use strata_executor::Executor;

#[path = "parity/support.rs"]
mod support;

/// Appends `count` events (payload carries the index) and returns each
/// event's recorded occurrence timestamp, indexed by sequence. Occurrence
/// timestamps are read back from the log (they are wall-clock, not the
/// logical commit clock the append receipt carries).
fn append_events(executor: &mut Executor, count: u64) -> Vec<u64> {
    for index in 0..count {
        let output = support::run(
            executor,
            &json!({"type": "event_append", "event_type": "parity.tick", "payload": {"i": index}}),
        );
        assert_eq!(
            output["data"]["sequence"].as_u64(),
            Some(index),
            "appends assign contiguous sequences"
        );
    }
    let forward = support::run(
        executor,
        &json!({"type": "event_range", "start_seq": 0, "direction": "forward"}),
    );
    forward["data"]["items"]
        .as_array()
        .unwrap_or_else(|| panic!("range output carries items: {forward}"))
        .iter()
        .map(|item| {
            item["event"]["timestamp"]
                .as_u64()
                .unwrap_or_else(|| panic!("range item carries event.timestamp: {item}"))
        })
        .collect()
}

fn range_sequences(output: &Value) -> Vec<u64> {
    output["data"]["items"]
        .as_array()
        .unwrap_or_else(|| panic!("range output carries items: {output}"))
        .iter()
        .map(|item| {
            item["event"]["sequence"]
                .as_u64()
                .unwrap_or_else(|| panic!("range item carries event.sequence: {item}"))
        })
        .collect()
}

/// Contract that holds today and under any #2694 fix: the full forward
/// window is the whole log in order.
#[test]
fn forward_full_window_is_the_whole_log_in_order() {
    let mut executor = support::executor();
    append_events(&mut executor, 5);
    let forward = support::run(
        &mut executor,
        &json!({"type": "event_range", "start_seq": 0, "direction": "forward"}),
    );
    assert_eq!(range_sequences(&forward), vec![0, 1, 2, 3, 4]);
}

/// #2694 fixed: reverse yields the forward window in descending order —
/// `reverse(window) == reversed(forward(window))` — so a reverse read anchored
/// at the log start returns the tail (the newest N), not a single event.
#[test]
fn reverse_range_is_the_forward_window_reversed() {
    let mut executor = support::executor();
    append_events(&mut executor, 5);

    // The former #2694 bug: reverse from start_seq=0 returned exactly [0]; it
    // now returns the whole log newest-first (the tail).
    let reverse_from_zero = support::run(
        &mut executor,
        &json!({"type": "event_range", "start_seq": 0, "direction": "reverse", "limit": 10}),
    );
    assert_eq!(range_sequences(&reverse_from_zero), vec![4, 3, 2, 1, 0]);

    // The parity contract now holds across the same bounded window.
    let forward = support::run(
        &mut executor,
        &json!({"type": "event_range", "start_seq": 1, "end_seq": 4, "direction": "forward"}),
    );
    let reverse = support::run(
        &mut executor,
        &json!({"type": "event_range", "start_seq": 1, "end_seq": 4, "direction": "reverse"}),
    );
    let mut forward_reversed = range_sequences(&forward);
    forward_reversed.reverse();
    assert_eq!(range_sequences(&reverse), forward_reversed);
    assert_eq!(range_sequences(&reverse), vec![3, 2, 1]);
}

/// PIN #2695: the sequence-addressed window excludes its end while the
/// timestamp-addressed window includes it — the same log, two endpoint
/// conventions.
#[test]
fn pin_2695_range_end_is_exclusive_but_range_by_time_end_is_inclusive() {
    support::pinned("event_range_endpoint", 2695);
    let mut executor = support::executor();
    let timestamps = append_events(&mut executor, 5);

    let by_sequence = support::run(
        &mut executor,
        &json!({"type": "event_range", "start_seq": 1, "end_seq": 3, "direction": "forward"}),
    );
    assert_eq!(
        range_sequences(&by_sequence),
        vec![1, 2],
        "end_seq is exclusive"
    );

    let by_time = support::run(
        &mut executor,
        &json!({
            "type": "event_range_by_time",
            "start_ts": timestamps[1],
            "end_ts": timestamps[3],
            "direction": "forward"
        }),
    );
    assert_eq!(
        range_sequences(&by_time),
        vec![1, 2, 3],
        "today: end_ts is inclusive, diverging from end_seq; if this fails, \
         #2695 was fixed — delete the ledger entry and assert one convention"
    );
}

/// Ledger guard (entry ⇒ pin): every `event_range*` ledger entry is pinned
/// by a test in this target.
#[test]
fn every_event_range_ledger_entry_is_pinned_here() {
    support::assert_ledger_entries_all_pinned(
        "event_range",
        &[
            // event_range_direction (#2694) fixed — reverse == reversed(forward).
            ("event_range_endpoint", 2695),
        ],
    );
}
