//! TCP4.4b — the authored JSON number-edge contract at the wire.
//!
//! `conformance/number_contract.tsv` pins Strata's number semantics — what
//! is preserved exactly (the full i64/u64 integer domain, including the
//! 2^53+1 values JavaScript loses), what is refused (integers beyond u64 via
//! the wire guard, exponents that overflow to infinity), what narrows
//! silently within IEEE-754 (precision collapse, sub-denormal rounding,
//! deep-underflow-to-zero with sign), and the exact canonical text the wire
//! emits on read-back (shortest-representation printing, so `1e2` comes
//! back as `100.0`).
//!
//! Unlike 4.4a's blessed `i_` verdicts, this table is AUTHORED from
//! IEEE-754/serde expectations first — there is deliberately no bless mode.
//! A mismatch is a finding: either the expectation is wrong (fix the table
//! with justification in review) or the pipeline is (file the bug).
//!
//! Pipeline per case, identical to 4.4a: the input spliced as the document
//! value into a `json_set` envelope → `guard_json_integers` →
//! `from_str::<Command>` → engine validation → store → `json_get` →
//! `serde_json::to_string` of the stored value compared byte-for-byte
//! against the pinned expectation.

use std::path::PathBuf;

use strata_executor::{guard_json_integers, Command, Executor, Output};

/// Exact contract shape — a drift means rows were added or removed without
/// updating the pin.
const ACCEPT_CASES: usize = 36;
const REJECT_CASES: usize = 7;

fn contract_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/conformance/number_contract.tsv")
}

struct Case {
    input: String,
    expected: Option<String>,
}

fn contract() -> Vec<Case> {
    let text = std::fs::read_to_string(contract_path()).expect("committed number contract");
    let mut cases = Vec::new();
    for (line_no, line) in text.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        match fields.as_slice() {
            [input, "accept", expected] => cases.push(Case {
                input: (*input).to_owned(),
                expected: Some((*expected).to_owned()),
            }),
            [input, "reject"] => cases.push(Case {
                input: (*input).to_owned(),
                expected: None,
            }),
            other => panic!("malformed contract line {}: {other:?}", line_no + 1),
        }
    }
    cases
}

/// Runs one input through the full binding pipeline; returns the exact
/// serialized read-back on acceptance.
fn run_case(executor: &mut Executor, key: &str, input: &str) -> Option<String> {
    let envelope = format!(
        r#"{{"type":"json_set","key":{},"path":"$","value":{input}}}"#,
        serde_json::to_string(key).expect("key encodes"),
    );
    if guard_json_integers(&envelope).is_err() {
        return None;
    }
    let Ok(command) = serde_json::from_str::<Command>(&envelope) else {
        return None;
    };
    if executor.execute(command).is_err() {
        return None;
    }
    let stored = match executor
        .execute(Command::JsonGet {
            branch: None,
            space: None,
            key: key.to_owned(),
            path: "$".to_owned(),
            as_of: None,
        })
        .expect("an accepted document reads back")
    {
        Output::JsonVersionedValue(value) => value
            .value()
            .map(|value| value.value().clone())
            .expect("an accepted document is present"),
        other => panic!("unexpected json_get output: {other:?}"),
    };
    Some(serde_json::to_string(&stored).expect("stored value serializes"))
}

#[test]
fn the_number_edge_contract_holds_at_the_wire() {
    let mut executor = Executor::open_cache().expect("cache executor opens");
    let mut counts = (0usize, 0usize);

    for (index, case) in contract().into_iter().enumerate() {
        let key = format!("num-{index}");
        let observed = run_case(&mut executor, &key, &case.input);
        match (&case.expected, observed) {
            (Some(expected), Some(output)) => {
                counts.0 += 1;
                assert_eq!(
                    &output, expected,
                    "input `{}`: the wire read-back diverged from the pinned contract",
                    case.input
                );
            }
            (Some(_), None) => panic!("input `{}`: a pinned-accept case was refused", case.input),
            (None, None) => counts.1 += 1,
            (None, Some(output)) => panic!(
                "input `{}`: a pinned-reject case was accepted (read back `{output}`)",
                case.input
            ),
        }
    }

    assert_eq!(
        counts,
        (ACCEPT_CASES, REJECT_CASES),
        "the contract shape drifted — update the pins alongside the table"
    );
}
