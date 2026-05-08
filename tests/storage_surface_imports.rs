//! Ensure storage-facing types are imported from `strata_storage`.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const FORBIDDEN_DIRECT_PATTERNS: &[&str] = &[
    "strata_core::Storage",
    "strata_core::WriteMode",
    "strata_core::Key",
    "strata_core::Namespace",
    "strata_core::TypeTag",
    "strata_core::validate_space_name",
    "strata_core::traits::Storage",
    "strata_core::traits::WriteMode",
    "strata_core::types::Key",
    "strata_core::types::Namespace",
    "strata_core::types::TypeTag",
    "strata_concurrency::lock_ordering",
    "strata_concurrency::TransactionManager",
    "strata_durability::",
];

const ROOT_MOVED_TOKENS: &[&str] = &[
    "Storage",
    "WriteMode",
    "Key",
    "Namespace",
    "TypeTag",
    "validate_space_name",
];

const TYPES_MOVED_TOKENS: &[&str] = &["Key", "Namespace", "TypeTag"];
const CONCURRENCY_ROOT_MOVED_TOKENS: &[&str] = &[
    "TransactionContext",
    "TransactionStatus",
    "CommitError",
    "TransactionManager",
    "CoordinatorPlanError",
    "CoordinatorRecoveryError",
    "RecoveryCoordinator",
    "RecoveryPlan",
    "RecoveryResult",
    "RecoveryStats",
    "apply_wal_record_to_memory_storage",
];
const CONCURRENCY_TRANSACTION_MOVED_TOKENS: &[&str] = &[
    "ApplyResult",
    "CASOperation",
    "CommitError",
    "PendingOperations",
    "TransactionContext",
    "TransactionStatus",
];
const CONCURRENCY_VALIDATION_MOVED_TOKENS: &[&str] = &[
    "validate_cas_set",
    "validate_read_set",
    "validate_transaction",
    "ConflictType",
    "ValidationResult",
];
const CONCURRENCY_RECOVERY_MOVED_TOKENS: &[&str] = &[
    "CoordinatorPlanError",
    "CoordinatorRecoveryError",
    "RecoveryCoordinator",
    "RecoveryPlan",
    "RecoveryResult",
    "RecoveryStats",
    "apply_wal_record_to_memory_storage",
];

struct AllowedEngineConsolidationStorageUse {
    path: &'static str,
    removal_epic: &'static str,
}

#[derive(Debug)]
struct ManifestDependencyEntry {
    line_no: usize,
    text: String,
}

#[derive(Debug)]
struct ManifestFeatureEntry {
    line_no: usize,
    name: String,
    values: Vec<String>,
}

const ENGINE_CONSOLIDATION_PRODUCTION_SCAN_ROOTS: &[&str] = &[
    "src",
    "crates/executor",
    "crates/intelligence",
    "crates/cli",
];

const ENGINE_CONSOLIDATION_UPPER_PRODUCTION_SOURCE_ROOTS: &[&str] = &[
    "src",
    "crates/executor/src",
    "crates/intelligence/src",
    "crates/cli/src",
];

const ENGINE_CONSOLIDATION_VECTOR_CONSUMER_SCAN_ROOTS: &[&str] = &[
    "src",
    "benchmarks",
    "crates/executor",
    "crates/intelligence",
    "crates/cli",
];

const ALLOWED_ENGINE_CONSOLIDATION_STORAGE_USES: &[AllowedEngineConsolidationStorageUse] = &[];

const RETIRED_SECURITY_PACKAGE: &str = concat!("strata", "-", "security");
const RETIRED_SECURITY_IMPORT: &str = concat!("strata", "_", "security");
const RETIRED_SECURITY_RELATIVE_PATHS: &[&str] =
    &[concat!("../", "security"), concat!("crates/", "security")];
const RETIRED_EXECUTOR_LEGACY_PACKAGE: &str = concat!("strata", "-", "executor", "-", "legacy");
const RETIRED_EXECUTOR_LEGACY_IMPORT: &str = concat!("strata", "_", "executor", "_", "legacy");
const RETIRED_EXECUTOR_LEGACY_RELATIVE_PATHS: &[&str] = &[
    concat!("../", "executor", "-", "legacy"),
    concat!("crates/", "executor", "-", "legacy"),
];
const RETIRED_GRAPH_PACKAGE: &str = concat!("strata", "-", "graph");
const RETIRED_GRAPH_IMPORT: &str = concat!("strata", "_", "graph");
const RETIRED_GRAPH_RELATIVE_PATHS: &[&str] =
    &[concat!("../", "graph"), concat!("crates/", "graph")];
const RETIRED_VECTOR_PACKAGE: &str = concat!("strata", "-", "vector");
const RETIRED_VECTOR_IMPORT: &str = concat!("strata", "_", "vector");
const RETIRED_VECTOR_RELATIVE_PATHS: &[&str] =
    &[concat!("../", "vector"), concat!("crates/", "vector")];
const RETIRED_SEARCH_PACKAGE: &str = concat!("strata", "-", "search");
const RETIRED_SEARCH_IMPORT: &str = concat!("strata", "_", "search");
const RETIRED_SEARCH_RELATIVE_PATHS: &[&str] =
    &[concat!("../", "search"), concat!("crates/", "search")];
const RETIRED_CORE_LEGACY_PACKAGE: &str = concat!("strata", "-", "core", "-", "legacy");
const RETIRED_CORE_LEGACY_IMPORT: &str = concat!("strata", "_", "core", "_", "legacy");
const RETIRED_CONCURRENCY_PACKAGE: &str = concat!("strata", "-", "concurrency");
const RETIRED_CONCURRENCY_IMPORT: &str = concat!("strata", "_", "concurrency");
const RETIRED_DURABILITY_PACKAGE: &str = concat!("strata", "-", "durability");
const RETIRED_DURABILITY_IMPORT: &str = concat!("strata", "_", "durability");
const RETIRED_CRATE_DIRECTORIES: &[&str] = &[
    concat!("crates/", "security"),
    concat!("crates/", "executor", "-", "legacy"),
    concat!("crates/", "graph"),
    concat!("crates/", "vector"),
    concat!("crates/", "search"),
    concat!("crates/", "core", "-", "legacy"),
    concat!("crates/", "concurrency"),
    concat!("crates/", "durability"),
];
const RETIRED_CRATE_MARKERS: &[&str] = &[
    RETIRED_SECURITY_PACKAGE,
    RETIRED_SECURITY_IMPORT,
    concat!("../", "security"),
    concat!("crates/", "security"),
    RETIRED_EXECUTOR_LEGACY_PACKAGE,
    RETIRED_EXECUTOR_LEGACY_IMPORT,
    concat!("../", "executor", "-", "legacy"),
    concat!("crates/", "executor", "-", "legacy"),
    RETIRED_GRAPH_PACKAGE,
    RETIRED_GRAPH_IMPORT,
    concat!("../", "graph"),
    concat!("crates/", "graph"),
    RETIRED_VECTOR_PACKAGE,
    RETIRED_VECTOR_IMPORT,
    concat!("../", "vector"),
    concat!("crates/", "vector"),
    RETIRED_SEARCH_PACKAGE,
    RETIRED_SEARCH_IMPORT,
    concat!("../", "search"),
    concat!("crates/", "search"),
    RETIRED_CORE_LEGACY_PACKAGE,
    RETIRED_CORE_LEGACY_IMPORT,
    concat!("../", "core", "-", "legacy"),
    concat!("crates/", "core", "-", "legacy"),
    RETIRED_CONCURRENCY_PACKAGE,
    RETIRED_CONCURRENCY_IMPORT,
    concat!("../", "concurrency"),
    concat!("crates/", "concurrency"),
    RETIRED_DURABILITY_PACKAGE,
    RETIRED_DURABILITY_IMPORT,
    concat!("../", "durability"),
    concat!("crates/", "durability"),
];

const INTELLIGENCE_BOUNDARY_SCAN_ROOT: &str = "crates/intelligence";
const INTELLIGENCE_FORBIDDEN_DEPENDENCY_MARKERS: &[&str] = &[
    "strata-storage",
    "strata_storage",
    concat!("../", "storage"),
    concat!("crates/", "storage"),
    RETIRED_SECURITY_PACKAGE,
    RETIRED_SECURITY_IMPORT,
    concat!("../", "security"),
    concat!("crates/", "security"),
    RETIRED_EXECUTOR_LEGACY_PACKAGE,
    RETIRED_EXECUTOR_LEGACY_IMPORT,
    concat!("../", "executor", "-", "legacy"),
    concat!("crates/", "executor", "-", "legacy"),
    RETIRED_GRAPH_PACKAGE,
    RETIRED_GRAPH_IMPORT,
    concat!("../", "graph"),
    concat!("crates/", "graph"),
    RETIRED_VECTOR_PACKAGE,
    RETIRED_VECTOR_IMPORT,
    concat!("../", "vector"),
    concat!("crates/", "vector"),
    RETIRED_SEARCH_PACKAGE,
    RETIRED_SEARCH_IMPORT,
    concat!("../", "search"),
    concat!("crates/", "search"),
];
const INTELLIGENCE_FORBIDDEN_RUNTIME_MARKERS: &[&str] = &[
    "OpenSpec",
    "GraphSubsystem",
    "VectorSubsystem",
    "SearchSubsystem",
    ".with_subsystems",
    ".with_subsystem",
];
const ENGINE_FORBIDDEN_UPPER_LAYER_MARKERS: &[&str] = &[
    "strata-intelligence",
    "strata_intelligence",
    "strata-inference",
    "strata_inference",
];
const UPPER_FORBIDDEN_INFERENCE_MARKERS: &[&str] = &["strata-inference", "strata_inference"];
const DIRECT_INFERENCE_FEATURE_MARKERS: &[&str] = &["strata-inference", "dep:strata-inference"];
const UPPER_FORBIDDEN_RUNTIME_ASSEMBLY_MARKERS: &[&str] = &[
    "OpenSpec",
    "GraphSubsystem",
    "VectorSubsystem",
    "SearchSubsystem",
    ".with_subsystem",
    ".with_subsystems",
];
const UPPER_RUNTIME_ASSEMBLY_SCAN_ROOTS: &[&str] = &[
    "src",
    "crates/executor/src",
    "crates/intelligence/src",
    "crates/intelligence/tests",
    "crates/cli/src",
];

#[test]
fn storage_facing_types_are_not_imported_from_strata_core() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut rust_files = Vec::new();
    collect_rust_files(&repo_root.join("crates"), &mut rust_files);
    collect_rust_files(&repo_root.join("tests"), &mut rust_files);

    let mut violations = Vec::new();
    for file in rust_files {
        if should_skip(&file) {
            continue;
        }

        let contents = fs::read_to_string(&file).expect("read source file");
        for violation in find_violations(&contents) {
            violations.push(format!("{}:{violation}", file.display()));
        }
    }

    assert!(
        violations.is_empty(),
        "storage-facing types and the storage-owned transaction runtime must not be imported from legacy surfaces outside designated modules:\n{}",
        violations.join("\n")
    );
}

#[test]
fn engine_consolidation_security_surface_is_engine_owned() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = vec![repo_root.join("Cargo.toml")];
    collect_rust_files(&repo_root.join("src"), &mut files);
    collect_manifest_files(&repo_root.join("src"), &mut files);
    collect_rust_files(&repo_root.join("crates"), &mut files);
    collect_manifest_files(&repo_root.join("crates"), &mut files);

    let mut violations = Vec::new();
    for file in files {
        if should_skip(&file) || should_skip_manifest(&file) {
            continue;
        }

        let rel = repo_relative_path(&repo_root, &file);
        let contents = fs::read_to_string(&file).expect("read production file");
        violations.extend(find_retired_security_surface_violations(&rel, &contents));
    }

    assert!(
        violations.is_empty(),
        "security/open surface must be consumed from `strata_engine`; retired crate references found:\n{}",
        violations.join("\n")
    );
}

