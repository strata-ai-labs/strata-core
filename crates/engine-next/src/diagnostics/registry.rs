//! Canonical registry of engine error codes.
//!
//! Every engine error code is listed here exactly once, grouped by its stable
//! [`EngineErrorClass`]. This is the single source of truth that keeps codes
//! from drifting: a debug assertion in the error constructor validates that
//! every constructed `(code, class)` pair is registered, and the tests below
//! prove the registry stays complete and free of dead entries.

use super::EngineErrorClass;

/// One class and the codes that map to it.
struct CodeGroup {
    class: EngineErrorClass,
    codes: &'static [&'static str],
}

const INVALID_INPUT_CODES: &[&str] = &[
    "invalid_argument.engine.branch_catalog",
    "invalid_argument.engine.branch_delete",
    "invalid_argument.engine.branch_name",
    "invalid_argument.engine.branch_name_reserved",
    "invalid_argument.engine.config_key",
    "invalid_argument.engine.event_append",
    "invalid_argument.engine.event_batch",
    "invalid_argument.engine.event_metadata",
    "invalid_argument.engine.event_payload",
    "invalid_argument.engine.event_payload_too_large",
    "invalid_argument.engine.event_record",
    "invalid_argument.engine.event_type",
    "invalid_argument.engine.graph_batch",
    "invalid_argument.engine.graph_binding",
    "invalid_argument.engine.graph_binding_record",
    "invalid_argument.engine.graph_edge_endpoint",
    "invalid_argument.engine.graph_edge_record",
    "invalid_argument.engine.graph_edge_type",
    "invalid_argument.engine.graph_edge_type_reserved",
    "invalid_argument.engine.graph_edge_weight",
    "invalid_argument.engine.graph_metadata",
    "invalid_argument.engine.graph_name",
    "invalid_argument.engine.graph_name_reserved",
    "invalid_argument.engine.graph_node_id",
    "invalid_argument.engine.graph_node_record",
    "invalid_argument.engine.graph_properties",
    "invalid_argument.engine.graph_properties_too_large",
    "invalid_argument.engine.json_array_too_large",
    "invalid_argument.engine.json_batch",
    "invalid_argument.engine.json_batch_duplicate_document",
    "invalid_argument.engine.json_document",
    "invalid_argument.engine.json_document_id",
    "invalid_argument.engine.json_document_too_deep",
    "invalid_argument.engine.json_document_too_large",
    "invalid_argument.engine.json_index",
    "invalid_argument.engine.json_index_name",
    "invalid_argument.engine.json_index_name_reserved",
    "invalid_argument.engine.json_path",
    "invalid_argument.engine.json_path_not_found",
    "invalid_argument.engine.json_path_too_long",
    "invalid_argument.engine.json_path_type",
    "invalid_argument.engine.json_value",
    "invalid_argument.engine.kv_batch",
    "invalid_argument.engine.kv_batch_duplicate_key",
    "invalid_argument.engine.kv_key",
    "invalid_argument.engine.persistence",
    "invalid_argument.engine.product_space",
    "invalid_argument.engine.product_space_reserved",
    "invalid_argument.engine.space_catalog",
    "invalid_argument.engine.space_delete_default",
    "invalid_argument.engine.space_delete_too_large",
    "invalid_argument.engine.vector_artifact",
    "invalid_argument.engine.vector_artifact_budget",
    "invalid_argument.engine.vector_batch",
    "invalid_argument.engine.vector_collection",
    "invalid_argument.engine.vector_collection_reserved",
    "invalid_argument.engine.vector_dimension",
    "invalid_argument.engine.vector_embedding",
    "invalid_argument.engine.vector_filter",
    "invalid_argument.engine.vector_index_manifest",
    "invalid_argument.engine.vector_key",
    "invalid_argument.engine.vector_metadata",
    "invalid_argument.engine.vector_metadata_field",
    "invalid_argument.engine.vector_metadata_patch",
    "invalid_argument.engine.vector_metadata_too_large",
    "invalid_argument.engine.vector_record",
];

const NOT_FOUND_CODES: &[&str] = &[
    "not_found.engine.branch",
    "not_found.engine.graph",
    "not_found.engine.json_document",
    "not_found.engine.persistence",
    "not_found.engine.persistence_history",
    "not_found.engine.vector_collection",
];

const CONFLICT_CODES: &[&str] = &[
    "already_exists.engine.branch",
    "already_exists.engine.graph",
    "already_exists.engine.json_document",
    "already_exists.engine.json_index",
    "already_exists.engine.vector_collection",
    "conflict.engine.branch_generation",
    "conflict.engine.persistence",
    "failed_precondition.engine.space_not_empty",
];

