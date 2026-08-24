//! Branch revert/cherry-pick absence guard (TCP3.7, issue #2635).
//!
//! CLAUDE.md hard rule 20 says the remaining **mutating** promotion operations
//! that write a new commit — cherry-pick and revert — are absent in V1. This
//! guard enforces that: it scans the engine source for the distinctive
//! vocabulary those surfaces would introduce and fails if any appears.
//!
//! The guard narrows as each mutating op lands rather than being deleted.
//! Read-only **preview promotion** (merge-base, three-way diff) landed in M12C,
//! dropping those tokens; **promote (merge)** landed in M12D1, dropping the
//! `branch_merge`/`merge_branch` tokens. Cherry-pick and revert stay absent and
//! guarded until their slices (M12E cherry-pick, M12F revert); each must, when
//! it lands, drop its token here, amend rule 20, and add its strict-refusal
//! tests, so the typed-refusal surface can never ship untested.

use std::fs;
use std::path::{Path, PathBuf};

/// Distinctive tokens the still-absent **mutating** branch operations would
/// introduce (cherry-pick, revert). Deliberately compound so they do not
/// collide with implemented, unrelated vocabulary. Preview's
/// `merge_base`/`three_way` tokens were removed in M12C; promote's
/// `branch_merge`/`merge_branch` tokens were removed in M12D1.
const FORBIDDEN: &[&str] = &[
    "cherry_pick",
    "cherry-pick",
    "branch_cherry_pick",
    "branch_revert",
];

#[test]
fn branch_revert_cherry_pick_stay_absent_in_v1() {
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
        "mutating branch revert/cherry-pick vocabulary appeared in engine \
         source, but CLAUDE.md rule 20 states these are absent in V1. If you are \
         landing that surface: amend rule 20, add its strict-refusal tests, then \
         drop its token from FORBIDDEN. Offenders:\n{}",
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
