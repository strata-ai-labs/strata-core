//! Workspace error-code assertion guard (TCP3.0, Phase 3 Tier-1 tracker).
//!
//! Every stable error code a user can receive (`<class>.<area>.<detail>`)
//! must be asserted by at least one test, or carry a shrink-only allowlist
//! entry naming the slice that will assert it. The Phase 3 gap analysis
//! measured 68 codes that no test had ever asserted — including most of the
//! engine's `data_loss.*` corruption detectors. This guard turns that
//! finding into a ratchet: new codes without a coverage decision fail CI,
//! and allowlist entries die the moment a test asserts their code.
//!
//! Co-located with `testing_charter_guard.rs`: program-level guards live in
//! the storage test tree because it is the program's home crate. The scan
//! is workspace-wide.
//!
//! Classification rules (deliberately simple, grep-grade — this is a
//! ratchet, not a proof):
//! - A code is DEFINED if its string literal appears in product source
//!   (crate `src/` outside test locations, before any `#[cfg(test)]`).
//! - A code is ASSERTED if its literal appears in a test location: a
//!   `tests/` tree, a `testkit` module, `test_support`, or the
//!   `#[cfg(test)]` tail of a product file. Doc-comment mentions count;
//!   the ratchet tolerates that looseness in exchange for zero build
//!   machinery.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Error classes that form the first segment of a stable 3-part code.
/// Sourced from `v1-error-and-diagnostics-contract.md` plus `data_loss`,
/// which the engine emits today (part of the D2 doc/code reconciliation
/// tracked in the Phase 3 plan).
const CLASSES: &[&str] = &[
    "not_found",
    "already_exists",
    "invalid_argument",
    "failed_precondition",
    "access_denied",
    "conflict",
    "ambiguous_commit",
    "history_unavailable",
    "unsupported",
    "resource_exhausted",
    "unavailable",
    "io",
    "corruption",
    "serialization",
    "internal",
    "data_loss",
];

/// Areas that form the middle segment of a stable 3-part code.
const AREAS: &[&str] = &[
    // Product areas.
    "cli",
    "engine",
    "storage",
    "executor",
    "inference",
    "intelligence",
    "hub",
    "wasm",
    "sdk",
    // Storage's internal layer areas. Storage does not use "storage" as its
    // area segment — it names the failing layer — so without these the guard
    // was blind to 106 of its codes (TCP3.2a).
    "storage_api",
    "lifecycle",
    "branch",
    "commit",
    "table",
    "backend",
    "layout",
    "format",
    "service",
];