#[test]
fn engine_consolidation_legacy_executor_bootstrap_is_retired() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = vec![repo_root.join("Cargo.toml")];
    collect_rust_files(&repo_root.join("src"), &mut files);
    collect_manifest_files(&repo_root.join("src"), &mut files);
    collect_rust_files(&repo_root.join("crates"), &mut files);
    collect_manifest_files(&repo_root.join("crates"), &mut files);

    let mut violations = Vec::new();
    for file in files {
        if should_skip(&file) || should_skip_manifest(&file) {
            continue;
        }

        let rel = repo_relative_path(&repo_root, &file);
        let contents = fs::read_to_string(&file).expect("read production file");
        violations.extend(find_retired_executor_legacy_violations(&rel, &contents));
    }

    assert!(
        violations.is_empty(),
        "product open/bootstrap must be consumed from `strata_engine`; retired executor-legacy references found:\n{}",
        violations.join("\n")
    );
}

#[test]
fn engine_consolidation_graph_crate_is_retired() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let retired_dir = repo_root.join(RETIRED_GRAPH_RELATIVE_PATHS[1]);
    assert!(
        !retired_dir.exists(),
        "graph implementation must live in `strata_engine`; retired graph crate directory still exists at {}",
        retired_dir.display()
    );

    let mut files = vec![repo_root.join("Cargo.toml")];
    collect_rust_files(&repo_root.join("src"), &mut files);
    collect_manifest_files(&repo_root.join("src"), &mut files);
    collect_rust_files(&repo_root.join("crates"), &mut files);
    collect_manifest_files(&repo_root.join("crates"), &mut files);

    let mut violations = Vec::new();
    for file in files {
        if should_skip(&file) || should_skip_manifest(&file) {
            continue;
        }

        let rel = repo_relative_path(&repo_root, &file);
        let contents = fs::read_to_string(&file).expect("read production file");
        violations.extend(find_retired_graph_violations(&rel, &contents));
    }

    assert!(
        violations.is_empty(),
        "graph behavior must be consumed from `strata_engine`; retired graph crate references found:\n{}",
        violations.join("\n")
    );
}

#[test]
fn engine_consolidation_graph_subsystem_is_engine_product_owned() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for root in ENGINE_CONSOLIDATION_UPPER_PRODUCTION_SOURCE_ROOTS {
        collect_rust_files(&repo_root.join(root), &mut files);
    }

    let mut violations = Vec::new();
    for file in files {
        if should_skip(&file) {
            continue;
        }

        let rel = repo_relative_path(&repo_root, &file);
        if is_test_source_path(&rel) {
            continue;
        }

        let contents = fs::read_to_string(&file).expect("read production source file");
        violations.extend(find_upper_subsystem_violations(
            &rel,
            &contents,
            "GraphSubsystem",
        ));
    }

    assert!(
        violations.is_empty(),
        "production crates above engine must not assemble `GraphSubsystem`; graph product runtime registration is engine-owned:\n{}",
        violations.join("\n")
    );
}

#[test]
fn engine_consolidation_vector_subsystem_is_engine_product_owned() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for root in ENGINE_CONSOLIDATION_UPPER_PRODUCTION_SOURCE_ROOTS {
        collect_rust_files(&repo_root.join(root), &mut files);
    }

    let mut violations = Vec::new();
    for file in files {
        if should_skip(&file) {
            continue;
        }

        let rel = repo_relative_path(&repo_root, &file);
        if is_test_source_path(&rel) {
            continue;
        }

        let contents = fs::read_to_string(&file).expect("read production source file");
        violations.extend(find_upper_subsystem_violations(
            &rel,
            &contents,
            "VectorSubsystem",
        ));
    }

    assert!(
        violations.is_empty(),
        "production crates above engine must not assemble `VectorSubsystem`; vector product runtime registration is engine-owned:\n{}",
        violations.join("\n")
    );
}

#[test]
fn engine_consolidation_search_subsystem_is_engine_product_owned() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for root in ENGINE_CONSOLIDATION_UPPER_PRODUCTION_SOURCE_ROOTS {
        collect_rust_files(&repo_root.join(root), &mut files);
    }

    let mut violations = Vec::new();
    for file in files {
        if should_skip(&file) {
            continue;
        }

        let rel = repo_relative_path(&repo_root, &file);
        if is_test_source_path(&rel) {
            continue;
        }

        let contents = fs::read_to_string(&file).expect("read production source file");
        violations.extend(find_upper_subsystem_violations(
            &rel,
            &contents,
            "SearchSubsystem",
        ));
    }

    assert!(
        violations.is_empty(),
        "production crates above engine must not assemble `SearchSubsystem`; search product runtime registration is engine-owned:\n{}",
        violations.join("\n")
    );
}

#[test]
fn engine_consolidation_vector_crate_is_retired() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let retired_dir = repo_root.join(RETIRED_VECTOR_RELATIVE_PATHS[1]);
    assert!(
        !retired_dir.exists(),
        "vector implementation must live in `strata_engine`; retired vector crate directory still exists at {}",
        retired_dir.display()
    );

    let mut files = vec![repo_root.join("Cargo.toml")];
    collect_rust_files(&repo_root.join("src"), &mut files);
    collect_manifest_files(&repo_root.join("src"), &mut files);
    collect_rust_files(&repo_root.join("benchmarks"), &mut files);
    collect_manifest_files(&repo_root.join("benchmarks"), &mut files);
    collect_rust_files(&repo_root.join("crates"), &mut files);
    collect_manifest_files(&repo_root.join("crates"), &mut files);

    let mut violations = Vec::new();
    for file in files {
        if should_skip(&file) || should_skip_manifest(&file) {
            continue;
        }

        let rel = repo_relative_path(&repo_root, &file);
        let contents = fs::read_to_string(&file).expect("read production file");
        violations.extend(find_retired_vector_violations(&rel, &contents));
    }

    assert!(
        violations.is_empty(),
        "vector behavior must be consumed from `strata_engine`; retired vector crate references found:\n{}",
        violations.join("\n")
    );
}

#[test]
fn engine_consolidation_search_crate_is_retired() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let retired_dir = repo_root.join(RETIRED_SEARCH_RELATIVE_PATHS[1]);
    assert!(
        !retired_dir.exists(),
        "search implementation must live in `strata_engine`; retired search crate directory still exists at {}",
        retired_dir.display()
    );

    let mut files = vec![repo_root.join("Cargo.toml")];
    collect_rust_files(&repo_root.join("src"), &mut files);
    collect_manifest_files(&repo_root.join("src"), &mut files);
    collect_rust_files(&repo_root.join("benchmarks"), &mut files);
    collect_manifest_files(&repo_root.join("benchmarks"), &mut files);
    collect_rust_files(&repo_root.join("crates"), &mut files);
    collect_manifest_files(&repo_root.join("crates"), &mut files);

    let mut violations = Vec::new();
    for file in files {
        if should_skip(&file) || should_skip_manifest(&file) {
            continue;
        }

        let rel = repo_relative_path(&repo_root, &file);
        let contents = fs::read_to_string(&file).expect("read production file");
        violations.extend(find_retired_search_violations(&rel, &contents));
    }

    assert!(
        violations.is_empty(),
        "search behavior must be consumed from `strata_engine`; retired search crate references found:\n{}",
        violations.join("\n")
    );
}

#[test]
fn engine_consolidation_retired_crates_are_closed_out() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut violations = Vec::new();

    for retired_dir in RETIRED_CRATE_DIRECTORIES {
        let path = repo_root.join(retired_dir);
        if path.exists() {
            violations.push(format!(
                "{}: retired crate directory still exists",
                path.display()
            ));
        }
    }

    let mut files = vec![repo_root.join("Cargo.toml"), repo_root.join("Cargo.lock")];
    collect_rust_files(&repo_root.join("src"), &mut files);
    collect_manifest_files(&repo_root.join("src"), &mut files);
    collect_rust_files(&repo_root.join("benchmarks"), &mut files);
    collect_manifest_files(&repo_root.join("benchmarks"), &mut files);
    collect_rust_files(&repo_root.join("crates"), &mut files);
    collect_manifest_files(&repo_root.join("crates"), &mut files);

    for file in files {
        if should_skip(&file) || should_skip_manifest(&file) {
            continue;
        }

        let rel = repo_relative_path(&repo_root, &file);
        let contents =
            fs::read_to_string(&file).expect("read active source, manifest, or lockfile");
        violations.extend(find_retired_crate_marker_violations(&rel, &contents));
    }

    assert!(
        violations.is_empty(),
        "retired engine/storage consolidation crates must stay absent from active directories, manifests, lockfile, and production source:\n{}",
        violations.join("\n")
    );
}

#[test]
fn engine_consolidation_vector_consumers_use_engine_surface() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    files.push(repo_root.join("Cargo.toml"));
    for root in ENGINE_CONSOLIDATION_VECTOR_CONSUMER_SCAN_ROOTS {
        let path = repo_root.join(root);
        collect_rust_files(&path, &mut files);
        collect_manifest_files(&path, &mut files);
    }

    let mut violations = Vec::new();
    for file in files {
        if should_skip(&file) {
            continue;
        }

        let rel = repo_relative_path(&repo_root, &file);
        if is_test_source_path(&rel) {
            continue;
        }

        let contents = fs::read_to_string(&file).expect("read production source or manifest");
        for (line_no, line) in contents.lines().enumerate() {
            if line.contains("strata_vector") || line.contains("strata-vector") {
                violations.push(format!("{}:{}: {}", rel, line_no + 1, line.trim()));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "production crates above engine must consume vector surfaces from `strata_engine`, not the retired vector crate:\n{}",
        violations.join("\n")
    );
}

#[test]
fn engine_consolidation_executor_vector_transaction_surface_is_engine_owned() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    collect_rust_files(&repo_root.join("crates/executor/src"), &mut files);

    let mut violations = Vec::new();
    for file in files {
        if should_skip(&file) {
            continue;
        }

        let rel = repo_relative_path(&repo_root, &file);
        if is_test_source_path(&rel) {
            continue;
        }

        let contents = fs::read_to_string(&file).expect("read executor source file");
        for (line_no, line) in contents.lines().enumerate() {
            if line.contains("strata_vector")
                || line.contains("VectorStoreExt")
                || line.contains("StagedVectorOp")
                || line.contains("apply_staged_vector_op")
            {
                violations.push(format!("{}:{}: {}", rel, line_no + 1, line.trim()));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "executor production code must consume vector surfaces from `strata_engine`, not the retired vector crate or internal staged-op API:\n{}",
        violations.join("\n")
    );
}

#[test]
fn engine_consolidation_upper_layers_do_not_assemble_product_runtime() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for root in UPPER_RUNTIME_ASSEMBLY_SCAN_ROOTS {
        collect_rust_files(&repo_root.join(root), &mut files);
    }

    let mut violations = Vec::new();
    for file in files {
        if should_skip(&file) {
            continue;
        }

        let rel = repo_relative_path(&repo_root, &file);
        if is_test_source_path(&rel) && !rel.starts_with("crates/intelligence/tests/") {
            continue;
        }

        let contents = fs::read_to_string(&file).expect("read production source file");
        violations.extend(find_upper_runtime_assembly_violations(&rel, &contents));
    }

    assert!(
        violations.is_empty(),
        "production crates above engine must use engine-owned product open helpers, not `OpenSpec`/`Subsystem` runtime assembly:\n{}",
        violations.join("\n")
    );
}

#[test]
fn engine_consolidation_vector_storage_key_layout_is_engine_owned() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for root in ENGINE_CONSOLIDATION_UPPER_PRODUCTION_SOURCE_ROOTS {
        collect_rust_files(&repo_root.join(root), &mut files);
    }

    let mut violations = Vec::new();
    for file in files {
        if should_skip(&file) {
            continue;
        }

        let rel = repo_relative_path(&repo_root, &file);
        if is_test_source_path(&rel) {
            continue;
        }

        let contents = fs::read_to_string(&file).expect("read production source file");
        violations.extend(find_vector_storage_key_layout_violations(&rel, &contents));
    }

    assert!(
        violations.is_empty(),
        "production crates above engine must not construct vector storage key layout directly:\n{}",
        violations.join("\n")
    );
}

