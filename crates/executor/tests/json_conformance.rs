//! TCP4.4a — the vendored JSON parsing minefield (`JSONTestSuite`) through the
//! real wire ingress.
//!
//! Every case in nst/JSONTestSuite's `test_parsing/` corpus is spliced as the
//! document value into a `json_set` wire envelope and pushed through the
//! exact pipeline every binding uses (`wire_json` module docs): UTF-8
//! transport validation → `guard_json_integers` → `from_str::<Command>` →
//! engine-side `JsonValue` validation → store. The contract:
//!
//! - `y_` (must-accept) cases are ACCEPTED and read back via `json_get`
//!   semantically identical to the value the envelope parsed — the
//!   store/read path may not lose or reshape what the wire admitted.
//! - `n_` (must-reject) cases are REFUSED with a typed error at some
//!   pipeline stage — never a panic, hang, or stack overflow (the engine's
//!   own depth/size limits, finding U36, face the adversarial corpus here).
//! - `i_` (implementation-defined) cases take whatever verdict Strata's
//!   pipeline produces — PINNED in `conformance/i_verdicts.txt` as the
//!   documented parsing contract. Any drift (a `serde_json` upgrade, a limit
//!   change) fails this test; bless deliberately with
//!   `STRATA_JSON_CONFORMANCE_BLESS=1` and review the diff as a contract
//!   change.
//!
//! The suite's value is that it encodes a decade of other parsers' bug
//! reports (lone surrogates, BOMs, 100k-deep nesting, 1e400, raw invalid
//! UTF-8) — cases no in-house generator imagines.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::Value;
use strata_executor::{guard_json_integers, Command, Executor, Output};

/// Exact vendored-corpus shape; a drift here means the vendored data was
/// touched without re-blessing.
const Y_CASES: usize = 95;
const N_CASES: usize = 188;
const I_CASES: usize = 35;

fn suite_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/conformance/jsontestsuite")
}

fn verdicts_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/conformance/i_verdicts.txt")
}

/// One case's terminal fate in the wire pipeline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Verdict {
    Accepted,
    Rejected,
}

impl Verdict {
    fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accept",
            Self::Rejected => "reject",
        }
    }
}

/// Splices the candidate bytes into a `json_set` envelope and runs the full
/// binding pipeline. Returns the verdict and, on acceptance, the value the
/// envelope parse admitted (the round-trip reference).
fn run_case(executor: &mut Executor, key: &str, candidate: &[u8]) -> (Verdict, Option<Value>) {
    let mut envelope = format!(
        r#"{{"type":"json_set","key":{},"path":"$","value":"#,
        serde_json::to_string(key).expect("key encodes"),
    )
    .into_bytes();
    envelope.extend_from_slice(candidate);
    envelope.push(b'}');

    // Stage 1: transport — every real binding hands the wire a UTF-8 string.
    let Ok(envelope) = String::from_utf8(envelope) else {
        return (Verdict::Rejected, None);
    };
    // Stage 2: the wire integer guard (see `wire_json`).
    if guard_json_integers(&envelope).is_err() {
        return (Verdict::Rejected, None);
    }
    // Stage 3: the envelope parse every binding performs.
    let Ok(command) = serde_json::from_str::<Command>(&envelope) else {
        return (Verdict::Rejected, None);
    };
    let admitted = match &command {
        Command::JsonSet { value, .. } => value.clone(),
        other => panic!("the envelope must parse as json_set, got {other:?}"),
    };
    // Stage 4: engine-side validation + store.
    match executor.execute(command) {
        Ok(_) => (Verdict::Accepted, Some(admitted)),
        Err(_) => (Verdict::Rejected, None),
    }
}

/// Reads the stored document back — the accepted half of the contract.
fn read_back(executor: &mut Executor, key: &str) -> Value {
    match executor
        .execute(Command::JsonGet {
            branch: None,
            space: None,
            key: key.to_owned(),
            path: "$".to_owned(),
            as_of: None,
            as_of_time: None,
        })
        .expect("an accepted document reads back")
    {
        Output::JsonVersionedValue(value) => value
            .value()
            .map(|value| value.value().clone())
            .expect("an accepted document is present"),
        other => panic!("unexpected json_get output: {other:?}"),
    }
}

fn corpus() -> Vec<(String, Vec<u8>)> {
    let mut cases = Vec::new();
    for entry in std::fs::read_dir(suite_dir()).expect("vendored suite present") {
        let path = entry.expect("suite entry").path();
        let name = path
            .file_name()
            .expect("file name")
            .to_string_lossy()
            .into_owned();
        if !std::path::Path::new(&name)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
        {
            continue; // LICENSE / README
        }
        cases.push((name, std::fs::read(&path).expect("case bytes")));
    }
    cases.sort();
    cases
}

#[test]
fn the_json_parsing_minefield_holds_at_the_wire() {
    let mut executor = Executor::open_cache().expect("cache executor opens");
    let mut counts = (0usize, 0usize, 0usize);
    let mut i_verdicts: BTreeMap<String, Verdict> = BTreeMap::new();

    for (index, (name, bytes)) in corpus().into_iter().enumerate() {
        let key = format!("doc-{index}");
        let (verdict, admitted) = run_case(&mut executor, &key, &bytes);

        if name.starts_with("y_") {
            counts.0 += 1;
            assert_eq!(
                verdict,
                Verdict::Accepted,
                "{name}: a must-accept case was refused"
            );
            let stored = read_back(&mut executor, &key);
            assert_eq!(
                stored,
                admitted.expect("accepted cases carry the admitted value"),
                "{name}: the stored document diverged from what the wire admitted"
            );
        } else if name.starts_with("n_") {
            counts.1 += 1;
            assert_eq!(
                verdict,
                Verdict::Rejected,
                "{name}: a must-reject case was accepted"
            );
        } else if name.starts_with("i_") {
            counts.2 += 1;
            // Implementation-defined: the accepted ones must still round-trip.
            if verdict == Verdict::Accepted {
                let stored = read_back(&mut executor, &key);
                assert_eq!(
                    stored,
                    admitted.expect("accepted cases carry the admitted value"),
                    "{name}: the stored document diverged from what the wire admitted"
                );
            }
            i_verdicts.insert(name, verdict);
        } else {
            panic!("unclassified suite file: {name}");
        }
    }

    assert_eq!(
        counts,
        (Y_CASES, N_CASES, I_CASES),
        "the vendored corpus shape drifted — refresh wholesale and re-bless"
    );

    let mut rendered = String::new();
    for (name, verdict) in &i_verdicts {
        use std::fmt::Write as _;
        writeln!(rendered, "{name}\t{}", verdict.as_str()).expect("string write");
    }
    if std::env::var("STRATA_JSON_CONFORMANCE_BLESS").is_ok() {
        std::fs::write(verdicts_path(), &rendered).expect("bless verdicts");
        eprintln!("blessed {} i_ verdicts", i_verdicts.len());
        return;
    }
    let committed = std::fs::read_to_string(verdicts_path())
        .expect("committed i_ verdicts present (bless once with STRATA_JSON_CONFORMANCE_BLESS=1)");
    assert_eq!(
        rendered, committed,
        "implementation-defined verdicts drifted from the pinned parsing \
         contract — if intentional (serde_json upgrade, limit change), review \
         and re-bless with STRATA_JSON_CONFORMANCE_BLESS=1"
    );
}
