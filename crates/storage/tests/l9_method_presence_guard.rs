//! L9 public-method test-presence guard (TCP3.3c, Phase 3 Tier-1 tracker).
//!
//! `StorageRuntime` is the sole engine-facing surface (L9). Every public
//! method on it is a contract the engine depends on, so every one must be
//! exercised by at least one test, or carry a shrink-only allowlist entry
//! with a reason. The Phase 3 storage deep-dive found several boundary
//! methods (timeline lookups, immutable-source scan, maintenance drain error
//! arms) with no negative coverage; this guard makes "every L9 method is
//! touched by a test" a CI invariant so that cannot silently recur.
//!
//! The guard is coarse: it checks a method *name* is referenced somewhere in
//! the storage test tree, not that the reference is a good test. Depth is the
//! individual suites' job; presence is this guard's.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Public `StorageRuntime` methods that are legitimately not called by name in
/// a test, each with a reason. Shrink-only: an entry whose method gains a test
/// reference, or stops existing, fails the guard.
const ALLOWED_UNREFERENCED: &[(&str, &str)] = &[
    // Empty: every public StorageRuntime method is referenced by a test.
    // Entries here are shrink-only and must carry a reason.
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn runtime_source() -> String {
    let path = repo_root().join("src/api/runtime/mod.rs");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

/// Parse the public method names from the `StorageRuntime` surface.
fn public_method_names(source: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        let rest = trimmed
            .strip_prefix("pub fn ")
            .or_else(|| trimmed.strip_prefix("pub const fn "))
            .or_else(|| trimmed.strip_prefix("pub async fn "));
        if let Some(rest) = rest {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            // `_for_test` helpers exist only for tests; they need no separate
            // presence entry.
            if !name.is_empty() && !name.ends_with("_for_test") {
                names.insert(name);
            }
        }
    }
    names
}

/// Collect the text of every storage test location (integration tests and the
/// in-crate test/testkit trees).
fn test_corpus() -> String {
    let root = repo_root();
    let mut buffer = String::new();
    let mut roots = vec![root.join("tests"), root.join("src")];
    while let Some(dir) = roots.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                roots.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") && is_test_path(&path) {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    buffer.push_str(&text);
                    buffer.push('\n');
                }
            }
        }
    }
    buffer
}

fn is_test_path(path: &Path) -> bool {
    let text = path.to_string_lossy();
    // Integration tests, in-crate `tests/` module trees, and testkit harnesses.
    text.contains("/tests/")
        || text.contains("/testkit/")
        || text.contains("test_support")
        || text.ends_with("tests.rs")
}

#[test]
fn every_l9_public_method_is_referenced_by_a_test() {
    let methods = public_method_names(&runtime_source());
    assert!(
        methods.len() >= 25,
        "parsed only {} public methods — the parser no longer matches the source shape",
        methods.len()
    );
    let corpus = test_corpus();
    let allowed: BTreeSet<&str> = ALLOWED_UNREFERENCED.iter().map(|(name, _)| *name).collect();

    let mut unreferenced = Vec::new();
    for method in &methods {
        if allowed.contains(method.as_str()) {
            continue;
        }
        // A call reference: `.method(` or `method(` somewhere in a test.
        let call = format!("{method}(");
        if !corpus.contains(&call) {
            unreferenced.push(format!("  StorageRuntime::{method}"));
        }
    }
    assert!(
        unreferenced.is_empty(),
        "L9 public methods with no test reference (add a test that calls the \
         method, or an ALLOWED_UNREFERENCED entry with a reason):\n{}",
        unreferenced.join("\n")
    );
}

#[test]
fn method_presence_allowlist_only_shrinks() {
    let methods = public_method_names(&runtime_source());
    let corpus = test_corpus();

    let mut stale = Vec::new();
    for (method, reason) in ALLOWED_UNREFERENCED {
        if !methods.contains(*method) {
            // `graceful` / builders live in the same file but are not
            // `StorageRuntime` methods; tolerate their absence from the parsed
            // set only if they are genuinely gone from the source entirely.
            if !runtime_source().contains(&format!("fn {method}")) {
                stale.push(format!("  {method}: no longer in the source ({reason})"));
            }
            continue;
        }
        let call = format!("{method}(");
        if corpus.contains(&call) {
            stale.push(format!(
                "  {method}: now referenced by a test — delete its allowlist entry ({reason})"
            ));
        }
    }
    assert!(
        stale.is_empty(),
        "shrink-only method-presence allowlist has stale entries:\n{}",
        stale.join("\n")
    );
}