#[test]
fn storage_surface_manifests_do_not_regress_to_deleted_or_transitional_crates() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut manifests = vec![repo_root.join("Cargo.toml"), repo_root.join("Cargo.lock")];
    collect_manifest_files(&repo_root.join("benchmarks"), &mut manifests);
    collect_manifest_files(&repo_root.join("crates"), &mut manifests);

    let mut violations = Vec::new();
    for manifest in manifests {
        if should_skip_manifest(&manifest) {
            continue;
        }

        let contents = fs::read_to_string(&manifest).expect("read manifest");
        for violation in find_manifest_violations(&manifest, &contents) {
            violations.push(violation);
        }
    }

    assert!(
        violations.is_empty(),
        "workspace manifests must not regress to deleted or transitional crate edges outside designated seams:\n{}",
        violations.join("\n")
    );
}

#[test]
fn engine_consolidation_direct_storage_bypasses_are_allowlisted() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for root in ENGINE_CONSOLIDATION_PRODUCTION_SCAN_ROOTS {
        let path = repo_root.join(root);
        collect_rust_files(&path, &mut files);
        collect_manifest_files(&path, &mut files);
    }

    let mut violations = Vec::new();
    let mut observed = BTreeSet::new();
    let root_manifest = repo_root.join("Cargo.toml");
    let root_manifest_contents = fs::read_to_string(&root_manifest).expect("read root manifest");
    violations.extend(find_root_normal_storage_dependency_violations(
        &root_manifest,
        &root_manifest_contents,
    ));

    for file in files {
        if should_skip(&file) {
            continue;
        }
        let rel = repo_relative_path(&repo_root, &file);
        let contents = fs::read_to_string(&file).expect("read source or manifest");

        for (line_no, line) in find_direct_storage_references(&rel, &contents) {
            match allowed_engine_consolidation_storage_use(&rel) {
                Some(allowed) => {
                    observed.insert(allowed.path);
                }
                None => violations.push(format!(
                    "{}:{}: direct `strata-storage` use is not in the engine-consolidation allowlist: {}",
                    rel, line_no, line
                )),
            }
        }
    }

    let stale: Vec<_> = ALLOWED_ENGINE_CONSOLIDATION_STORAGE_USES
        .iter()
        .filter(|allowed| !observed.contains(allowed.path))
        .map(|allowed| format!("{} ({})", allowed.path, allowed.removal_epic))
        .collect();

    assert!(
        violations.is_empty(),
        "production crates above engine may only use `strata-storage` through explicitly allowlisted engine-consolidation seams:\n{}",
        violations.join("\n")
    );
    assert!(
        stale.is_empty(),
        "engine-consolidation storage bypass allowlist contains stale entries. Remove these entries when their epic lands:\n{}",
        stale.join("\n")
    );
}

#[test]
fn engine_consolidation_intelligence_boundary_is_engine_consuming() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let intelligence_root = repo_root.join(INTELLIGENCE_BOUNDARY_SCAN_ROOT);
    let mut files = Vec::new();
    collect_rust_files(&intelligence_root, &mut files);
    collect_manifest_files(&intelligence_root, &mut files);

    let mut violations = Vec::new();
    for file in files {
        if should_skip(&file) || should_skip_manifest(&file) {
            continue;
        }

        let rel = repo_relative_path(&repo_root, &file);
        let contents = fs::read_to_string(&file).expect("read intelligence boundary file");
        violations.extend(find_forbidden_marker_violations(
            &rel,
            &contents,
            INTELLIGENCE_FORBIDDEN_DEPENDENCY_MARKERS,
            "intelligence must consume engine surfaces instead of storage or retired runtime crates",
        ));
        if rel.ends_with(".rs") {
            violations.extend(find_forbidden_marker_violations(
                &rel,
                &contents,
                INTELLIGENCE_FORBIDDEN_RUNTIME_MARKERS,
                "intelligence source/tests must not assemble engine product subsystems directly",
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "intelligence must remain an engine consumer with no storage, retired runtime-crate, or subsystem-assembly bypasses:\n{}",
        violations.join("\n")
    );
}

#[test]
fn engine_consolidation_inference_boundary_stays_above_intelligence() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut engine_files = vec![repo_root.join("crates/engine/Cargo.toml")];
    collect_rust_files(&repo_root.join("crates/engine/src"), &mut engine_files);

    let mut upper_files = vec![
        repo_root.join("Cargo.toml"),
        repo_root.join("crates/executor/Cargo.toml"),
        repo_root.join("crates/cli/Cargo.toml"),
    ];
    collect_rust_files(&repo_root.join("src"), &mut upper_files);
    collect_rust_files(&repo_root.join("crates/executor/src"), &mut upper_files);
    collect_rust_files(&repo_root.join("crates/cli/src"), &mut upper_files);

    let mut violations = Vec::new();
    for file in engine_files {
        if should_skip(&file) || should_skip_manifest(&file) {
            continue;
        }

        let rel = repo_relative_path(&repo_root, &file);
        let contents = fs::read_to_string(&file).expect("read engine boundary file");
        violations.extend(find_forbidden_marker_violations(
            &rel,
            &contents,
            ENGINE_FORBIDDEN_UPPER_LAYER_MARKERS,
            "engine must not depend on intelligence or inference",
        ));
    }

    for file in upper_files {
        if should_skip(&file) || should_skip_manifest(&file) {
            continue;
        }

        let rel = repo_relative_path(&repo_root, &file);
        let contents = fs::read_to_string(&file).expect("read upper boundary file");
        violations.extend(find_forbidden_marker_violations(
            &rel,
            &contents,
            UPPER_FORBIDDEN_INFERENCE_MARKERS,
            "executor/CLI/root package must reach inference through intelligence only",
        ));
    }

    assert!(
        violations.is_empty(),
        "inference must remain optional behind `strata-intelligence`; lower or sibling crates may not import it directly:\n{}",
        violations.join("\n")
    );
}

#[test]
fn engine_consolidation_optional_embed_edges_are_policy_bound() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root_manifest = read_repo_manifest(&repo_root, "Cargo.toml");
    let cli_manifest = read_repo_manifest(&repo_root, "crates/cli/Cargo.toml");
    let executor_manifest = read_repo_manifest(&repo_root, "crates/executor/Cargo.toml");
    let intelligence_manifest = read_repo_manifest(&repo_root, "crates/intelligence/Cargo.toml");

    let mut violations = Vec::new();

    expect_feature_values(
        "Cargo.toml",
        &root_manifest,
        "embed",
        &["strata-executor/embed"],
        &mut violations,
    );
    for (provider, expected) in [
        ("anthropic", &["strata-executor/anthropic"][..]),
        ("openai", &["strata-executor/openai"][..]),
        ("google", &["strata-executor/google"][..]),
    ] {
        expect_feature_values(
            "Cargo.toml",
            &root_manifest,
            provider,
            expected,
            &mut violations,
        );
    }
    reject_feature_markers(
        "Cargo.toml",
        &root_manifest,
        DIRECT_INFERENCE_FEATURE_MARKERS,
        &mut violations,
    );
    reject_feature_markers(
        "Cargo.toml",
        &root_manifest,
        &["strata-intelligence", "dep:strata-intelligence"],
        &mut violations,
    );
    reject_normal_dependency(
        "Cargo.toml",
        &root_manifest,
        "strata-intelligence",
        "root package default graph must reach intelligence through executor only",
        &mut violations,
    );

    expect_feature_values(
        "crates/cli/Cargo.toml",
        &cli_manifest,
        "default",
        &[],
        &mut violations,
    );
    expect_feature_values(
        "crates/cli/Cargo.toml",
        &cli_manifest,
        "embed",
        &["strata-executor/embed", "dep:strata-intelligence"],
        &mut violations,
    );
    for (provider, expected) in [
        ("anthropic", &["embed", "strata-executor/anthropic"][..]),
        ("openai", &["embed", "strata-executor/openai"][..]),
        ("google", &["embed", "strata-executor/google"][..]),
    ] {
        expect_feature_values(
            "crates/cli/Cargo.toml",
            &cli_manifest,
            provider,
            expected,
            &mut violations,
        );
    }
    reject_feature_markers(
        "crates/cli/Cargo.toml",
        &cli_manifest,
        DIRECT_INFERENCE_FEATURE_MARKERS,
        &mut violations,
    );
    reject_feature_markers_outside_features(
        "crates/cli/Cargo.toml",
        &cli_manifest,
        &["dep:strata-intelligence", "strata-intelligence"],
        &["embed"],
        &mut violations,
    );
    expect_dependency_line_contains(
        "crates/cli/Cargo.toml",
        &cli_manifest,
        "strata-executor",
        &["path = \"../executor\""],
        &mut violations,
    );
    expect_dependency_line_contains(
        "crates/cli/Cargo.toml",
        &cli_manifest,
        "strata-intelligence",
        &[
            "path = \"../intelligence\"",
            "features = [\"embed\"]",
            "optional = true",
        ],
        &mut violations,
    );

    expect_feature_values(
        "crates/executor/Cargo.toml",
        &executor_manifest,
        "default",
        &[],
        &mut violations,
    );
    expect_feature_values(
        "crates/executor/Cargo.toml",
        &executor_manifest,
        "embed",
        &["strata-intelligence/embed", "strata-engine/embed"],
        &mut violations,
    );
    for (provider, expected) in [
        ("anthropic", &["embed", "strata-intelligence/anthropic"][..]),
        ("openai", &["embed", "strata-intelligence/openai"][..]),
        ("google", &["embed", "strata-intelligence/google"][..]),
    ] {
        expect_feature_values(
            "crates/executor/Cargo.toml",
            &executor_manifest,
            provider,
            expected,
            &mut violations,
        );
    }
    reject_feature_markers(
        "crates/executor/Cargo.toml",
        &executor_manifest,
        DIRECT_INFERENCE_FEATURE_MARKERS,
        &mut violations,
    );
    expect_dependency_line_contains(
        "crates/executor/Cargo.toml",
        &executor_manifest,
        "strata-intelligence",
        &["path = \"../intelligence\""],
        &mut violations,
    );
    reject_dependency_line_markers(
        "crates/executor/Cargo.toml",
        &executor_manifest,
        "strata-intelligence",
        &["optional = true"],
        &mut violations,
    );

    expect_feature_values(
        "crates/intelligence/Cargo.toml",
        &intelligence_manifest,
        "default",
        &[],
        &mut violations,
    );
    expect_feature_values(
        "crates/intelligence/Cargo.toml",
        &intelligence_manifest,
        "embed",
        &[
            "dep:strata-inference",
            "strata-inference/local",
            "strata-inference/download",
        ],
        &mut violations,
    );
    for (provider, expected) in [
        ("anthropic", &["embed", "strata-inference/anthropic"][..]),
        ("openai", &["embed", "strata-inference/openai"][..]),
        ("google", &["embed", "strata-inference/google"][..]),
    ] {
        expect_feature_values(
            "crates/intelligence/Cargo.toml",
            &intelligence_manifest,
            provider,
            expected,
            &mut violations,
        );
    }
    expect_dependency_line_contains(
        "crates/intelligence/Cargo.toml",
        &intelligence_manifest,
        "strata-inference",
        &[
            "path = \"../inference\"",
            "optional = true",
            "default-features = false",
        ],
        &mut violations,
    );

    assert!(
        violations.is_empty(),
        "optional embed/provider edges must keep inference behind intelligence and CLI's direct intelligence edge feature-gated:\n{}",
        violations.join("\n")
    );
}

fn collect_rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    if !dir.exists() {
        return;
    }

    for entry in fs::read_dir(dir).expect("read directory") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.is_dir() {
            collect_rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

fn collect_manifest_files(dir: &Path, out: &mut Vec<PathBuf>) {
    if !dir.exists() {
        return;
    }

    for entry in fs::read_dir(dir).expect("read directory") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.is_dir() {
            collect_manifest_files(&path, out);
        } else if path.file_name().is_some_and(|name| name == "Cargo.toml") {
            out.push(path);
        }
    }
}

fn should_skip(path: &Path) -> bool {
    let rel = path.to_string_lossy();
    rel.contains("/target/") || rel.ends_with("/tests/storage_surface_imports.rs")
}

fn should_skip_manifest(path: &Path) -> bool {
    let rel = path.to_string_lossy();
    rel.contains("/target/")
}

fn is_test_source_path(rel: &str) -> bool {
    rel == "tests" || rel.starts_with("tests/") || rel.contains("/tests/")
}

