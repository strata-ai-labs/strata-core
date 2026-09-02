use crate::support::*;

#[allow(clippy::too_many_lines)]
pub(super) fn hub_outputs() -> Vec<Output> {
    vec![
        Output::HubInfo(HubInfo {
            protocol_version: "v1".to_owned(),
            server_implementation: "stratahub".to_owned(),
            server_version: "0.1.0".to_owned(),
            hash_algorithm: "blake3".to_owned(),
            max_object_size_bytes: 536_870_912,
            max_manifest_size_bytes: 1_048_576,
            max_dataset_size_bytes: 5_368_709_120,
            supported_object_content_types: vec!["application/octet-stream".to_owned()],
            telemetry_endpoint_enabled: false,
        }),
        Output::HubDatasets(HubDatasetPage {
            total: 1,
            offset: 0,
            limit: 20,
            items: vec![hub_dataset_summary()],
        }),
        Output::HubDataset(HubDatasetCard {
            summary: hub_dataset_summary(),
            owner: "stratahub".to_owned(),
            summary_excerpt: "Classic passenger-survival dataset.".to_owned(),
            created: "2026-09-01T00:00:00Z".to_owned(),
            manifest_hash:
                "blake3:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_owned(),
            engine_version_required: ">=1.1.0".to_owned(),
            format_version: "v1".to_owned(),
            capability_registry_version: 1,
            clone_command: "strata clone titanic".to_owned(),
            readme: "# Titanic\n".to_owned(),
            quick_start_snippets: [("python".to_owned(), "print('titanic')".to_owned())]
                .into_iter()
                .collect(),
            frontmatter_extras: [("source".to_owned(), json!("fixture"))]
                .into_iter()
                .collect(),
            sample_preview: Some(json!({
                "kv": [{"key": "passenger:1", "value_summary": "survived=true"}]
            })),
            schema: Some(json!({
                "kv": {
                    "namespaces": [{
                        "prefix": "passenger:",
                        "value_type": "json",
                        "entry_count": 1
                    }]
                }
            })),
            strata_features: Some(HubStrataFeatures {
                branches: vec![HubBranchHighlight {
                    name: "main".to_owned(),
                    is_default: true,
                }],
                time_travel_highlights: Vec::new(),
                multi_primitive_demos: Vec::new(),
                example_notebook: Some("examples/titanic.ipynb".to_owned()),
            }),
            citation: Some("Fixture citation.".to_owned()),
            provenance: Some(HubProvenance {
                source: "fixture".to_owned(),
                curator: "stratahub".to_owned(),
                license_text_url: Some("https://example.test/license".to_owned()),
            }),
        }),
        Output::HubRefs(HubRefList {
            dataset: "titanic".to_owned(),
            default_branch: "main".to_owned(),
            refs: vec![HubRefEntry {
                branch: "main".to_owned(),
                manifest_hash:
                    "blake3:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                        .to_owned(),
                last_updated: "2026-09-02T00:00:00Z".to_owned(),
            }],
        }),
        Output::HubYanked(HubYankedList {
            generated_at: "2026-09-02T00:00:00Z".to_owned(),
            total: 1,
            items: vec![HubYankedEntry {
                dataset: "bad-dataset".to_owned(),
                branch: "main".to_owned(),
                manifest_hash:
                    "blake3:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                        .to_owned(),
                yanked_at: "2026-09-02T00:00:00Z".to_owned(),
                reason: "policy_violation".to_owned(),
            }],
        }),
        Output::HubCloneProgress(HubCloneProgress {
            stage: HubCloneProgressStage::Resolved,
            dataset: "titanic".to_owned(),
            branch: Some("main".to_owned()),
            manifest_hash: Some(
                "blake3:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    .to_owned(),
            ),
            object_count: None,
            total_bytes: None,
            index: None,
            bytes: None,
        }),
    ]
}

fn hub_dataset_summary() -> HubDatasetSummary {
    HubDatasetSummary {
        name: "titanic".to_owned(),
        description: "Classic passenger-survival dataset.".to_owned(),
        size_bytes: 1024,
        downloads: 7,
        primitives: vec!["kv".to_owned()],
        tasks: vec!["classification".to_owned()],
        tags: vec!["tabular".to_owned()],
        license: "CC0".to_owned(),
        default_branch: "main".to_owned(),
        last_updated: "2026-09-02T00:00:00Z".to_owned(),
        badge: Some("official".to_owned()),
    }
}
