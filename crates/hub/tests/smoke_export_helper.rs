//! Ignored helper: exports a bundle to files for manual CLI smoke runs.
//! Invoke explicitly: `cargo test -p strata-hub --test smoke_export_helper -- --ignored`

#[test]
#[ignore = "manual smoke helper, writes to STRATA_SMOKE_DIR"]
fn export_bundle_to_files() {
    let dir = std::path::PathBuf::from(std::env::var("STRATA_SMOKE_DIR").expect("dir set"));
    let mut engine = strata_hub::StrataCoreEngine::open(&dir.join("source")).expect("open");
    let output = engine
        .export_bundle(&strata_hub::EngineExportOptions::default())
        .expect("export");
    let bundle = dir.join("bundle");
    std::fs::create_dir_all(bundle.join("objects")).expect("mkdir");
    std::fs::write(
        bundle.join("manifest.bin"),
        &output.manifest_canonical_bytes,
    )
    .expect("write");
    let hash = strata_hub::stratahub_protocol::hash_bytes(&output.manifest_canonical_bytes);
    std::fs::write(bundle.join("hash.txt"), hash.as_str()).expect("write");
    for object in output.objects {
        std::fs::write(
            bundle.join("objects").join(object.hash.as_str()),
            &object.bytes,
        )
        .expect("write");
    }
}