fn repo_relative_path(repo_root: &Path, path: &Path) -> String {
    path.strip_prefix(repo_root)
        .expect("path should be under repository root")
        .to_string_lossy()
        .replace('\\', "/")
}

fn read_repo_manifest(repo_root: &Path, rel: &str) -> String {
    fs::read_to_string(repo_root.join(rel)).expect("read cargo manifest")
}

fn contains_direct_storage_reference(line: &str) -> bool {
    line.contains("strata_storage")
        || line.contains("strata-storage")
        || line.contains("use strata_storage")
        || line.contains("pub use strata_storage")
}

fn find_direct_storage_references(rel: &str, contents: &str) -> Vec<(usize, String)> {
    if !rel.ends_with(".rs") {
        return contents
            .lines()
            .enumerate()
            .filter(|(_, line)| contains_direct_storage_reference(line))
            .map(|(line_no, line)| (line_no + 1, line.trim().to_string()))
            .collect();
    }

    let mut references = Vec::new();
    let mut pending_cfg_test = false;
    let mut skipped_cfg_test_depth = None;
    let mut lex_state = RustCodeLexState::default();

    for (line_no, line) in contents.lines().enumerate() {
        let code = rust_code_only_line(line, &mut lex_state);

        if let Some(depth) = skipped_cfg_test_depth.as_mut() {
            update_brace_depth(depth, &code);
            if *depth == 0 {
                skipped_cfg_test_depth = None;
            }
            continue;
        }

        let trimmed = code.trim();
        if is_cfg_test_attr(trimmed) {
            pending_cfg_test = true;
            continue;
        }

        if pending_cfg_test {
            if trimmed.is_empty() || trimmed.starts_with("#[") {
                continue;
            }

            if line_declares_inline_module(trimmed) {
                let mut depth = 0usize;
                update_brace_depth(&mut depth, &code);
                if depth > 0 {
                    skipped_cfg_test_depth = Some(depth);
                }
                pending_cfg_test = false;
                continue;
            }

            pending_cfg_test = false;
        }

        if contains_direct_storage_reference(&code) {
            references.push((line_no + 1, line.trim().to_string()));
        }
    }

    references
}

fn find_root_normal_storage_dependency_violations(path: &Path, contents: &str) -> Vec<String> {
    let rel = path.to_string_lossy();
    let mut violations = Vec::new();
    let mut section = String::new();

    for (line_no, line) in contents.lines().enumerate() {
        let code = toml_code_only_line(line);
        let trimmed = code.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(next_section) = parse_toml_section(trimmed) {
            section.clear();
            section.push_str(next_section);
            continue;
        }

        if is_normal_dependency_section(&section)
            && manifest_line_mentions_storage_dependency(trimmed)
        {
            violations.push(format!(
                "{}:{}: root package has a normal `strata-storage` dependency above engine: {}",
                rel,
                line_no + 1,
                line.trim()
            ));
        }
    }

    violations
}

fn parse_toml_section(trimmed: &str) -> Option<&str> {
    if trimmed.starts_with("[[") || !trimmed.starts_with('[') || !trimmed.ends_with(']') {
        return None;
    }

    trimmed
        .strip_prefix('[')
        .and_then(|section| section.strip_suffix(']'))
        .map(str::trim)
}

fn is_normal_dependency_section(section: &str) -> bool {
    section == "dependencies"
        || (section.starts_with("target.") && section.ends_with(".dependencies"))
}

fn normal_dependency_table_name(section: &str) -> Option<&str> {
    if let Some(name) = section.strip_prefix("dependencies.") {
        return Some(name);
    }

    if !section.starts_with("target.") {
        return None;
    }

    let marker = ".dependencies.";
    section
        .find(marker)
        .map(|index| &section[index + marker.len()..])
}

fn manifest_normal_dependency_entries(
    contents: &str,
    dependency: &str,
) -> Vec<ManifestDependencyEntry> {
    let lines = contents.lines().collect::<Vec<_>>();
    let mut section = String::new();
    let mut entries = Vec::new();
    let mut index = 0usize;

    while index < lines.len() {
        let line = lines[index];
        let code = toml_code_only_line(line);
        let trimmed = code.trim();
        if trimmed.is_empty() {
            index += 1;
            continue;
        }

        if let Some(next_section) = parse_toml_section(trimmed) {
            if let Some(table_name) = normal_dependency_table_name(next_section) {
                let start_line = index + 1;
                let mut text = trimmed.to_string();
                index += 1;
                while index < lines.len() {
                    let next_code = toml_code_only_line(lines[index]);
                    let next_trimmed = next_code.trim();
                    if parse_toml_section(next_trimmed).is_some() {
                        break;
                    }
                    if !next_trimmed.is_empty() {
                        text.push(' ');
                        text.push_str(next_trimmed);
                    }
                    index += 1;
                }
                if dependency_entry_targets_dependency(table_name, &text, dependency) {
                    entries.push(ManifestDependencyEntry {
                        line_no: start_line,
                        text,
                    });
                }
                continue;
            }

            section.clear();
            section.push_str(next_section);
            index += 1;
            continue;
        }

        if is_normal_dependency_section(&section) {
            if let Some(key) = toml_assignment_key(trimmed) {
                let start_line = index + 1;
                let (text, next_index) = collect_toml_assignment_text(&lines, index);
                if dependency_entry_targets_dependency(key, &text, dependency) {
                    entries.push(ManifestDependencyEntry {
                        line_no: start_line,
                        text,
                    });
                }
                index = next_index;
                continue;
            }
        }

        index += 1;
    }

    entries
}

fn dependency_entry_targets_dependency(key: &str, text: &str, dependency: &str) -> bool {
    toml_key_matches(key, dependency)
        || dependency_text_contains_marker(text, &format!("package = \"{dependency}\""))
        || dependency_text_contains_marker(text, &format!("package = '{dependency}'"))
}

fn toml_key_matches(key: &str, expected: &str) -> bool {
    let key = key.trim();
    key == expected
        || key
            .strip_prefix('"')
            .and_then(|key| key.strip_suffix('"'))
            .is_some_and(|key| key == expected)
        || key
            .strip_prefix('\'')
            .and_then(|key| key.strip_suffix('\''))
            .is_some_and(|key| key == expected)
}

fn collect_toml_assignment_text(lines: &[&str], start_index: usize) -> (String, usize) {
    let code = toml_code_only_line(lines[start_index]);
    let trimmed = code.trim();
    let mut text = trimmed.to_string();
    let mut balance = toml_collection_balance(trimmed);
    let mut index = start_index + 1;

    while balance > 0 && index < lines.len() {
        let next_code = toml_code_only_line(lines[index]);
        let next_trimmed = next_code.trim();
        if parse_toml_section(next_trimmed).is_some() {
            break;
        }
        if !next_trimmed.is_empty() {
            text.push(' ');
            text.push_str(next_trimmed);
            balance += toml_collection_balance(next_trimmed);
        }
        index += 1;
    }

    (text, index)
}

fn toml_collection_balance(line: &str) -> isize {
    let mut balance = 0isize;
    let mut in_string = false;
    let mut quote = '\0';
    let mut escaped = false;

    for ch in line.chars() {
        if in_string {
            if escaped {
                escaped = false;
                continue;
            }

            if quote == '"' && ch == '\\' {
                escaped = true;
            } else if ch == quote {
                in_string = false;
            }
            continue;
        }

        if ch == '"' || ch == '\'' {
            in_string = true;
            quote = ch;
            continue;
        }

        match ch {
            '{' | '[' => balance += 1,
            '}' | ']' => balance -= 1,
            _ => {}
        }
    }

    balance
}

fn manifest_dependency_text(contents: &str, dependency: &str) -> Option<String> {
    manifest_normal_dependency_entries(contents, dependency)
        .into_iter()
        .next()
        .map(|entry| entry.text)
}

fn manifest_feature_values(contents: &str, feature: &str) -> Option<Vec<String>> {
    manifest_feature_entries(contents)
        .into_iter()
        .find(|entry| entry.name == feature)
        .map(|entry| entry.values)
}

fn manifest_feature_entries(contents: &str) -> Vec<ManifestFeatureEntry> {
    let mut section = String::new();
    let mut pending_feature = None::<String>;
    let mut pending_line_no = 0usize;
    let mut pending_value = String::new();
    let mut entries = Vec::new();

    for (line_no, line) in contents.lines().enumerate() {
        let code = toml_code_only_line(line);
        let trimmed = code.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(next_section) = parse_toml_section(trimmed) {
            section.clear();
            section.push_str(next_section);
            pending_feature = None;
            pending_line_no = 0;
            pending_value.clear();
            continue;
        }

        if section != "features" {
            continue;
        }

        if let Some(pending) = pending_feature.as_ref() {
            pending_value.push(' ');
            pending_value.push_str(trimmed);
            if trimmed.contains(']') {
                let values = parse_toml_string_array(&pending_value);
                entries.push(ManifestFeatureEntry {
                    line_no: pending_line_no,
                    name: pending.clone(),
                    values,
                });
                pending_feature = None;
                pending_line_no = 0;
                pending_value.clear();
            }
            continue;
        }

        let Some((key, value)) = toml_assignment(trimmed) else {
            continue;
        };
        if !value.contains('[') {
            continue;
        }

        if value.contains(']') {
            entries.push(ManifestFeatureEntry {
                line_no: line_no + 1,
                name: key.to_string(),
                values: parse_toml_string_array(value),
            });
            continue;
        }

        pending_feature = Some(key.to_string());
        pending_line_no = line_no + 1;
        pending_value.clear();
        pending_value.push_str(value);
    }

    entries
}

fn toml_assignment(trimmed: &str) -> Option<(&str, &str)> {
    let (key, value) = trimmed.split_once('=')?;
    Some((key.trim(), value.trim()))
}

fn toml_assignment_key(trimmed: &str) -> Option<&str> {
    toml_assignment(trimmed).map(|(key, _)| key)
}

fn parse_toml_string_array(value: &str) -> Vec<String> {
    let array = value
        .split_once('[')
        .and_then(|(_, rest)| rest.rsplit_once(']').map(|(body, _)| body))
        .unwrap_or(value);
    let mut values = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut quote = '\0';
    let mut escaped = false;

    for ch in array.chars() {
        if in_string {
            if escaped {
                current.push(ch);
                escaped = false;
                continue;
            }

            if quote == '"' && ch == '\\' {
                escaped = true;
                continue;
            }

            if ch == quote {
                values.push(std::mem::take(&mut current));
                in_string = false;
                continue;
            }

            current.push(ch);
            continue;
        }

        if ch == '"' || ch == '\'' {
            quote = ch;
            in_string = true;
        }
    }

    values
}

fn expect_feature_values(
    rel: &str,
    contents: &str,
    feature: &str,
    expected: &[&str],
    violations: &mut Vec<String>,
) {
    match manifest_feature_values(contents, feature) {
        Some(actual) if feature_values_match(&actual, expected) => {}
        Some(actual) => violations.push(format!(
            "{}: feature `{}` expected {:?}, found {:?}",
            rel, feature, expected, actual
        )),
        None => violations.push(format!("{}: missing feature `{}`", rel, feature)),
    }
}

fn feature_values_match(actual: &[String], expected: &[&str]) -> bool {
    let actual = actual.iter().map(String::as_str).collect::<BTreeSet<_>>();
    let expected = expected.iter().copied().collect::<BTreeSet<_>>();
    actual == expected
}

fn reject_feature_markers(
    rel: &str,
    contents: &str,
    forbidden: &[&str],
    violations: &mut Vec<String>,
) {
    let mut section = String::new();

    for (line_no, line) in contents.lines().enumerate() {
        let code = toml_code_only_line(line);
        let trimmed = code.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(next_section) = parse_toml_section(trimmed) {
            section.clear();
            section.push_str(next_section);
            continue;
        }

        if section == "features" {
            for marker in forbidden {
                if trimmed.contains(marker) {
                    violations.push(format!(
                        "{}:{}: feature value must not reference `{}` directly: {}",
                        rel,
                        line_no + 1,
                        marker,
                        line.trim()
                    ));
                }
            }
        }
    }
}

