use crate::support::*;

pub(super) fn hub_commands() -> Vec<Command> {
    vec![
        Command::HubInfo { hub_url: None },
        Command::HubListDatasets {
            hub_url: Some("https://hub.example.test".to_owned()),
            tasks: vec!["classification".to_owned()],
            tags: vec!["tabular".to_owned()],
            primitives: vec!["kv".to_owned()],
            license: Some("CC0".to_owned()),
            size_min_bytes: Some(1),
            size_max_bytes: Some(1024),
            sort: Some(HubDatasetSort::Downloads),
            limit: Some(20),
            offset: Some(0),
        },
        Command::HubGetDataset {
            name: "titanic".to_owned(),
            hub_url: None,
        },
        Command::HubListRefs {
            dataset: "titanic".to_owned(),
            hub_url: None,
        },
        Command::HubListYanked {
            since: Some("2026-09-02T00:00:00Z".to_owned()),
            hub_url: None,
        },
    ]
}
