//! Hub-neutrality guard (resolution-config doc §5, coordination Q8):
//! no source file in the workspace may reference a specific hub host.
//! Strata carries **no default hub** — a fresh install refuses
//! hub-touching commands until the user configures a URL.

use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn no_crate_source_references_a_specific_hub_host() {
    let crates_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates dir")
        .to_owned();

    // Assembled at runtime so this guard's own source stays clean.
    let needle = format!("stratahub{}", ".io");
    let mut offenders = Vec::new();
    for file in rust_files(&crates_root) {
        let text = fs::read_to_string(&file).expect("read source");
        if text.contains(&needle) {
            offenders.push(file);
        }
    }
    assert!(
        offenders.is_empty(),
        "a specific hub host leaked into source (the resolver's refusal \
         message is the only sanctioned place to discuss hub configuration, \
         and it names no host): {offenders:?}"
    );
}

fn rust_files(root: &Path) -> Vec<PathBuf> {
    let mut pending = vec![root.to_owned()];
    let mut files = Vec::new();
    while let Some(dir) = pending.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            let name = entry.file_name();
            if path.is_dir() {
                if name != "target" && name != ".git" {
                    pending.push(path);
                }
            } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }
    files
}
