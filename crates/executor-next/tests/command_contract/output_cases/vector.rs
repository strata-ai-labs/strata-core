use crate::support::*;

pub(super) fn vector_outputs() -> Vec<Output> {
    let mut outputs = vector_mutation_outputs();
    outputs.extend(vector_read_outputs());
    outputs.extend(vector_batch_outputs());
    outputs
}

pub(super) fn vector_mutation_outputs() -> Vec<Output> {
    vec![
        Output::VectorWriteResult {
            collection: "docs".to_owned(),
            key: "doc-a".to_owned(),
            effect: MutationEffect::created(),
            commit: commit_receipt(1, 10, 1, 0),
            version: 1,
            timestamp: 10,
            vector_revision: 1,
        },
        Output::VectorMetadataUpdateResult {
            collection: "docs".to_owned(),
            key: "doc-a".to_owned(),
            updated: true,
            effect: MutationEffect::updated(),
            commit: Some(commit_receipt(2, 20, 1, 0)),
            version: Some(2),
            timestamp: Some(20),
            vector_revision: Some(2),
        },
        Output::VectorDeleteResult {
            collection: "docs".to_owned(),
            key: "doc-a".to_owned(),
            deleted: true,
            effect: MutationEffect::deleted(),
            commit: Some(commit_receipt(3, 30, 0, 1)),
            version: Some(3),
            timestamp: Some(30),
        },
        Output::VectorBulkDeleteResult {
            collection: "docs".to_owned(),
            deleted_count: 2,
            effect: deleted_count_effect(2),
            commit: Some(commit_receipt(4, 40, 0, 2)),
            version: Some(4),
            timestamp: Some(40),
        },
    ]
}

pub(super) fn vector_read_outputs() -> Vec<Output> {
    vec![
        Output::VectorData(Some(VectorVersionedData::new(
            "doc-a".to_owned(),
            VectorData::new(vec![1.0, 0.0], Some(json!({"kind": "doc"}))),
            1,
            10,
            1,
        ))),
        Output::VectorData(None),
        Output::VectorVersionHistory(Some(vec![
            VectorHistoryItem::new(
                "doc-a".to_owned(),
                Some(VectorData::new(vec![1.0, 0.0], None)),
                1,
                10,
                Some(1),
                false,
            ),
            VectorHistoryItem::new("doc-a".to_owned(), None, 2, 20, None, true),
        ])),
        Output::VectorVersionHistory(None),
        Output::VectorMatches(vec![VectorMatch::new(
            "doc-a".to_owned(),
            1.0,
            Some(json!({"kind": "doc"})),
        )]),
        Output::VectorIndexQuery(VectorIndexQueryResult::new(
            vec![VectorMatch::new(
                "doc-a".to_owned(),
                1.0,
                Some(json!({"kind": "doc"})),
            )],
            vector_index_diagnostics_fixture(),
        )),
        Output::VectorKeyPage {
            items: vec!["doc-a".to_owned()],
            page: PageInfo::new(true, Some("doc-a".to_owned())),
        },
        Output::VectorCollectionList {
            items: vec![VectorCollectionInfo::new(
                "docs".to_owned(),
                2,
                VectorDistanceMetric::Cosine,
                1,
            )],
            page: PageInfo::terminal(),
        },
    ]
}

pub(super) fn vector_batch_outputs() -> Vec<Output> {
    vec![
        Output::VectorBatchUpsertResults(vector_batch(vec![
            VectorBatchItemResult::new_with_effect(
                true,
                MutationEffect::created(),
                Some(commit_receipt(1, 10, 1, 0)),
                Some(1),
                Some(10),
                Some(1),
            ),
        ])),
        Output::VectorBatchUpsertResults(vector_batch(vec![VectorBatchItemResult::failed(
            "invalid vector",
        )])),
        Output::VectorBatchGetResults(vector_batch_get(vec![VectorBatchGetItemResult::new(Some(
            VectorVersionedData::new(
                "doc-a".to_owned(),
                VectorData::new(vec![1.0, 0.0], None),
                1,
                10,
                1,
            ),
        ))])),
        Output::VectorBatchGetResults(vector_batch_get(vec![VectorBatchGetItemResult::failed(
            "invalid vector key",
        )])),
        Output::VectorBatchDeleteResults(vector_batch(vec![
            VectorBatchItemResult::new_with_effect(
                true,
                MutationEffect::deleted(),
                Some(commit_receipt(2, 20, 0, 1)),
                Some(2),
                Some(20),
                None,
            ),
        ])),
    ]
}

pub(crate) fn vector_index_diagnostics_fixture() -> VectorIndexDiagnostics {
    VectorIndexDiagnostics::new(
        "docs".to_owned(),
        "loaded".to_owned(),
        Some(7),
        2,
        1,
        1,
        3,
        "auto".to_owned(),
        64,
        64,
        u64::MAX,
        4,
        true,
        16,
        64 * 1024 * 1024,
        40,
        "flat".to_owned(),
        0,
        0,
        1,
        0,
        1,
        0,
        1,
        100,
        8192,
        true,
        None,
        vec![VectorIndexArtifactSource::new(
            "artifact-flat-1".to_owned(),
            "loaded".to_owned(),
            true,
        )],
    )
}
