//! Source boundary guards for executor inference support.

#![cfg(feature = "inference")]

use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn executor_depends_on_inference_facade_as_optional_dependency() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = fs::read_to_string(manifest_dir.join("Cargo.toml")).expect("manifest");

    assert!(
        manifest.contains("inference = [\"dep:strata-inference\"]"),
        "inference feature must be the only base feature enabling the dependency"
    );
    assert!(
        manifest.contains("strata-inference = { path = \"../inference\", optional = true }"),
        "executor must depend on the optional inference facade and inherit default cloud providers"
    );
}

#[test]
fn executor_sources_do_not_import_inference_internals() {
    let mut files = Vec::new();
    collect_rs_files(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );

    let forbidden = [
        "provider::",
        "strata_inference::registry::",
        "llama::",
        "ffi",
        "CloudEmbeddingEngine",
        "GenerationEngine",
        "EmbeddingEngine",
        "RankingEngine",
        "api_key_env_var",
        "OPENAI_API_KEY",
        "ANTHROPIC_API_KEY",
        "GOOGLE_API_KEY",
    ];

    for file in files {
        let source = fs::read_to_string(&file).expect("source");
        for pattern in forbidden {
            assert!(
                !source.contains(pattern),
                "{} must not contain inference implementation detail {pattern}",
                file.display()
            );
        }
    }
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}
