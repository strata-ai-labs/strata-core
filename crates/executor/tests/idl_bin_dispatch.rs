//! The `strata-idl` bin dispatch for the TCP4.1 test-generation verbs,
//! exercised hermetically: `STRATA_IDL_REPO_ROOT` points the bin at a scratch
//! copy of the IDL tree, so `generate-tests` never writes into the real
//! repository. Kills the match-arm mutants `cargo test` alone cannot see
//! (a deleted arm falls through to the unknown-verb exit 2).
#![cfg(all(feature = "idl-tooling", feature = "inference", feature = "testkit"))]

use std::path::Path;
use std::process::Command;

fn copy_tree(source: &Path, destination: &Path) {
    std::fs::create_dir_all(destination).expect("scratch mkdir");
    for entry in std::fs::read_dir(source).expect("scratch read_dir") {
        let entry = entry.expect("scratch entry");
        let target = destination.join(entry.file_name());
        if entry.file_type().expect("scratch file_type").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("scratch copy");
        }
    }
}

fn run(scratch: &Path, verb: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_strata-idl"))
        .arg(verb)
        .env("STRATA_IDL_REPO_ROOT", scratch)
        .output()
        .expect("run strata-idl")
}

#[test]
fn the_test_generation_verbs_dispatch_and_the_unknown_verb_refuses() {
    let real = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("executor lives under crates/")
        .to_path_buf();
    let scratch = tempfile::tempdir().expect("scratch root");
    for relative in ["crates/executor/idl/v1", "crates/executor/tests/fixtures"] {
        copy_tree(&real.join(relative), &scratch.path().join(relative));
    }
    // The resolver also scans the enum sources for variant coverage.
    let src = scratch.path().join("crates/executor/src");
    std::fs::create_dir_all(&src).expect("scratch src dir");
    for file in ["command.rs", "output.rs"] {
        std::fs::copy(real.join("crates/executor/src").join(file), src.join(file))
            .expect("copy enum source");
    }
    let generated = scratch
        .path()
        .join("crates/executor/tests/generated/conformance_cases.rs");
    std::fs::create_dir_all(generated.parent().expect("parent")).expect("mkdir generated");
    std::fs::write(&generated, "// bogus\n").expect("seed bogus file");

    // Stale scratch file: `check-tests` dispatches and reports failure.
    let stale = run(scratch.path(), "check-tests");
    assert_eq!(stale.status.code(), Some(1), "stale check must exit 1");

    // `generate-tests` dispatches and repairs the scratch copy...
    let generate = run(scratch.path(), "generate-tests");
    assert!(
        generate.status.success(),
        "generate-tests failed: {}",
        String::from_utf8_lossy(&generate.stderr)
    );
    // ...after which `check-tests` passes.
    let fresh = run(scratch.path(), "check-tests");
    assert!(
        fresh.status.success(),
        "fresh check failed: {}",
        String::from_utf8_lossy(&fresh.stderr)
    );

    // A verb the dispatch does not know exits 2 (the usage error) — the
    // failure mode a deleted match arm would collapse real verbs into.
    let unknown = run(scratch.path(), "definitely-not-a-verb");
    assert_eq!(unknown.status.code(), Some(2), "unknown verb must exit 2");
}
