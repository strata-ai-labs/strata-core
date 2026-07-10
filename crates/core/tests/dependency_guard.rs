//! Dependency boundary tests for core.

#![deny(unsafe_code)]

use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    process::Command,
};

const PACKAGE_NAME: &str = "strata-core";

#[test]
fn core_next_has_no_strata_crate_dependencies() {
    let metadata = cargo_metadata();
    let packages = metadata
        .get("packages")
        .and_then(Value::as_array)
        .expect("cargo metadata should include packages");
    let nodes = metadata
        .pointer("/resolve/nodes")
        .and_then(Value::as_array)
        .expect("cargo metadata should include resolve nodes");

    let package_by_id: HashMap<&str, &Value> = packages
        .iter()
        .filter_map(|package| Some((package.get("id")?.as_str()?, package)))
        .collect();
    let node_by_id: HashMap<&str, &Value> = nodes
        .iter()
        .filter_map(|node| Some((node.get("id")?.as_str()?, node)))
        .collect();

    let package = packages
        .iter()
        .find(|package| package.get("name").and_then(Value::as_str) == Some(PACKAGE_NAME))
        .expect("core package should exist in cargo metadata");
    let package_id = package
        .get("id")
        .and_then(Value::as_str)
        .expect("core package should have an id");

    let manifest_violations = manifest_strata_dependency_violations(package);
    let graph_violations =
        graph_strata_dependency_violations(package_id, &package_by_id, &node_by_id);

    assert!(
        manifest_violations.is_empty() && graph_violations.is_empty(),
        "core must not depend on any Strata crate. Manifest violations: {manifest_violations:?}. Graph violations: {graph_violations:?}"
    );
}

fn cargo_metadata() -> Value {
    let workspace_root = workspace_root();
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .args(["metadata", "--locked", "--format-version", "1"])
        .current_dir(&workspace_root)
        .output()
        .expect("failed to run cargo metadata");

    assert!(
        output.status.success(),
        "cargo metadata failed in {}: {}",
        workspace_root.display(),
        String::from_utf8_lossy(&output.stderr)
    );

    serde_json::from_slice(&output.stdout).expect("cargo metadata should return json")
}

fn workspace_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .and_then(|crates_dir| crates_dir.parent())
        .expect("core should live under crates/core")
        .to_path_buf()
}

fn manifest_strata_dependency_violations(package: &Value) -> Vec<String> {
    package
        .get("dependencies")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|dependency| is_forbidden_manifest_dependency(dependency))
        .map(dependency_label)
        .collect()
}

fn graph_strata_dependency_violations(
    root_id: &str,
    package_by_id: &HashMap<&str, &Value>,
    node_by_id: &HashMap<&str, &Value>,
) -> Vec<String> {
    let mut violations = Vec::new();
    let mut seen = HashSet::new();
    let mut stack = vec![(root_id.to_owned(), vec![PACKAGE_NAME.to_owned()])];

    while let Some((package_id, path)) = stack.pop() {
        if !seen.insert(package_id.clone()) {
            continue;
        }

        let Some(node) = node_by_id.get(package_id.as_str()) else {
            continue;
        };
        let Some(deps) = node.get("deps").and_then(Value::as_array) else {
            continue;
        };

        for dep in deps {
            let Some(dep_id) = dep.get("pkg").and_then(Value::as_str) else {
                continue;
            };
            let dep_package = package_by_id.get(dep_id);
            let dep_name = dep_package
                .and_then(|package| package_name(package))
                .unwrap_or("<unknown>");

            let mut dep_path = path.clone();
            dep_path.push(dep_name.to_owned());

            if dep_package.is_some_and(|package| is_forbidden_strata_package(package)) {
                violations.push(dep_path.join(" -> "));
            } else {
                stack.push((dep_id.to_owned(), dep_path));
            }
        }
    }

    violations
}

fn is_forbidden_manifest_dependency(dependency: &Value) -> bool {
    let Some(name) = dependency.get("name").and_then(Value::as_str) else {
        return false;
    };
    name != PACKAGE_NAME
        && (source_is_workspace(dependency.get("source")) || is_forbidden_strata_name(name))
}

fn is_forbidden_strata_package(package: &Value) -> bool {
    let Some(name) = package_name(package) else {
        return false;
    };
    name != PACKAGE_NAME
        && (source_is_workspace(package.get("source")) || is_forbidden_strata_name(name))
}

fn is_forbidden_strata_name(name: &str) -> bool {
    name == "stratadb" || name.starts_with("strata-")
}

fn source_is_workspace(source: Option<&Value>) -> bool {
    source.is_some_and(Value::is_null)
}