fn reject_feature_markers_outside_features(
    rel: &str,
    contents: &str,
    forbidden: &[&str],
    allowed_features: &[&str],
    violations: &mut Vec<String>,
) {
    let allowed_features = allowed_features.iter().copied().collect::<BTreeSet<_>>();

    for entry in manifest_feature_entries(contents) {
        if allowed_features.contains(entry.name.as_str()) {
            continue;
        }

        for value in &entry.values {
            if let Some(marker) = forbidden.iter().find(|marker| value.contains(**marker)) {
                violations.push(format!(
                    "{}:{}: feature `{}` must not reference `{}` directly: {:?}",
                    rel, entry.line_no, entry.name, marker, entry.values
                ));
            }
        }
    }
}

fn reject_normal_dependency(
    rel: &str,
    contents: &str,
    dependency: &str,
    reason: &str,
    violations: &mut Vec<String>,
) {
    for entry in manifest_normal_dependency_entries(contents, dependency) {
        violations.push(format!(
            "{}:{}: {}: {}",
            rel, entry.line_no, reason, entry.text
        ));
    }
}

fn expect_dependency_line_contains(
    rel: &str,
    contents: &str,
    dependency: &str,
    expected_markers: &[&str],
    violations: &mut Vec<String>,
) {
    let Some(text) = manifest_dependency_text(contents, dependency) else {
        violations.push(format!(
            "{}: missing normal dependency `{}`",
            rel, dependency
        ));
        return;
    };

    for marker in expected_markers {
        if !dependency_text_contains_marker(&text, marker) {
            violations.push(format!(
                "{}: dependency `{}` must contain `{}`: {}",
                rel, dependency, marker, text
            ));
        }
    }
}

fn reject_dependency_line_markers(
    rel: &str,
    contents: &str,
    dependency: &str,
    forbidden_markers: &[&str],
    violations: &mut Vec<String>,
) {
    let Some(text) = manifest_dependency_text(contents, dependency) else {
        violations.push(format!(
            "{}: missing normal dependency `{}`",
            rel, dependency
        ));
        return;
    };

    for marker in forbidden_markers {
        if dependency_text_contains_marker(&text, marker) {
            violations.push(format!(
                "{}: dependency `{}` must not contain `{}`: {}",
                rel, dependency, marker, text
            ));
        }
    }
}

fn dependency_text_contains_marker(text: &str, marker: &str) -> bool {
    normalize_toml_dependency_text(text).contains(&normalize_toml_dependency_text(marker))
}

fn normalize_toml_dependency_text(text: &str) -> String {
    text.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn manifest_line_mentions_storage_dependency(trimmed: &str) -> bool {
    trimmed.contains("strata-storage")
        || trimmed.contains("package = \"strata-storage\"")
        || trimmed.contains("package='strata-storage'")
}

fn toml_code_only_line(line: &str) -> String {
    let mut code = String::with_capacity(line.len());
    let mut in_string = false;
    let mut quote = '\0';
    let mut escaped = false;

    for ch in line.chars() {
        if in_string {
            code.push(ch);
            if escaped {
                escaped = false;
                continue;
            }

            if quote == '"' && ch == '\\' {
                escaped = true;
            } else if ch == quote {
                in_string = false;
            }
            continue;
        }

        if ch == '#' {
            break;
        }

        if ch == '"' || ch == '\'' {
            in_string = true;
            quote = ch;
        }

        code.push(ch);
    }

    code
}

fn find_retired_security_surface_violations(rel: &str, contents: &str) -> Vec<String> {
    let mut violations = Vec::new();

    for (line_no, line) in contents.lines().enumerate() {
        if line.contains(RETIRED_SECURITY_PACKAGE)
            || line.contains(RETIRED_SECURITY_IMPORT)
            || RETIRED_SECURITY_RELATIVE_PATHS
                .iter()
                .any(|path| line.contains(path))
        {
            violations.push(format!(
                "{}:{}: retired security/open crate reference: {}",
                rel,
                line_no + 1,
                line.trim()
            ));
        }
    }

    violations
}

fn find_retired_executor_legacy_violations(rel: &str, contents: &str) -> Vec<String> {
    let mut violations = Vec::new();

    for (line_no, line) in contents.lines().enumerate() {
        if line.contains(RETIRED_EXECUTOR_LEGACY_PACKAGE)
            || line.contains(RETIRED_EXECUTOR_LEGACY_IMPORT)
            || RETIRED_EXECUTOR_LEGACY_RELATIVE_PATHS
                .iter()
                .any(|path| line.contains(path))
        {
            violations.push(format!(
                "{}:{}: retired executor-legacy bootstrap reference: {}",
                rel,
                line_no + 1,
                line.trim()
            ));
        }
    }

    violations
}

fn find_retired_graph_violations(rel: &str, contents: &str) -> Vec<String> {
    let mut violations = Vec::new();

    for (line_no, line) in contents.lines().enumerate() {
        if line.contains(RETIRED_GRAPH_PACKAGE)
            || line.contains(RETIRED_GRAPH_IMPORT)
            || RETIRED_GRAPH_RELATIVE_PATHS
                .iter()
                .any(|path| line.contains(path))
        {
            violations.push(format!(
                "{}:{}: retired graph crate reference: {}",
                rel,
                line_no + 1,
                line.trim()
            ));
        }
    }

    violations
}

fn find_retired_vector_violations(rel: &str, contents: &str) -> Vec<String> {
    let mut violations = Vec::new();

    for (line_no, line) in contents.lines().enumerate() {
        if line.contains(RETIRED_VECTOR_PACKAGE)
            || line.contains(RETIRED_VECTOR_IMPORT)
            || RETIRED_VECTOR_RELATIVE_PATHS
                .iter()
                .any(|path| line.contains(path))
        {
            violations.push(format!(
                "{}:{}: retired vector crate reference: {}",
                rel,
                line_no + 1,
                line.trim()
            ));
        }
    }

    violations
}

fn find_retired_search_violations(rel: &str, contents: &str) -> Vec<String> {
    let mut violations = Vec::new();

    for (line_no, line) in contents.lines().enumerate() {
        if line.contains(RETIRED_SEARCH_PACKAGE)
            || line.contains(RETIRED_SEARCH_IMPORT)
            || RETIRED_SEARCH_RELATIVE_PATHS
                .iter()
                .any(|path| line.contains(path))
        {
            violations.push(format!(
                "{}:{}: retired search crate reference: {}",
                rel,
                line_no + 1,
                line.trim()
            ));
        }
    }

    violations
}

fn find_forbidden_marker_violations(
    rel: &str,
    contents: &str,
    markers: &[&str],
    reason: &str,
) -> Vec<String> {
    let mut violations = Vec::new();

    if rel.ends_with(".rs") {
        let mut lex_state = RustCodeLexState::default();
        for (line_no, line) in contents.lines().enumerate() {
            let code = rust_code_only_line(line, &mut lex_state);
            if let Some(marker) = markers
                .iter()
                .find(|marker| rust_code_contains_marker(&code, marker))
            {
                violations.push(format!(
                    "{}:{}: {} (`{}`): {}",
                    rel,
                    line_no + 1,
                    reason,
                    marker,
                    line.trim()
                ));
            }
        }
        return violations;
    }

    for (line_no, line) in contents.lines().enumerate() {
        let code = toml_code_only_line(line);
        if let Some(marker) = markers.iter().find(|marker| code.contains(**marker)) {
            violations.push(format!(
                "{}:{}: {} (`{}`): {}",
                rel,
                line_no + 1,
                reason,
                marker,
                line.trim()
            ));
        }
    }

    violations
}

fn find_retired_crate_marker_violations(rel: &str, contents: &str) -> Vec<String> {
    let mut violations = Vec::new();

    if rel.ends_with(".rs") {
        let mut lex_state = RustCodeLexState::default();
        for (line_no, line) in contents.lines().enumerate() {
            let code = rust_code_only_line(line, &mut lex_state);
            if let Some(marker) = RETIRED_CRATE_MARKERS
                .iter()
                .find(|marker| rust_code_contains_marker(&code, marker))
            {
                violations.push(format!(
                    "{}:{}: active Rust source references retired crate marker `{}`: {}",
                    rel,
                    line_no + 1,
                    marker,
                    line.trim()
                ));
            }
        }
        return violations;
    }

    for (line_no, line) in contents.lines().enumerate() {
        let code = toml_code_only_line(line);
        if let Some(marker) = RETIRED_CRATE_MARKERS
            .iter()
            .find(|marker| text_contains_marker_token(&code, marker))
        {
            violations.push(format!(
                "{}:{}: active manifest/lockfile references retired crate marker `{}`: {}",
                rel,
                line_no + 1,
                marker,
                line.trim()
            ));
        }
    }

    violations
}

fn rust_code_contains_marker(code: &str, marker: &str) -> bool {
    if marker.starts_with('.') {
        return contains_method_marker(code, marker);
    }

    if marker
        .chars()
        .any(|ch| !(ch == '_' || ch.is_ascii_alphanumeric()))
    {
        return code.contains(marker);
    }

    contains_rust_identifier_token(code, marker)
}

fn text_contains_marker_token(text: &str, marker: &str) -> bool {
    let mut offset = 0usize;
    while let Some(relative) = text[offset..].find(marker) {
        let start = offset + relative;
        let end = start + marker.len();
        let before = text[..start].chars().next_back();
        let after = text[end..].chars().next();

        let before_is_boundary = before.is_none_or(|ch| !is_manifest_marker_continue(ch));
        let after_is_boundary = after.is_none_or(|ch| !is_manifest_marker_continue(ch));
        if before_is_boundary && after_is_boundary {
            return true;
        }

        offset = end;
    }

    false
}

fn is_manifest_marker_continue(ch: char) -> bool {
    ch == '-' || ch == '_' || ch.is_ascii_alphanumeric()
}

fn contains_method_marker(code: &str, marker: &str) -> bool {
    let mut offset = 0usize;
    while let Some(relative) = code[offset..].find(marker) {
        let start = offset + relative;
        let end = start + marker.len();
        let after = code[end..].chars().next();
        if after.is_none_or(|ch| !is_rust_identifier_continue(ch)) {
            return true;
        }

        offset = end;
    }

    false
}

fn contains_rust_identifier_token(code: &str, token: &str) -> bool {
    let mut offset = 0usize;
    while let Some(relative) = code[offset..].find(token) {
        let start = offset + relative;
        let end = start + token.len();
        let before = code[..start].chars().next_back();
        let after = code[end..].chars().next();

        let before_is_boundary = before.is_none_or(|ch| !is_rust_identifier_continue(ch));
        let after_is_boundary = after.is_none_or(|ch| !is_rust_identifier_continue(ch));
        if before_is_boundary && after_is_boundary {
            return true;
        }

        offset = end;
    }

    false
}

const VECTOR_STORAGE_KEY_LAYOUT_MARKERS: &[&str] = &[
    "TypeTag::Vector",
    "Key::new_vector",
    "Key::new_vector_config",
    "Key::vector_collection_prefix",
    "new_vector_config_prefix",
];

fn find_vector_storage_key_layout_violations(rel: &str, contents: &str) -> Vec<String> {
    let mut violations = Vec::new();
    let mut pending_cfg_test = false;
    let mut skipped_cfg_test_depth = None;
    let mut lex_state = RustCodeLexState::default();

    for (line_no, line) in contents.lines().enumerate() {
        let code = rust_code_only_line(line, &mut lex_state);

        if let Some(depth) = skipped_cfg_test_depth.as_mut() {
            update_brace_depth(depth, &code);
            if *depth == 0 {
                skipped_cfg_test_depth = None;
            }
            continue;
        }

        let trimmed = code.trim();
        if is_cfg_test_attr(trimmed) {
            pending_cfg_test = true;
            continue;
        }

        if pending_cfg_test {
            if trimmed.is_empty() || trimmed.starts_with("#[") {
                continue;
            }

            if line_declares_inline_module(trimmed) {
                let mut depth = 0usize;
                update_brace_depth(&mut depth, &code);
                if depth > 0 {
                    skipped_cfg_test_depth = Some(depth);
                }
                pending_cfg_test = false;
                continue;
            }

            pending_cfg_test = false;
        }

        if VECTOR_STORAGE_KEY_LAYOUT_MARKERS
            .iter()
            .any(|marker| code.contains(marker))
        {
            violations.push(format!("{}:{}: {}", rel, line_no + 1, line.trim()));
        }
    }

    violations
}

fn find_upper_subsystem_violations(rel: &str, contents: &str, subsystem_name: &str) -> Vec<String> {
    let mut violations = Vec::new();
    let mut pending_cfg_test = false;
    let mut skipped_cfg_test_depth = None;
    let mut lex_state = RustCodeLexState::default();

    for (line_no, line) in contents.lines().enumerate() {
        let code = rust_code_only_line(line, &mut lex_state);

        if let Some(depth) = skipped_cfg_test_depth.as_mut() {
            update_brace_depth(depth, &code);
            if *depth == 0 {
                skipped_cfg_test_depth = None;
            }
            continue;
        }

        let trimmed = code.trim();
        if is_cfg_test_attr(trimmed) {
            pending_cfg_test = true;
            continue;
        }

        if pending_cfg_test {
            if trimmed.is_empty() || trimmed.starts_with("#[") {
                continue;
            }

            if line_declares_inline_module(trimmed) {
                let mut depth = 0usize;
                update_brace_depth(&mut depth, &code);
                if depth > 0 {
                    skipped_cfg_test_depth = Some(depth);
                }
                pending_cfg_test = false;
                continue;
            }

            pending_cfg_test = false;
        }

        if code.contains(subsystem_name) {
            violations.push(format!(
                "{}:{}: production upper crate names `{}`: {}",
                rel,
                line_no + 1,
                subsystem_name,
                line.trim()
            ));
        }
    }

    violations
}

fn find_upper_runtime_assembly_violations(rel: &str, contents: &str) -> Vec<String> {
    let mut violations = Vec::new();
    let mut pending_cfg_test = false;
    let mut skipped_cfg_test_depth = None;
    let mut lex_state = RustCodeLexState::default();

    for (line_no, line) in contents.lines().enumerate() {
        let code = rust_code_only_line(line, &mut lex_state);

        if let Some(depth) = skipped_cfg_test_depth.as_mut() {
            update_brace_depth(depth, &code);
            if *depth == 0 {
                skipped_cfg_test_depth = None;
            }
            continue;
        }

        let trimmed = code.trim();
        if is_cfg_test_attr(trimmed) {
            pending_cfg_test = true;
            continue;
        }

        if pending_cfg_test {
            if trimmed.is_empty() || trimmed.starts_with("#[") {
                continue;
            }

            if line_declares_inline_module(trimmed) {
                let mut depth = 0usize;
                update_brace_depth(&mut depth, &code);
                if depth > 0 {
                    skipped_cfg_test_depth = Some(depth);
                }
                pending_cfg_test = false;
                continue;
            }

            pending_cfg_test = false;
        }

        if let Some(marker) = find_upper_runtime_assembly_marker(&code) {
            violations.push(format!(
                "{}:{}: production upper crate assembles engine runtime via `{}`: {}",
                rel,
                line_no + 1,
                marker,
                line.trim()
            ));
        }
    }

    violations
}

fn find_upper_runtime_assembly_marker(code: &str) -> Option<&'static str> {
    if let Some(marker) = UPPER_FORBIDDEN_RUNTIME_ASSEMBLY_MARKERS
        .iter()
        .find(|marker| rust_code_contains_marker(code, marker))
    {
        return Some(marker);
    }

    if contains_method_call_token(code, "with_subsystem") {
        return Some("with_subsystem");
    }

    if contains_method_call_token(code, "with_subsystems") {
        return Some("with_subsystems");
    }

    if contains_dyn_subsystem_trait(code) {
        return Some("dyn Subsystem");
    }

    if contains_engine_subsystem_trait_path(code) {
        return Some("recovery::Subsystem");
    }

    None
}

