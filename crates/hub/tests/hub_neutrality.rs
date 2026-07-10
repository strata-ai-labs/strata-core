//! Hub single-surface guard (the resolution-config §5/Q8 amendment):
//! the official hub host may appear in exactly one place in workspace
//! source — `DEFAULT_HUB_URL` in the resolver, strata-core's designated
//! defaults surface. Everything else (messages, docs strings, tests)
//! must reach it through the const so the default stays swappable and
//! auditable at a single site.

use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn the_hub_host_appears_only_in_the_designated_defaults_surface() {
    let crates_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates dir")
        .to_owned();
    let sanctioned = crates_root.join("hub/src/resolve.rs");

    // Assembled at runtime so this guard's own source stays clean.
    let needle = format!("stratahub{}", ".io");
    let mut offenders = Vec::new();
    let mut sanctioned_carries_it = false;
    for file in rust_files(&crates_root) {
        let text = fs::read_to_string(&file).expect("read source");
        if !text.contains(&needle) {
            continue;
        }
        if file == sanctioned {
            sanctioned_carries_it = true;
        } else {
            offenders.push(file);
        }
    }
    assert!(
        offenders.is_empty(),
        "the hub host leaked outside the designated defaults surface \
         (crates/hub/src/resolve.rs); reference DEFAULT_HUB_URL instead: \
         {offenders:?}"
    );
    assert!(
        sanctioned_carries_it,
        "DEFAULT_HUB_URL moved out of crates/hub/src/resolve.rs; update \
         this guard's sanctioned path so the single-surface rule keeps \
         tracking it"
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
