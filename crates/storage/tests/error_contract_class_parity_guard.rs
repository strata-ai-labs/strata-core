//! Error-contract class-parity guard (TCP3.5a, Phase 3 Tier-1 tracker).
//!
//! `v1-error-and-diagnostics-contract.md` declares the error *classes* (the
//! `<class>` segment of every `<class>.<area>.<detail>` code). The workspace
//! error-code assertion guard tracks every code emitted by product source and
//! filters them by a `CLASSES` allowlist — so `CLASSES` is, by construction,
//! the exact set of class prefixes real error codes use (a class not in
//! `CLASSES` would leave its codes invisible; TCP3.2a/3.3d proved that failure
//! mode with the storage-area and `unknown`/`deadline_exceeded` drifts).
//!
//! This guard ties the two together: the contract doc's Error Class table must
//! equal the workspace guard's `CLASSES`. A code that starts using a new class
//! must be added to `CLASSES` (or its codes go untracked), and this guard then
//! forces the same class into the contract doc — closing the doc↔code drift
//! that let `unknown.*` and `deadline_exceeded.*` codes ship against classes
//! the contract explicitly excluded (issues #2646, #2633).

use std::collections::BTreeSet;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root above crates/storage")
        .to_path_buf()
}

/// The error classes the contract doc declares, parsed from the `### Error
/// Class` table (the rows between that heading and the prose that follows).
/// The doc has a second single-word backticked table — retryability values
/// (`never`, `unknown`, …) — which this deliberately excludes by stopping at
/// the table's end.
fn documented_classes() -> BTreeSet<String> {
    let doc = std::fs::read_to_string(
        repo_root().join("docs/architecture/v1-error-and-diagnostics-contract.md"),
    )
    .expect("read error contract");

    let start = doc
        .find("### Error Class")
        .expect("contract keeps the Error Class section");
    // The table ends at the explanatory prose that follows it.
    let section_end = doc[start..]
        .find("Classes are intentionally few")
        .map(|i| start + i)
        .expect("Error Class table is followed by its summary prose");
    let section = &doc[start..section_end];

    let mut classes = BTreeSet::new();
    for line in section.lines() {
        let trimmed = line.trim_start();
        // Table rows look like: `| `class` | description |`. The class is the
        // first backticked token, and description columns never are single
        // lowercase idents in the first column.
        if !trimmed.starts_with("| `") {
            continue;
        }
        let rest = &trimmed[3..];
        let Some(end) = rest.find('`') else { continue };
        let token = &rest[..end];
        if !token.is_empty() && token.bytes().all(|b| b.is_ascii_lowercase() || b == b'_') {
            classes.insert(token.to_owned());
        }
    }
    classes
}

/// The class list the workspace error-code guard tracks, parsed from its
/// `CLASSES` constant. Kept as a parse (not a shared constant) so the two test
/// crates stay independent and a change to either is caught here.
fn tracked_classes() -> BTreeSet<String> {
    let guard = std::fs::read_to_string(
        repo_root().join("crates/storage/tests/error_code_assertion_guard.rs"),
    )
    .expect("read error-code assertion guard");

    let start = guard
        .find("const CLASSES: &[&str] = &[")
        .expect("guard keeps the CLASSES constant");
    let end = start
        + guard[start..]
            .find("];")
            .expect("CLASSES constant is closed");
    let body = &guard[start..end];

    let mut classes = BTreeSet::new();
    for raw in body.split('"').skip(1).step_by(2) {
        if !raw.is_empty() && raw.bytes().all(|b| b.is_ascii_lowercase() || b == b'_') {
            classes.insert(raw.to_owned());
        }
    }
    classes
}

#[test]
fn contract_error_classes_match_the_tracked_class_set() {
    let documented = documented_classes();
    let tracked = tracked_classes();

    assert!(
        documented.len() >= 10,
        "parsed only {} documented classes — the Error Class table parser has drifted",
        documented.len()
    );

    let doc_only: Vec<&String> = documented.difference(&tracked).collect();
    let code_only: Vec<&String> = tracked.difference(&documented).collect();

    assert!(
        doc_only.is_empty(),
        "the contract doc declares error classes the workspace guard does not track \
         (either code should emit them, or remove them from the contract): {doc_only:?}"
    );
    assert!(
        code_only.is_empty(),
        "the workspace guard tracks class prefixes the contract doc does not declare \
         (a code is using an undeclared error class — declare it in \
         v1-error-and-diagnostics-contract.md, or rename the code's class): {code_only:?}"
    );
}