const CORRUPTION_CODES: &[&str] = &[
    "data_loss.engine.branch_catalog",
    "data_loss.engine.branch_create_pending",
    "data_loss.engine.branch_id",
    "data_loss.engine.control_name",
    "data_loss.engine.control_plane",
    "data_loss.engine.control_plane_missing",
    "data_loss.engine.event_index_key",
    "data_loss.engine.event_key",
    "data_loss.engine.event_metadata",
    "data_loss.engine.event_record",
    "data_loss.engine.graph_binding_key",
    "data_loss.engine.graph_binding_record",
    "data_loss.engine.graph_edge_key",
    "data_loss.engine.graph_edge_record",
    "data_loss.engine.graph_index",
    "data_loss.engine.graph_key",
    "data_loss.engine.graph_metadata",
    "data_loss.engine.graph_node_key",
    "data_loss.engine.graph_node_record",
    "data_loss.engine.graph_reverse_edge_key",
    "data_loss.engine.json_document",
    "data_loss.engine.json_index",
    "data_loss.engine.json_index_key",
    "data_loss.engine.json_key",
    "data_loss.engine.kv_key",
    "data_loss.engine.kv_value",
    "data_loss.engine.persistence_recovery",
    "data_loss.engine.space_catalog",
    "data_loss.engine.vector_artifact",
    "data_loss.engine.vector_artifacts",
    "data_loss.engine.vector_collection",
    "data_loss.engine.vector_collection_key",
    "data_loss.engine.vector_index_manifest",
    "data_loss.engine.vector_index_manifest_key",
    "data_loss.engine.vector_key",
    "data_loss.engine.vector_record",
];

const AMBIGUOUS_COMMIT_CODES: &[&str] = &["ambiguous_commit.engine.persistence"];

const UNAVAILABLE_CODES: &[&str] = &[
    "failed_precondition.engine.persistence",
    "unavailable.engine.control_plane",
    "unavailable.engine.persistence",
    "unavailable.engine.persistence_budget",
    "unavailable.engine.persistence_capability",
    "unavailable.engine.vector_artifacts",
];

const INCOMPATIBLE_LAYOUT_CODES: &[&str] = &[
    "failed_precondition.engine.branch_status",
    "failed_precondition.engine.capability_registry",
    "failed_precondition.engine.control_payload_version",
    "failed_precondition.engine.default_branch",
    "failed_precondition.engine.layout_version",
    "failed_precondition.engine.migration_registry",
    "failed_precondition.engine.storage_registry",
    "failed_precondition.engine.vector_artifact",
    "failed_precondition.engine.vector_index_manifest",
];

const CLOSED_RUNTIME_CODES: &[&str] = &["failed_precondition.engine.runtime_closed"];

const INTERNAL_CODES: &[&str] = &["internal.engine.persistence"];

const GROUPS: &[CodeGroup] = &[
    CodeGroup {
        class: EngineErrorClass::InvalidInput,
        codes: INVALID_INPUT_CODES,
    },
    CodeGroup {
        class: EngineErrorClass::NotFound,
        codes: NOT_FOUND_CODES,
    },
    CodeGroup {
        class: EngineErrorClass::Conflict,
        codes: CONFLICT_CODES,
    },
    CodeGroup {
        class: EngineErrorClass::Corruption,
        codes: CORRUPTION_CODES,
    },
    CodeGroup {
        class: EngineErrorClass::AmbiguousCommit,
        codes: AMBIGUOUS_COMMIT_CODES,
    },
    CodeGroup {
        class: EngineErrorClass::Unavailable,
        codes: UNAVAILABLE_CODES,
    },
    CodeGroup {
        class: EngineErrorClass::IncompatibleLayout,
        codes: INCOMPATIBLE_LAYOUT_CODES,
    },
    CodeGroup {
        class: EngineErrorClass::ClosedRuntime,
        codes: CLOSED_RUNTIME_CODES,
    },
    CodeGroup {
        class: EngineErrorClass::Internal,
        codes: INTERNAL_CODES,
    },
];

