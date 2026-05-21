//! Commit runtime fuzz target and seed inventory checks.

#![deny(unsafe_code)]

mod common;

use std::fs;
use std::path::Path;

#[cfg(feature = "testkit")]
use strata_storage_next::testkit::{
    check_commit_runtime_batch_contract, check_commit_runtime_conflict_contract,
    check_commit_runtime_durable_contract, check_commit_runtime_timeline_contract,
    CommitRuntimeAssuranceOutcome,
};

const COMMIT_RUNTIME_FUZZ_TARGETS: [(&str, &str); 4] = [
    (
        "commit_runtime_batch",
        "check_commit_runtime_batch_contract",
    ),
    (
        "commit_runtime_conflict",
        "check_commit_runtime_conflict_contract",
    ),
    (
        "commit_runtime_durable",
        "check_commit_runtime_durable_contract",
    ),
    (
        "commit_runtime_timeline",
        "check_commit_runtime_timeline_contract",
    ),
];

#[test]
fn commit_runtime_fuzz_targets_are_registered_and_routed() {
    let root = common::crate_root();
    let manifest = read_file(&root.join("fuzz/Cargo.toml"));

    for (target, contract) in COMMIT_RUNTIME_FUZZ_TARGETS {
        let target_path = root.join(format!("fuzz/fuzz_targets/{target}.rs"));
        let target_text = read_file(&target_path);
        assert!(
            manifest.contains(&format!("name = \"{target}\"")),
            "fuzz/Cargo.toml should register {target}"
        );
        assert!(
            manifest.contains(&format!("path = \"fuzz_targets/{target}.rs\"")),
            "fuzz/Cargo.toml should point {target} at its target file"
        );
        assert!(
            target_text.contains(contract),
            "{target} should call {contract}"
        );
        assert!(
            !target_text.contains("check_commit_runtime_scaffold_contract"),
            "{target} must not route fuzz bytes through the scaffold-only contract"
        );
        for (_, other_contract) in COMMIT_RUNTIME_FUZZ_TARGETS {
            if other_contract != contract {
                assert!(
                    !target_text.contains(other_contract),
                    "{target} should not call unrelated contract {other_contract}"
                );
            }
        }
    }
}

#[test]
fn commit_runtime_fuzz_corpora_have_multiple_nonempty_seeds() {
    let root = common::crate_root();
    for (target, _) in COMMIT_RUNTIME_FUZZ_TARGETS {
        let corpus = root.join(format!("fuzz/corpus/{target}"));
        let entries = fs::read_dir(&corpus)
            .unwrap_or_else(|error| panic!("read corpus {}: {error}", corpus.display()))
            .collect::<Result<Vec<_>, _>>()
            .unwrap_or_else(|error| panic!("read corpus entries {}: {error}", corpus.display()));
        assert!(
            entries.len() >= 2,
            "corpus {} should contain at least two seeds",
            corpus.display()
        );
        for entry in entries {
            let metadata = entry.metadata().unwrap_or_else(|error| {
                panic!("read metadata {}: {error}", entry.path().display())
            });
            assert!(
                metadata.is_file() && metadata.len() > 0,
                "corpus seed {} should be a non-empty file",
                entry.path().display()
            );
        }
    }
}

#[cfg(feature = "testkit")]
#[test]
fn commit_runtime_fuzz_corpus_seeds_exercise_success_and_failure_routes() {
    let root = common::crate_root();
    for (target, _) in COMMIT_RUNTIME_FUZZ_TARGETS {
        let corpus = root.join(format!("fuzz/corpus/{target}"));
        let mut saw_success = false;
        let mut saw_failure = false;
        for entry in fs::read_dir(&corpus)
            .unwrap_or_else(|error| panic!("read corpus {}: {error}", corpus.display()))
        {
            let path = entry
                .unwrap_or_else(|error| panic!("read corpus entry {}: {error}", corpus.display()))
                .path();
            let seed = fs::read(&path)
                .unwrap_or_else(|error| panic!("read seed {}: {error}", path.display()));
            let outcome = run_target_contract(target, &seed)
                .unwrap_or_else(|error| panic!("run seed {}: {error}", path.display()));
            saw_success |= seed_reaches_success_route(&outcome);
            saw_failure |= seed_reaches_failure_route(&outcome);
        }
        assert!(
            saw_success,
            "corpus {} should include a seed that reaches a success route",
            corpus.display()
        );
        assert!(
            saw_failure,
            "corpus {} should include a seed that reaches a rejection or fault route",
            corpus.display()
        );
    }
}

fn read_file(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

#[cfg(feature = "testkit")]
fn run_target_contract(
    target: &str,
    seed: &[u8],
) -> Result<CommitRuntimeAssuranceOutcome, strata_storage_next::testkit::TestkitError> {
    match target {
        "commit_runtime_batch" => check_commit_runtime_batch_contract(seed),
        "commit_runtime_conflict" => check_commit_runtime_conflict_contract(seed),
        "commit_runtime_durable" => check_commit_runtime_durable_contract(seed),
        "commit_runtime_timeline" => check_commit_runtime_timeline_contract(seed),
        _ => panic!("unknown commit runtime target {target}"),
    }
}

#[cfg(feature = "testkit")]
fn seed_reaches_success_route(outcome: &CommitRuntimeAssuranceOutcome) -> bool {
    outcome.cache_successes > 0
        || outcome.durable_successes > 0
        || outcome.read_only_diagnostics > 0
        || outcome.branch_lifecycle_transitions > 0
        || outcome.replay_successes > 0
        || outcome.timeline_checks > 0
}

#[cfg(feature = "testkit")]
fn seed_reaches_failure_route(outcome: &CommitRuntimeAssuranceOutcome) -> bool {
    outcome.wal_failures > 0
        || outcome.post_wal_failures > 0
        || outcome.conflict_rejections > 0
        || outcome.guard_or_quiesce_rejections > 0
        || outcome.branch_lifecycle_rejections > 0
}