fn package_name(package: &Value) -> Option<&str> {
    package.get("name").and_then(Value::as_str)
}

fn dependency_label(dependency: &Value) -> String {
    let name = dependency
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("<unknown>");
    match dependency.get("rename").and_then(Value::as_str) {
        Some(rename) => format!("{name} as {rename}"),
        None => name.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        graph_strata_dependency_violations, manifest_strata_dependency_violations, PACKAGE_NAME,
    };
    use serde_json::{json, Value};
    use std::collections::HashMap;

    #[test]
    fn manifest_guard_allows_external_dependencies() {
        let package = package_with_dependencies(&[dependency(
            "serde",
            Some("registry+https://github.com/rust-lang/crates.io-index"),
            None,
        )]);

        assert_eq!(
            manifest_strata_dependency_violations(&package),
            Vec::<String>::new()
        );
    }

    #[test]
    fn manifest_guard_rejects_named_strata_dependencies() {
        let package =
            package_with_dependencies(&[dependency("strata-storage", None, Some("storage_alias"))]);

        assert_eq!(
            manifest_strata_dependency_violations(&package),
            vec!["strata-storage as storage_alias"]
        );
    }

    #[test]
    fn manifest_guard_rejects_workspace_dependencies_without_strata_prefix() {
        let package = package_with_dependencies(&[dependency("internal-tools", None, None)]);

        assert_eq!(
            manifest_strata_dependency_violations(&package),
            vec!["internal-tools"]
        );
    }

    #[test]
    fn graph_guard_allows_external_dependency_graph() {
        let packages = vec![
            package(PACKAGE_NAME, "core", None),
            package(
                "serde",
                "serde",
                Some("registry+https://github.com/rust-lang/crates.io-index"),
            ),
        ];
        let nodes = vec![node("core", vec!["serde"]), node("serde", vec![])];
        let package_by_id = package_map(&packages);
        let node_by_id = node_map(&nodes);

        assert_eq!(
            graph_strata_dependency_violations("core", &package_by_id, &node_by_id),
            Vec::<String>::new()
        );
    }

    #[test]
    fn graph_guard_rejects_direct_workspace_dependencies() {
        let packages = vec![
            package(PACKAGE_NAME, "core", None),
            package("internal-tools", "internal-tools", None),
        ];
        let nodes = vec![
            node("core", vec!["internal-tools"]),
            node("internal-tools", vec![]),
        ];
        let package_by_id = package_map(&packages);
        let node_by_id = node_map(&nodes);

        assert_eq!(
            graph_strata_dependency_violations("core", &package_by_id, &node_by_id),
            vec!["strata-core -> internal-tools"]
        );
    }

    #[test]
    fn graph_guard_rejects_transitive_strata_dependencies() {
        let packages = vec![
            package(PACKAGE_NAME, "core", None),
            package(
                "external",
                "external",
                Some("registry+https://github.com/rust-lang/crates.io-index"),
            ),
            package("strata-storage", "strata-storage", None),
        ];
        let nodes = vec![
            node("core", vec!["external"]),
            node("external", vec!["strata-storage"]),
            node("strata-storage", vec![]),
        ];
        let package_by_id = package_map(&packages);
        let node_by_id = node_map(&nodes);

        assert_eq!(
            graph_strata_dependency_violations("core", &package_by_id, &node_by_id),
            vec!["strata-core -> external -> strata-storage"]
        );
    }

    fn package_with_dependencies(dependencies: &[Value]) -> Value {
        json!({ "dependencies": dependencies })
    }

    fn dependency(name: &str, source: Option<&str>, rename: Option<&str>) -> Value {
        json!({
            "name": name,
            "source": source,
            "rename": rename,
        })
    }

    fn package(name: &str, id: &str, source: Option<&str>) -> Value {
        json!({
            "id": id,
            "name": name,
            "source": source,
        })
    }

    fn node(id: &str, deps: Vec<&str>) -> Value {
        json!({
            "id": id,
            "deps": deps
                .into_iter()
                .map(|dep| json!({ "pkg": dep }))
                .collect::<Vec<_>>(),
        })
    }

    fn package_map(packages: &[Value]) -> HashMap<&str, &Value> {
        packages
            .iter()
            .map(|package| {
                (
                    package
                        .get("id")
                        .and_then(Value::as_str)
                        .expect("package id"),
                    package,
                )
            })
            .collect()
    }

    fn node_map(nodes: &[Value]) -> HashMap<&str, &Value> {
        nodes
            .iter()
            .map(|node| {
                (
                    node.get("id").and_then(Value::as_str).expect("node id"),
                    node,
                )
            })
            .collect()
    }
}
