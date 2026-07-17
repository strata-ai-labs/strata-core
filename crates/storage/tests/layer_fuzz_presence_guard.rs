//! Layer-fuzz presence guard (TCP3.3, Phase 3 Tier-1 tracker).
//!
//! Every storage layer that decodes untrusted or recovered bytes must have at
//! least one fuzz target. The Phase 3 storage deep-dive found L2 (object
//! layout / id codec) was the one decoder layer with no fuzzer — a malformed
//! object name read back during a `list` could panic recovery, uncovered. This
//! guard turns "every decoder layer is fuzzed" into a CI invariant: a new
//! decoder layer, or a deleted target that leaves a layer bare, fails here.
//!
//! The guard is deliberately prefix-based and coarse — it does not verify a
//! target is *good*, only that the layer is represented. Depth is the fuzz
//! corpus's job; presence is this guard's.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Each decoder layer and the fuzz-target filename prefix that covers it. A
/// layer is covered when at least one `fuzz_targets/<prefix>*.rs` exists.
const LAYER_PREFIXES: &[(&str, &str)] = &[
    ("L2 object layout / id codec", "layout_"),
    ("L3 durable format / codec", "format_"),
    ("L4 log/manifest/snapshot services", "service_"),
    ("L5 table runtime", "table_runtime_"),
    ("L6 branch-isolated LSM runtime", "branch_lsm_"),
    ("L7 commit runtime", "commit_runtime_"),
    ("L8 lifecycle / recovery / maintenance", "lifecycle_"),
];

fn fuzz_targets_dir() -> PathBuf {
    // CARGO_MANIFEST_DIR = crates/storage.
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fuzz/fuzz_targets")
}

fn target_file_stems(dir: &Path) -> BTreeSet<String> {
    let mut stems = BTreeSet::new();
    let entries =
        std::fs::read_dir(dir).unwrap_or_else(|error| panic!("read {}: {error}", dir.display()));
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "rs") {
            if let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) {
                stems.insert(stem.to_owned());
            }
        }
    }
    stems
}

#[test]
fn every_decoder_layer_has_at_least_one_fuzz_target() {
    let stems = target_file_stems(&fuzz_targets_dir());
    assert!(
        stems.len() >= LAYER_PREFIXES.len(),
        "found only {} fuzz targets — the guard cannot be reading the right directory",
        stems.len()
    );

    let mut uncovered = Vec::new();
    for (layer, prefix) in LAYER_PREFIXES {
        if !stems.iter().any(|stem| stem.starts_with(prefix)) {
            uncovered.push(format!("  {layer}: no fuzz_targets/{prefix}*.rs"));
        }
    }
    assert!(
        uncovered.is_empty(),
        "decoder layers with no fuzz target (add one, or if the layer truly \
         decodes nothing, remove its row from LAYER_PREFIXES):\n{}",
        uncovered.join("\n")
    );
}

/// Every fuzz target must belong to a known layer prefix, so a target added
/// under a novel prefix forces a decision: either it fits an existing layer
/// (rename it) or it names a new decoder layer (add a row above). Without this
/// the guard would silently ignore whole categories of new targets.
#[test]
fn every_fuzz_target_maps_to_a_known_layer() {
    let stems = target_file_stems(&fuzz_targets_dir());
    let mut orphans = Vec::new();
    for stem in &stems {
        if !LAYER_PREFIXES
            .iter()
            .any(|(_, prefix)| stem.starts_with(prefix))
        {
            orphans.push(format!("  {stem}"));
        }
    }
    assert!(
        orphans.is_empty(),
        "fuzz targets whose prefix maps to no layer in LAYER_PREFIXES \
         (rename to an existing layer prefix, or add the new layer):\n{}",
        orphans.join("\n")
    );
}