/// Codes with no asserting test yet. Every entry names the Phase 3 slice
/// (or reason) that owns closing it. Shrink-only: the guard fails if an
/// entry's code becomes asserted (remove the entry) or stops existing
/// (delete the dead entry).
const ALLOWED_UNASSERTED: &[(&str, &str)] = &[
    // --- storage internal-layer codes (surfaced by the TCP3.2a area fix) ---
    ("already_exists.lifecycle.branch", "lifecycle inner code; asserted by TCP3.2c lifecycle hop"),
    ("corruption.lifecycle.quarantine", "lifecycle inner code; asserted by TCP3.2c lifecycle hop"),
    ("failed_precondition.lifecycle.branch", "lifecycle inner code; asserted by TCP3.2c lifecycle hop"),
    ("failed_precondition.lifecycle.branch_generation", "lifecycle inner code; asserted by TCP3.2c lifecycle hop"),
    ("failed_precondition.lifecycle.branch_history", "lifecycle inner code; asserted by TCP3.2c lifecycle hop"),
    ("failed_precondition.lifecycle.branch_state", "lifecycle inner code; asserted by TCP3.2c lifecycle hop"),
    ("failed_precondition.lifecycle.fork_source_unflushed", "lifecycle inner code; asserted by TCP3.2c lifecycle hop"),
    ("failed_precondition.lifecycle.pinned_view_release", "lifecycle inner code; asserted by TCP3.2c lifecycle hop"),
    ("failed_precondition.lifecycle.quarantine_publication", "lifecycle inner code; asserted by TCP3.2c lifecycle hop"),
    ("failed_precondition.lifecycle.storage_pressure", "lifecycle inner code; asserted by TCP3.2c lifecycle hop"),
    ("failed_precondition.lifecycle.table_manifest_branch_install", "lifecycle inner code; asserted by TCP3.2c lifecycle hop"),
    ("failed_precondition.lifecycle.table_runtime", "lifecycle inner code; asserted by TCP3.2c lifecycle hop"),
    ("failed_precondition.lifecycle.timestamp_history", "lifecycle inner code; asserted by TCP3.2c lifecycle hop"),
    ("internal.lifecycle.layout", "lifecycle inner code; asserted by TCP3.2c lifecycle hop"),
    ("resource_exhausted.lifecycle.branch_generation", "lifecycle inner code; asserted by TCP3.2c lifecycle hop"),
    ("serialization.lifecycle.format", "lifecycle inner code; asserted by TCP3.2c lifecycle hop"),
    ("unavailable.lifecycle.rewrite_output_sweep_race", "lifecycle inner code; asserted by TCP3.2c lifecycle hop"),
    ("data_loss.engine.branch_id", "corruption detector; asserted by TCP3.15 corruption injection"),
    ("data_loss.engine.control_name", "corruption detector; asserted by TCP3.15 corruption injection"),
    ("data_loss.engine.graph_edge_record", "corruption detector; asserted by TCP3.15 corruption injection"),
    ("data_loss.engine.graph_node_record", "corruption detector; asserted by TCP3.15 corruption injection"),
    ("data_loss.engine.graph_type_index_key", "corruption detector; asserted by TCP3.15 corruption injection"),
    ("data_loss.engine.graph_type_index_record", "corruption detector; asserted by TCP3.15 corruption injection"),
    ("data_loss.engine.json_index", "corruption detector; asserted by TCP3.15 corruption injection"),
    ("data_loss.engine.kv_value", "corruption detector; asserted by TCP3.15 corruption injection"),
    ("data_loss.engine.vector_artifact", "corruption detector; asserted by TCP3.15 corruption injection"),
    ("data_loss.engine.vector_artifacts", "corruption detector; asserted by TCP3.15 corruption injection"),
    ("data_loss.engine.vector_index_manifest", "corruption detector; asserted by TCP3.15 corruption injection"),
    ("failed_precondition.cli.binary_not_on_path", "doctor/environment path; asserted by TCP3.10 CLI slice"),
    ("failed_precondition.cli.home_not_directory", "doctor/environment path; asserted by TCP3.10 CLI slice"),
    ("failed_precondition.cli.home_unresolved", "doctor/environment path; asserted by TCP3.10 CLI slice"),
    ("failed_precondition.engine.vector_artifact", "vector artifact/index precondition; asserted by TCP3.5 vector refusal batch"),
    ("failed_precondition.engine.vector_index_manifest", "vector artifact/index precondition; asserted by TCP3.5 vector refusal batch"),
    ("failed_precondition.executor.hub_clone", "asserted by TCP3.13 hub fault lane via executor envelope"),
    ("invalid_argument.engine.event_batch", "validation refusal; asserted by TCP3.5 refusal batches"),
    ("invalid_argument.engine.event_metadata", "validation refusal; asserted by TCP3.5 refusal batches"),
    ("invalid_argument.engine.event_payload_too_large", "validation refusal; asserted by TCP3.5 refusal batches"),
    ("invalid_argument.engine.event_record", "validation refusal; asserted by TCP3.5 refusal batches"),
    ("invalid_argument.engine.graph_batch", "validation refusal; asserted by TCP3.5 refusal batches"),
    ("invalid_argument.engine.graph_binding", "validation refusal; asserted by TCP3.5 refusal batches"),
    ("invalid_argument.engine.graph_binding_record", "validation refusal; asserted by TCP3.5 refusal batches"),
    ("invalid_argument.engine.graph_edge_record", "validation refusal; asserted by TCP3.5 refusal batches"),
    ("invalid_argument.engine.graph_edge_type", "validation refusal; asserted by TCP3.5 refusal batches"),
    ("invalid_argument.engine.graph_edge_type_reserved", "validation refusal; asserted by TCP3.5 refusal batches"),
    ("invalid_argument.engine.graph_metadata", "validation refusal; asserted by TCP3.5 refusal batches"),
    ("invalid_argument.engine.graph_name", "validation refusal; asserted by TCP3.5 refusal batches"),
    ("invalid_argument.engine.graph_name_reserved", "validation refusal; asserted by TCP3.5 refusal batches"),
    ("invalid_argument.engine.graph_node_id", "validation refusal; asserted by TCP3.5 refusal batches"),
    ("invalid_argument.engine.graph_node_record", "validation refusal; asserted by TCP3.5 refusal batches"),
    ("invalid_argument.engine.graph_ontology_record", "validation refusal; asserted by TCP3.5 refusal batches"),
    ("invalid_argument.engine.graph_properties_too_large", "validation refusal; asserted by TCP3.5 refusal batches"),
    ("invalid_argument.engine.graph_property_name", "validation refusal; asserted by TCP3.5 refusal batches"),
    ("invalid_argument.engine.graph_type_hint", "validation refusal; asserted by TCP3.5 refusal batches"),
    ("invalid_argument.engine.graph_type_index_record", "validation refusal; asserted by TCP3.5 refusal batches"),
    ("invalid_argument.engine.json_batch", "validation refusal; asserted by TCP3.5 refusal batches"),
    ("invalid_argument.engine.json_batch_duplicate_document", "validation refusal; asserted by TCP3.5 refusal batches"),
    ("invalid_argument.engine.json_document", "validation refusal; asserted by TCP3.5 refusal batches"),
    ("invalid_argument.engine.json_index", "validation refusal; asserted by TCP3.5 refusal batches"),
    ("invalid_argument.engine.json_index_name", "validation refusal; asserted by TCP3.5 refusal batches"),
    ("invalid_argument.engine.json_value", "validation refusal; asserted by TCP3.5 refusal batches"),
    ("invalid_argument.engine.space_catalog", "validation refusal; asserted by TCP3.5 refusal batches"),
    ("invalid_argument.engine.space_delete_default", "validation refusal; asserted by TCP3.5 refusal batches"),
    ("invalid_argument.engine.space_delete_too_large", "validation refusal; asserted by TCP3.5 refusal batches"),
    ("invalid_argument.engine.vector_artifact", "validation refusal; asserted by TCP3.5 refusal batches"),
    ("invalid_argument.engine.vector_batch", "validation refusal; asserted by TCP3.5 refusal batches"),
    ("invalid_argument.engine.vector_index_manifest", "validation refusal; asserted by TCP3.5 refusal batches"),
    ("invalid_argument.engine.vector_metadata", "validation refusal; asserted by TCP3.5 refusal batches"),
    ("invalid_argument.engine.vector_metadata_too_large", "validation refusal; asserted by TCP3.5 refusal batches"),
    ("invalid_argument.engine.vector_record", "validation refusal; asserted by TCP3.5 refusal batches"),
    ("invalid_argument.executor.arrow_base64", "asserted by TCP3.8 error-envelope replay fixtures"),
    ("invalid_argument.executor.arrow_json_key", "asserted by TCP3.8 error-envelope replay fixtures"),
    ("invalid_argument.executor.arrow_vector_dimension", "asserted by TCP3.8 error-envelope replay fixtures"),
    ("invalid_argument.executor.arrow_vector_key", "asserted by TCP3.8 error-envelope replay fixtures"),
    ("invalid_argument.executor.graph_analytics_budget", "asserted by TCP3.8 error-envelope replay fixtures"),
    ("invalid_argument.executor.hub_branch", "asserted by TCP3.8 error-envelope replay fixtures"),
    ("invalid_argument.executor.hub_dataset", "asserted by TCP3.8 error-envelope replay fixtures"),
    ("invalid_argument.executor.kv_batch_duplicate_key", "asserted by TCP3.8 error-envelope replay fixtures"),
    ("invalid_argument.executor.limit", "asserted by TCP3.8 error-envelope replay fixtures"),
    ("invalid_argument.executor.vector_limit", "asserted by TCP3.8 error-envelope replay fixtures"),
    ("not_found.cli.database_path", "doctor/environment path; asserted by TCP3.10 CLI slice"),
    ("not_found.engine.database", "emitted by hub remote.rs under an engine area (layering review); asserted by TCP3.13 hub fault lane"),
    ("unavailable.engine.vector_artifacts", "vector artifact/index precondition; asserted by TCP3.5 vector refusal batch"),
];

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = crates/storage.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root above crates/storage")
        .to_path_buf()
}

