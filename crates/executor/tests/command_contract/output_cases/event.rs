use crate::support::*;

pub(super) fn event_outputs() -> Vec<Output> {
    vec![
        Output::EventAppendResult {
            sequence: 0,
            event_type: "user.created".to_owned(),
            effect: MutationEffect::created(),
            commit: commit_receipt(1, 10, 2, 0),
        },
        Output::EventRecord(Some(event_versioned_data(0, "user.created", 1, 10))),
        Output::EventRecord(None),
        Output::EventRecords {
            items: vec![
                event_versioned_data(0, "user.created", 1, 10),
                event_versioned_data(1, "user.updated", 2, 20),
            ],
            page: PageInfo::terminal(),
        },
        Output::EventLength { count: 2 },
        Output::EventTypeList {
            items: vec!["user.created".to_owned(), "user.updated".to_owned()],
            page: PageInfo::terminal(),
        },
        Output::EventRangeResult {
            items: vec![event_versioned_data(0, "user.created", 1, 10)],
            page: PageInfo::new(true, Some(0)),
        },
        Output::EventRangeResult {
            items: Vec::new(),
            page: PageInfo::terminal(),
        },
        Output::EventBatchAppendResults(event_batch(vec![BatchItem::ok(
            0,
            true,
            Some(MutationEffect::created()),
            Some(commit_receipt(1, 10, 2, 0)),
            EventBatchAppendItemResult::new(Some(0), Some("user.created".to_owned())),
        )])),
        Output::EventBatchAppendResults(event_batch(vec![BatchItem::failed(
            0,
            Some(EventBatchAppendItemResult::new(None, None)),
            item_error("invalid event"),
        )])),
        Output::EventChainVerification(EventChainVerification::new(true, 2, None, None)),
        Output::EventChainVerification(EventChainVerification::new(
            false,
            2,
            Some(1),
            Some("hash mismatch".to_owned()),
        )),
    ]
}
