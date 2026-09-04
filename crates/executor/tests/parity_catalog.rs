//! TCP4.7 axis: catalog executability + scan addressing (#2704) and the
//! #2691 export↔import symmetry regression pin.
//!
//! The command catalog should hand an agent everything needed to construct a
//! real call, and sibling addressing surfaces should share one convention.
//! Today the catalog's dotted ids are not the executable wire tags, and
//! `json_scan` seeks past its "prefix" — both ledgered under #2704.

use std::path::Path;

use serde_json::{json, Value};
use strata_executor::Command;

#[path = "parity/support.rs"]
mod support;

/// PIN #2704: `json_scan` treats `start` as an inclusive seek lower bound
/// over the whole space — keys past the prefix come back — while sibling
/// `json_count` filters by real prefix.
#[test]
fn pin_2704_json_scan_seeks_rather_than_prefix_filters() {
    support::pinned("json_scan_addressing", 2704);
    let mut executor = support::executor();
    for key in ["prod:1", "prod:2", "zzz:9"] {
        support::run(
            &mut executor,
            &json!({"type": "json_set", "key": key, "path": "$", "value": {"k": key}}),
        );
    }

    let count = support::run(
        &mut executor,
        &json!({"type": "json_count", "prefix": "prod:"}),
    );
    assert_eq!(
        count["data"].as_u64(),
        Some(2),
        "json_count filters by prefix"
    );

    let scan = support::run(
        &mut executor,
        &json!({"type": "json_scan", "start": "prod:", "limit": 10}),
    );
    let keys: Vec<&str> = scan["data"]["items"]
        .as_array()
        .unwrap_or_else(|| panic!("scan output carries items: {scan}"))
        .iter()
        .map(|item| {
            item["key"]
                .as_str()
                .unwrap_or_else(|| panic!("scan item carries a key: {item}"))
        })
        .collect();
    assert_eq!(
        keys,
        vec!["prod:1", "prod:2", "zzz:9"],
        "today: json_scan seeks past the prefix; if this fails, #2704's scan \
         leg was fixed — delete the ledger entry and assert prefix parity"
    );
}

/// #2704 (promoted from `pin_2704_catalog_ids_are_not_executable_wire_names`):
/// every catalog entry now publishes its executable wire `type` tag in the
/// `wire` field, so a tool reading the offline catalog can construct a real
/// call. The dotted `id` stays a stable identifier, never a wire tag.
#[test]
fn catalog_entries_publish_executable_wire_names() {
    let index_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("idl/v1/generated/command-index.json");
    let index: Value = serde_json::from_str(
        &std::fs::read_to_string(&index_path)
            .unwrap_or_else(|err| panic!("read {}: {err}", index_path.display())),
    )
    .expect("parse command index");
    let commands = index["commands"]
        .as_array()
        .expect("index carries commands");
    assert!(commands.len() >= 100, "the catalog is the full surface");

    let mut executable_wire_seen = false;
    for entry in commands {
        let id = entry["id"].as_str().expect("entry carries an id");
        let wire = entry["wire"]
            .as_str()
            .unwrap_or_else(|| panic!("entry `{id}` must carry an executable `wire` tag"));
        assert!(
            !wire.is_empty()
                && wire
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
            "wire `{wire}` (id `{id}`) must be a snake_case wire tag"
        );
        assert_ne!(wire, id, "the wire tag is not the dotted id (`{id}`)");
        // The dotted id is a stable identifier, never a wire type tag.
        assert!(
            serde_json::from_value::<Command>(json!({"type": id})).is_err(),
            "catalog id `{id}` must not parse as a wire type tag"
        );
        // A field-less command's published wire tag constructs a real call.
        if serde_json::from_value::<Command>(json!({"type": wire})).is_ok() {
            executable_wire_seen = true;
        }
    }
    assert!(
        executable_wire_seen,
        "at least one catalog `wire` tag parses as a command, proving the field is executable"
    );

    // Exact wire tags for representative commands pin the derivation.
    let wire_of = |command_id: &str| -> String {
        commands
            .iter()
            .find(|entry| entry["id"] == command_id)
            .unwrap_or_else(|| panic!("catalog carries `{command_id}`"))["wire"]
            .as_str()
            .unwrap_or_else(|| panic!("`{command_id}` carries a wire tag"))
            .to_owned()
    };
    assert_eq!(wire_of("kv.list"), "kv_list");
    assert_eq!(wire_of("json.scan"), "json_scan");
    assert_eq!(wire_of("admin.ping"), "ping");
}

/// #2691 regression pin (fixed): every primitive that can be exported can be
/// imported — the two direction enums accept the same five primitives.
#[test]
fn contract_2691_arrow_export_and_import_cover_the_same_primitives() {
    for primitive in ["kv", "json", "event", "vector", "graph"] {
        serde_json::from_value::<Command>(json!({
            "type": "arrow_export", "primitive": primitive, "format": "csv", "path": "out.csv"
        }))
        .unwrap_or_else(|err| panic!("arrow_export accepts `{primitive}`: {err}"));
        serde_json::from_value::<Command>(json!({
            "type": "arrow_import", "file_path": "in.csv", "format": "csv", "target": primitive
        }))
        .unwrap_or_else(|err| panic!("arrow_import accepts `{primitive}` (#2691): {err}"));
    }
}

/// Ledger guards (entry ⇒ pin) for the axes this target owns.
#[test]
fn every_catalog_and_scan_ledger_entry_is_pinned_here() {
    // catalog_id_not_wire_name (#2704) is fixed: entries now publish `wire`.
    support::assert_ledger_entries_all_pinned("catalog", &[]);
    support::assert_ledger_entries_all_pinned("json_scan", &[("json_scan_addressing", 2704)]);
}