/// Returns the registered class for `code`, or `None` if it is not registered.
pub(crate) fn class_for_code(code: &str) -> Option<EngineErrorClass> {
    GROUPS
        .iter()
        .find(|group| group.codes.contains(&code))
        .map(|group| group.class)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::{Path, PathBuf};

    const CLASS_PREFIXES: &[&str] = &[
        "invalid_argument",
        "not_found",
        "already_exists",
        "conflict",
        "failed_precondition",
        "data_loss",
        "ambiguous_commit",
        "unavailable",
        "internal",
    ];

    fn registered_codes() -> Vec<(&'static str, EngineErrorClass)> {
        GROUPS
            .iter()
            .flat_map(|group| group.codes.iter().map(move |code| (*code, group.class)))
            .collect()
    }

    fn is_code_char(byte: u8) -> bool {
        byte == b'_' || byte.is_ascii_lowercase()
    }

    /// Extracts every `<class>.engine.<detail>` string-literal code from a body.
    fn extract_codes(body: &str) -> Vec<String> {
        let bytes = body.as_bytes();
        let needle = b".engine.";
        let mut found = Vec::new();
        let mut index = 0;
        while index + needle.len() <= bytes.len() {
            if &bytes[index..index + needle.len()] != needle {
                index += 1;
                continue;
            }
            let mut prefix_start = index;
            while prefix_start > 0 && is_code_char(bytes[prefix_start - 1]) {
                prefix_start -= 1;
            }
            let mut detail_end = index + needle.len();
            while detail_end < bytes.len() && is_code_char(bytes[detail_end]) {
                detail_end += 1;
            }
            let has_prefix = prefix_start < index;
            let has_detail = detail_end > index + needle.len();
            let delimited = prefix_start > 0
                && bytes[prefix_start - 1] == b'"'
                && detail_end < bytes.len()
                && bytes[detail_end] == b'"';
            let known_prefix = has_prefix && CLASS_PREFIXES.contains(&&body[prefix_start..index]);
            if has_detail && delimited && known_prefix {
                found.push(body[prefix_start..detail_end].to_string());
            }
            index = detail_end;
        }
        found
    }

    fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).expect("read source directory") {
            let path = entry.expect("directory entry").path();
            if path.is_dir() {
                collect_rs_files(&path, out);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                out.push(path);
            }
        }
    }

    /// All codes emitted as string literals anywhere under `src`, except this
    /// registry file itself (so the reverse test cannot self-justify).
    fn scanned_source_codes() -> BTreeSet<String> {
        let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
        let registry = src.join("diagnostics").join("registry.rs");
        let mut files = Vec::new();
        collect_rs_files(&src, &mut files);
        let mut codes = BTreeSet::new();
        for file in files {
            if file == registry {
                continue;
            }
            let body = fs::read_to_string(&file).expect("read source file");
            for code in extract_codes(&body) {
                codes.insert(code);
            }
        }
        codes
    }

    #[test]
    fn registry_has_no_duplicate_codes() {
        let mut seen = BTreeSet::new();
        for (code, _class) in registered_codes() {
            assert!(seen.insert(code), "duplicate registry code: {code}");
        }
    }

    #[test]
    fn registered_classes_follow_prefix_convention() {
        for (code, class) in registered_codes() {
            let prefix = code.split('.').next().expect("code has a prefix");
            let expected = match prefix {
                "invalid_argument" => Some(EngineErrorClass::InvalidInput),
                "not_found" => Some(EngineErrorClass::NotFound),
                "already_exists" | "conflict" => Some(EngineErrorClass::Conflict),
                "data_loss" => Some(EngineErrorClass::Corruption),
                "ambiguous_commit" => Some(EngineErrorClass::AmbiguousCommit),
                "unavailable" => Some(EngineErrorClass::Unavailable),
                "internal" => Some(EngineErrorClass::Internal),
                // `failed_precondition` intentionally maps to several classes.
                "failed_precondition" => None,
                other => panic!("unknown class prefix: {other}"),
            };
            if let Some(expected) = expected {
                assert_eq!(class, expected, "code {code} has an unexpected class");
            } else {
                assert!(
                    matches!(
                        class,
                        EngineErrorClass::IncompatibleLayout
                            | EngineErrorClass::Conflict
                            | EngineErrorClass::Unavailable
                            | EngineErrorClass::ClosedRuntime
                    ),
                    "failed_precondition code {code} has unexpected class {class:?}"
                );
            }
        }
    }

    #[test]
    fn every_source_code_is_registered() {
        let registered: BTreeSet<&'static str> = registered_codes()
            .into_iter()
            .map(|(code, _)| code)
            .collect();
        let unregistered: Vec<String> = scanned_source_codes()
            .into_iter()
            .filter(|code| !registered.contains(code.as_str()))
            .collect();
        assert!(
            unregistered.is_empty(),
            "source emits unregistered engine error codes: {unregistered:?}"
        );
    }

    #[test]
    fn every_registered_code_appears_in_source() {
        let scanned = scanned_source_codes();
        let dead: Vec<&str> = registered_codes()
            .into_iter()
            .map(|(code, _)| code)
            .filter(|code| !scanned.contains(*code))
            .collect();
        assert!(
            dead.is_empty(),
            "registry has codes not emitted by source: {dead:?}"
        );
    }
}
