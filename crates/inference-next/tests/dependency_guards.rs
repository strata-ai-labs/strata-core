//! Source boundary guards for the inference runtime crate.

use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn inference_crate_does_not_depend_on_upper_layers() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = fs::read_to_string(manifest_dir.join("Cargo.toml")).expect("manifest");
    for forbidden in [
        "strata-engine",
        "strata-engine-next",
        "strata-storage",
        "strata-storage-next",
        "strata-intelligence",
        "strata-executor",
        "strata-executor-next",
    ] {
        assert!(
            !manifest.contains(forbidden),
            "inference manifest must not depend on {forbidden}"
        );
    }
}

#[test]
fn inference_sources_do_not_import_database_layers() {
    let mut files = Vec::new();
    collect_rs_files(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    let forbidden = [
        "strata_engine",
        "strata_engine_next",
        "strata_storage",
        "strata_storage_next",
        "strata_intelligence",
        "strata_executor",
        "strata_executor_next",
    ];
    for file in files {
        let source = fs::read_to_string(&file).expect("source");
        for pattern in forbidden {
            assert!(
                !source.contains(pattern),
                "{} must not contain {pattern}",
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
