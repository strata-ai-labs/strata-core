//! CLI inference deterministic-verb behavior (TCP3.11c).
//!
//! The inference *compute* verbs (generate/embed/rank/tokenize) need a model
//! and a provider key, so they are not hermetic and are covered elsewhere. But
//! four inference verbs are pure functions of the static model catalog and
//! provider facts — `models list`, `models local`, `cache-status`, and
//! `capability` — and had no CLI integration coverage. Run under a temp `HOME`
//! (so no locally-downloaded model leaks in) they are fully deterministic.

#![deny(unsafe_code)]

use std::path::Path;
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

/// Runs `strata --db <db> --json inference <args>` under a hermetic HOME.
fn inference(home: &TempDir, db: &Path, args: &[&str]) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_strata"))
        .arg("--db")
        .arg(db)
        .arg("--json")
        .arg("inference")
        .args(args)
        .env("HOME", home.path())
        .env_remove("STRATA_HOME")
        .env_remove("STRATA_DB")
        .output()
        .expect("run strata binary");
    assert!(
        output.status.success(),
        "inference {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("stdout is JSON")
}

fn db(home: &TempDir) -> std::path::PathBuf {
    home.path().join("db")
}

/// Finds a catalog model by name in an `inference_models` page.
fn model<'a>(page: &'a Value, name: &str) -> &'a Value {
    page["data"]["items"]
        .as_array()
        .expect("items")
        .iter()
        .find(|m| m["name"] == name)
        .unwrap_or_else(|| panic!("model {name} not in catalog"))
}

#[test]
fn models_list_returns_the_static_catalog_with_nothing_local() {
    let home = tempfile::tempdir().expect("temp home");
    let page = inference(&home, &db(&home), &["models", "list"]);
    assert_eq!(page["type"], "inference_models");

    // The catalog is static: known models with fixed task/architecture facts.
    let minilm = model(&page, "miniLM");
    assert_eq!(minilm["task"], "embed");
    assert_eq!(minilm["architecture"], "bert");
    assert_eq!(minilm["embedding_dim"], 384);
    assert_eq!(model(&page, "tinyllama")["task"], "generate");
    assert_eq!(model(&page, "bge-m3")["task"], "embed");

    // Under a hermetic HOME nothing is downloaded, so nothing reports local.
    let any_local = page["data"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["is_local"] == true);
    assert!(!any_local, "a model reported is_local under a temp HOME");
}

#[test]
fn models_local_is_empty_without_downloads() {
    let home = tempfile::tempdir().expect("temp home");
    let page = inference(&home, &db(&home), &["models", "local"]);
    assert_eq!(page["type"], "inference_models");
    assert_eq!(page["data"]["items"], serde_json::json!([]));
}

#[test]
fn cache_status_reports_empty_pools_when_nothing_is_loaded() {
    let home = tempfile::tempdir().expect("temp home");
    let status = inference(&home, &db(&home), &["cache-status"]);
    assert_eq!(status["type"], "inference_cache_status");
    assert_eq!(status["data"]["embedding_models"], serde_json::json!([]));
    assert_eq!(status["data"]["generation_models"], serde_json::json!([]));
    assert_eq!(status["data"]["ranking_models"], serde_json::json!([]));
}

#[test]
fn capability_reports_static_facts_for_cloud_and_local_specs() {
    let home = tempfile::tempdir().expect("temp home");
    let db = db(&home);

    // A cloud spec: provider facts are static and need no key to inspect.
    let cloud = inference(&home, &db, &["capability", "openai:gpt-4o-mini"]);
    assert_eq!(cloud["type"], "inference_capability");
    assert_eq!(cloud["data"]["provider"], "openai");
    assert_eq!(cloud["data"]["model"], "gpt-4o-mini");
    assert_eq!(cloud["data"]["requires_api_key"], true);
    assert_eq!(cloud["data"]["requires_network"], true);
    assert_eq!(cloud["data"]["can_generate"], true);
    assert_eq!(cloud["data"]["supports_tools"], true);

    // A local embedding spec: no key, no network, tokenizes, fixed dimension.
    let local = inference(&home, &db, &["capability", "miniLM"]);
    assert_eq!(local["data"]["provider"], "local");
    assert_eq!(local["data"]["requires_api_key"], false);
    assert_eq!(local["data"]["requires_network"], false);
    assert_eq!(local["data"]["can_embed"], true);
    assert_eq!(local["data"]["can_tokenize"], true);
    assert_eq!(local["data"]["embedding_dim"], 384);
}