fn contains_method_call_token(code: &str, method: &str) -> bool {
    let mut offset = 0usize;
    while let Some(relative) = code[offset..].find(method) {
        let start = offset + relative;
        let end = start + method.len();
        let before = code[..start].chars().next_back();
        let after = code[end..].chars().next();
        let before_is_boundary = before.is_none_or(|ch| !is_rust_identifier_continue(ch));
        let after_is_boundary = after.is_none_or(|ch| !is_rust_identifier_continue(ch));
        if before_is_boundary && after_is_boundary && has_dot_before_method(code, start) {
            return true;
        }

        offset = end;
    }

    false
}

fn has_dot_before_method(code: &str, method_start: usize) -> bool {
    code[..method_start]
        .chars()
        .rev()
        .find(|ch| !ch.is_whitespace())
        .is_some_and(|ch| ch == '.')
}

fn contains_dyn_subsystem_trait(code: &str) -> bool {
    let tokens = code
        .split(|ch: char| !(ch == '_' || ch.is_ascii_alphanumeric()))
        .filter(|token| !token.is_empty());

    let mut previous_was_dyn = false;
    for token in tokens {
        if previous_was_dyn && token == "Subsystem" {
            return true;
        }

        previous_was_dyn = token == "dyn";
    }

    false
}

fn contains_engine_subsystem_trait_path(code: &str) -> bool {
    let tokens = code
        .split(|ch: char| !(ch == '_' || ch.is_ascii_alphanumeric()))
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();

    tokens.contains(&"strata_engine")
        && tokens
            .windows(2)
            .any(|window| window == ["recovery", "Subsystem"])
}

fn is_cfg_test_attr(trimmed_code: &str) -> bool {
    let compact = trimmed_code
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>();

    compact == "#[cfg(test)]"
}

fn line_declares_inline_module(trimmed_code: &str) -> bool {
    if !trimmed_code.contains('{') {
        return false;
    }

    let before_brace = trimmed_code.split('{').next().unwrap_or_default();
    let mut saw_mod = false;
    for token in before_brace
        .split(|ch: char| !is_rust_identifier_continue(ch))
        .filter(|token| !token.is_empty())
    {
        if saw_mod {
            return true;
        }
        saw_mod = token == "mod";
    }

    false
}

fn update_brace_depth(depth: &mut usize, line: &str) {
    for ch in line.chars() {
        match ch {
            '{' => *depth += 1,
            '}' => *depth = depth.saturating_sub(1),
            _ => {}
        }
    }
}

#[derive(Default)]
struct RustCodeLexState {
    mode: RustCodeLexMode,
}

#[derive(Default)]
enum RustCodeLexMode {
    #[default]
    Code,
    BlockComment {
        depth: usize,
    },
    CookedLiteral {
        delimiter: u8,
        escaped: bool,
    },
    RawString {
        hashes: usize,
    },
}

fn rust_code_only_line(line: &str, state: &mut RustCodeLexState) -> String {
    let bytes = line.as_bytes();
    let mut code = String::with_capacity(line.len());
    let mut index = 0;

    while index < bytes.len() {
        match &mut state.mode {
            RustCodeLexMode::Code => {
                if starts_with(bytes, index, b"//") {
                    push_spaces_for_remaining(&mut code, bytes, index);
                    break;
                }

                if starts_with(bytes, index, b"/*") {
                    state.mode = RustCodeLexMode::BlockComment { depth: 1 };
                    push_spaces(&mut code, 2);
                    index += 2;
                    continue;
                }

                if let Some((literal_len, hashes)) = raw_string_start(bytes, index) {
                    state.mode = RustCodeLexMode::RawString { hashes };
                    push_spaces(&mut code, literal_len);
                    index += literal_len;
                    continue;
                }

                if bytes[index] == b'"'
                    || (bytes[index] == b'\'' && !looks_like_lifetime(bytes, index))
                {
                    state.mode = RustCodeLexMode::CookedLiteral {
                        delimiter: bytes[index],
                        escaped: false,
                    };
                    code.push(' ');
                    index += 1;
                    continue;
                }

                code.push(bytes[index] as char);
                index += 1;
            }
            RustCodeLexMode::BlockComment { depth } => {
                if starts_with(bytes, index, b"/*") {
                    *depth += 1;
                    push_spaces(&mut code, 2);
                    index += 2;
                    continue;
                }

                if starts_with(bytes, index, b"*/") {
                    *depth = depth.saturating_sub(1);
                    push_spaces(&mut code, 2);
                    index += 2;
                    if *depth == 0 {
                        state.mode = RustCodeLexMode::Code;
                    }
                    continue;
                }

                code.push(' ');
                index += 1;
            }
            RustCodeLexMode::CookedLiteral { delimiter, escaped } => {
                code.push(' ');
                if *escaped {
                    *escaped = false;
                } else if bytes[index] == b'\\' {
                    *escaped = true;
                } else if bytes[index] == *delimiter {
                    state.mode = RustCodeLexMode::Code;
                }
                index += 1;
            }
            RustCodeLexMode::RawString { hashes } => {
                if raw_string_ends_at(bytes, index, *hashes) {
                    push_spaces(&mut code, 1 + *hashes);
                    index += 1 + *hashes;
                    state.mode = RustCodeLexMode::Code;
                    continue;
                }

                code.push(' ');
                index += 1;
            }
        }
    }

    code
}

fn raw_string_start(bytes: &[u8], index: usize) -> Option<(usize, usize)> {
    let mut cursor = match bytes.get(index) {
        Some(b'r') => index + 1,
        Some(b'b') if bytes.get(index + 1) == Some(&b'r') => index + 2,
        _ => return None,
    };

    while bytes.get(cursor) == Some(&b'#') {
        cursor += 1;
    }

    if bytes.get(cursor) == Some(&b'"') {
        Some((
            cursor + 1 - index,
            cursor - index - usize::from(bytes[index] == b'b') - 1,
        ))
    } else {
        None
    }
}

fn raw_string_ends_at(bytes: &[u8], index: usize, hashes: usize) -> bool {
    bytes.get(index) == Some(&b'"')
        && (0..hashes).all(|offset| bytes.get(index + 1 + offset) == Some(&b'#'))
}

fn starts_with(bytes: &[u8], index: usize, needle: &[u8]) -> bool {
    bytes.get(index..index + needle.len()) == Some(needle)
}

fn looks_like_lifetime(bytes: &[u8], index: usize) -> bool {
    let Some(next) = bytes.get(index + 1).copied() else {
        return false;
    };

    is_rust_identifier_start(next as char) && bytes.get(index + 2) != Some(&b'\'')
}

fn is_rust_identifier_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

fn is_rust_identifier_continue(ch: char) -> bool {
    is_rust_identifier_start(ch) || ch.is_ascii_digit()
}

fn push_spaces(code: &mut String, count: usize) {
    code.extend(std::iter::repeat_n(' ', count));
}

fn push_spaces_for_remaining(code: &mut String, bytes: &[u8], index: usize) {
    push_spaces(code, bytes.len().saturating_sub(index));
}

fn allowed_engine_consolidation_storage_use(
    rel: &str,
) -> Option<&'static AllowedEngineConsolidationStorageUse> {
    ALLOWED_ENGINE_CONSOLIDATION_STORAGE_USES
        .iter()
        .find(|allowed| allowed.path == rel)
}