fn is_test_location(path: &Path) -> bool {
    let text = path.to_string_lossy();
    text.contains("/tests/")
        || text.contains("/testkit/")
        || text.contains("test_support")
        || text.ends_with("testkit.rs")
}

fn walk(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name();
            if name == "target" || name == ".git" {
                continue;
            }
            walk(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
}

/// Extract every `<class>.<area>.<detail>` string literal from `text`.
fn extract_codes(text: &str, into: &mut BTreeSet<String>) {
    for candidate in text.split('"').skip(1).step_by(2) {
        let mut parts = candidate.splitn(3, '.');
        let (Some(class), Some(area), Some(detail)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let segment_ok = |segment: &str| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        };
        if CLASSES.contains(&class)
            && AREAS.contains(&area)
            && segment_ok(class)
            && segment_ok(detail)
        {
            into.insert(candidate.to_owned());
        }
    }
}

fn collect() -> (BTreeMap<String, PathBuf>, BTreeSet<String>) {
    let root = repo_root();
    let mut files = Vec::new();
    walk(&root.join("crates"), &mut files);

    let guard_file = root.join("crates/storage/tests/error_code_assertion_guard.rs");
    let mut defined: BTreeMap<String, PathBuf> = BTreeMap::new();
    let mut asserted: BTreeSet<String> = BTreeSet::new();

    for path in files {
        // The guard's own allowlist must not count as assertions.
        if path == guard_file {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if is_test_location(&path) {
            extract_codes(&text, &mut asserted);
            continue;
        }
        // Product file: the `#[cfg(test)]` tail counts as test content.
        let split = text.find("#[cfg(test)]").unwrap_or(text.len());
        let mut product_codes = BTreeSet::new();
        extract_codes(&text[..split], &mut product_codes);
        extract_codes(&text[split..], &mut asserted);
        for code in product_codes {
            defined.entry(code).or_insert_with(|| path.clone());
        }
    }
    (defined, asserted)
}

#[test]
fn every_error_code_is_asserted_or_carries_a_reasoned_allowlist_entry() {
    let (defined, asserted) = collect();
    let allowlist: BTreeMap<&str, &str> = ALLOWED_UNASSERTED.iter().copied().collect();

    let mut unasserted_and_unlisted = Vec::new();
    for (code, first_site) in &defined {
        if !asserted.contains(code) && !allowlist.contains_key(code.as_str()) {
            unasserted_and_unlisted
                .push(format!("  {code}  (defined in {})", first_site.display()));
        }
    }
    assert!(
        unasserted_and_unlisted.is_empty(),
        "error codes with no asserting test and no allowlist entry — add a test \
         asserting the code, or an ALLOWED_UNASSERTED entry naming the owning slice:\n{}",
        unasserted_and_unlisted.join("\n")
    );
}

#[test]
fn allowlist_entries_shrink_when_their_codes_gain_tests_or_die() {
    let (defined, asserted) = collect();

    let mut stale = Vec::new();
    for (code, reason) in ALLOWED_UNASSERTED {
        if asserted.contains(*code) {
            stale.push(format!(
                "  {code} is now asserted by a test — delete its allowlist entry ({reason})"
            ));
        } else if !defined.contains_key(*code) {
            stale.push(format!(
                "  {code} no longer exists in product sources — delete the dead entry ({reason})"
            ));
        }
    }
    assert!(
        stale.is_empty(),
        "shrink-only allowlist has stale entries:\n{}",
        stale.join("\n")
    );
}
