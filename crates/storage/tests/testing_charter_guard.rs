//! The charter anti-drift guard (STH-7d): "the map may not lie", enforced.
//!
//! The storage-testing charter documents drifted on their sharpest claims
//! within two weeks of being written — checkmarks survived while the cited
//! artifacts changed underneath them. This guard parses the evidence anchors
//! (backticked repository paths) out of the charter documents and asserts
//! every cited artifact exists in the tree. Deleting or renaming a cited
//! suite now breaks CI instead of silently invalidating the map.
//!
//! The guard asserts *existence*, not content, so it catches deletion and
//! rename without being brittle about wording.

#![deny(unsafe_code)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Charter documents whose evidence anchors are enforced.
const CHARTER_DOCUMENTS: &[&str] = &[
    "docs/architecture/v1-storage-testing-taxonomy-and-gaps.md",
    "docs/architecture/v1-storage-testing-gold-standard-delta.md",
    "docs/architecture/v1-test-coverage-program.md",
    "docs/architecture/implementation-plans/storage-testing/README.md",
];

/// Anchors that legitimately reference artifacts that do not exist yet (or
/// no longer exist) — each entry needs a reason. Keep this list short: every
/// entry is a hole in the map.
const ALLOWED_MISSING: &[(&str, &str)] = &[
    (
        "docs/product/strata-v1-non-functional-requirements.md",
        "referenced by the taxonomy's related-documents list; product doc tree is being reorganized",
    ),
    (
        "crates/cli/tests/",
        "program charter Phase 2.3 deliverable: the CLI integration suite does not exist yet — \
         building it is the phase; remove this entry when it lands",
    ),
];

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = crates/storage.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root above crates/storage")
        .to_path_buf()
}

/// Candidate roots an anchor may resolve against: the repository root and
/// the storage crate (charter docs frequently cite storage-relative paths
/// like `tests/fault_sweep.rs` or `src/testkit/...`).
fn resolves(root: &Path, anchor: &str) -> bool {
    let candidates = [
        root.join(anchor),
        root.join("crates/storage").join(anchor),
        root.join("crates/storage/src").join(anchor),
    ];
    candidates.iter().any(|path| path.exists())
}

/// A backticked token counts as a path anchor when it looks like a concrete
/// repository path: contains a separator, no spaces or macro/glob syntax,
/// and ends in a known artifact extension or names a directory under a known
/// tree. Prose tokens (`cargo test ...`, type names, error codes) never
/// match.
fn is_path_anchor(token: &str) -> bool {
    if token.contains(' ')
        || token.contains('{')
        || token.contains('(')
        || token.contains('*')
        || token.contains("::")
    {
        return false;
    }
    if !token.contains('/') {
        return false;
    }
    let known_prefix = [
        "crates/",
        "docs/",
        ".github/",
        "src/",
        "tests/",
        "testdata/",
        "fuzz/",
        "testkit/",
    ]
    .iter()
    .any(|prefix| token.starts_with(prefix));
    let known_suffix = [".rs", ".md", ".yml", ".yaml", ".toml", ".hex"]
        .iter()
        .any(|suffix| token.ends_with(suffix));
    // Extensionless tokens under a known tree are module/directory anchors
    // (`testkit/fault_sweep`); the caller probes them as dir and `.rs`.
    known_prefix && (known_suffix || token.ends_with('/') || !token.contains('.'))
}

fn backticked_tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    for chunk in text.split('`').skip(1).step_by(2) {
        tokens.push(chunk.to_string());
    }
    tokens
}

#[test]
fn every_charter_evidence_anchor_exists() {
    let root = repo_root();
    let mut missing: BTreeSet<(String, String)> = BTreeSet::new();
    let mut checked = 0usize;

    for document in CHARTER_DOCUMENTS {
        let path = root.join(document);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("charter document {document} unreadable: {err}"));
        for token in backticked_tokens(&text) {
            // `testkit/...` module anchors have no extension in prose; try
            // them as directories and as `.rs` modules.
            #[allow(
                clippy::case_sensitive_file_extension_comparisons,
                reason = "charter anchors are authored lowercase; case variance would itself be drift"
            )]
            let anchor_variants = if token.ends_with(".rs") || token.contains('.') {
                vec![token.clone()]
            } else {
                vec![token.clone(), format!("{token}.rs"), format!("{token}/")]
            };
            if !is_path_anchor(&token) {
                continue;
            }
            checked += 1;
            let exists = anchor_variants
                .iter()
                .any(|variant| resolves(&root, variant.trim_end_matches('/')));
            let allowed = ALLOWED_MISSING.iter().any(|(allowed, _)| *allowed == token);
            if !exists && !allowed {
                missing.insert(((*document).to_string(), token));
            }
        }
    }

    assert!(
        checked > 20,
        "the guard parsed only {checked} anchors; the charter documents moved or the \
         parser regressed — the map guard is vacuous"
    );
    assert!(
        missing.is_empty(),
        "charter evidence anchors name artifacts that do not exist — the testing map \
         has drifted. Fix the document or the tree (or, with a written reason, extend \
         ALLOWED_MISSING):\n{}",
        missing
            .iter()
            .map(|(doc, anchor)| format!("  {doc}: `{anchor}`"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// The STH program's slice ledger and the coverage program's phase ledger
/// must never claim an implemented slice whose primary artifact is gone.
#[test]
fn implemented_slice_claims_cite_existing_artifacts() {
    let root = repo_root();
    let claims: &[(&str, &str)] = &[
        // (claimed-implemented artifact, claiming document)
        (
            "crates/storage/src/testkit/recovery_oracle",
            "STH-1 recovery oracle",
        ),
        (
            "crates/storage/src/testkit/fault_sweep",
            "STH-2 fault sweeps",
        ),
        (
            "crates/storage/src/testkit/reordering_backend.rs",
            "STH-3a durability realism",
        ),
        (
            "crates/storage/src/testkit/write_ordering_watchdog.rs",
            "STH-3b write-ordering watchdog",
        ),
        ("crates/storage/src/testkit/simulation", "STH-4 DST"),
        (
            "crates/storage/src/testkit/compound_faults.rs",
            "STH-5 failure-during-failure",
        ),
        (
            "crates/storage/src/testkit/config_differential.rs",
            "STH-6 config differential",
        ),
        (
            "crates/storage/src/api/tests/liveness_matrix.rs",
            "STH-6 liveness matrix",
        ),
        (
            ".github/workflows/nightly.yml",
            "STH-7a memory-safety lanes",
        ),
        (".github/workflows/fuzz.yml", "STH-7c scheduled fuzzing"),
        (
            "crates/storage/tests/testing_charter_guard.rs",
            "STH-7d charter guard",
        ),
    ];
    for (artifact, slice) in claims {
        assert!(
            root.join(artifact).exists(),
            "{slice} is claimed implemented but its artifact {artifact} is missing"
        );
    }
}
