//! TCP4.2b corpus replay lane — the per-PR drift net over the committed,
//! oracle-validated corpora (`tests/corpus/*.jsonl`).
//!
//! No feature gate and no external engine: every corpus line was validated
//! against its reference oracle at record time (see `corpus/README.md`);
//! replay pins Strata against its own recorded behavior byte-for-byte
//! (post-canonicalization). Runs in the plain workspace lane on every PR.

#[path = "parity/support.rs"]
mod support;

#[path = "parity/corpus.rs"]
mod corpus;

fn corpus_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus")
}

#[test]
fn every_committed_corpus_replays_byte_identically() {
    let mut replayed = 0usize;
    for entry in std::fs::read_dir(corpus_dir()).expect("corpus directory exists") {
        let path = entry.expect("corpus entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("corpus file name")
            .to_owned();
        let contents = std::fs::read_to_string(&path).expect("corpus file reads");
        corpus::replay_corpus(&name, &contents);
        replayed += 1;
    }
    assert!(
        replayed > 0,
        "no corpora found — the replay lane must never pass vacuously"
    );
}

/// Sabotage: a corrupted expected envelope must fail replay — the lane
/// actually compares, not merely executes.
#[test]
fn replay_detects_a_corrupted_expectation() {
    let entry = std::fs::read_dir(corpus_dir())
        .expect("corpus directory exists")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.extension().and_then(|ext| ext.to_str()) == Some("jsonl"))
        .expect("at least one corpus file");
    let name = "sabotaged".to_owned();
    let contents = std::fs::read_to_string(&entry).expect("corpus file reads");
    // Corrupt the first expectation's payload.
    let corrupted = contents.replacen("\"expect\":", "\"expect\":{\"sabotaged\":", 1);
    let corrupted = if corrupted == contents {
        // No success expectation in the first file (unlikely) — corrupt an
        // error code instead.
        contents.replacen("\"expect_err\":\"", "\"expect_err\":\"sabotaged.", 1)
    } else {
        // Close the injected wrapper object so the line still parses.
        let (head, tail) = corrupted.split_once('\n').expect("header line");
        let tail = tail.replacen('\n', "}\n", 1);
        format!("{head}\n{tail}")
    };
    let result = std::panic::catch_unwind(|| {
        corpus::replay_corpus(&name, &corrupted);
    });
    assert!(
        result.is_err(),
        "a corrupted expectation must fail the replay"
    );
}
