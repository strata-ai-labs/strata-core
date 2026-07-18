//! Branch merge/revert/cherry-pick absence guard (TCP3.7, issue #2635).
//!
//! CLAUDE.md hard rule 20 says branch merge/revert/cherry-pick are **absent**
//! in V1 — there is no entrypoint, so there is no strict-refusal surface yet.
//! Of the named branch invariants (generation monotonicity, empty-branch
//! creation, cross-branch rejection, recreate-after-delete), this was the only
//! one with no test of any kind: a hard rule describing a surface that does not
//! exist and is not enforced.
//!
//! This guard turns "unverified absence" into "enforced absence". It scans the
//! engine source for the distinctive vocabulary a branch merge/revert/
//! cherry-pick surface would introduce and fails if any appears. When the
//! post-V1 merge work lands, this test fails by design — the implementer must
//! then delete it, amend rule 20, and add the strict-refusal tests the rule
//! promises, so the typed-refusal surface can never ship untested.

use std::fs;
use std::path::{Path, PathBuf};

/// Distinctive branch-operation tokens. Each is absent from engine source
/// today; a merge/revert/cherry-pick family (fork, diff, three-way diff, merge,
/// merge-base, cherry-pick, revert — see the branch-operation contract) would
/// introduce them. Deliberately compound so they do not collide with the
/// implemented, unrelated merges: vector candidate reranking
/// (`merge_and_rerank_candidates`) and document-level JSON merge (rule 21).
const FORBIDDEN: &[&str] = &[
    "cherry_pick",
    "cherry-pick",
    "branch_cherry_pick",
    "branch_merge",
    "merge_branch",
    "merge_base",
    "merge-base",
    "branch_revert",
    "three_way",
    "three-way",
];

#[test]
fn branch_merge_revert_cherry_pick_stay_absent_in_v1() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders: Vec<String> = Vec::new();
    for path in rust_files(&root) {
        let text = fs::read_to_string(&path).expect("read engine source file");
        for token in FORBIDDEN {
            if text.contains(token) {
                offenders.push(format!("{}: `{token}`", path.display()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "branch merge/revert/cherry-pick vocabulary appeared in engine source, \
         but CLAUDE.md rule 20 states these are absent in V1. If you are landing \
         the post-V1 merge surface: amend rule 20, add its strict-refusal tests, \
         then delete this guard. Offenders:\n{}",
        offenders.join("\n")
    );
}

fn rust_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_rust_files(root, &mut files);
    files
}

fn collect_rust_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rust_files(&path, files);
        } else if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("rs"))
        {
            files.push(path);
        }
    }
}
