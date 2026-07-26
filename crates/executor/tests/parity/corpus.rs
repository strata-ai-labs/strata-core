#![allow(dead_code)] // shared via #[path]; each target uses a subset

//! TCP4.2b corpus format support: canonicalization, record, and replay.
//!
//! See `tests/corpus/README.md` for the format contract. This module is
//! shared (via `#[path]`) between the ungated replay lane and the
//! differential recorders; it must stay dependency-free beyond `serde_json`.

use serde_json::{json, Value};

/// Object fields scrubbed before comparison — wall-clock artifacts. Commit
/// versions are deliberately NOT here: the logical commit clock is
/// deterministic for a fixed op sequence on a fresh database.
const VOLATILE_FIELDS: [&str; 7] = [
    "timestamp",
    "timestamps",
    "recorded_at",
    "elapsed_ms",
    "duration_ms",
    // Event-chain hashes derive from the wall-clock event timestamp
    // (TCP4.2d), so they change per recording run; chain INTEGRITY is
    // pinned by event_verify_chain, not by frozen hash bytes.
    "hash",
    "previous_hash",
];

pub(crate) const CORPUS_FORMAT_VERSION: u64 = 1;

/// Recursively replaces volatile field values with `0` so recorded and
/// replayed envelopes compare byte-for-byte.
pub(crate) fn canonicalize(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if VOLATILE_FIELDS.contains(&key.as_str()) {
                    *child = json!(0);
                } else {
                    canonicalize(child);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                canonicalize(item);
            }
        }
        _ => {}
    }
}

/// One replayable corpus case: a wire op and its expected outcome.
pub(crate) struct CorpusCase {
    pub(crate) op: Value,
    pub(crate) expect: Result<Value, String>,
}

impl CorpusCase {
    fn to_line(&self) -> String {
        let record = match &self.expect {
            Ok(output) => json!({"op": self.op, "expect": output}),
            Err(code) => json!({"op": self.op, "expect_err": code}),
        };
        serde_json::to_string(&record).expect("corpus case serializes")
    }

    fn from_line(line: &str) -> Self {
        let record: Value =
            serde_json::from_str(line).unwrap_or_else(|err| panic!("corpus line parses: {err}"));
        let op = record["op"].clone();
        assert!(op.is_object(), "corpus case carries an op: {record}");
        let expect = if let Some(code) = record["expect_err"].as_str() {
            Err(code.to_owned())
        } else {
            Ok(record["expect"].clone())
        };
        Self { op, expect }
    }
}

/// Serializes a corpus file: header line + one line per case.
pub(crate) fn corpus_file_contents(
    name: &str,
    capability: &str,
    seed: u64,
    generator: &str,
    validated_against: &str,
    cases: &[CorpusCase],
) -> String {
    let header = json!({
        "format": CORPUS_FORMAT_VERSION,
        "corpus": name,
        "capability": capability,
        "seed": seed,
        "ops": cases.len(),
        "generator": generator,
        "validated_against": validated_against,
        "modes": ["cache"],
    });
    let mut contents = serde_json::to_string(&header).expect("header serializes");
    contents.push('\n');
    for case in cases {
        contents.push_str(&case.to_line());
        contents.push('\n');
    }
    contents
}

/// Executes one wire op against the executor, returning the canonicalized
/// outcome (output envelope or stable error code).
pub(crate) fn execute_canonicalized(
    executor: &mut strata_executor::Executor,
    op: &Value,
) -> Result<Value, String> {
    let command: strata_executor::Command = serde_json::from_value(op.clone())
        .unwrap_or_else(|err| panic!("corpus op must parse as a wire command ({op}): {err}"));
    match executor.execute(command) {
        Ok(output) => {
            let mut value = serde_json::to_value(&output).expect("output serializes");
            canonicalize(&mut value);
            Ok(value)
        }
        Err(error) => {
            let status = serde_json::to_value(error.status()).expect("status serializes");
            Err(status["code"]
                .as_str()
                .unwrap_or_else(|| panic!("error status carries a code: {status}"))
                .to_owned())
        }
    }
}

/// Replays a corpus file's cases in order against a fresh cache executor,
/// panicking with corpus/line context on the first mismatch.
pub(crate) fn replay_corpus(name: &str, contents: &str) {
    let mut lines = contents.lines();
    let header: Value = serde_json::from_str(lines.next().unwrap_or_else(|| {
        panic!("corpus {name} has a header line");
    }))
    .unwrap_or_else(|err| panic!("corpus {name} header parses: {err}"));
    assert_eq!(
        header["format"].as_u64(),
        Some(CORPUS_FORMAT_VERSION),
        "corpus {name} format version is supported"
    );

    let mut executor = super::support::executor();
    for (index, line) in lines.enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let case = CorpusCase::from_line(line);
        let observed = execute_canonicalized(&mut executor, &case.op);
        let line_number = index + 2;
        match (&case.expect, &observed) {
            (Ok(expected), Ok(actual)) => assert_eq!(
                expected, actual,
                "{name}:{line_number}: replayed output diverged from the recorded \
                 envelope for op {}",
                case.op
            ),
            (Err(expected), Err(actual)) => assert_eq!(
                expected, actual,
                "{name}:{line_number}: replayed error code diverged for op {}",
                case.op
            ),
            (Ok(_), Err(actual)) => panic!(
                "{name}:{line_number}: recorded success now fails with {actual} for op {}",
                case.op
            ),
            (Err(expected), Ok(actual)) => panic!(
                "{name}:{line_number}: recorded error {expected} now succeeds with {actual} \
                 for op {}",
                case.op
            ),
        }
    }
}