fn find_violations(contents: &str) -> Vec<String> {
    let mut violations = Vec::new();

    for pattern in FORBIDDEN_DIRECT_PATTERNS {
        for (line_no, line) in contents.lines().enumerate() {
            if line.contains(pattern) {
                violations.push(format!("{}: contains `{pattern}`", line_no + 1));
            }
        }
    }

    violations.extend(scan_import_blocks(
        contents,
        "strata_core::{",
        ROOT_MOVED_TOKENS,
    ));
    violations.extend(scan_import_blocks(
        contents,
        "strata_core::types::{",
        TYPES_MOVED_TOKENS,
    ));
    violations.extend(scan_import_blocks(
        contents,
        "strata_core::traits::{",
        &["Storage", "WriteMode"],
    ));
    violations.extend(scan_import_blocks(
        contents,
        "strata_concurrency::{",
        CONCURRENCY_ROOT_MOVED_TOKENS,
    ));
    violations.extend(scan_import_blocks(
        contents,
        "strata_concurrency::transaction::{",
        CONCURRENCY_TRANSACTION_MOVED_TOKENS,
    ));
    violations.extend(scan_import_blocks(
        contents,
        "strata_concurrency::validation::{",
        CONCURRENCY_VALIDATION_MOVED_TOKENS,
    ));
    violations.extend(scan_import_blocks(
        contents,
        "strata_concurrency::recovery::{",
        CONCURRENCY_RECOVERY_MOVED_TOKENS,
    ));
    violations.extend(scan_alias_uses(
        contents,
        "strata_core",
        ROOT_MOVED_TOKENS,
        &[
            "traits::Storage",
            "traits::WriteMode",
            "types::Key",
            "types::Namespace",
            "types::TypeTag",
        ],
    ));
    violations.extend(scan_alias_uses(
        contents,
        "strata_core::types",
        TYPES_MOVED_TOKENS,
        &[],
    ));
    violations.extend(scan_alias_uses(
        contents,
        "strata_core::traits",
        &["Storage", "WriteMode"],
        &[],
    ));
    violations.extend(scan_alias_uses(
        contents,
        "strata_concurrency",
        CONCURRENCY_ROOT_MOVED_TOKENS,
        &[],
    ));
    violations.extend(scan_alias_uses(
        contents,
        "strata_concurrency::transaction",
        CONCURRENCY_TRANSACTION_MOVED_TOKENS,
        &[],
    ));
    violations.extend(scan_alias_uses(
        contents,
        "strata_concurrency::validation",
        CONCURRENCY_VALIDATION_MOVED_TOKENS,
        &[],
    ));
    violations.extend(scan_alias_uses(
        contents,
        "strata_concurrency::recovery",
        CONCURRENCY_RECOVERY_MOVED_TOKENS,
        &[],
    ));

    violations
}

fn find_manifest_violations(path: &Path, contents: &str) -> Vec<String> {
    let rel = path.to_string_lossy();
    find_retired_crate_marker_violations(&rel, contents)
}

fn scan_import_blocks(contents: &str, marker: &str, forbidden_tokens: &[&str]) -> Vec<String> {
    let mut violations = Vec::new();
    let mut in_block = false;
    let mut block = String::new();
    let mut start_line = 0usize;

    for (line_no, line) in contents.lines().enumerate() {
        if !in_block
            && line.contains(marker)
            && (line.contains("use ") || line.contains("pub use "))
        {
            in_block = true;
            start_line = line_no + 1;
            block.clear();
        }

        if in_block {
            block.push_str(line);
            block.push('\n');

            if line.contains("};") || line.contains('}') {
                if forbidden_tokens
                    .iter()
                    .any(|token| block_contains_token(&block, token))
                {
                    violations.push(format!(
                        "{start_line}: forbidden {} import block",
                        marker.trim_end_matches('{')
                    ));
                }
                in_block = false;
            }
        }
    }

    violations
}

fn block_contains_token(block: &str, token: &str) -> bool {
    block.contains(&format!("{token},"))
        || block.contains(&format!("{token} "))
        || block.contains(&format!("{token}\n"))
        || block.contains(&format!("{token}}}"))
        || block.contains(&format!("{token} as "))
}

fn scan_alias_uses(
    contents: &str,
    target: &str,
    forbidden_symbols: &[&str],
    forbidden_paths: &[&str],
) -> Vec<String> {
    let mut violations = Vec::new();
    for alias in collect_aliases(contents, target) {
        for (line_no, line) in contents.lines().enumerate() {
            for symbol in forbidden_symbols {
                if line.contains(&format!("{alias}::{symbol}")) {
                    violations.push(format!(
                        "{}: contains aliased `{target}` symbol `{alias}::{symbol}`",
                        line_no + 1
                    ));
                }
            }
            for path in forbidden_paths {
                if line.contains(&format!("{alias}::{path}")) {
                    violations.push(format!(
                        "{}: contains aliased `{target}` path `{alias}::{path}`",
                        line_no + 1
                    ));
                }
            }
        }
    }
    violations
}

fn collect_aliases(contents: &str, target: &str) -> Vec<String> {
    let mut aliases = BTreeSet::new();
    let direct_use = format!("use {target} as ");
    let direct_pub_use = format!("pub use {target} as ");

    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.contains(&direct_use) || trimmed.contains(&direct_pub_use) {
            if let Some(alias) = extract_alias_after_as(trimmed) {
                aliases.insert(alias);
            }
        }
    }

    let block_marker = format!("{target}::{{");
    for (_, block) in collect_multiline_use_blocks(contents, &block_marker) {
        for alias in extract_self_aliases(&block) {
            aliases.insert(alias);
        }
    }

    aliases.into_iter().collect()
}

fn extract_alias_after_as(line: &str) -> Option<String> {
    let (_, alias) = line.split_once(" as ")?;
    let alias = alias
        .trim()
        .trim_end_matches(';')
        .trim_end_matches(',')
        .trim();
    if alias.is_empty() {
        None
    } else {
        Some(alias.to_string())
    }
}

fn collect_multiline_use_blocks(contents: &str, marker: &str) -> Vec<(usize, String)> {
    let mut blocks = Vec::new();
    let mut current = String::new();
    let mut start_line = 0usize;
    let mut depth = 0isize;
    let mut collecting = false;

    for (idx, line) in contents.lines().enumerate() {
        let trimmed = line.trim_start();
        if !collecting
            && (trimmed.contains(&format!("use {marker}"))
                || trimmed.contains(&format!("pub use {marker}")))
        {
            collecting = true;
            start_line = idx + 1;
            current.clear();
            depth = 0;
        }

        if collecting {
            current.push_str(line);
            current.push('\n');
            depth += brace_delta(line);
            if depth <= 0 && line.contains(';') {
                blocks.push((start_line, current.clone()));
                collecting = false;
            }
        }
    }

    blocks
}

fn extract_self_aliases(block: &str) -> Vec<String> {
    let mut aliases = Vec::new();
    for part in block.split(',') {
        if let Some((_, alias)) = part.split_once("self as ") {
            let alias = alias
                .trim()
                .trim_end_matches('}')
                .trim_end_matches(';')
                .trim();
            if !alias.is_empty() {
                aliases.push(alias.to_string());
            }
        }
    }
    aliases
}

fn brace_delta(line: &str) -> isize {
    let opens = line.chars().filter(|c| *c == '{').count() as isize;
    let closes = line.chars().filter(|c| *c == '}').count() as isize;
    opens - closes
}

#[test]
fn aliased_storage_imports_are_detected() {
    let contents = r#"
use strata_core as sc;
use strata_core::types as types;
use strata_core::traits::{
    self as storage_traits,
};

fn sample() {
    let _ = sc::Key::new_space_prefix(sc::BranchId::new());
    let _ = types::Namespace::for_branch(sc::BranchId::new());
    let _storage: &dyn storage_traits::Storage;
}
"#;

    let violations = find_violations(contents);
    assert!(
        violations.iter().any(|line| line.contains("sc::Key")),
        "root alias should be detected"
    );
    assert!(
        violations
            .iter()
            .any(|line| line.contains("types::Namespace")),
        "module alias should be detected"
    );
    assert!(
        violations
            .iter()
            .any(|line| line.contains("storage_traits::Storage")),
        "self-alias import blocks should be detected"
    );
}

#[test]
fn vector_storage_key_layout_guard_detects_production_use() {
    let contents = r#"
fn product_runtime() {
    let _tag = TypeTag::Vector;
    let _row = Key::new_vector(namespace, collection, key);
    let _config = Key::new_vector_config(namespace, collection);
    let _prefix = Key::vector_collection_prefix(namespace, collection);
    let _config_prefix = new_vector_config_prefix(namespace);
}
"#;

    let violations =
        find_vector_storage_key_layout_violations("crates/example/src/lib.rs", contents);
    assert_eq!(violations.len(), 5, "{violations:?}");
}

#[test]
fn vector_storage_key_layout_guard_ignores_literals_comments_and_cfg_test_modules() {
    let contents = r###"
const TAG_TEXT: &str = "TypeTag::Vector";
const RAW_PREFIX: &str = r#"Key::vector_collection_prefix"#;

// Key::new_vector(namespace, collection, key)
/* Key::new_vector_config(namespace, collection) */

#[cfg(test)]
mod vector_storage_tests {
    fn test_runtime() {
        let _tag = TypeTag::Vector;
        let _row = Key::new_vector(namespace, collection, key);
    }
}

fn product_runtime() {
    let _tag = TypeTag::Vector;
}
"###;

    let violations =
        find_vector_storage_key_layout_violations("crates/example/src/lib.rs", contents);
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert!(violations[0].contains("TypeTag::Vector"), "{violations:?}");
}

#[test]
fn graph_subsystem_guard_detects_production_use() {
    let contents = r#"
use strata_engine::GraphSubsystem;

fn product_runtime() {
    let _subsystem = GraphSubsystem;
}
"#;

    let violations =
        find_upper_subsystem_violations("crates/example/src/lib.rs", contents, "GraphSubsystem");
    assert_eq!(violations.len(), 2);
}

#[test]
fn direct_storage_guard_detects_production_use() {
    let contents = r#"
use strata_storage::{Key, Namespace};

fn product_runtime() {
    let _ = strata_storage::TypeTag::KV;
}
"#;

    let references = find_direct_storage_references("crates/example/src/lib.rs", contents);
    assert_eq!(references.len(), 2, "{references:?}");
}

#[test]
fn direct_storage_guard_ignores_literals_comments_and_cfg_test_modules() {
    let contents = r###"
const PACKAGE_TEXT: &str = "strata-storage";
const IMPORT_TEXT: &str = r#"strata_storage"#;

// use strata_storage::{Key, Namespace};
/* strata-storage */

#[cfg(test)]
mod tests {
    use strata_storage::{Key, Namespace};

    fn fixture() {
        let _key = Key::new_json(namespace, "doc");
    }
}

fn product_runtime() {
    let _ = strata_storage::TypeTag::KV;
}
"###;

    let references = find_direct_storage_references("crates/example/src/lib.rs", contents);
    assert_eq!(references.len(), 1, "{references:?}");
    assert!(references[0].1.contains("strata_storage::TypeTag"));
}

#[test]
fn forbidden_marker_guard_detects_rust_code_markers() {
    let contents = r#"
use strata_storage::Key;
use strata_engine::database::OpenSpec;

fn product_runtime() {
    let _ = OpenSpec::cache().with_subsystem(SearchSubsystem);
    let _ = runtime.with_subsystem(SearchSubsystem);
}
"#;

    let violations = find_forbidden_marker_violations(
        "crates/example/src/lib.rs",
        contents,
        &["strata_storage", "OpenSpec", ".with_subsystem"],
        "test marker",
    );
    assert_eq!(violations.len(), 4, "{violations:?}");
}

#[test]
fn forbidden_marker_guard_ignores_rust_literals_comments_and_substrings() {
    let contents = r###"
const TEXT: &str = "OpenSpec";
const RAW: &str = r#"strata_storage"#;

// use strata_storage::Key;
/* OpenSpec::cache().with_subsystem(SearchSubsystem); */

fn product_runtime() {
    let _ = OpenSpeculativeRuntime;
    let _ = runtime.with_subsystematic();
}
"###;

    let violations = find_forbidden_marker_violations(
        "crates/example/src/lib.rs",
        contents,
        &["strata_storage", "OpenSpec", ".with_subsystem"],
        "test marker",
    );
    assert!(violations.is_empty(), "{violations:?}");
}

#[test]
fn forbidden_marker_guard_detects_toml_dependency_markers() {
    let contents = r#"
[dependencies]
strata-storage = { path = "../storage" }
inference = { package = "strata-inference", path = "../inference" }
"#;

    let violations = find_forbidden_marker_violations(
        "crates/example/Cargo.toml",
        contents,
        &["strata-storage", "strata-inference"],
        "test marker",
    );
    assert_eq!(violations.len(), 2, "{violations:?}");
}

#[test]
fn forbidden_marker_guard_ignores_toml_comments() {
    let contents = r#"
[dependencies]
# strata-storage = { path = "../storage" }
strata-engine = { path = "../engine" }
"#;

    let violations = find_forbidden_marker_violations(
        "crates/example/Cargo.toml",
        contents,
        &["strata-storage"],
        "test marker",
    );
    assert!(violations.is_empty(), "{violations:?}");
}

