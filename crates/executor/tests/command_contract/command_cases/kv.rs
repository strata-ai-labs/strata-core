use crate::support::*;

pub(super) fn kv_commands() -> Vec<Command> {
    vec![
        Command::KvPut {
            branch: None,
            space: None,
            key: bytes("alpha"),
            value: bytes("one"),
        },
        Command::KvGet {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            key: bytes("alpha"),
            as_of: Some(42),
        },
        Command::KvDelete {
            branch: Some("feature".to_owned()),
            space: None,
            key: bytes("alpha"),
        },
        Command::KvList {
            branch: None,
            space: Some("space-a".to_owned()),
            prefix: Some(bytes("a")),
            cursor: Some(bytes("alpha")),
            limit: Some(2),
            as_of: Some(99),
        },
        Command::KvScan {
            branch: None,
            space: None,
            start: Some(bytes("a")),
            limit: Some(10),
        },
        Command::KvBatchPut {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            entries: vec![BatchKvEntry::new(bytes("a"), bytes("one"))],
        },
        Command::KvBatchGet {
            branch: None,
            space: None,
            keys: vec![bytes("a"), bytes("missing")],
        },
        Command::KvBatchDelete {
            branch: None,
            space: None,
            keys: vec![bytes("a"), bytes("missing")],
        },
        Command::KvBatchExists {
            branch: None,
            space: None,
            keys: vec![bytes("a"), bytes("missing")],
        },
        Command::KvExists {
            branch: None,
            space: None,
            key: bytes("a"),
        },
        Command::KvGetv {
            branch: None,
            space: None,
            key: bytes("a"),
        },
        Command::KvCount {
            branch: None,
            space: None,
            prefix: Some(bytes("a")),
        },
        Command::KvSample {
            branch: None,
            space: None,
            prefix: Some(bytes("a")),
            count: Some(4),
        },
    ]
}