#[test]
fn retired_crate_guard_detects_manifests_lockfiles_and_rust_imports() {
    let manifest = r#"
[dependencies]
strata-security = { path = "../security" }
graph = { package = "strata-graph", path = "../graph" }
"#;
    let manifest_violations =
        find_retired_crate_marker_violations("crates/example/Cargo.toml", manifest);
    assert_eq!(manifest_violations.len(), 2, "{manifest_violations:?}");

    let lockfile = r#"
[[package]]
name = "strata-core-legacy"
"#;
    let lockfile_violations = find_retired_crate_marker_violations("Cargo.lock", lockfile);
    assert_eq!(lockfile_violations.len(), 1, "{lockfile_violations:?}");

    let source = r#"
use strata_concurrency::TransactionManager;
use strata_durability::wal::WalWriter;
"#;
    let source_violations =
        find_retired_crate_marker_violations("crates/example/src/lib.rs", source);
    assert_eq!(source_violations.len(), 2, "{source_violations:?}");
}

#[test]
fn retired_crate_guard_ignores_comments_literals_and_substrings() {
    let manifest = r#"
[dependencies]
# strata-security = { path = "../security" }
strata-graphical = "0.1"
"#;
    let manifest_violations =
        find_retired_crate_marker_violations("crates/example/Cargo.toml", manifest);
    assert!(manifest_violations.is_empty(), "{manifest_violations:?}");

    let source = r###"
const TEXT: &str = "strata_durability";
const RAW: &str = r#"strata_concurrency"#;

// use strata_concurrency::TransactionManager;
/* use strata_durability::wal::WalWriter; */

fn product_runtime() {
    let _ = strata_concurrent_runtime;
}
"###;
    let source_violations =
        find_retired_crate_marker_violations("crates/example/src/lib.rs", source);
    assert!(source_violations.is_empty(), "{source_violations:?}");
}

#[test]
fn upper_runtime_assembly_guard_detects_runtime_assembly_patterns() {
    let contents = r#"
use strata_engine::database::OpenSpec;
use strata_engine::SearchSubsystem;
use strata_engine::recovery::Subsystem;
use strata_engine::recovery::Subsystem as RuntimeSubsystem;

fn product_runtime() {
    let _spec = OpenSpec::cache();
    let _spec = _spec . with_subsystem(SearchSubsystem);
    let _spec = _spec . with_subsystems(Vec::new());
    let _subsystems: Vec<Box<dyn Subsystem>> = Vec::new();
    let _qualified: Box<dyn strata_engine::recovery::Subsystem>;
    let _aliased: Box<dyn RuntimeSubsystem>;
    let _search = SearchSubsystem;
}
"#;

    let violations = find_upper_runtime_assembly_violations("crates/example/src/lib.rs", contents);
    assert!(
        violations.len() >= 7,
        "expected OpenSpec, whitespace with_subsystem(s), Subsystem import/alias variants, and SearchSubsystem violations: {violations:?}"
    );
}

#[test]
fn upper_runtime_assembly_guard_ignores_literals_comments_cfg_tests_and_substrings() {
    let contents = r###"
const TEXT: &str = "OpenSpec";
const RAW: &str = r#"SearchSubsystem"#;

// OpenSpec::cache().with_subsystem(SearchSubsystem);
/* let _: Box<dyn Subsystem>; */

#[cfg(test)]
mod tests {
    use strata_engine::database::OpenSpec;
    use strata_engine::SearchSubsystem;

    fn test_runtime() {
        let _spec = OpenSpec::cache().with_subsystem(SearchSubsystem);
    }
}

fn product_runtime() {
    let _ = OpenSpeculativeRuntime;
    let _ = search.with_subsystematic();
}
"###;

    let violations = find_upper_runtime_assembly_violations("crates/example/src/lib.rs", contents);
    assert!(violations.is_empty(), "{violations:?}");
}

#[test]
fn upper_runtime_assembly_guard_requires_adjacent_dyn_subsystem_trait() {
    let contents = r#"
fn product_runtime() {
    let _: Box<dyn std::fmt::Debug>;
    let Subsystem = 1usize;
    let _ = Subsystem;
}
"#;

    let violations = find_upper_runtime_assembly_violations("crates/example/src/lib.rs", contents);
    assert!(violations.is_empty(), "{violations:?}");
}

#[test]
fn upper_runtime_assembly_guard_does_not_skip_feature_enabled_cfg_modules() {
    let contents = r#"
#[cfg(any(test, feature = "embed"))]
mod runtime {
    use strata_engine::database::OpenSpec;

    fn product_runtime() {
        let _spec = OpenSpec::cache();
    }
}
"#;

    let violations = find_upper_runtime_assembly_violations("crates/example/src/lib.rs", contents);
    assert_eq!(violations.len(), 2, "{violations:?}");
}

#[test]
fn optional_edge_guard_parses_feature_arrays_and_dependency_lines() {
    let contents = r#"
[features]
default = []
embed = [
    "strata-executor/embed",
    "dep:strata-intelligence",
]
openai = ["embed", "strata-executor/openai"]

[dependencies]
strata-executor = { path = "../executor" }
strata-intelligence = { path = "../intelligence", features = ["embed"], optional = true }
"#;

    assert_eq!(
        manifest_feature_values(contents, "embed").as_deref(),
        Some(
            &[
                "strata-executor/embed".to_string(),
                "dep:strata-intelligence".to_string()
            ][..]
        )
    );
    assert_eq!(
        manifest_feature_values(contents, "openai").as_deref(),
        Some(&["embed".to_string(), "strata-executor/openai".to_string()][..])
    );
    let text =
        manifest_dependency_text(contents, "strata-intelligence").expect("parse dependency line");
    assert!(dependency_text_contains_marker(&text, "optional = true"));
    assert!(dependency_text_contains_marker(
        &text,
        "features = [\"embed\"]"
    ));

    let dotted = r#"
[dependencies.strata-intelligence]
path="../intelligence"
features=["embed"]
optional=true
"#;
    let text =
        manifest_dependency_text(dotted, "strata-intelligence").expect("parse dotted dependency");
    assert!(dependency_text_contains_marker(
        &text,
        "path = \"../intelligence\""
    ));
    assert!(dependency_text_contains_marker(
        &text,
        "features = [\"embed\"]"
    ));
    assert!(dependency_text_contains_marker(&text, "optional = true"));

    let alias = r#"
[target.'cfg(unix)'.dependencies.ai]
package = "strata-intelligence"
path = "../intelligence"
features = [
    "embed",
]
optional = true
"#;
    let text =
        manifest_dependency_text(alias, "strata-intelligence").expect("parse aliased dependency");
    assert!(dependency_text_contains_marker(
        &text,
        "package = \"strata-intelligence\""
    ));
    assert!(dependency_text_contains_marker(&text, "features = ["));
    assert!(dependency_text_contains_marker(&text, "\"embed\""));

    let quoted_inline = r#"
[dependencies]
"strata-intelligence" = { path = "../intelligence", features = ["embed"], optional = true }
"#;
    let text = manifest_dependency_text(quoted_inline, "strata-intelligence")
        .expect("parse quoted inline dependency key");
    assert!(dependency_text_contains_marker(&text, "optional = true"));

    let quoted_dotted = r#"
[dependencies."strata-intelligence"]
path = "../intelligence"
optional = true
"#;
    let text = manifest_dependency_text(quoted_dotted, "strata-intelligence")
        .expect("parse quoted dotted dependency key");
    assert!(dependency_text_contains_marker(&text, "optional = true"));
}

#[test]
fn optional_edge_guard_detects_root_intelligence_and_cli_feature_leaks() {
    let root = r#"
[dependencies]
"strata-intelligence" = { path = "crates/intelligence" }
ai = { package = "strata-intelligence", path = "crates/intelligence" }

[dev-dependencies]
strata-intelligence = { path = "crates/intelligence" }
"#;
    let mut violations = Vec::new();
    reject_normal_dependency(
        "Cargo.toml",
        root,
        "strata-intelligence",
        "root package default graph must reach intelligence through executor only",
        &mut violations,
    );
    assert_eq!(violations.len(), 2, "{violations:?}");

    let cli = r#"
[features]
default = []
embed = ["strata-executor/embed", "dep:strata-intelligence"]
experimental = ["dep:strata-intelligence"]
provider = ["strata-intelligence/openai"]
"#;
    let mut violations = Vec::new();
    reject_feature_markers_outside_features(
        "crates/cli/Cargo.toml",
        cli,
        &["dep:strata-intelligence", "strata-intelligence"],
        &["embed"],
        &mut violations,
    );
    assert_eq!(violations.len(), 2, "{violations:?}");
}

#[test]
fn root_storage_dependency_guard_allows_dev_dependency() {
    let contents = r#"
[dependencies]
strata-executor = { path = "crates/executor" }

[dev-dependencies]
strata-storage = { path = "crates/storage" }
"#;

    let violations =
        find_root_normal_storage_dependency_violations(Path::new("Cargo.toml"), contents);
    assert!(violations.is_empty(), "{violations:?}");
}

#[test]
fn root_storage_dependency_guard_detects_normal_dependency() {
    let contents = r#"
[dependencies]
strata-storage = { path = "crates/storage" }

[target.'cfg(unix)'.dependencies]
storage = { package = "strata-storage", path = "crates/storage" }

[target.'cfg(unix)'.dev-dependencies]
strata-storage = { path = "crates/storage" }
"#;

    let violations =
        find_root_normal_storage_dependency_violations(Path::new("Cargo.toml"), contents);
    assert_eq!(violations.len(), 2, "{violations:?}");
}

#[test]
fn graph_subsystem_guard_ignores_cfg_test_modules() {
    let contents = r#"
fn production_code() {}

#[cfg(test)]
mod tests {
    use strata_engine::GraphSubsystem;

    fn test_runtime() {
        let _subsystem = GraphSubsystem;
    }
}
"#;

    let violations =
        find_upper_subsystem_violations("crates/example/src/lib.rs", contents, "GraphSubsystem");
    assert!(violations.is_empty(), "{violations:?}");
}

#[test]
fn graph_subsystem_guard_ignores_braces_inside_cfg_test_literals_and_comments() {
    let contents = r###"
#[cfg(test)]
mod tests {
    use strata_engine::GraphSubsystem;

    const CLOSE_BRACE: &str = "}";
    const RAW_BRACES: &str = r#"{ }"#;

    // }
    /* { */

    fn test_runtime() {
        let _subsystem = GraphSubsystem;
    }
}

fn product_runtime() {
    let _subsystem = strata_engine::GraphSubsystem;
}
"###;

    let violations =
        find_upper_subsystem_violations("crates/example/src/lib.rs", contents, "GraphSubsystem");
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert!(
        violations[0].contains("product_runtime")
            || violations[0].contains("strata_engine::GraphSubsystem"),
        "{violations:?}"
    );
}

#[test]
fn graph_subsystem_guard_does_not_ignore_feature_enabled_cfg_modules() {
    let contents = r#"
#[cfg(any(test, feature = "test-support"))]
pub(crate) mod graph_tests {
    use strata_engine::GraphSubsystem;

    fn test_runtime() {
        let _subsystem = GraphSubsystem;
    }
}
"#;

    let violations =
        find_upper_subsystem_violations("crates/example/src/lib.rs", contents, "GraphSubsystem");
    assert_eq!(violations.len(), 2, "{violations:?}");
}

#[test]
fn graph_subsystem_guard_does_not_ignore_not_test_cfg_modules() {
    let contents = r#"
#[cfg(not(test))]
mod runtime {
    fn product_runtime() {
        let _subsystem = strata_engine::GraphSubsystem;
    }
}
"#;

    let violations =
        find_upper_subsystem_violations("crates/example/src/lib.rs", contents, "GraphSubsystem");
    assert_eq!(violations.len(), 1, "{violations:?}");
}

#[test]
fn graph_subsystem_guard_does_not_treat_test_substrings_as_test_cfg() {
    let contents = r#"
#[cfg(test_support)]
mod runtime {
    fn product_runtime() {
        let _subsystem = strata_engine::GraphSubsystem;
    }
}
"#;

    let violations =
        find_upper_subsystem_violations("crates/example/src/lib.rs", contents, "GraphSubsystem");
    assert_eq!(violations.len(), 1, "{violations:?}");
}
